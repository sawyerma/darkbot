//! The C3 PAPER path in the runtime (component `BSC_EXEC`, effective
//! `SHADOW`). Its lifecycle, outside the entry path:
//!
//! 1. the PAPER identity is loaded at start (its address is watched by C2,
//!    so the follower reads its nonce facts);
//! 2. when the component is SHADOW and the C2 owner runs, the live Flap
//!    deployment and venue terms are probed read-only on a helper thread,
//!    pinned to one block (never assumed): a drift, other terms or a funded
//!    PAPER sender stop here, visibly;
//! 3. once C2 knows the PAPER sender's next nonce exactly (zero: the
//!    identity never sends), the executor arms its fenced lanes;
//! 4. every C1-scope launch then runs the production path to the PAPER
//!    sink;
//! 5. while armed, the deployment and terms are probed again every
//!    `BSC_EXEC_REPROBE_MS`: a drift, other terms or a funded sender
//!    disarms the path, visibly, and so does a re-probe that fails more
//!    than `BSC_EXEC_REPROBE_FAILURES` times in a row (a PAPER BUY never
//!    leaves the process, so a bounded run of transient read failures keeps
//!    the observation going; each is visible). Nothing is re-armed
//!    automatically: switching the component off and on again starts over
//!    (with a new fence); a clean runtime stop disarms.

use crate::chain_follower::FollowerLimits;
use crate::paper::PaperIdentity;
use crate::paper_exec::{
    Arming, End, ExecBuy, ExecConfig, ExecIntent, Executor, LaunchContext, PaperBuy, PaperExecutor,
    PendingLaunch, Refusal,
};
use crate::runtime::state::StateOwner;
use crate::submission::transport::Clock;
use crate::venue_probe::{VenueObservation, probe_flap};
use alloy_primitives::{Address, U256};
use darkbot_bsc_geth_ipc::ingress::decode_tx_target;
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status::exec as e;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    VenueBuyTerms, WITNESSED_MAINNET_TERMS,
};
use darkbot_bsc_trading::venues::flap::deployment::{
    FlapDeployment, ValidatedDeployment, bsc_mainnet_v5_27_0,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

/// The PAPER path's configuration (`BSC_EXEC_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecSetup {
    /// The PAPER identity file (absolute after `~` expansion).
    pub identity_path: PathBuf,
    /// Accounts the PAPER identity must never be.
    pub refused_accounts: Vec<[u8; 20]>,
    pub exec: ExecConfig,
    /// How often an armed path probes the deployment again.
    pub reprobe: Duration,
    /// Consecutive failed re-probes an armed path tolerates.
    pub reprobe_failures: u32,
}

impl ExecSetup {
    /// `~/` at the start of `path` is the real user's account home
    /// (`sources::account_home`, BSC3-052), never another directory a
    /// `$HOME` names: then a `~/` path is refused (an isolated run names an
    /// absolute path).
    pub fn expand_home(path: &str) -> Result<PathBuf, String> {
        expand_home_in(
            path,
            crate::sources::account_home(),
            std::env::var_os("HOME"),
        )
    }
}

fn expand_home_in(
    path: &str,
    account: Result<PathBuf, String>,
    env_home: Option<std::ffi::OsString>,
) -> Result<PathBuf, String> {
    let Some(rest) = path.strip_prefix("~/") else {
        return Ok(PathBuf::from(path));
    };
    let home = account.map_err(|e| format!("{path}: {e}"))?;
    if let Some(h) = env_home.filter(|h| !h.is_empty())
        && Path::new(&h) != home
    {
        return Err(format!(
            "{path}: HOME is not the account home {}: name an absolute path",
            home.display()
        ));
    }
    Ok(home.join(rest))
}

/// One read-only probe of the live deployment, venue terms and caller
/// balance (`venue_probe::probe_flap` in production).
pub type Prober =
    fn(&str, &FollowerLimits, &FlapDeployment, Address) -> Result<VenueObservation, String>;

pub(crate) fn live_probe(
    endpoint: &str,
    limits: &FollowerLimits,
    deployment: &FlapDeployment,
    caller: Address,
) -> Result<VenueObservation, String> {
    probe_flap(endpoint, limits, deployment, caller).map_err(|e| e.to_string())
}

/// Where the PAPER path stands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Off,
    IdentityRefused(String),
    WaitingC2,
    Probing,
    ProbeFailed(String),
    DeploymentDrift(String),
    TermsDrift(String),
    /// The live deployment is decode-only: its economics are not witnessed
    /// on its implementation (nothing may be admitted on it).
    EconomicsUnqualified(String),
    Funded(String),
    WaitingNonce,
    ArmFailed(String),
    Armed,
}

impl Phase {
    /// A state only switching the component off and on again leaves.
    fn terminal(&self) -> bool {
        matches!(
            self,
            Self::IdentityRefused(_)
                | Self::ProbeFailed(_)
                | Self::DeploymentDrift(_)
                | Self::TermsDrift(_)
                | Self::EconomicsUnqualified(_)
                | Self::Funded(_)
                | Self::ArmFailed(_)
        )
    }
}

/// The runtime's PAPER path controller.
pub struct ExecRuntime {
    setup: ExecSetup,
    repo_root: PathBuf,
    fence_dir: PathBuf,
    sender: Option<[u8; 20]>,
    identity_error: Option<String>,
    phase: Phase,
    probe: Option<Receiver<Result<VenueObservation, String>>>,
    observation: Option<VenueObservation>,
    validated: Option<(ValidatedDeployment, VenueBuyTerms)>,
    executor: Option<PaperExecutor>,
    /// Code generations issued by successful first probes.
    codehash_generation: u64,
    /// Arms of this process (each its own fence).
    arms: u64,
    /// When the last probe was started.
    last_probe: Option<Instant>,
    probes: u64,
    /// Disarms whose fence could not be removed.
    disarm_errors: u64,
    /// Consecutive failed re-probes of the armed path, and the last error.
    reprobe_failed: u32,
    last_probe_error: Option<String>,
    prober: Prober,
}

impl ExecRuntime {
    /// At process start: loads the PAPER identity (outside the repository,
    /// owner-only, never a refused account) to learn its address.
    pub fn new(setup: ExecSetup, repo_root: &Path, fence_dir: PathBuf) -> Self {
        Self::with_prober(setup, repo_root, fence_dir, live_probe)
    }

    fn with_prober(setup: ExecSetup, repo_root: &Path, fence_dir: PathBuf, prober: Prober) -> Self {
        let loaded = PaperIdentity::load(&setup.identity_path, repo_root, &setup.refused_accounts);
        let (sender, identity_error) = match loaded {
            Ok(identity) => (Some(identity.address()), None),
            Err(e) => (None, Some(e.to_string())),
        };
        ExecRuntime {
            setup,
            repo_root: repo_root.to_path_buf(),
            fence_dir,
            sender,
            identity_error,
            phase: Phase::Off,
            probe: None,
            observation: None,
            validated: None,
            executor: None,
            codehash_generation: 0,
            arms: 0,
            last_probe: None,
            probes: 0,
            disarm_errors: 0,
            reprobe_failed: 0,
            last_probe_error: None,
            prober,
        }
    }

    /// The PAPER sender, when its identity loaded.
    pub fn sender(&self) -> Option<[u8; 20]> {
        self.sender
    }

    /// A clean runtime stop: the armed path ends and its fence goes.
    pub fn shutdown(&mut self) {
        self.probe = None;
        self.stop(Phase::Off);
    }

    fn disarm(&mut self) {
        if let Some(x) = self.executor.take()
            && x.disarm().is_err()
        {
            self.disarm_errors += 1;
        }
    }

    /// Ends the path in `phase`: the executor and the validation go.
    fn stop(&mut self, phase: Phase) {
        self.disarm();
        self.validated = None;
        self.phase = phase;
    }

    fn spawn_probe(&mut self, endpoint: &str, limits: &FollowerLimits, sender: [u8; 20]) {
        let (tx, rx) = channel();
        let endpoint = endpoint.to_string();
        let limits = *limits;
        let prober = self.prober;
        let spawned = thread::Builder::new()
            .name("bsc-exec-probe".into())
            .spawn(move || {
                let _ = tx.send(prober(
                    &endpoint,
                    &limits,
                    &bsc_mainnet_v5_27_0(),
                    Address::from(sender),
                ));
            });
        match spawned {
            Ok(_) => {
                self.probe = Some(rx);
                self.last_probe = Some(Instant::now());
                self.probes += 1;
            }
            Err(e) => self.stop(Phase::ProbeFailed(format!("spawn: {e}"))),
        }
    }

    /// The outcome of one probe: a refusal ends the path (armed or not);
    /// a matching observation validates it, or keeps it armed.
    fn observed(&mut self, result: Result<VenueObservation, String>) {
        let observation = match result {
            Ok(o) => o,
            // An armed path tolerates a bounded run of failed re-probes.
            Err(e)
                if self.executor.is_some() && self.reprobe_failed < self.setup.reprobe_failures =>
            {
                self.reprobe_failed += 1;
                self.last_probe_error = Some(e);
                return;
            }
            Err(e) => return self.stop(Phase::ProbeFailed(e)),
        };
        self.reprobe_failed = 0;
        self.last_probe_error = None;
        let generation = self.codehash_generation + 1;
        let validated = bsc_mainnet_v5_27_0().validate(&observation.deployment(), generation);
        let terms = observation.terms;
        let balance = observation.caller_balance_wei;
        let o = &observation.observed;
        let seen = format!(
            "observed {} implementation {} sha256 0x{}",
            o.portal_version,
            o.portal_implementation,
            alloy_primitives::hex::encode(o.portal_implementation_code.sha256)
        );
        self.observation = Some(observation);
        match validated {
            Err(e) => self.stop(Phase::DeploymentDrift(format!("{e}: {seen}"))),
            Ok(v) if !v.economics_witnessed() => self.stop(Phase::EconomicsUnqualified(format!(
                "{seen} is decode-only"
            ))),
            Ok(_) if terms != WITNESSED_MAINNET_TERMS => {
                self.stop(Phase::TermsDrift(format!("{terms:?}")))
            }
            Ok(_) if balance != U256::ZERO => self.stop(Phase::Funded(format!("{balance} wei"))),
            Ok(v) => {
                // An armed path stays on its validated deployment (the same
                // fingerprints); a first probe validates it.
                if self.executor.is_none() {
                    self.codehash_generation = generation;
                    self.validated = Some((v, terms));
                }
            }
        }
    }

    /// One reconcile step: `wanted` is the component effective SHADOW;
    /// `session` names this process's fences.
    pub fn tick(
        &mut self,
        wanted: bool,
        owner: Option<&StateOwner>,
        canonical: Option<(&str, &FollowerLimits)>,
        session: u64,
    ) {
        if !wanted {
            self.probe = None;
            self.observation = None;
            self.last_probe = None;
            self.stop(Phase::Off);
            return;
        }
        if let Some(error) = &self.identity_error {
            self.phase = Phase::IdentityRefused(error.clone());
            return;
        }
        let Some(sender) = self.sender else {
            return;
        };
        if self.phase.terminal() {
            return;
        }
        let (Some(owner), Some((endpoint, limits))) = (owner, canonical) else {
            self.phase = Phase::WaitingC2;
            return;
        };
        // A probe in flight: its outcome first.
        if let Some(rx) = &self.probe {
            match rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    if self.executor.is_none() {
                        self.phase = Phase::Probing;
                    }
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    self.probe = None;
                    return self.stop(Phase::ProbeFailed("probe thread ended".into()));
                }
                Ok(result) => {
                    self.probe = None;
                    self.observed(result);
                    if self.phase.terminal() {
                        return;
                    }
                }
            }
        }
        if self.executor.is_some() {
            self.phase = Phase::Armed;
            if self
                .last_probe
                .is_none_or(|at| at.elapsed() >= self.setup.reprobe)
            {
                self.spawn_probe(endpoint, limits, sender);
            }
            return;
        }
        let Some((deployment, terms)) = self.validated else {
            self.spawn_probe(endpoint, limits, sender);
            if !self.phase.terminal() {
                self.phase = Phase::Probing;
            }
            return;
        };
        let Some(next_nonce) = owner.clean_next_nonce(&sender) else {
            self.phase = Phase::WaitingNonce;
            return;
        };
        // A PAPER identity never sends: any consumed nonce means the key
        // was used elsewhere.
        if next_nonce != 0 {
            return self.stop(Phase::IdentityRefused(format!(
                "the PAPER sender's next nonce is {next_nonce}: the key has sent transactions"
            )));
        }
        let armed = PaperIdentity::load(
            &self.setup.identity_path,
            &self.repo_root,
            &self.setup.refused_accounts,
        )
        .map_err(|e| e.to_string())
        .and_then(|identity| {
            if identity.address() != sender {
                return Err("the PAPER identity file changed since start".to_string());
            }
            PaperExecutor::arm(
                identity,
                deployment,
                terms,
                self.setup.exec.clone(),
                Arming {
                    next_nonce,
                    fence_dir: &self.fence_dir,
                    session,
                    arm: self.arms,
                },
            )
            .map_err(|e| e.to_string())
        });
        self.arms += 1;
        match armed {
            Ok(executor) => {
                self.executor = Some(executor);
                self.phase = Phase::Armed;
            }
            Err(e) => self.stop(Phase::ArmFailed(e)),
        }
    }

    /// A verified pending C1-scope launch (`raw` signed bytes), received at
    /// `ingress_ns` on `clock`: the production path to the sink, when
    /// armed.
    pub fn on_launch(
        &mut self,
        hash: [u8; 32],
        raw: &[u8],
        ingress_ns: u64,
        owner: &StateOwner,
        clock: &dyn Clock,
    ) -> Option<PaperBuy> {
        let x = self.executor.as_mut()?;
        let next_nonce = owner.clean_next_nonce(&x.sender());
        launch_to(x, hash, raw, ingress_ns, owner, next_nonce, None, clock)
    }

    /// Phase I: one Strategy C decision on the PAPER path, when armed:
    /// the intents that reached the sink (an exit's approval even when its
    /// SELL was refused) and the refusal, counted by the executor. `None`
    /// when the executor is not armed.
    pub fn on_strategy(
        &mut self,
        action: &crate::runtime_strategy::StrategyAction,
        owner: &StateOwner,
        clock: &dyn Clock,
        now_unix_ms: u64,
    ) -> Option<(
        Vec<crate::paper_exec::PaperIntent>,
        Option<crate::paper_exec::Refusal>,
    )> {
        let x = self.executor.as_mut()?;
        let next_nonce = owner.clean_next_nonce(&x.sender());
        Some(strategy_to(
            x,
            action,
            owner,
            next_nonce,
            None,
            clock,
            now_unix_ms,
        ))
    }

    pub fn draft(&self) -> Draft {
        let (state, reason) = match &self.phase {
            Phase::Off => ("OFF", None),
            Phase::IdentityRefused(r) => ("IDENTITY_REFUSED", Some(r.clone())),
            Phase::WaitingC2 => ("WAITING_C2", None),
            Phase::Probing => ("PROBING", None),
            Phase::ProbeFailed(r) => ("PROBE_FAILED", Some(r.clone())),
            Phase::DeploymentDrift(r) => ("DEPLOYMENT_DRIFT", Some(r.clone())),
            Phase::TermsDrift(r) => ("TERMS_DRIFT", Some(r.clone())),
            Phase::EconomicsUnqualified(r) => ("ECONOMICS_UNQUALIFIED", Some(r.clone())),
            Phase::Funded(r) => ("FUNDED", Some(r.clone())),
            Phase::WaitingNonce => ("WAITING_NONCE", None),
            Phase::ArmFailed(r) => ("ARM_FAILED", Some(r.clone())),
            Phase::Armed => {
                let failing = self.last_probe_error.as_ref().map(|e| {
                    format!(
                        "re-probe failed {}/{} in a row: {e}",
                        self.reprobe_failed, self.setup.reprobe_failures
                    )
                });
                match self.executor.as_ref().map(PaperExecutor::exhausted) {
                    // Every lane used, or no room for one more BUY under the
                    // session cap.
                    Some(true) => ("EXHAUSTED", failing),
                    _ => ("ARMED", failing),
                }
            }
        };
        let mut d = Draft::new(&e::SCHEMA);
        d.set(e::STATE, state);
        if let Some(r) = reason {
            d.set(e::REASON, r.as_str());
        }
        if let Some(sender) = self.sender {
            d.set(
                e::PAPER_SENDER,
                format!("0x{}", alloy_primitives::hex::encode(sender)).as_str(),
            );
        }
        if let Some(o) = &self.observation {
            d.set(e::PORTAL_VERSION, o.observed.portal_version.as_str())
                .set(e::PROBE_BLOCK, o.block_number)
                .set(
                    e::PAPER_BALANCE_WEI,
                    darkbot_bsc_record::Value::U256(o.caller_balance_wei.to_be_bytes()),
                );
        }
        let (armed, left) = self.executor.as_ref().map_or((0, 0), PaperExecutor::lanes);
        let counters = self.executor.as_ref().map(|x| x.counters().clone());
        let refused = |stage: &str| counters.as_ref().map_or(0, |c| c.refused(stage));
        let spent = self
            .executor
            .as_ref()
            .map_or(U256::ZERO, PaperExecutor::spent_wei);
        d.set(e::LANES_ARMED, armed as u64)
            .set(e::LANES_LEFT, left as u64)
            .set(e::LAUNCHES, counters.as_ref().map_or(0, |c| c.launches))
            .set(e::HANDOFFS, counters.as_ref().map_or(0, |c| c.handoffs))
            .set(e::REFUSED_NOT_DECODED, refused("NOT_DECODED"))
            .set(e::REFUSED_PROFILE, refused("PROFILE"))
            .set(e::REFUSED_NO_SNAPSHOT, refused("NO_SNAPSHOT"))
            .set(e::REFUSED_ADMISSION, refused("ADMISSION"))
            .set(e::REFUSED_INTENT, refused("INTENT"))
            .set(e::REFUSED_LANES_EXHAUSTED, refused("LANES_EXHAUSTED"))
            .set(e::REFUSED_LANE, refused("LANE"))
            .set(e::REFUSED_BUILD, refused("BUILD"))
            .set(e::REFUSED_DUPLICATE_TOKEN, refused("DUPLICATE_TOKEN"))
            .set(e::REFUSED_SESSION_CAP, refused("SESSION_CAP"))
            .set(e::REFUSED_PLAN, refused("PLAN"))
            .set(e::REFUSED_DEADLINE, refused("DEADLINE"))
            .set(e::REFUSED_DISPATCH, refused("DISPATCH"))
            .set(
                e::SESSION_SPENT_WEI,
                darkbot_bsc_record::Value::U256(spent.to_be_bytes()),
            )
            .set(e::PROBES, self.probes)
            .set(e::DISARM_ERRORS, self.disarm_errors);
        let strategy = self
            .executor
            .as_ref()
            .map(|x| x.strategy_counters().clone());
        let (_, lanes_left, next_nonce, strategy_spent, nonce_gap) = self
            .executor
            .as_ref()
            .map_or((0, 0, 0, U256::ZERO, false), PaperExecutor::strategy_lanes);
        d.set(
            e::STRATEGY_INTENTS,
            strategy.as_ref().map_or(0, |c| c.launches),
        )
        .set(
            e::STRATEGY_HANDOFFS,
            strategy.as_ref().map_or(0, |c| c.handoffs),
        )
        .set(
            e::STRATEGY_REFUSED,
            strategy
                .as_ref()
                .map_or(0, |c| c.refusals.iter().map(|(_, n)| n).sum::<u64>()),
        )
        .set(e::STRATEGY_LANES_LEFT, lanes_left as u64)
        .set(e::STRATEGY_NEXT_NONCE, next_nonce)
        .set(e::STRATEGY_NONCE_GAP, nonce_gap)
        .set(
            e::STRATEGY_SPENT_WEI,
            darkbot_bsc_record::Value::U256(strategy_spent.to_be_bytes()),
        );
        for (r, n) in strategy.iter().flat_map(|c| c.refusals.iter()) {
            d.push(e::STRATEGY_REFUSALS, format!("{}={n}", r.name()).as_str());
        }
        // Gate M2: the paths that take the sender, and every C3 refusal.
        d.set(e::SENDER_PATHS, self.setup.exec.sender_paths.name());
        for (r, n) in counters.iter().flat_map(|c| c.refusals.iter()) {
            d.push(e::REFUSALS, format!("{}={n}", r.name()).as_str());
        }
        let latencies = self.executor.as_ref().map(PaperExecutor::latencies);
        d.set(e::LATENCY_SAMPLES, latencies.map_or(0, |l| l.len() as u64));
        if let Some((p50, p99, max)) = latencies.and_then(|l| l.quantiles()) {
            d.set(e::LATENCY_P50_NS, p50)
                .set(e::LATENCY_P99_NS, p99)
                .set(e::LATENCY_MAX_NS, max);
        }
        d
    }
}

/// A verified pending C1-scope launch through `executor`, whichever its
/// end (PAPER or LIVE, plan 28.10.): the one production path. `next_nonce`:
/// the nonce a new transaction of the sender may take now (BSC3-048);
/// `exit_reserve_wei`: what the exits of the open positions may still cost,
/// when the end's ledger knows it (BSC3-057).
#[allow(clippy::too_many_arguments)]
pub fn launch_to<E: End>(
    executor: &mut Executor<E>,
    hash: [u8; 32],
    raw: &[u8],
    ingress_ns: u64,
    owner: &StateOwner,
    next_nonce: Option<u64>,
    exit_reserve_wei: Option<U256>,
    clock: &dyn Clock,
) -> Option<ExecBuy<E::Handoff>> {
    let Ok(target) = decode_tx_target(raw) else {
        executor.refuse_undecodable();
        return None;
    };
    executor
        .on_launch(
            PendingLaunch {
                hash,
                raw,
                to: target.to,
                input: target.data,
                ingress_ns,
            },
            &LaunchContext {
                snapshot: &|c, p, g| owner.snapshot(c, p, g),
                head: owner.head(),
                next_nonce,
                exit_reserve_wei,
            },
            clock,
        )
        .ok()
}

/// One Strategy C decision through `executor`, whichever its end: the
/// intents that reached it (an exit's approval even when its SELL was
/// refused) and the refusal. `next_nonce` and `exit_reserve_wei` as in
/// [`launch_to`].
pub fn strategy_to<E: End>(
    executor: &mut Executor<E>,
    action: &crate::runtime_strategy::StrategyAction,
    owner: &StateOwner,
    next_nonce: Option<u64>,
    exit_reserve_wei: Option<U256>,
    clock: &dyn Clock,
    now_unix_ms: u64,
) -> (Vec<ExecIntent<E::Handoff>>, Option<Refusal>) {
    use crate::paper_exec::CanonicalContext;
    use crate::runtime_strategy::StrategyAction;
    let c2 = CanonicalContext {
        snapshot: &|policy, codehash| owner.canonical_snapshot(policy, codehash),
        head: owner.head(),
        now_unix_ms,
        next_nonce,
        exit_reserve_wei,
    };
    match *action {
        StrategyAction::Buy {
            token,
            curve,
            input_wei,
            decided_ns,
        } => match executor.on_strategy_buy(token, curve, input_wei, &c2, clock, decided_ns) {
            Ok(i) => (vec![i], None),
            Err(r) => (Vec::new(), Some(r)),
        },
        StrategyAction::Exit {
            token,
            tokens,
            curve,
            decided_ns,
        } => executor.on_strategy_exit(token, tokens, curve, &c2, clock, decided_ns),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::chain_follower::CanonicalEvent;
    use crate::paper_exec::FnClock;
    use crate::runtime::state::StateConfig;
    use crate::venue_probe::{CodeFingerprint, ObservedDeploymentData};
    use alloy_primitives::{B256, keccak256};
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status::exec as e;
    use darkbot_bsc_trading::chain_state::engine::StateLimits;
    use darkbot_bsc_trading::signer::Signer;
    use darkbot_bsc_trading::transactions::legacy::LegacyTx;
    use darkbot_bsc_trading::venues::flap::abi::{LaunchParamsV6, encode_new_token_v6};
    use std::cell::Cell;

    pub(crate) fn setup(path: PathBuf, reprobe: Duration) -> ExecSetup {
        ExecSetup {
            identity_path: path,
            refused_accounts: Vec::new(),
            exec: ExecConfig {
                input_wei: 1_000_000_000_000_000,
                slippage_bps: 100,
                exit_slippage_bps: 100,
                gas_price_wei: 50_000_000,
                gas_limit: 3_000_000,
                session_cap_wei: 1_150_000_000_000_000,
                lanes: 2,
                max_calldata: 16_384,
                latency_samples: 16,
                policy_generation: 1,
                capability_generation: 1,
                validity_blocks: 2,
                start_limit_ns: 1_000_000,
                max_frame: 65_536,
                strategy_lanes: 8,
                strategy_session_cap_wei: 100_000_000_000_000_000,
                approve_gas_limit: 100_000,
                sell_gas_limit: 1_500_000,
                strategy_max_head_age_ms: 30_000,
                live_max_head_age_ms: 86_400_000,
                live_exit_reverts_max: 2,
                sender_paths: crate::paper_exec::SenderPaths::Both,
            },
            reprobe,
            reprobe_failures: 1,
        }
    }

    fn record(x: &ExecRuntime) -> darkbot_bsc_record::Record {
        darkbot_bsc_record::decode(&e::SCHEMA, &x.draft().encode().unwrap()).unwrap()
    }

    fn state(x: &ExecRuntime) -> (String, Option<String>) {
        let r = record(x);
        (
            r.text(e::STATE).unwrap().to_owned(),
            r.opt_text(e::REASON).unwrap().map(str::to_owned),
        )
    }

    pub(crate) fn limits() -> FollowerLimits {
        FollowerLimits {
            poll: Duration::from_millis(100),
            reorg_depth: 8,
            max_catch_up: 8,
            connect_timeout: Duration::from_millis(200),
            handshake_timeout: Duration::from_millis(200),
            read_tick: Duration::from_millis(20),
            write_timeout: Duration::from_millis(200),
            request_timeout: Duration::from_millis(200),
            max_response_bytes: 1 << 20,
            backoff_initial: Duration::from_millis(100),
            backoff_max: Duration::from_millis(1_000),
            tx_attempts: 1,
            max_launch_calldata: 16_384,
        }
    }

    pub(crate) fn owner(dir: &Path, sender: [u8; 20]) -> StateOwner {
        StateOwner::fresh(
            &StateConfig {
                limits: StateLimits {
                    max_txs: 64,
                    max_candidates_per_nonce: 2,
                    max_watched_senders: 2,
                    reorg_window: 8,
                    finality_depth: 2,
                },
                watched: vec![sender],
                checkpoint_ms: 1_000,
                nonce_fact_blocks: 1,
            },
            dir.join("c2.bin"),
        )
        .unwrap()
    }

    pub(crate) const HEAD: u64 = 129_900_000;

    /// The live deployment exactly as the constants witness it.
    pub(crate) fn observation(version: &str, implementation: Address) -> VenueObservation {
        let d = bsc_mainnet_v5_27_0();
        let fingerprint = |sha256| CodeFingerprint {
            sha256,
            keccak256: B256::ZERO,
            len: 1,
        };
        VenueObservation {
            block_number: HEAD,
            block_hash: [0xb1; 32],
            chain_id: d.chain_id,
            observed: ObservedDeploymentData {
                portal_version: version.to_string(),
                portal_code: fingerprint(d.portal_runtime_sha256),
                portal_implementation: implementation,
                portal_implementation_code: fingerprint(d.portal_implementation_runtime_sha256),
                tax_v3_implementation_code: fingerprint(d.tax_v3_implementation_runtime_sha256),
            },
            terms: WITNESSED_MAINNET_TERMS,
            caller_balance_wei: U256::ZERO,
        }
    }

    fn witnessed(
        _: &str,
        _: &FollowerLimits,
        d: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        Ok(observation(d.portal_version, d.portal_implementation))
    }

    fn upgraded(
        _: &str,
        _: &FollowerLimits,
        _: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        Ok(observation("v5.27.2", Address::repeat_byte(0x6f)))
    }

    fn unreachable(
        _: &str,
        _: &FollowerLimits,
        _: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        Err("node unreachable".into())
    }

    /// The witnessed self-launch, really signed by a third party.
    pub(crate) fn launch() -> ([u8; 32], Vec<u8>, [u8; 20]) {
        launch_n(0)
    }

    /// Launch `n` of the same launcher: its own salt (token) and nonce.
    pub(crate) fn launch_n(n: u8) -> ([u8; 32], Vec<u8>, [u8; 20]) {
        let mut beneficiary = [0u8; 20];
        beneficiary[18] = 0xa1;
        beneficiary[19] = 0x1c;
        let params = LaunchParamsV6 {
            name: b"DarkBot G06A Economics",
            symbol: b"DBG06E",
            uri: b"ipfs://darkbot-g06a-economics",
            dex_thresh: 1,
            salt: {
                let mut salt = alloy_primitives::hex!(
                    "09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb"
                );
                salt[31] ^= n;
                B256::from(salt)
            },
            migrator_type: 1,
            quote_token: Address::ZERO,
            quote_amt: U256::ZERO,
            beneficiary: Address::from(beneficiary),
            permit: &[],
            extension_id: B256::ZERO,
            extension_data: &[],
            dex_id: 0,
            lp_fee_profile: 0,
            buy_tax: 100,
            sell_tax: 100,
            tax_duration: 31_536_000,
            anti_farmer_duration: 86_400,
            mkt_bps: 10_000,
            deflation_bps: 0,
            dividend_bps: 0,
            lp_bps: 0,
            minimum_share_balance: U256::ZERO,
            dividend_token: Address::ZERO,
            commission_receiver: Address::ZERO,
            token_version: 6,
        };
        let mut input = Vec::new();
        encode_new_token_v6(&params, &mut input);
        let launcher = Signer::from_secret_bytes(&[0x5a; 32]).unwrap();
        let (mut scratch, mut raw) = (Vec::new(), Vec::new());
        launcher
            .sign_legacy(
                &LegacyTx {
                    chain_id: 56,
                    nonce: 3 + u64::from(n),
                    gas_price: 1_000_000_000,
                    gas_limit: 6_000_000,
                    to: bsc_mainnet_v5_27_0().portal,
                    value: U256::ZERO,
                    input: &input,
                },
                &mut scratch,
                &mut raw,
            )
            .unwrap();
        (keccak256(&raw).0, raw, launcher.address().into_array())
    }

    /// Ticks until the state is not `PROBING` (the probe thread answers).
    fn settle(x: &mut ExecRuntime, owner: &StateOwner, endpoint: &str) {
        let lim = limits();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            x.tick(true, Some(owner), Some((endpoint, &lim)), 7);
            if state(x).0 != "PROBING" || Instant::now() > deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The lifecycle outside the entry path: a missing identity is refused
    /// visibly; without C2 it waits; a probe that cannot reach the node
    /// fails visibly and is not retried; switching off resets.
    #[test]
    fn the_paper_path_waits_probes_and_fails_closed() {
        let dir = std::env::temp_dir().join(format!("bsc-exec-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Path::new("/nonexistent-repo");
        let reprobe = Duration::from_secs(60);
        let mut missing =
            ExecRuntime::new(setup(dir.join("none.key"), reprobe), repo, dir.join("f"));
        missing.tick(true, None, None, 1);
        assert_eq!(state(&missing).0, "IDENTITY_REFUSED");
        missing.tick(false, None, None, 1);
        assert_eq!(state(&missing), ("OFF".to_owned(), None));

        let key = dir.join("id/paper.key");
        PaperIdentity::create(&key, repo).unwrap();
        let mut x = ExecRuntime::new(setup(key, reprobe), repo, dir.join("fences"));
        let sender = x.sender().unwrap();
        x.tick(true, None, None, 1);
        assert_eq!(state(&x).0, "WAITING_C2");
        let owner = owner(&dir, sender);
        // A closed local port: the probe fails, once.
        settle(&mut x, &owner, "https://127.0.0.1:1");
        let (s, reason) = state(&x);
        assert_eq!(s, "PROBE_FAILED");
        assert!(reason.is_some());
        let lim = limits();
        x.tick(true, Some(&owner), Some(("https://127.0.0.1:1", &lim)), 1);
        assert_eq!(state(&x).0, "PROBE_FAILED", "no automatic retry");
        x.tick(false, Some(&owner), None, 1);
        assert_eq!(state(&x).0, "OFF");
        assert_eq!(
            record(&x).text(e::PAPER_SENDER).unwrap(),
            format!("0x{}", alloy_primitives::hex::encode(sender))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Gate review M5/M2/M3: the integrated path. Armed on a C2 nonce fact,
    /// a real C1-scope launch goes to the sink (status counters, latency);
    /// a re-probe that finds the Portal upgraded disarms visibly (fence
    /// gone, nothing armed); OFF and SHADOW again arm a new fence.
    #[test]
    fn a_strategy_action_reaches_the_sink_only_when_armed() {
        use crate::paper_exec::{IntentKind, StrategyCurve};
        use crate::runtime_strategy::StrategyAction;
        use darkbot_bsc_trading::economics::flap::native_curve20::{
            HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
        };
        let dir = std::env::temp_dir().join(format!("bsc-exec-strategy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Path::new("/nonexistent-repo");
        let key = dir.join("id/paper.key");
        PaperIdentity::create(&key, repo).unwrap();
        let mut x = ExecRuntime::with_prober(
            setup(key, Duration::from_secs(3_600)),
            repo,
            dir.join("fences"),
            witnessed,
        );
        let sender = x.sender().unwrap();
        let mut owner = owner(&dir, sender);
        let now = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                now.set(now.get() + 3);
                now.get()
            },
        };
        let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
        let q = quote_native_buy(&NativeBuyInputs {
            token: terms,
            reserve_before: U256::ZERO,
            circulating_before: U256::ZERO,
            input_amount: U256::from(1_000_000_000_000_000_000u128),
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        let curve = StrategyCurve {
            terms,
            reserve: q.reserve_after,
            circulating: q.supply_after,
        };
        // Gate H1: the decision record is 5 s older than its processing
        // (the clock starts there); the deadline runs from the processing.
        now.set(5_000_000_000);
        let buy = StrategyAction::Buy {
            token: [0xAB; 20],
            curve,
            input_wei: U256::from(10_000_000_000_000_000u64),
            decided_ns: 0,
        };
        let exit = StrategyAction::Exit {
            token: [0xAB; 20],
            tokens: U256::from(1_000_000u64) * U256::from(10u64).pow(U256::from(18u8)),
            curve,
            decided_ns: 0,
        };
        // Not armed: nothing is executed.
        // The head below is of 1_793_000_000 s; now is 10 s later.
        let now_ms = 1_793_000_010_000;
        assert!(x.on_strategy(&buy, &owner, &clock, now_ms).is_none());
        settle(&mut x, &owner, "https://probe.invalid");
        owner.canonical(&CanonicalEvent::Block {
            number: HEAD,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            timestamp_s: 1_793_000_000,
            timestamp_ms: 1_793_000_000_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: Vec::new(),
        });
        owner.canonical(&CanonicalEvent::NonceFact {
            sender,
            next_nonce: 0,
            block_number: HEAD,
            block_hash: [0xb1; 32],
        });
        settle(&mut x, &owner, "https://probe.invalid");
        assert_eq!(state(&x).0, "ARMED");
        // Gate M1: no exit without a held BUY.
        let (none, refused) = x.on_strategy(&exit, &owner, &clock, now_ms).unwrap();
        assert_eq!(
            (none.len(), refused),
            (0, Some(crate::paper_exec::Refusal::NotHeld))
        );
        let (intents, refused) = x.on_strategy(&buy, &owner, &clock, now_ms).unwrap();
        assert_eq!((intents.len(), refused), (1, None));
        assert_eq!(intents[0].kind, IntentKind::Buy);
        assert_eq!(intents[0].times.ingress_ns, 0);
        let (intents, refused) = x.on_strategy(&exit, &owner, &clock, now_ms).unwrap();
        assert_eq!(refused, None);
        assert_eq!(
            intents.iter().map(|i| i.kind).collect::<Vec<_>>(),
            [IntentKind::Approve, IntentKind::Sell]
        );
        // A second exit of the token is refused and counted, not executed.
        assert!(
            x.on_strategy(&exit, &owner, &clock, now_ms)
                .unwrap()
                .0
                .is_empty()
        );
        let r = record(&x);
        assert_eq!(r.u64(e::STRATEGY_INTENTS).unwrap(), 4);
        assert_eq!(r.u64(e::STRATEGY_HANDOFFS).unwrap(), 3);
        assert_eq!(r.u64(e::STRATEGY_REFUSED).unwrap(), 2);
        // BUY 0, approval 1, SELL 2: the next strategy nonce is 3.
        assert_eq!(r.u64(e::STRATEGY_NEXT_NONCE).unwrap(), 3);
        // C3's own launch path is untouched.
        assert_eq!(r.u64(e::LAUNCHES).unwrap(), 0);
        // Gate M2: with the strategy holding nonce 0, a C3 BUY of this arm
        // is refused NONCE_TAKEN, and the status says so (before: counted
        // under no projected name); the paths that take the sender too.
        let (hash, raw, launcher) = launch();
        owner.pending(hash, launcher, 3, 0, 10);
        assert!(x.on_launch(hash, &raw, 10, &owner, &clock).is_none());
        let r = record(&x);
        assert_eq!(r.u64(e::LAUNCHES).unwrap(), 1);
        assert_eq!(r.texts(e::REFUSALS).unwrap(), ["NONCE_TAKEN=1"]);
        assert_eq!(r.text(e::SENDER_PATHS).unwrap(), "C3+STRATEGY_C");
        x.shutdown();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_armed_path_takes_a_launch_to_the_sink_and_disarms_on_drift() {
        let dir = std::env::temp_dir().join(format!("bsc-exec-int-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Path::new("/nonexistent-repo");
        let key = dir.join("id/paper.key");
        PaperIdentity::create(&key, repo).unwrap();
        let fences = dir.join("fences");
        let mut x = ExecRuntime::with_prober(
            setup(key, Duration::from_secs(3_600)),
            repo,
            fences.clone(),
            witnessed,
        );
        let sender = x.sender().unwrap();
        let mut owner = owner(&dir, sender);
        settle(&mut x, &owner, "https://probe.invalid");
        assert_eq!(state(&x).0, "WAITING_NONCE");
        // C2: a canonical head and the PAPER sender's nonce fact.
        owner.canonical(&CanonicalEvent::Block {
            number: HEAD,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            timestamp_s: 1_793_000_000,
            timestamp_ms: 1_793_000_000_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: Vec::new(),
        });
        owner.canonical(&CanonicalEvent::NonceFact {
            sender,
            next_nonce: 0,
            block_number: HEAD,
            block_hash: [0xb1; 32],
        });
        settle(&mut x, &owner, "https://probe.invalid");
        assert_eq!(state(&x), ("ARMED".to_owned(), None));
        assert_eq!(std::fs::read_dir(&fences).unwrap().count(), 1);

        let (hash, raw, launcher) = launch();
        owner.pending(hash, launcher, 3, 0, 10);
        let now = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                now.set(now.get() + 3);
                now.get()
            },
        };
        let buy = x.on_launch(hash, &raw, 10, &owner, &clock).unwrap();
        assert_eq!(buy.launch_hash, hash);
        // Signed bytes that are no transaction are counted, not lost.
        assert!(x.on_launch([9; 32], b"no tx", 20, &owner, &clock).is_none());
        let r = record(&x);
        assert_eq!(r.u64(e::LAUNCHES).unwrap(), 2);
        assert_eq!(r.u64(e::HANDOFFS).unwrap(), 1);
        assert_eq!(r.u64(e::REFUSED_NOT_DECODED).unwrap(), 1);
        assert_eq!(r.u64(e::LANES_LEFT).unwrap(), 1);
        assert_eq!(r.u64(e::LATENCY_SAMPLES).unwrap(), 1);
        assert_eq!(
            r.u64(e::LATENCY_MAX_NS).unwrap(),
            buy.times.sink_ns - buy.times.ingress_ns
        );

        // The shipped cap holds one BUY: no room for another (pass-2 LOW 3).
        assert_eq!(record(&x).text(e::STATE).unwrap(), "EXHAUSTED");
        // One failed re-probe is tolerated and visible (pass-2 LOW 2).
        x.prober = unreachable;
        x.last_probe = None;
        let lim = limits();
        for _ in 0..200 {
            x.tick(true, Some(&owner), Some(("https://probe.invalid", &lim)), 7);
            if state(&x).1.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let (s, reason) = state(&x);
        assert_eq!(s, "EXHAUSTED");
        assert!(reason.unwrap().contains("re-probe failed 1/1"));
        // The Portal is upgraded: the next probe disarms (M3).
        x.prober = upgraded;
        x.last_probe = None;
        settle(&mut x, &owner, "https://probe.invalid");
        // Until the drift is seen (the state before it is EXHAUSTED, not
        // ARMED: waiting for "not ARMED" raced the probe).
        for _ in 0..200 {
            x.tick(true, Some(&owner), Some(("https://probe.invalid", &lim)), 7);
            if state(&x).0 == "DEPLOYMENT_DRIFT" {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let (s, reason) = state(&x);
        assert_eq!(s, "DEPLOYMENT_DRIFT");
        assert!(reason.unwrap().contains("v5.27.2"));
        assert_eq!(record(&x).u64(e::LANES_ARMED).unwrap(), 0);
        assert_eq!(std::fs::read_dir(&fences).unwrap().count(), 0);
        assert!(x.on_launch(hash, &raw, 30, &owner, &clock).is_none());

        // OFF, then SHADOW on the witnessed deployment: a new arm (M2).
        x.prober = witnessed;
        x.tick(false, Some(&owner), None, 7);
        settle(&mut x, &owner, "https://probe.invalid");
        assert_eq!(state(&x), ("ARMED".to_owned(), None));
        assert_eq!(std::fs::read_dir(&fences).unwrap().count(), 1);
        // A clean runtime stop disarms and removes the fence (pass-2 LOW 5).
        x.shutdown();
        assert_eq!(state(&x).0, "OFF");
        assert_eq!(std::fs::read_dir(&fences).unwrap().count(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Gate review L2: a PAPER identity whose nonce was consumed has sent
    /// transactions; it is refused, never armed.
    #[test]
    fn a_used_identity_is_refused() {
        let dir = std::env::temp_dir().join(format!("bsc-exec-used-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Path::new("/nonexistent-repo");
        let key = dir.join("id/paper.key");
        PaperIdentity::create(&key, repo).unwrap();
        let mut x = ExecRuntime::with_prober(
            setup(key, Duration::from_secs(3_600)),
            repo,
            dir.join("fences"),
            witnessed,
        );
        let sender = x.sender().unwrap();
        let mut owner = owner(&dir, sender);
        owner.canonical(&CanonicalEvent::Block {
            number: HEAD,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            timestamp_s: 1,
            timestamp_ms: 1_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: Vec::new(),
        });
        owner.canonical(&CanonicalEvent::NonceFact {
            sender,
            next_nonce: 4,
            block_number: HEAD,
            block_hash: [0xb1; 32],
        });
        settle(&mut x, &owner, "https://probe.invalid");
        let (s, reason) = state(&x);
        assert_eq!(s, "IDENTITY_REFUSED");
        assert!(reason.unwrap().contains("next nonce is 4"));
        assert!(!dir.join("fences").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase I gate H1/M2, at runtime level: a Flap launch whose canonical
    /// events (launch, terms, the creator's purchase, the blocks past the
    /// observation window) run through the runtime's own
    /// `apply_canonical`, every record dated at time 0 (seconds before its
    /// processing): at the decision block's `BlockComplete` the PASS goes
    /// through the armed executor to the sink and is journaled as `J`
    /// (before: the transport deadline ran from the record and refused it).
    /// The 1 ms transport start limit holds in an optimized build only
    /// (`cargo test --release ... -- --include-ignored`).
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "the 1 ms transport start limit needs a release build"
    )]
    fn a_runtime_pass_reaches_the_sink_and_the_journal() {
        use crate::chain_follower::{
            CanonicalLaunch, CanonicalTerm, CanonicalTrade, FlapTerm, FourToken, Venue,
        };
        use crate::runtime::{
            Canonical, Clock, RuntimeConfig, apply_canonical, journal_max_payload,
        };
        use crate::runtime_journal::{JournalSetup, RuntimeJournal};
        use crate::runtime_strategy::StrategyRuntime;
        use darkbot_bsc_journal::RecordKind;
        use darkbot_bsc_journal::storage::run::RunReader;
        use darkbot_bsc_trading::economics::flap::native_curve20::{
            HoldingCap, NativeBuyInputs, PricedToken, curve20, profile_terms, quote_native_buy,
        };
        use darkbot_bsc_trading::economics::network::{
            Action, GasEvidence, PathClass, Venue as GasVenue,
        };
        let dir =
            std::env::temp_dir().join(format!("bsc-exec-runtime-pass-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Path::new("/nonexistent-repo");
        let key = dir.join("id/paper.key");
        PaperIdentity::create(&key, repo).unwrap();
        let mut x = ExecRuntime::with_prober(
            setup(key, Duration::from_secs(3_600)),
            repo,
            dir.join("fences"),
            witnessed,
        );
        let sender = x.sender().unwrap();
        let mut owner = owner(&dir, sender);
        settle(&mut x, &owner, "https://probe.invalid");
        owner.canonical(&CanonicalEvent::Block {
            number: HEAD,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            timestamp_s: 1_793_000_000,
            timestamp_ms: 1_793_000_000_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: Vec::new(),
        });
        owner.canonical(&CanonicalEvent::NonceFact {
            sender,
            next_nonce: 0,
            block_number: HEAD,
            block_hash: [0xb1; 32],
        });
        settle(&mut x, &owner, "https://probe.invalid");
        assert_eq!(state(&x).0, "ARMED");

        let config =
            RuntimeConfig::parse(include_str!("../../../../config/ingress.public.env")).unwrap();
        // Gate L-a: blocks of now (an older head than
        // BSC_EXEC_STRATEGY_MAX_HEAD_AGE_MS is refused).
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let mut c = Canonical::new(config.canonical.as_ref().unwrap(), t);
        let mut cfg = config.strategy.engine.clone();
        cfg.rules = Vec::new();
        let mut gas = GasEvidence::default();
        for (action, used) in [
            (Action::Buy, 723_604),
            (Action::Approve, 49_255),
            (Action::Sell, 768_101),
        ] {
            gas.add(
                PathClass {
                    venue: GasVenue::Flap,
                    action,
                },
                used,
            )
            .unwrap();
        }
        c.strategy = Some(StrategyRuntime::new(cfg, gas));
        let mut journal = RuntimeJournal::open(
            &dir.join("journal"),
            7,
            1,
            JournalSetup::whole(256, journal_max_payload(&config)),
        )
        .unwrap();
        // The runtime's clock started 600 ms ago: every record dated 0 is
        // older than a whole ingress pass when it is processed.
        let clock = Clock(
            Instant::now()
                .checked_sub(Duration::from_millis(600))
                .unwrap(),
        );
        let (mut owner, mut exec) = (Some(owner), Some(x));
        let mut live = None;
        let mut apply = |c: &mut Canonical, journal: &mut RuntimeJournal, event| {
            // Every record is dated at 0, before the processing.
            apply_canonical(
                c, &mut owner, &mut exec, &mut live, journal, clock, 0, event,
            )
        };
        let token = [0xAB; 20];
        let creator = [0xCC; 20];
        let hash = |i: u64| [(i % 250) as u8 + 1; 32];
        let block = |i: u64| CanonicalEvent::Block {
            number: HEAD + i,
            hash: hash(i),
            parent_hash: if i == 1 { [0xb1; 32] } else { hash(i - 1) },
            timestamp_s: (t + (i - 1) * 450) / 1_000,
            timestamp_ms: t + (i - 1) * 450,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: Vec::new(),
        };
        let complete = |i: u64| CanonicalEvent::BlockComplete {
            number: HEAD + i,
            hash: hash(i),
        };
        apply(&mut c, &mut journal, block(1));
        apply(
            &mut c,
            &mut journal,
            CanonicalEvent::Launch(CanonicalLaunch {
                venue: Venue::Flap,
                block_number: HEAD + 1,
                block_hash: hash(1),
                tx_index: 1,
                tx_hash: [0x11; 32],
                token: Some(token),
                creator: Some(creator),
                external_buys_after: Some(0),
                first_external_buy_index: None,
                sender: Some(creator),
                nonce: Some(0),
                c1_scope: true,
                venue_target: true,
                four_token: FourToken::NotApplicable,
                unverifiable: None,
            }),
        );
        let c20 = curve20();
        let word = |v: U256| v.to_be_bytes::<32>();
        for (i, term) in [
            FlapTerm::Version(6),
            FlapTerm::QuoteToken([0; 20]),
            FlapTerm::Curve {
                r: word(c20.r),
                h: word(c20.h),
                k: word(c20.k),
            },
            FlapTerm::DexThreshold(word(
                U256::from(8u64) * U256::from(10u64).pow(U256::from(26u64)),
            )),
            FlapTerm::Tax {
                buy: word(U256::from(100u8)),
                sell: word(U256::from(100u8)),
            },
        ]
        .into_iter()
        .enumerate()
        {
            apply(
                &mut c,
                &mut journal,
                CanonicalEvent::FlapTerm(CanonicalTerm {
                    block_number: HEAD + 1,
                    block_hash: hash(1),
                    tx_index: 1,
                    log_index: i as u64,
                    token,
                    term,
                }),
            );
        }
        let eth = U256::from(5_000_000_000_000_000u64);
        let q = quote_native_buy(&NativeBuyInputs {
            token: PricedToken::new(&profile_terms(100, 100)).unwrap(),
            reserve_before: U256::ZERO,
            circulating_before: U256::ZERO,
            input_amount: eth,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        apply(
            &mut c,
            &mut journal,
            CanonicalEvent::FlapTrade(CanonicalTrade {
                block_number: HEAD + 1,
                block_hash: hash(1),
                tx_index: 1,
                tx_hash: [0x11; 32],
                log_index: 9,
                origin: creator,
                sell: false,
                token,
                trader: creator,
                amount: word(q.output_amount),
                eth: word(eth),
                fee: word(q.protocol_fee),
                post_price: [0; 32],
            }),
        );
        apply(&mut c, &mut journal, complete(1));
        // 450 ms blocks: the 3 s window closes at the 8th block.
        for i in 2..=9 {
            apply(&mut c, &mut journal, block(i));
            apply(&mut c, &mut journal, complete(i));
        }
        let strategy = c.strategy.as_ref().unwrap();
        assert_eq!(
            (strategy.counters.passed, strategy.counters.paper_unarmed),
            (1, 0)
        );
        let x = exec.as_ref().unwrap();
        let r = record(x);
        assert_eq!(r.u64(e::STRATEGY_INTENTS).unwrap(), 1);
        assert_eq!(
            r.u64(e::STRATEGY_HANDOFFS).unwrap(),
            1,
            "{:?}",
            r.get(e::STRATEGY_REFUSALS)
        );
        journal.finish().unwrap();
        let mut reader =
            RunReader::open(&dir.join("journal").join("7"), journal_max_payload(&config)).unwrap();
        let (mut intents, mut acted) = (0, 0);
        while let Some(rec) = reader.next_record().unwrap() {
            // Gate M1: the action at the PASS block's completion is journaled.
            if rec.kind == RecordKind::Runtime
                && rec.payload.len() == crate::runtime_strategy::ACTION_LEN
                && rec.payload[0] == b'A'
            {
                acted += 1;
            }
            if rec.kind == RecordKind::Runtime
                && crate::runtime_strategy::paper_intent_kind(&rec.payload)
                    == Some(crate::paper_exec::IntentKind::Buy)
            {
                // Gate L-a: J names the head block the BUY was formed on.
                let block = u64::from_le_bytes(rec.payload[62..70].try_into().unwrap());
                assert!(block > HEAD, "{block}");
                intents += 1;
            }
        }
        assert_eq!((intents, acted), (1, 1));
        exec.unwrap().shutdown();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// BSC3-052: `~/` is the account home, also with `$HOME` unset; a
    /// `$HOME` naming another directory refuses it; an absolute path stays.
    #[test]
    fn home_is_the_account_home_or_refused() {
        let account = || Ok(PathBuf::from("/Users/bsc"));
        let at = |h: &str| Some(std::ffi::OsString::from(h));
        assert_eq!(
            expand_home_in("~/k/buyer.key", account(), at("/Users/bsc")),
            Ok(PathBuf::from("/Users/bsc/k/buyer.key"))
        );
        assert_eq!(
            expand_home_in("~/k/buyer.key", account(), None),
            Ok(PathBuf::from("/Users/bsc/k/buyer.key"))
        );
        let e = expand_home_in("~/k/buyer.key", account(), at("/tmp/other")).unwrap_err();
        assert!(e.contains("not the account home"), "{e}");
        assert_eq!(
            expand_home_in("/tmp/run/buyer.key", account(), at("/tmp/other")),
            Ok(PathBuf::from("/tmp/run/buyer.key"))
        );
        assert!(expand_home_in("~/k", Err("no account".into()), None).is_err());
    }
}
