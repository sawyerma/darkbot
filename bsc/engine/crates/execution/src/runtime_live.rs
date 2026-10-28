//! The LIVE path in the runtime (plan 28.10., package 5): the one executor
//! armed towards its LIVE end, under the owner's effective LIVE. Its
//! lifecycle, outside the entry path, in every reconcile pass:
//!
//! 1. wanted only while the owner's LIVE gate is clear now for the runtime,
//!    the grant's venue and its components (`Store::live_now`) and the LIVE
//!    qualification on file verifies; otherwise the path is disarmed and
//!    its key dropped, with the reason;
//! 2. the buyer must be a watched C2 sender; the live Flap deployment and
//!    terms are probed read-only with the buyer as caller, pinned to one
//!    block: a drift or other terms stop here; the buyer must hold one BUY
//!    (the configured input and its gas at the signed price);
//! 3. C2's clean next nonce of the buyer is the lanes' first nonce;
//! 4. the key is loaded under the `LiveNow` (a private file outside the
//!    repository, the buyer's address), the LIVE end built over its route,
//!    its connections established, the executor armed behind its own fence;
//! 5. while armed, every pass checks the gate again (a revoke, the expiry,
//!    a switch off or a restart disarms, visibly), keeps the connections
//!    and probes the deployment again every `BSC_EXEC_REPROBE_MS`.
//!
//! Launches reach it only as verified full pending transactions (the
//! runtime never hands it a hash-only sighting). Nothing is re-armed
//! automatically after a refusal: switching the scope off and on again
//! starts over, with a new fence.

use crate::chain_follower::FollowerLimits;
use crate::live_end::{LiveEnd, LiveHandoff, LiveRoute};
use crate::live_key::LiveKey;
use crate::live_qualification;
use crate::paper_exec::{Arming, ExecBuy, ExecIntent, Executor, NonceEvidence, Refusal};
use crate::runtime::exec::{ExecSetup, Prober, launch_to, live_probe, strategy_to};
use crate::runtime::state::{LaneEvidence, StateOwner};
use crate::sources::{ComponentId, KEEP_GENERATIONS, LiveStrategies, SourceId, Store};
use crate::submission::tls::{AuthorizationToken, TlsSetup};
use crate::submission::transport::Clock;
use crate::venue_probe::VenueObservation;
use alloy_primitives::{Address, U256};
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status::live as l;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    VenueBuyTerms, WITNESSED_MAINNET_TERMS,
};
use darkbot_bsc_trading::intent::IntentState;
use darkbot_bsc_trading::venues::flap::deployment::{ValidatedDeployment, bsc_mainnet_v5_27_0};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

/// Where the LIVE path's requests go (`BSC_LIVE_ROUTE`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveRouteConfig {
    /// The granted builders over TLS; the BlockRazor authorization from a
    /// private file, if BlockRazor is granted.
    Builders { blockrazor_token: Option<PathBuf> },
    /// The builder stand-in on 127.0.0.1:`port`: nothing leaves the host.
    Loopback { port: u16 },
}

impl LiveRouteConfig {
    /// `BUILDERS` or `LOOPBACK:<port>`.
    pub fn parse(text: &str, blockrazor_token: Option<PathBuf>) -> Result<Self, String> {
        if text == "BUILDERS" {
            return Ok(Self::Builders { blockrazor_token });
        }
        if blockrazor_token.is_some() {
            return Err("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE needs BSC_LIVE_ROUTE=BUILDERS".into());
        }
        text.strip_prefix("LOOPBACK:")
            .and_then(|p| p.parse::<u16>().ok())
            .filter(|p| *p > 0)
            .map(|port| Self::Loopback { port })
            .ok_or_else(|| format!("BSC_LIVE_ROUTE {text:?}: BUILDERS or LOOPBACK:<port>"))
    }
}

/// The LIVE path's configuration (`BSC_LIVE_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveSetup {
    /// The buyer's key file (absolute after `~` expansion).
    pub key_path: PathBuf,
    pub buyer: [u8; 20],
    pub route: LiveRouteConfig,
    pub connect_timeout: Duration,
}

/// Where the LIVE path stands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Off,
    /// The owner's gate or the qualification refuses LIVE now.
    Blocked(String),
    NotWatched,
    WaitingC2,
    Probing,
    ProbeFailed(String),
    DeploymentDrift(String),
    TermsDrift(String),
    EconomicsUnqualified(String),
    Unfunded(String),
    WaitingNonce,
    KeyRefused(String),
    ConnectFailed(String),
    ArmFailed(String),
    Armed,
    /// Armed until the gate closed: the reason it ended.
    Ended(String),
}

impl Phase {
    /// A state only a closed and reopened gate leaves.
    fn terminal(&self) -> bool {
        matches!(
            self,
            Self::ProbeFailed(_)
                | Self::DeploymentDrift(_)
                | Self::TermsDrift(_)
                | Self::EconomicsUnqualified(_)
                | Self::KeyRefused(_)
                | Self::ArmFailed(_)
                | Self::Ended(_)
        )
    }

    fn name(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::Off => ("OFF", None),
            Self::Blocked(r) => ("BLOCKED", Some(r)),
            Self::NotWatched => ("NOT_WATCHED", None),
            Self::WaitingC2 => ("WAITING_C2", None),
            Self::Probing => ("PROBING", None),
            Self::ProbeFailed(r) => ("PROBE_FAILED", Some(r)),
            Self::DeploymentDrift(r) => ("DEPLOYMENT_DRIFT", Some(r)),
            Self::TermsDrift(r) => ("TERMS_DRIFT", Some(r)),
            Self::EconomicsUnqualified(r) => ("ECONOMICS_UNQUALIFIED", Some(r)),
            Self::Unfunded(r) => ("UNFUNDED", Some(r)),
            Self::WaitingNonce => ("WAITING_NONCE", None),
            Self::KeyRefused(r) => ("KEY_REFUSED", Some(r)),
            Self::ConnectFailed(r) => ("CONNECT_FAILED", Some(r)),
            Self::ArmFailed(r) => ("ARM_FAILED", Some(r)),
            Self::Armed => ("ARMED", None),
            Self::Ended(r) => ("ENDED", Some(r)),
        }
    }
}

/// The components a grant of `strategies` arms.
fn components(strategies: LiveStrategies) -> Vec<ComponentId> {
    let mut out = Vec::with_capacity(2);
    if strategies.c3() {
        out.push(ComponentId::ExecC3);
    }
    if strategies.strategy_c() {
        out.push(ComponentId::StrategyC);
    }
    out
}

/// The BlockRazor authorization from an owner-only file.
fn token_from(path: &Path) -> Result<AuthorizationToken, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
        return Err(format!("{} is not a private regular file", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    AuthorizationToken::new(text.trim().to_string()).map_err(|e| format!("{e:?}"))
}

fn wall_ms() -> u64 {
    // Unreadable: every grant counts as expired.
    darkbot_bsc_record::observed_now().unwrap_or(u64::MAX)
}

/// The runtime's LIVE path controller.
pub struct LiveRuntime {
    setup: LiveSetup,
    exec: ExecSetup,
    repo_root: PathBuf,
    state_dir: PathBuf,
    fence_dir: PathBuf,
    /// The PAPER identity's address: never the LIVE key.
    paper_sender: Option<[u8; 20]>,
    phase: Phase,
    probe: Option<Receiver<Result<VenueObservation, String>>>,
    observation: Option<VenueObservation>,
    validated: Option<(ValidatedDeployment, VenueBuyTerms)>,
    executor: Option<Executor<LiveEnd>>,
    codehash_generation: u64,
    arms: u64,
    last_probe: Option<Instant>,
    /// When an arming last failed in a way a later pass may get past.
    last_attempt: Option<Instant>,
    probes: u64,
    disarm_errors: u64,
    next_nonce: Option<u64>,
    /// Launches first seen by a source the grant does not name.
    source_refused: u64,
    prober: Prober,
    /// The owner's boot session (tests: a fixed one) and wall clock.
    boot: Option<String>,
    wall: fn() -> u64,
}

impl LiveRuntime {
    pub fn new(
        setup: LiveSetup,
        exec: ExecSetup,
        repo_root: &Path,
        state_dir: PathBuf,
        fence_dir: PathBuf,
        paper_sender: Option<[u8; 20]>,
    ) -> Self {
        Self::with_prober(
            setup,
            exec,
            repo_root,
            state_dir,
            fence_dir,
            paper_sender,
            live_probe,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_prober(
        setup: LiveSetup,
        exec: ExecSetup,
        repo_root: &Path,
        state_dir: PathBuf,
        fence_dir: PathBuf,
        paper_sender: Option<[u8; 20]>,
        prober: Prober,
    ) -> Self {
        LiveRuntime {
            setup,
            exec,
            repo_root: repo_root.to_path_buf(),
            state_dir,
            fence_dir,
            paper_sender,
            phase: Phase::Off,
            probe: None,
            observation: None,
            validated: None,
            executor: None,
            codehash_generation: 0,
            arms: 0,
            last_probe: None,
            last_attempt: None,
            probes: 0,
            disarm_errors: 0,
            next_nonce: None,
            source_refused: 0,
            prober,
            boot: None,
            wall: wall_ms,
        }
    }

    /// The same path in a fixed boot session and wall clock (tests).
    #[cfg(test)]
    pub(crate) fn in_session(mut self, boot: &str, wall: fn() -> u64) -> Self {
        self.boot = Some(boot.into());
        self.wall = wall;
        self
    }

    pub fn buyer(&self) -> [u8; 20] {
        self.setup.buyer
    }

    /// Whether the LIVE path is armed (the status MODE is `LIVE`).
    pub fn armed(&self) -> bool {
        self.executor.is_some()
    }

    /// A clean runtime stop: the armed path ends, its key goes.
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

    fn stop(&mut self, phase: Phase) {
        self.disarm();
        self.validated = None;
        self.phase = phase;
    }

    fn spawn_probe(&mut self, endpoint: &str, limits: &FollowerLimits) {
        let (tx, rx) = channel();
        let endpoint = endpoint.to_string();
        let limits = *limits;
        let prober = self.prober;
        let caller = Address::from(self.setup.buyer);
        let spawned = thread::Builder::new()
            .name("bsc-live-probe".into())
            .spawn(move || {
                let _ = tx.send(prober(&endpoint, &limits, &bsc_mainnet_v5_27_0(), caller));
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

    /// One BUY's most: the configured input and its gas at the signed price.
    fn one_buy(&self) -> U256 {
        let x = &self.exec.exec;
        U256::from(x.input_wei) + U256::from(x.gas_price_wei) * U256::from(x.gas_limit)
    }

    fn observed(&mut self, result: Result<VenueObservation, String>) {
        let observation = match result {
            Ok(o) => o,
            Err(e) => return self.stop(Phase::ProbeFailed(e)),
        };
        let generation = self.codehash_generation + 1;
        let validated = bsc_mainnet_v5_27_0().validate(&observation.deployment(), generation);
        let terms = observation.terms;
        let balance = observation.caller_balance_wei;
        let o = &observation.observed;
        let seen = format!(
            "observed {} implementation {}",
            o.portal_version, o.portal_implementation
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
            Ok(_) if balance < self.one_buy() => {
                self.disarm();
                self.validated = None;
                self.phase =
                    Phase::Unfunded(format!("{balance} wei, one BUY needs {}", self.one_buy()));
            }
            Ok(v) => {
                if self.executor.is_none() {
                    self.codehash_generation = generation;
                    self.validated = Some((v, terms));
                }
            }
        }
    }

    fn retry_due(&self) -> bool {
        self.last_attempt
            .is_none_or(|at| at.elapsed() >= self.exec.reprobe)
    }

    /// One reconcile step at `now_ms` (unix): `session` names this
    /// process's fences.
    pub fn tick(
        &mut self,
        owner: Option<&StateOwner>,
        canonical: Option<(&str, &FollowerLimits)>,
        session: u64,
        now_ms: u64,
    ) {
        // The gate, as it holds now.
        let opened = match &self.boot {
            Some(boot) => {
                Store::open_in_session(&self.state_dir, KEEP_GENERATIONS, Ok(boot.clone()))
            }
            None => Store::open(&self.state_dir, KEEP_GENERATIONS),
        };
        let store = match opened {
            Ok(s) => s,
            Err(e) => return self.close(format!("owner state: {e}")),
        };
        let (verified, invalid) = live_qualification::view(&self.repo_root);
        if let Some(why) = invalid {
            return self.close(format!("LIVE_QUALIFICATION_INVALID: {why}"));
        }
        let strategies = match store.current() {
            Ok(g) => g.live.map(|grant| grant.strategies),
            Err(e) => return self.close(format!("owner state: {e}")),
        };
        let Some(strategies) = strategies else {
            return self.close(crate::sources::LIVE_GRANT_MISSING.into());
        };
        let live_now = match store.live_now(&components(strategies), &verified.view, now_ms) {
            Ok(n) => n,
            Err(why) => return self.close(why),
        };
        if let Some(x) = self.executor.as_mut() {
            // Armed under another grant: this one ends, a new arm follows.
            if x.end().armed() != &live_now {
                return self.close(crate::sources::LIVE_GRANT_CHANGED.into());
            }
            let _ = x.end_mut().maintain(self.setup.connect_timeout);
            self.phase = Phase::Armed;
            self.reprobe(canonical);
            return;
        }
        if self.phase.terminal() {
            return;
        }
        let (Some(owner), Some((endpoint, limits))) = (owner, canonical) else {
            self.phase = Phase::WaitingC2;
            return;
        };
        if !owner.watches(&self.setup.buyer) {
            self.phase = Phase::NotWatched;
            return;
        }
        if let Some(rx) = &self.probe {
            match rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    self.phase = Phase::Probing;
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
        let Some((deployment, terms)) = self.validated else {
            if matches!(self.phase, Phase::Unfunded(_))
                && self
                    .last_probe
                    .is_some_and(|at| at.elapsed() < self.exec.reprobe)
            {
                return;
            }
            self.spawn_probe(endpoint, limits);
            self.phase = Phase::Probing;
            return;
        };
        let Some(next_nonce) = owner.clean_next_nonce(&self.setup.buyer) else {
            self.phase = Phase::WaitingNonce;
            return;
        };
        if matches!(self.phase, Phase::ConnectFailed(_)) && !self.retry_due() {
            return;
        }
        let refused: Vec<[u8; 20]> = self.paper_sender.into_iter().collect();
        let key = match LiveKey::load(
            &self.setup.key_path,
            &self.repo_root,
            self.setup.buyer,
            &refused,
            &live_now,
        ) {
            Ok(k) => k,
            Err(e) => return self.stop(Phase::KeyRefused(e)),
        };
        let route = match &self.setup.route {
            LiveRouteConfig::Loopback { port } => LiveRoute::Loopback { port: *port },
            LiveRouteConfig::Builders { blockrazor_token } => {
                let setup = match TlsSetup::web_pki_roots() {
                    Ok(s) => s,
                    Err(e) => return self.stop(Phase::ArmFailed(format!("TLS setup: {e:?}"))),
                };
                let blockrazor = match blockrazor_token.as_deref().map(token_from).transpose() {
                    Ok(t) => t,
                    Err(e) => return self.stop(Phase::ArmFailed(e)),
                };
                LiveRoute::Builders { setup, blockrazor }
            }
        };
        let end = LiveEnd::new(
            key,
            live_now,
            store,
            verified,
            route,
            deployment.deployment().chain_id,
            self.exec.exec.capability_generation,
            Box::new(self.wall),
        );
        let mut end = match end {
            Ok(e) => e,
            Err(e) => return self.stop(Phase::ArmFailed(e)),
        };
        let failed = end.connect(self.setup.connect_timeout);
        if !failed.is_empty() {
            // The end and its key go; a later pass tries again.
            drop(end);
            self.last_attempt = Some(Instant::now());
            self.phase = Phase::ConnectFailed(
                failed
                    .iter()
                    .map(|(a, e)| format!("{}: {e}", a.name()))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
            return;
        }
        let armed = Executor::arm_end(
            end,
            deployment,
            terms,
            self.exec.exec.clone(),
            Arming {
                next_nonce,
                fence_dir: &self.fence_dir,
                session,
                arm: self.arms,
            },
        );
        self.arms += 1;
        match armed {
            Ok(executor) => {
                self.next_nonce = Some(next_nonce);
                self.executor = Some(executor);
                self.phase = Phase::Armed;
            }
            Err(e) => self.stop(Phase::ArmFailed(e.to_string())),
        }
    }

    /// The gate is closed: an armed path ends with the reason, an unarmed
    /// one shows it.
    fn close(&mut self, why: String) {
        self.probe = None;
        if self.executor.is_some() {
            self.stop(Phase::Ended(why));
        } else {
            self.validated = None;
            self.phase = Phase::Blocked(why);
        }
    }

    fn reprobe(&mut self, canonical: Option<(&str, &FollowerLimits)>) {
        if let Some(rx) = &self.probe {
            match rx.try_recv() {
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.probe = None;
                    return self.stop(Phase::ProbeFailed("probe thread ended".into()));
                }
                Ok(result) => {
                    self.probe = None;
                    return self.observed(result);
                }
            }
        }
        if let Some((endpoint, limits)) = canonical
            && self
                .last_probe
                .is_none_or(|at| at.elapsed() >= self.exec.reprobe)
        {
            self.spawn_probe(endpoint, limits);
        }
    }

    /// A verified full pending C1-scope launch first seen by `source`,
    /// when armed: only a source the grant names triggers a LIVE BUY.
    pub fn on_launch(
        &mut self,
        source: SourceId,
        hash: [u8; 32],
        raw: &[u8],
        ingress_ns: u64,
        owner: &StateOwner,
        clock: &dyn Clock,
    ) -> Option<ExecBuy<LiveHandoff>> {
        let x = self.executor.as_mut()?;
        if !x.end().armed().grant().sources.contains(&source) {
            self.source_refused += 1;
            return None;
        }
        launch_to(x, hash, raw, ingress_ns, owner, clock)
    }

    /// One Strategy C decision, when armed.
    pub fn on_strategy(
        &mut self,
        action: &crate::runtime_strategy::StrategyAction,
        owner: &StateOwner,
        clock: &dyn Clock,
        now_unix_ms: u64,
    ) -> Option<(Vec<ExecIntent<LiveHandoff>>, Option<Refusal>)> {
        Some(strategy_to(
            self.executor.as_mut()?,
            action,
            owner,
            clock,
            now_unix_ms,
        ))
    }

    /// C2's evidence about the buyer's nonces, applied to the armed lanes.
    pub fn apply_evidence(&mut self, evidence: &[LaneEvidence]) -> Vec<Option<IntentState>> {
        let Some(x) = self.executor.as_mut() else {
            return Vec::new();
        };
        evidence
            .iter()
            .map(|e| {
                x.apply_evidence(match e {
                    LaneEvidence::Inclusion(p) => NonceEvidence::Inclusion(p),
                    LaneEvidence::Finality(p) => NonceEvidence::Finality(p),
                    LaneEvidence::Orphan(p) => NonceEvidence::Orphan(p),
                })
            })
            .collect()
    }

    pub fn draft(&self) -> Draft {
        let (state, reason) = self.phase.name();
        let mut d = Draft::new(&l::SCHEMA);
        d.set(l::STATE, state)
            .set(
                l::BUYER,
                format!("0x{}", alloy_primitives::hex::encode(self.setup.buyer)).as_str(),
            )
            .set(
                l::ROUTE,
                match self.setup.route {
                    LiveRouteConfig::Builders { .. } => "BUILDERS",
                    LiveRouteConfig::Loopback { .. } => "LOOPBACK",
                },
            )
            .set(l::PROBES, self.probes);
        if let Some(r) = reason {
            d.set(l::REASON, r);
        }
        if let Some(o) = &self.observation {
            d.set(
                l::BALANCE_WEI,
                darkbot_bsc_record::Value::U256(o.caller_balance_wei.to_be_bytes()),
            );
        }
        let x = self.executor.as_ref();
        if let Some(n) = self.next_nonce.filter(|_| x.is_some()) {
            d.set(l::NEXT_NONCE, n);
        }
        if let Some(x) = x {
            d.set(l::GRANT_GENERATION, x.end().armed().grant_generation());
            if let Some(r) = x.end().last_refusal() {
                d.set(l::LAST_REFUSAL, r);
            }
        }
        let released = x.map_or(&[][..], |x| x.end().released());
        let ev = x.map(|x| x.evidence()).unwrap_or_default();
        d.set(l::RELEASED, released.len() as u64)
            .set(
                l::UNKNOWN,
                released.iter().filter(|r| !r.handed_off).count() as u64,
            )
            .set(l::INCLUDED, ev.included)
            .set(l::REVERTED, ev.reverted)
            .set(l::REPLACED, ev.replaced)
            .set(l::FINAL, ev.finalized)
            .set(l::ORPHANED, ev.orphaned);
        if self.source_refused > 0 {
            d.push(
                l::REFUSALS,
                format!("SOURCE_NOT_GRANTED={}", self.source_refused).as_str(),
            );
        }
        for (r, n) in x.into_iter().flat_map(|x| {
            x.counters()
                .refusals
                .iter()
                .chain(x.strategy_counters().refusals.iter())
        }) {
            d.push(l::REFUSALS, format!("{}={n}", r.name()).as_str());
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain_follower::{CanonicalEvent, CanonicalTx};
    use crate::live_end::tests::{club48_findings, key_file, stand_in};
    use crate::live_qualification::tests::fixture;
    use crate::live_qualification::{LIVE_QUALIFICATION_PATH, Spec, build};
    use crate::paper_exec::FnClock;
    use crate::runtime::exec::tests::{HEAD, launch, limits, observation, owner, setup};
    use crate::sources::tests::{grant_of, live_ready_granting};
    use crate::sources::{
        Entity, LIVE_GRANT_MISSING, LiveAdapter, LiveGrant, Mode, VenueId, buy_usage,
    };
    use darkbot_bsc_record::Record;
    use darkbot_bsc_trading::venues::flap::deployment::FlapDeployment;
    use std::cell::Cell;
    use std::fs;

    const NOW: u64 = 1_793_059_200_000;

    fn fixed() -> u64 {
        NOW
    }

    fn funded(
        _: &str,
        _: &FollowerLimits,
        d: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        let mut o = observation(d.portal_version, d.portal_implementation);
        o.caller_balance_wei = U256::from(10_000_000_000_000_000_000u128);
        Ok(o)
    }

    fn empty(
        _: &str,
        _: &FollowerLimits,
        d: &FlapDeployment,
        _: Address,
    ) -> Result<VenueObservation, String> {
        Ok(observation(d.portal_version, d.portal_implementation))
    }

    fn record(x: &LiveRuntime) -> Record {
        darkbot_bsc_record::decode(&l::SCHEMA, &x.draft().encode().unwrap()).unwrap()
    }

    /// Ticks until the state is not `PROBING` (the probe thread answers).
    fn settle(x: &mut LiveRuntime, owner: &StateOwner) {
        let lim = limits();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            x.tick(Some(owner), Some(("https://probe.invalid", &lim)), 7, NOW);
            if record(x).text(l::STATE).unwrap() != "PROBING" || Instant::now() > deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Plan 28.10.: the LIVE path arms only under the owner's clear gate
    /// with its qualification on file, a funded buyer and C2's clean nonce;
    /// it takes only launches first seen by a granted source; C2's
    /// inclusion of its BUY resolves the lane; a revoked grant ends it and
    /// its key goes.
    #[test]
    fn the_live_path_arms_under_the_gate_and_ends_with_it() {
        let (state, store, now) = live_ready_granting("live-rt", |now| LiveGrant {
            max_input_wei: 10_000_000_000_000_000,
            max_spend_wei: 100_000_000_000_000_000,
            adapters: vec![LiveAdapter::Club48BuilderV1],
            ..grant_of(now + 3_600_000, LiveStrategies::C3)
        });
        assert_eq!(now, NOW);
        let root = fixture("live-rt", "PASS");
        let ev = || vec!["ev/measure.dbrec".to_string()];
        let spec = Spec {
            venues: vec![(VenueId::Flap, ev())],
            sources: vec![(SourceId::PublicNode, ev())],
            adapters: vec![(LiveAdapter::Club48BuilderV1, club48_findings(), ev())],
            acquisition: Some("ev/g08.dbrec".into()),
            risk: Some(("operator".into(), 1, "synthetic scope".into(), ev())),
        };
        fs::create_dir_all(root.join("bsc/config")).unwrap();
        fs::write(
            root.join(LIVE_QUALIFICATION_PATH),
            build(&root, &spec, 1).unwrap(),
        )
        .unwrap();
        let d = std::env::temp_dir().join(format!("bsc-live-rt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let (key, buyer) = key_file(&d);
        let (port, requests) = stand_in();
        let live_setup = LiveSetup {
            key_path: key,
            buyer,
            route: LiveRouteConfig::Loopback { port },
            connect_timeout: Duration::from_secs(2),
        };
        let exec = setup(d.join("unused.key"), Duration::from_secs(3_600));
        let new = |prober: Prober| {
            LiveRuntime::with_prober(
                live_setup.clone(),
                exec.clone(),
                &root,
                state.clone(),
                d.join("fences"),
                None,
                prober,
            )
            .in_session("boot-1", fixed)
        };
        // Not watched by C2, then unfunded: nothing arms.
        let mut x = new(funded);
        let foreign = owner(&d.join("c2-other"), [7; 20]);
        settle(&mut x, &foreign);
        assert_eq!(record(&x).text(l::STATE).unwrap(), "NOT_WATCHED");
        let mut c2 = owner(&d, buyer);
        let mut poor = new(empty);
        settle(&mut poor, &c2);
        assert_eq!(record(&poor).text(l::STATE).unwrap(), "UNFUNDED");
        assert!(!poor.armed());
        // Funded: it waits for C2's clean nonce of the buyer, then arms.
        settle(&mut x, &c2);
        assert_eq!(record(&x).text(l::STATE).unwrap(), "WAITING_NONCE");
        c2.canonical(&CanonicalEvent::Block {
            number: HEAD,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            timestamp_s: 1_793_000_000,
            timestamp_ms: 1_793_000_000_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: Vec::new(),
        });
        c2.canonical(&CanonicalEvent::NonceFact {
            sender: buyer,
            next_nonce: 5,
            block_number: HEAD,
            block_hash: [0xb1; 32],
        });
        let _ = c2.take_evidence();
        settle(&mut x, &c2);
        let r = record(&x);
        assert_eq!(
            r.text(l::STATE).unwrap(),
            "ARMED",
            "{:?}",
            r.opt_text(l::REASON)
        );
        assert_eq!(r.opt_u64(l::NEXT_NONCE).unwrap(), Some(5));
        assert!(x.armed());
        // Only a granted source's verified full sighting triggers.
        let (hash, raw, launcher) = launch();
        c2.pending(hash, launcher, 3, 0, 10);
        let t = Cell::new(10u64);
        let clock = FnClock {
            id: 1,
            now: || {
                t.set(t.get() + 3);
                t.get()
            },
        };
        assert!(
            x.on_launch(SourceId::Drpc, hash, &raw, 10, &c2, &clock)
                .is_none()
        );
        let buy = x
            .on_launch(SourceId::PublicNode, hash, &raw, 10, &c2, &clock)
            .unwrap();
        let request = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            String::from_utf8(request)
                .unwrap()
                .contains("eth_sendBundle")
        );
        assert_eq!(buy_usage(&state, NOW).unwrap().used, 1);
        let r = record(&x);
        assert_eq!(r.u64(l::RELEASED).unwrap(), 1);
        assert!(
            r.texts(l::REFUSALS)
                .unwrap()
                .contains(&"SOURCE_NOT_GRANTED=1")
        );
        // C2 includes the BUY at the buyer's nonce: the lane resolves it.
        c2.canonical(&CanonicalEvent::Block {
            number: HEAD + 1,
            hash: [0xb2; 32],
            parent_hash: [0xb1; 32],
            timestamp_s: 1_793_000_001,
            timestamp_ms: 1_793_000_001_000,
            observed: Instant::now(),
            observed_unix_ms: None,
            txs: vec![CanonicalTx {
                hash: buy.handoff.tx_hash,
                index: 0,
                sender: buyer,
                nonce: 5,
                succeeded: true,
            }],
        });
        let evidence = c2.take_evidence();
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            x.apply_evidence(&evidence),
            vec![Some(IntentState::Included)]
        );
        assert_eq!(record(&x).u64(l::INCLUDED).unwrap(), 1);
        // Revoked: the path ends at the next pass, the key goes.
        let owner_store = Store::open_in_session(&state, 8, Ok("boot-1".into())).unwrap();
        let g = owner_store.current().unwrap().generation;
        assert!(
            owner_store
                .request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
                .unwrap()
                .applied()
        );
        settle(&mut x, &c2);
        let r = record(&x);
        assert_eq!(
            (r.text(l::STATE).unwrap(), r.opt_text(l::REASON).unwrap()),
            ("ENDED", Some(LIVE_GRANT_MISSING))
        );
        assert!(!x.armed());
        drop(store);
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(state).unwrap();
    }
}
