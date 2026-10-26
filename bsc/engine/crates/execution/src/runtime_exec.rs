//! The C3 PAPER path in the runtime (component `BSC_EXEC`, effective
//! `SHADOW`). Its lifecycle, outside the entry path:
//!
//! 1. the PAPER identity is loaded at start (its address is watched by C2,
//!    so the follower reads its nonce facts);
//! 2. when the component is SHADOW and the C2 owner runs, the live Flap
//!    deployment and venue terms are probed read-only on a helper thread,
//!    pinned to one block (never assumed): a drift, other terms or a funded
//!    PAPER sender stop here, visibly;
//! 3. once C2 knows the PAPER sender's next nonce exactly, the executor arms
//!    its fenced lanes;
//! 4. every C1-scope launch then runs the production path to the PAPER
//!    sink. No automatic re-probe or re-arm: switching the component off
//!    and on again starts over.

use crate::chain_follower::FollowerLimits;
use crate::paper::PaperIdentity;
use crate::paper_exec::{ExecConfig, PaperBuy, PaperExecutor};
use crate::runtime::state::StateOwner;
use crate::venue_probe::{VenueObservation, probe_flap};
use alloy_primitives::{Address, U256};
use darkbot_bsc_geth_ipc::ingress::decode_tx_target;
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status::exec as e;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    VenueBuyTerms, WITNESSED_MAINNET_TERMS,
};
use darkbot_bsc_trading::venues::flap::deployment::{ValidatedDeployment, bsc_mainnet_v5_27_0};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;

/// The PAPER path's configuration (`BSC_EXEC_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecSetup {
    /// The PAPER identity file (absolute after `~` expansion).
    pub identity_path: PathBuf,
    /// Accounts the PAPER identity must never be.
    pub refused_accounts: Vec<[u8; 20]>,
    pub exec: ExecConfig,
}

impl ExecSetup {
    /// `~/` at the start of `path` is the home directory.
    pub fn expand_home(path: &str) -> Result<PathBuf, String> {
        match path.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .ok_or_else(|| "BSC_EXEC_PAPER_IDENTITY: HOME is not set".to_string()),
            None => Ok(PathBuf::from(path)),
        }
    }
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
    /// Code generations issued by successful probes.
    codehash_generation: u64,
}

impl ExecRuntime {
    /// At process start: loads the PAPER identity (outside the repository,
    /// owner-only, never a refused account) to learn its address.
    pub fn new(setup: ExecSetup, repo_root: &Path, fence_dir: PathBuf) -> Self {
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
        }
    }

    /// The PAPER sender, when its identity loaded.
    pub fn sender(&self) -> Option<[u8; 20]> {
        self.sender
    }

    /// One reconcile step: `wanted` is the component effective SHADOW.
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
            self.validated = None;
            self.executor = None;
            self.phase = Phase::Off;
            return;
        }
        if let Some(error) = &self.identity_error {
            self.phase = Phase::IdentityRefused(error.clone());
            return;
        }
        let Some(sender) = self.sender else {
            return;
        };
        let (Some(owner), Some((endpoint, limits))) = (owner, canonical) else {
            self.phase = Phase::WaitingC2;
            return;
        };
        if self.executor.is_some() {
            self.phase = Phase::Armed;
            return;
        }
        if self.validated.is_none() {
            match self.phase {
                Phase::ProbeFailed(_)
                | Phase::DeploymentDrift(_)
                | Phase::TermsDrift(_)
                | Phase::EconomicsUnqualified(_)
                | Phase::Funded(_) => return,
                _ => {}
            }
            let Some(rx) = &self.probe else {
                let (tx, rx) = channel();
                let endpoint = endpoint.to_string();
                let limits = *limits;
                let spawned =
                    thread::Builder::new()
                        .name("bsc-exec-probe".into())
                        .spawn(move || {
                            let result = probe_flap(
                                &endpoint,
                                &limits,
                                &bsc_mainnet_v5_27_0(),
                                Address::from(sender),
                            )
                            .map_err(|e| e.to_string());
                            let _ = tx.send(result);
                        });
                match spawned {
                    Ok(_) => {
                        self.probe = Some(rx);
                        self.phase = Phase::Probing;
                    }
                    Err(e) => self.phase = Phase::ProbeFailed(format!("spawn: {e}")),
                }
                return;
            };
            match rx.try_recv() {
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.probe = None;
                    self.phase = Phase::ProbeFailed("probe thread ended".into());
                    return;
                }
                Ok(Err(e)) => {
                    self.probe = None;
                    self.phase = Phase::ProbeFailed(e);
                    return;
                }
                Ok(Ok(observation)) => {
                    self.probe = None;
                    self.codehash_generation += 1;
                    let validated = bsc_mainnet_v5_27_0()
                        .validate(&observation.deployment(), self.codehash_generation);
                    let terms = observation.terms;
                    let balance = observation.caller_balance_wei;
                    self.observation = Some(observation);
                    match validated {
                        Err(e) => {
                            self.phase = Phase::DeploymentDrift(e.to_string());
                            return;
                        }
                        Ok(v) if !v.economics_witnessed() => {
                            self.phase = Phase::EconomicsUnqualified(format!(
                                "Portal {} ({}) is decode-only",
                                self.observation
                                    .as_ref()
                                    .map_or("?", |o| o.observed.portal_version.as_str()),
                                v.deployment().portal_implementation
                            ));
                            return;
                        }
                        Ok(_) if terms != WITNESSED_MAINNET_TERMS => {
                            self.phase = Phase::TermsDrift(format!("{terms:?}"));
                            return;
                        }
                        Ok(_) if balance != U256::ZERO => {
                            self.phase = Phase::Funded(format!("{balance} wei"));
                            return;
                        }
                        Ok(v) => self.validated = Some((v, terms)),
                    }
                }
            }
        }
        let Some(next_nonce) = owner.clean_next_nonce(&sender) else {
            self.phase = Phase::WaitingNonce;
            return;
        };
        let Some((deployment, terms)) = self.validated else {
            return;
        };
        if matches!(self.phase, Phase::ArmFailed(_)) {
            return;
        }
        let armed = PaperIdentity::load(
            &self.setup.identity_path,
            &self.repo_root,
            &self.setup.refused_accounts,
        )
        .map_err(|e| e.to_string())
        .and_then(|identity| {
            PaperExecutor::arm(
                identity,
                deployment,
                terms,
                self.setup.exec.clone(),
                next_nonce,
                &self.fence_dir,
                session,
            )
            .map_err(|e| e.to_string())
        });
        match armed {
            Ok(executor) => {
                self.executor = Some(executor);
                self.phase = Phase::Armed;
            }
            Err(e) => self.phase = Phase::ArmFailed(e),
        }
    }

    /// A verified pending C1-scope launch (`raw` signed bytes), observed at
    /// `ingress_ns`: the production path to the sink, when armed.
    pub fn on_launch(
        &mut self,
        hash: [u8; 32],
        raw: &[u8],
        ingress_ns: u64,
        owner: &StateOwner,
        now_ns: impl Fn() -> u64,
    ) -> Option<PaperBuy> {
        let executor = self.executor.as_mut()?;
        let target = decode_tx_target(raw).ok()?;
        executor
            .on_launch(
                hash,
                target.to,
                target.data,
                ingress_ns,
                |c, p, g| owner.snapshot(c, p, g),
                now_ns,
            )
            .ok()
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
            Phase::Armed => match self.executor.as_ref().map(PaperExecutor::lanes) {
                Some((_, 0)) => ("EXHAUSTED", None),
                _ => ("ARMED", None),
            },
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
                .set(e::PROBE_BLOCK, o.block_number);
        }
        let (armed, left) = self.executor.as_ref().map_or((0, 0), PaperExecutor::lanes);
        let counters = self.executor.as_ref().map(|x| x.counters().clone());
        let refused = |stage: &str| counters.as_ref().map_or(0, |c| c.refused(stage));
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
            .set(e::REFUSED_BUILD, refused("BUILD"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::state::StateConfig;
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status::exec as e;
    use darkbot_bsc_trading::chain_state::engine::StateLimits;
    use std::time::{Duration, Instant};

    fn setup(path: PathBuf) -> ExecSetup {
        ExecSetup {
            identity_path: path,
            refused_accounts: Vec::new(),
            exec: ExecConfig {
                input_wei: 1_000_000_000_000_000,
                slippage_bps: 100,
                gas_price_wei: 50_000_000,
                gas_limit: 3_000_000,
                session_cap_wei: 1_150_000_000_000_000,
                lanes: 2,
                max_calldata: 16_384,
                latency_samples: 16,
                policy_generation: 1,
            },
        }
    }

    fn state(x: &ExecRuntime) -> (String, Option<String>) {
        let r = darkbot_bsc_record::decode(&e::SCHEMA, &x.draft().encode().unwrap()).unwrap();
        (
            r.text(e::STATE).unwrap().to_owned(),
            r.opt_text(e::REASON).unwrap().map(str::to_owned),
        )
    }

    fn limits() -> FollowerLimits {
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

    /// The lifecycle outside the entry path: a missing identity is refused
    /// visibly; without C2 it waits; a probe that cannot reach the node
    /// fails visibly and is not retried; switching off resets.
    #[test]
    fn the_paper_path_waits_probes_and_fails_closed() {
        let dir = std::env::temp_dir().join(format!("bsc-exec-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Path::new("/nonexistent-repo");
        let missing = ExecRuntime::new(setup(dir.join("none.key")), repo, dir.join("fences"));
        let mut missing = missing;
        missing.tick(true, None, None, 1);
        assert_eq!(state(&missing).0, "IDENTITY_REFUSED");
        missing.tick(false, None, None, 1);
        assert_eq!(state(&missing), ("OFF".to_owned(), None));

        let key = dir.join("id/paper.key");
        PaperIdentity::create(&key, repo).unwrap();
        let mut x = ExecRuntime::new(setup(key), repo, dir.join("fences"));
        let sender = x.sender().unwrap();
        x.tick(true, None, None, 1);
        assert_eq!(state(&x).0, "WAITING_C2");
        let owner = StateOwner::fresh(
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
        .unwrap();
        // A closed local port: the probe fails, once.
        let lim = limits();
        x.tick(true, Some(&owner), Some(("https://127.0.0.1:1", &lim)), 1);
        assert_eq!(state(&x).0, "PROBING");
        let deadline = Instant::now() + Duration::from_secs(10);
        while state(&x).0 == "PROBING" && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            x.tick(true, Some(&owner), Some(("https://127.0.0.1:1", &lim)), 1);
        }
        let (s, reason) = state(&x);
        assert_eq!(s, "PROBE_FAILED");
        assert!(reason.is_some());
        x.tick(true, Some(&owner), Some(("https://127.0.0.1:1", &lim)), 1);
        assert_eq!(state(&x).0, "PROBE_FAILED", "no automatic retry");
        x.tick(false, Some(&owner), None, 1);
        assert_eq!(state(&x).0, "OFF");
        assert_eq!(
            darkbot_bsc_record::decode(&e::SCHEMA, &x.draft().encode().unwrap())
                .unwrap()
                .text(e::PAPER_SENDER)
                .unwrap(),
            format!("0x{}", alloy_primitives::hex::encode(sender))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
