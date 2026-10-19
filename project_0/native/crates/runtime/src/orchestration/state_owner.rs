use crate::analytics::record::Decision;
use crate::analytics::shadow_ledger::ShadowLedger;
#[cfg(feature = "crate_health")]
use crate::execution::builder::p0::{P0ReceivershipBuilder, ProtocolSizingProvider};
use crate::execution::pending::PendingSubmission;
use crate::execution::pipeline::{pipeline_timing_snapshot, Mode, NetworkBudget, Pipeline};
#[cfg(feature = "crate_health")]
use crate::execution::route::JupiterV2RouteProvider;
use crate::market::winner::{race_outcome, RaceOutcome};
use crate::orchestration::remote_workers::{
    OraclePriceProjectionWork, ProjectionWork, ReconcileWork, RemoteWorkers, WinnerWatch,
};
use crate::orchestration::winner_tracker::{
    is_winner_relevant, WinnerDispatch, WinnerResolution, WinnerTracker, WinnerTrackerConfig,
};
use darkbot_common::channel::{bounded, ChannelError};
use darkbot_common::config;
use darkbot_common::lifecycle::Shutdown;
use darkbot_common::timing::Clock;
use darkbot_project0::economics::{FailedCostMatrix, Path};
use darkbot_project0::execution::Freshness;
#[cfg(feature = "crate_health")]
use darkbot_project0::health::crate_impl::bank_observation_keys;
use darkbot_project0::health::{AccountSource, HealthEngine, HealthError};
#[cfg(feature = "crate_health")]
use darkbot_project0::index::{ReverseIndex, ThresholdIndex};
use darkbot_project0::planner::Allowlist;
use darkbot_project0::protocol;
use darkbot_project0::protocol::{classify as classify_account, AccountType}; // [FIX-47] Alias
use darkbot_project0::safety::{Allow, Limits, Safety};
use darkbot_solana::ingress::ws_rpc::{WsRpcConfig, WsRpcControlSource};
use darkbot_solana::ingress::SlotWatch;
use darkbot_solana::ingress::{
    IngressEvent, IngressHalt, IngressSource, ProgramFilter, SourceKind, Subscription,
};
use darkbot_solana::state::blockhash::BlockhashCache;
use darkbot_solana::state::recovery::{RecoveryDriver, SnapshotData};
use darkbot_solana::state::snapshot;
use darkbot_solana::state::StateCore;
use darkbot_solana::submit::rpc::RpcSubmitter;
use darkbot_solana::tx::{self, Resources, TxLimits};
#[cfg(feature = "crate_health")]
use marginfi_type_crate::{
    constants::{ASSET_TAG_DEFAULT, ASSET_TAG_SOL},
    types::{BalanceSide, Bank, BankOperationalState, MarginfiAccount},
};
#[cfg(feature = "crate_health")]
use rayon::prelude::*;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    account::Account,
    hash::{hash, Hash},
    pubkey::Pubkey,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::str::FromStr;
use std::time::Duration;

/// Owner-thread diagnostics (PERSIST-02): the line is formatted here but journaled and
/// printed only by the evidence writer thread, so the owner never blocks on stderr. A full
/// queue closes admission, which the owner loop checks on every iteration.
macro_rules! owner_log {
    ($owner:expr, $($arg:tt)*) => {{
        let line = format!($($arg)*);
        let _ = $owner.evidence.diagnostic(line);
    }};
}

mod accounts;
mod candidates;
mod census;
use accounts::*;
use candidates::*;

use crate::analytics::evidence::{HealthEvidence, OwnerEvidence, OwnerEvidenceWriter, RecordEvent};
mod detect;
mod dispatch;
mod events;
mod ingress;
mod lifecycle;
mod remote_apply;
mod setup;
use ingress::{IngressSession, IngressSessions, RetiredEvents};
mod shutdown;
mod sweep;
mod telemetry;

mod metrics;
use metrics::OwnerMetrics;

pub(super) struct ShadowOwner {
    metrics: OwnerMetrics,
    mode: String,
    // Read once at setup, never from the environment on the owner thread.
    eval_slippage_bps: u64,
    zero_prefund_enabled: bool,
    /// [C0-02] Opt-in one-shot census at the first READY; taken when it runs.
    c0_census: Option<census::CensusRequest>,
    required_dependency_keys: Vec<Pubkey>,
    mt0_reporter: darkbot_common::mt0::Reporter,
    optional_dependency_keys: Vec<Pubkey>,
    subscribed: HashSet<Pubkey>,
    required_set: HashSet<Pubkey>,
    optional_set: HashSet<Pubkey>,
    prereq_allowed: Vec<Pubkey>,
    min_profit: i128,
    integration_probe_enabled: bool,
    integration_probe_limit_per_kind: usize,
    remote_worker_capacity: usize,
    pending: Vec<ReconcileWork>,
    projection_cache: HashMap<Pubkey, CachedProjection>,
    projection_pending: HashSet<Pubkey>,
    detect_queue: VecDeque<DetectWork>,
    oracle_detect_order: VecDeque<Pubkey>,
    oracle_detect_pending: HashMap<Pubkey, DetectWork>,
    oracle_bank_projection_pending: HashMap<Pubkey, (u64, u64)>,
    integration_probe_expected: HashSet<Pubkey>,
    integration_probe_queue: VecDeque<(Pubkey, u64)>,
    integration_probe_terminal: HashSet<Pubkey>,
    threshold_fallback: HashSet<Pubkey>,
    threshold_bootstrap: VecDeque<Pubkey>,
    threshold_bootstrap_batch: usize,
    use_threshold_trigger: bool,
    sweep_batch_max_events: usize,
    sweep_batch_max_age_ns: u64,
    sweep_dirty_banks: HashMap<Pubkey, (u64, u64)>,
    sweep_batch_events: usize,
    sweep_batch_started_ns: Option<u64>,
    candidate_recheck_slots: u64,
    candidate_error_recheck_slots: u64,
    candidate_repeat_soft_cap: usize,
    candidate_cache: HashMap<Pubkey, CandidateCacheEntry>,
    candidate_force_rearm: HashSet<Pubkey>,
    candidate_input_epoch: u64,
    bank_input_revision: HashMap<Pubkey, u64>,
    ingress_burst_before_detect: usize,
    ingress_since_detect: usize,
    /// B6: bound on the synchronous network work of candidates while ingress is behind.
    network: NetworkBudget,
    mt0_order_rx: VecDeque<u64>,
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
    operational_start: u64,
    operational_window: Option<crate::analytics::measurement::ReadyWindow>,
    operational_failure: Option<&'static str>,
    program: Pubkey,
    group: Pubkey,
    payer: solana_sdk::signature::Keypair,
    liquidator: Pubkey,
    shutdown: Shutdown,
    halt: IngressHalt,
    slot_watch: SlotWatch,
    producer: darkbot_common::channel::Producer<IngressEvent>,
    consumer: darkbot_common::channel::Consumer<IngressEvent>,
    program_data: Pubkey,
    program_data_baseline_hash: Hash,
    /// X1: `executable` of the ProgramData account as read at startup.
    program_data_executable: bool,
    /// X1: the approved network audit of a `p0ctl` start, bound at the first Ready (admission)
    /// and then dropped; `None` on the legacy local Shadow path.
    admission: Option<Box<super::boot_audit::NetworkAudit>>,
    sol_bank: Pubkey,
    ingress_session: IngressSession,
    /// C6/D10: queued events of replaced (joined) session generations.
    retired_events: RetiredEvents,
    st: StateCore,
    drv: RecoveryDriver,
    dep_shared: std::sync::Arc<std::sync::Mutex<Vec<Pubkey>>>,
    opt_shared: std::sync::Arc<std::sync::Mutex<Vec<Pubkey>>>,
    blockhash: BlockhashCache,
    /// Refresh thread of `blockhash`; joined in `finish` after the shutdown request (M3).
    blockhash_thread: Option<std::thread::JoinHandle<()>>,
    safety: Safety,
    allowlist: Allowlist,
    /// E2: pinned instruction table the planner gate classifies built instructions with.
    #[cfg(feature = "crate_health")]
    instruction_table: darkbot_project0::planner::InstructionTable,
    global_allowed: Vec<Pubkey>,
    submitter: crate::execution::observe_only::ObserveOnly<RpcSubmitter>,
    failed_costs: FailedCostMatrix,
    routes: JupiterV2RouteProvider,
    resources: Resources,
    freshness: Freshness,
    remote: RemoteWorkers,
    integration_probe_projection_ready: usize,
    integration_probe_projection_failed: usize,
    integration_probe_projection_stale: usize,
    integration_probe_completion_reported: bool,
    ledger: ShadowLedger,
    winner_tracker: WinnerTracker,
    evidence: OwnerEvidenceWriter,
    counts: [u64; 6],
    last_reported_update_total: u64,
    bootstrap_probe_done: bool,
    program_upgrade_latched: bool,
    reverse_index: ReverseIndex,
    threshold_index: ThresholdIndex,
    threshold_ready: bool,
    mt0_owner: darkbot_common::mt0::OwnerTimer,
}

/// C9: the one blockhash freshness bound of the Shadow owner (detection gates and the pipeline's
/// build limit); a change needs evidence, never a silent widening at one gate.
const BLOCKHASH_MAX_AGE_NS: u64 = 30_000_000_000;

enum OwnerStep<T> {
    Next(T),
    Continue,
}

/// Terminal end of the Shadow owner with a fixed, secret-free code (C1/C7): the run stops in
/// order and is INCOMPLETE; never a passive HALT until the window ends.
#[derive(Debug)]
pub(crate) struct OwnerTerminal(pub &'static str);
impl std::fmt::Display for OwnerTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for OwnerTerminal {}

pub(super) fn run_shadow(
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
    admission: Option<super::boot_audit::NetworkAudit>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut owner, mut fetch, ingress) = setup::prepare(clock, econ, admission)?;
    let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
        loop {
            owner.evidence.check()?;
            if !owner.lifecycle() {
                break;
            }
            let worker_now_s = owner.remote_apply()?;
            if let Some(h) = &owner.econ {
                if h.admission_closed() {
                    owner.ingress_session.request();
                    if owner.winner_tracker.stats().len == 0 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
            }
            owner.dispatch(worker_now_s)?;
            owner.sweep();
            let detect = match owner.events(&mut fetch, &ingress)? {
                OwnerStep::Next(detect) => detect,
                OwnerStep::Continue => continue,
            };
            if !owner.detect(detect)? {
                continue;
            }
            owner.telemetry();
        }
        Ok(())
    })();
    if outcome.is_err() {
        owner.shutdown.request();
        owner.ingress_session.request();
    }
    // Every exit, including errors, joins and drains the evidence writer.
    let finish = owner.finish();
    finish.and(outcome)
}

#[cfg(test)]
use super::startup::shadow_failure_code;
#[cfg(test)]
#[path = "state_owner/tests.rs"]
mod integration_probe_tests;
#[cfg(all(test, feature = "crate_health"))]
#[path = "state_owner/owner_e2e_tests.rs"]
mod owner_e2e_tests;
