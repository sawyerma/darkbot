//! Latest-wins, read-only status channel of the market capture. Producers copy values they
//! already hold; no I/O, protobuf or socket on the producer path. Blocks that change rarely sit
//! behind `Arc`, so a reader's clone under the lock stays a few pointer copies. Readers confirm
//! delivered states, so the capture can hand its final state over before it returns.
pub use crate::market::observer::{MarketStats, ReceiverRank};
use crate::storage::writer::StorageStatus;
pub use darkbot_solana::provider::{
    CooldownCause, CooldownStatus, ProviderSnapshot, RetryAfterHeader,
};
use darkbot_solana::runtime_config;
pub use darkbot_solana::runtime_config::{EffectiveConfig, Limits, Profile};
pub use darkbot_solana::state::StateCounters;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Wall clock of a status publication (display only, never evidence); `None` when the host
/// clock is unavailable (D08). Samples are then not published at all (never with time 0); a phase
/// change keeps its previous stamp. Only a start refused for CLOCK_UNAVAILABLE carries no stamp
/// (0: `PhaseStatus.changed_unix_ms` has implicit presence on the wire, and the frontend shows it
/// as "clock unavailable" next to that end result).
pub fn unix_ms_now() -> Option<u64> {
    darkbot_common::timing::wall_now()
        .ok()
        .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Run identity and configuration, once at capture start. Endpoints only as scheme://host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunSnapshot {
    pub run_id: String,
    pub program_id: String,
    pub source: &'static str,
    pub commitment: &'static str,
    pub ws_host: String,
    pub rpc_host: Option<String>,
    pub configured_duration_seconds: u64,
    pub ready_timeout_seconds: u64,
    pub drain_timeout_seconds: u64,
    pub heartbeat_seconds: u64,
    pub min_sample_liquidations: u64,
    pub ingress_buffer_events: u64,
    pub run_dir: String,
    pub enrich_max_pending: u64,
    pub enrich_max_attempts: u64,
    pub enrich_retry_ms: u64,
    pub enrich_timeout_ms: u64,
    pub enrich_phase_max_seconds: u64,
    pub source_coverage_verified: bool,
    pub enrichment_available: bool,
    pub recovery_budget_ms: Option<u64>,
    pub recovery_attempts: u32,
    pub idle_timeout_ms: u64,
    pub healthy_reset_ms: u64,
    pub readiness: String,
    pub process_started_unix_s: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CapturePhase {
    #[default]
    WaitingForStream,
    Capture,
    Drain,
    Finalize,
    Ended,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhaseSnapshot {
    pub phase: CapturePhase,
    pub changed_unix_ms: u64,
    pub stream_ready_unix_s: Option<u64>,
    pub stop_reason: &'static str,
    pub drain_complete: Option<bool>,
    pub write_error: Option<String>,
    /// After `Ended`: `CAPTURE_COMPLETE` or exactly the error the capture returned.
    pub end_result: String,
}

/// Ingress health report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransportSnapshot {
    pub connected: bool,
    /// Age of the last application frame when this snapshot was published (`sampled_at`).
    pub last_frame_age_ns: Option<u64>,
    pub last_slot: Option<u64>,
    pub application_frames: u64,
    /// R19: monotonic publication time of `last_frame_age_ns`, so a reader reports the age at
    /// its own send time instead of re-sending a stale relative age as fresh.
    pub sampled_at: Option<std::time::Instant>,
}
impl TransportSnapshot {
    /// R19: the frame age now: the published age plus the time since publication. `None`
    /// stays unknown; a snapshot without publication time keeps its published age.
    pub fn last_frame_age_ns_now(&self) -> Option<u64> {
        let age = self.last_frame_age_ns?;
        let since = self.sampled_at.map_or(0, |t| {
            u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX)
        });
        Some(age.saturating_add(since))
    }
}

/// Last transport state event (recovery, 429, abort); rare, so owned strings are fine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransportStateSnapshot {
    pub state: String,
    pub code: String,
    pub attempt: u32,
    pub last_valid_slot: Option<u64>,
    pub at_unix_ms: u64,
}

/// Counter copy for the event path: plain fields, no allocation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarketSnapshot {
    pub observing: bool,
    pub stream_ready: bool,
    pub stats: MarketStats,
}

/// Completeness and distribution; built at the capture's 2 s display cadence and at the end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssessmentSnapshot {
    pub verdict: &'static str,
    pub coverage_complete: bool,
    pub decode_complete: bool,
    pub enrichment_complete: bool,
    pub incomplete_reasons: Vec<&'static str>,
    pub truncated_unqualified_reasons: Vec<(&'static str, u64)>,
    pub gap_records: u64,
    pub truncated_pending: u64,
    pub deferred_liquidations: u64,
    pub distinct_targets: u64,
    pub distinct_receivers: u64,
    pub gross_edge_total_usd: f64,
    pub gross_edge_p50_usd: Option<f64>,
    pub gross_edge_p90_usd: Option<f64>,
    pub gross_edge_p99_usd: Option<f64>,
    pub top_receivers: Vec<ReceiverRank>,
    pub deployment_error: Option<&'static str>,
    pub assessed_unix_ms: u64,
}

/// The writer's own sample, unchanged, plus the wall clock of its publication.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StorageSample {
    pub at_unix_ms: u64,
    pub status: StorageStatus,
}

/// The process provider budget at one instant (F-01/C07: every RPC client is budgeted), taken
/// at the capture's 2 s display cadence; values as the budget holds them.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSample {
    pub at_unix_ms: u64,
    pub snapshot: ProviderSnapshot,
    pub cooldown: CooldownStatus,
    pub halt_code: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterValue {
    pub field: &'static str,
    pub unit: &'static str,
    pub value: u64,
    pub default: u64,
    pub env: &'static str,
    pub source: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OperationalSnapshot {
    pub parameters: Vec<ParameterValue>,
    pub effective: Option<EffectiveConfig>,
}

/// Every entry of the engine's parameter registry with its value and source, plus the
/// secret-free effective record. Endpoint URLs are never included.
pub fn operational_snapshot_of(
    limits: &Limits,
    effective: Option<&EffectiveConfig>,
) -> OperationalSnapshot {
    let parameters = runtime_config::PARAMETERS
        .iter()
        .filter_map(|p| {
            Some(ParameterValue {
                field: p.field,
                unit: p.unit,
                value: limits.get(p.field)?,
                default: p.default,
                env: p.env,
                source: effective
                    .and_then(|e| e.sources.get(p.field).cloned())
                    .unwrap_or_else(|| "builtin_default".into()),
            })
        })
        .collect();
    OperationalSnapshot {
        parameters,
        effective: effective.cloned(),
    }
}

/// The operational limits of this process (installed config or built-in defaults).
pub fn active_limits() -> &'static Limits {
    &runtime_config::active().limits
}

/// The operational configuration of this process.
pub fn operational_snapshot() -> OperationalSnapshot {
    operational_snapshot_of(
        &runtime_config::active().limits,
        runtime_config::effective(),
    )
}

/// P0-PROD-CTRL-04: the Shadow/econ4h state owner's projection, copied by the owner at most every
/// `owner_status_ms` (`state_owner/status.rs`). Values as the owner holds them; an unknown value
/// is `None`, never 0. Topics project0.liquidator.state, .wallets.state, .finance.summary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OwnerSnapshot {
    pub at_unix_ms: u64,
    pub run_id: String,
    /// shadow | econ4h.
    pub mode: String,
    pub wallet: String,
    pub program_id: String,
    /// `None` while the owner runs; at its end RUN_COMPLETE or the terminal code.
    pub end_result: Option<String>,
    pub engine: OwnerEngine,
    pub trigger: OwnerTrigger,
    pub pipeline: OwnerPipeline,
    pub safety: OwnerSafety,
    pub capabilities: Vec<OwnerCapability>,
    pub wallets: OwnerWallets,
    pub finance: OwnerFinance,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnerEngine {
    /// BOOTSTRAPPING | RECONCILING | READY | HALTED.
    pub run_state: &'static str,
    pub halt_reason: Option<String>,
    pub ingress_generation: u64,
    /// A01: Reconnected events applied to the state of this run.
    pub reconnects: u64,
    pub gap_open: bool,
    pub highest_slot: u64,
    pub ingress_halt_raised: bool,
    pub program_upgrade_latched: bool,
    pub counters: StateCounters,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnerTrigger {
    /// sweep | threshold.
    pub mode: &'static str,
    pub threshold_ready: bool,
    /// Pending candidates of L0, L1, L2.
    pub lanes: [u64; 3],
    pub detect_queue: u64,
    pub indexed_accounts: u64,
    pub suppressed: u64,
    pub softcap_suppressed: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnerPipeline {
    pub evaluations: u64,
    /// The Shadow ledger's current day.
    pub day: crate::analytics::shadow_ledger::DaySummary,
    pub decision_p95_ns: Option<u64>,
    pub fee_quote_p95_ns: Option<u64>,
    pub simulation_p95_ns: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnerSafety {
    pub capital_in_use_usd_micro: u64,
    pub max_total_capital_usd_micro: u64,
    pub max_daily_loss_usd_micro: u64,
    pub max_exposure_per_attempt_usd_micro: u64,
    pub max_concurrent_positions: u32,
    pub max_outstanding_attempts: u32,
    pub max_failed_attempts_per_day: u32,
    pub halt_reason: Option<String>,
    pub day_realized_usd_micro: i128,
    pub day_failed_attempts: u32,
    pub day_attempts: u32,
}

/// One switchable capability (P0-PROD-CTRL-03): requested, effective, why blocked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnerCapability {
    pub name: &'static str,
    pub requested: &'static str,
    pub effective: &'static str,
    pub block_reason: &'static str,
}

/// FE-04 wallet state from the accounting ledger and the reservation book.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnerWallets {
    pub finance_snapshot_id: String,
    pub journal_read: bool,
    pub verdict: String,
    pub unknown_reasons: Vec<String>,
    pub totals: crate::execution::accounting::Totals,
    pub holds: std::collections::BTreeMap<String, u64>,
    pub policy_reserves: std::collections::BTreeMap<String, u64>,
    /// NONE | RESERVED | UNKNOWN | FINALIZED_UNBOOKED | SETTLED.
    pub held_attempt: &'static str,
}

/// The finance summary (`FinanceSnapshot` of the frontend contract); unknown stays `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnerFinance {
    pub as_of_slot: u64,
    pub total_capital_usd_micro: Option<i128>,
    pub available_capital_usd_micro: Option<i128>,
    pub reserved_capital_usd_micro: Option<i128>,
    pub realized_net_pnl_usd_micro: Option<i128>,
    pub withdrawable_profit_usd_micro: Option<i128>,
    pub transferred_to_treasury_usd_micro: Option<i128>,
    /// VERIFIED | PARTIAL | UNKNOWN.
    pub status: &'static str,
}

static INSTALLED: std::sync::OnceLock<StatusTap> = std::sync::OnceLock::new();

/// The process status tap a composition root installed before the runtime starts (the state
/// owner publishes its projection into it); without one nothing is published.
pub fn install(tap: StatusTap) -> Result<(), &'static str> {
    INSTALLED
        .set(tap)
        .map_err(|_| "STATUS_TAP_ALREADY_INSTALLED")
}

pub fn installed() -> Option<&'static StatusTap> {
    INSTALLED.get()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusSnapshot {
    pub seq: u64,
    /// Bumped by every change of the process-level block (`operational`).
    pub system_version: u64,
    /// Bumped by every change of a capture block.
    pub capture_version: u64,
    /// Bumped by every owner publication.
    pub owner_version: u64,
    pub owner: Option<Arc<OwnerSnapshot>>,
    pub operational: Option<Arc<OperationalSnapshot>>,
    pub run: Option<Arc<RunSnapshot>>,
    pub phase: Option<PhaseSnapshot>,
    pub transport: Option<TransportSnapshot>,
    pub transport_state: Option<Arc<TransportStateSnapshot>>,
    pub market: Option<MarketSnapshot>,
    pub assessment: Option<Arc<AssessmentSnapshot>>,
    pub storage: Option<StorageSample>,
    pub provider: Option<Arc<ProviderSample>>,
}

#[derive(Default)]
struct Delivery {
    seq: u64,
    readers: usize,
}

struct Inner {
    latest: Mutex<StatusSnapshot>,
    changed: Condvar,
    delivery: Mutex<Delivery>,
    delivered: Condvar,
}

enum Block {
    System,
    Capture,
    Owner,
}

#[derive(Clone)]
pub struct StatusTap(Arc<Inner>);

impl Default for StatusTap {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusTap {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            latest: Mutex::new(StatusSnapshot::default()),
            changed: Condvar::new(),
            delivery: Mutex::new(Delivery::default()),
            delivered: Condvar::new(),
        }))
    }

    fn publish(&self, system: bool, f: impl FnOnce(&mut StatusSnapshot)) {
        self.publish_block(
            if system {
                Block::System
            } else {
                Block::Capture
            },
            f,
        )
    }

    fn publish_block(&self, block: Block, f: impl FnOnce(&mut StatusSnapshot)) {
        let mut s = lock(&self.0.latest);
        f(&mut s);
        s.seq = s.seq.checked_add(1).expect("status-tap sequence overflow");
        match block {
            Block::System => s.system_version += 1,
            Block::Capture => s.capture_version += 1,
            Block::Owner => s.owner_version += 1,
        }
        drop(s);
        self.0.changed.notify_all();
    }

    /// The state owner's projection (one publication replaces the previous one).
    pub fn publish_owner(&self, owner: OwnerSnapshot) {
        let owner = Arc::new(owner);
        self.publish_block(Block::Owner, |v| v.owner = Some(owner));
    }

    pub fn publish_operational(&self, s: OperationalSnapshot) {
        let s = Arc::new(s);
        self.publish(true, |v| v.operational = Some(s));
    }

    pub fn publish_run(&self, run: RunSnapshot) {
        let run = Arc::new(run);
        self.publish(false, |v| v.run = Some(run));
    }

    /// Updates the phase in place (earlier fields are kept) and stamps the change time.
    pub fn publish_phase(&self, f: impl FnOnce(&mut PhaseSnapshot)) {
        self.publish_phase_at(unix_ms_now(), f)
    }

    fn publish_phase_at(&self, at: Option<u64>, f: impl FnOnce(&mut PhaseSnapshot)) {
        self.publish(false, |v| {
            let p = v.phase.get_or_insert_with(PhaseSnapshot::default);
            f(p);
            if let Some(at) = at {
                p.changed_unix_ms = at;
            }
        });
    }

    pub fn publish_transport(&self, s: TransportSnapshot) {
        let s = TransportSnapshot {
            sampled_at: Some(std::time::Instant::now()),
            ..s
        };
        self.publish(false, |v| v.transport = Some(s));
    }

    pub fn publish_transport_state(
        &self,
        state: String,
        code: String,
        attempt: u32,
        last_valid_slot: Option<u64>,
    ) {
        let Some(at_unix_ms) = unix_ms_now() else {
            return;
        };
        let s = Arc::new(TransportStateSnapshot {
            state,
            code,
            attempt,
            last_valid_slot,
            at_unix_ms,
        });
        self.publish(false, |v| v.transport_state = Some(s));
    }

    pub fn publish_market(&self, s: MarketSnapshot) {
        self.publish(false, |v| v.market = Some(s));
    }

    pub fn publish_assessment(&self, s: AssessmentSnapshot) {
        let s = Arc::new(s);
        self.publish(false, |v| v.assessment = Some(s));
    }

    /// Called at the capture's 2 s display cadence with the sample it already took.
    pub fn publish_storage(&self, status: StorageStatus) {
        let Some(at_unix_ms) = unix_ms_now() else {
            return;
        };
        self.publish(false, |v| {
            v.storage = Some(StorageSample { at_unix_ms, status })
        });
    }

    /// Called at the capture's 2 s display cadence with the process provider budget.
    pub fn publish_provider(&self, budget: &darkbot_solana::provider::Budget) {
        self.publish_provider_at(unix_ms_now(), budget)
    }

    fn publish_provider_at(&self, at: Option<u64>, budget: &darkbot_solana::provider::Budget) {
        let Some(at_unix_ms) = at else {
            return;
        };
        let sample = Arc::new(ProviderSample {
            at_unix_ms,
            snapshot: budget.snapshot(),
            cooldown: budget.cooldown(),
            halt_code: budget.halt_code(),
        });
        self.publish(false, |v| v.provider = Some(sample));
    }

    pub fn reader(&self) -> Reader {
        lock(&self.0.delivery).readers += 1;
        Reader {
            tap: self.clone(),
            last: 0,
        }
    }

    /// Waits for a reader to confirm writing the latest state, at most `timeout`.
    /// Returns false immediately without a reader. This confirms a socket write,
    /// not a gateway/browser acknowledgement or durable delivery.
    pub fn flush(&self, timeout: Duration) -> bool {
        let target = lock(&self.0.latest).seq;
        let deadline = Instant::now() + timeout;
        let mut d = lock(&self.0.delivery);
        loop {
            if d.seq >= target {
                return true;
            }
            if d.readers == 0 {
                return false;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (guard, _) = self
                .0
                .delivered
                .wait_timeout(d, deadline - now)
                .unwrap_or_else(|e| e.into_inner());
            d = guard;
        }
    }
}

pub struct Reader {
    tap: StatusTap,
    last: u64,
}

impl Reader {
    pub fn wait_changed(&mut self, timeout: Duration) -> Option<StatusSnapshot> {
        let inner = &self.tap.0;
        let mut s = lock(&inner.latest);
        if s.seq == self.last && !timeout.is_zero() {
            let (next, _) = inner
                .changed
                .wait_timeout(s, timeout)
                .unwrap_or_else(|e| e.into_inner());
            s = next;
        }
        if s.seq == self.last {
            return None;
        }
        self.last = s.seq;
        Some(s.clone())
    }

    /// Confirms that the state with `seq` reached the consumer.
    pub fn delivered(&self, seq: u64) {
        let mut d = lock(&self.tap.0.delivery);
        d.seq = d.seq.max(seq);
        drop(d);
        self.tap.0.delivered.notify_all();
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        let mut d = lock(&self.tap.0.delivery);
        d.readers = d.readers.saturating_sub(1);
        drop(d);
        self.tap.0.delivered.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_only() {
        let tap = StatusTap::new();
        let mut reader = tap.reader();
        for i in 1..=1000 {
            let mut m = MarketSnapshot::default();
            m.stats.gap_count = i;
            tap.publish_market(m);
        }
        let value = reader.wait_changed(Duration::from_millis(5)).unwrap();
        assert_eq!(value.seq, 1000);
        assert_eq!(value.market.unwrap().stats.gap_count, 1000);
        assert!(reader.wait_changed(Duration::from_millis(5)).is_none());
    }

    #[test]
    fn unreported_blocks_stay_unset() {
        let tap = StatusTap::new();
        let mut reader = tap.reader();
        tap.publish_market(MarketSnapshot::default());
        let s = reader.wait_changed(Duration::from_millis(5)).unwrap();
        assert!(s.storage.is_none() && s.run.is_none() && s.phase.is_none());
        assert!(s.operational.is_none() && s.assessment.is_none());
    }

    #[test]
    fn storage_sample_is_kept_whole_and_stamped() {
        let tap = StatusTap::new();
        let mut reader = tap.reader();
        let status = StorageStatus {
            queue_bytes: 7,
            disk_free_bytes: Some(42),
            ..Default::default()
        };
        tap.publish_storage(status.clone());
        let sample = reader
            .wait_changed(Duration::from_millis(5))
            .unwrap()
            .storage
            .unwrap();
        assert_eq!(sample.status, status);
        assert!(sample.at_unix_ms > 1_600_000_000_000);
    }

    /// Review 10 (D08): without a wall clock no sample is published (never time 0) and a phase
    /// change keeps its previous stamp.
    #[test]
    fn unavailable_clock_never_publishes_time_zero() {
        let tap = StatusTap::new();
        let budget = darkbot_solana::provider::Budget::configured(
            &darkbot_solana::runtime_config::RuntimeConfig::default(),
        );
        tap.publish_provider_at(None, &budget);
        tap.publish_phase_at(Some(7), |p| p.phase = CapturePhase::Capture);
        tap.publish_phase_at(None, |p| p.phase = CapturePhase::Drain);
        let s = tap.reader().wait_changed(Duration::ZERO).unwrap();
        assert!(s.provider.is_none(), "no provider sample without a clock");
        let phase = s.phase.unwrap();
        assert_eq!(
            (phase.phase, phase.changed_unix_ms),
            (CapturePhase::Drain, 7)
        );
    }

    /// F-01b: the provider budget reaches the tap whole (counters, cooldown, halt state).
    #[test]
    fn provider_sample_is_the_budget_as_it_holds_it() {
        let tap = StatusTap::new();
        let mut reader = tap.reader();
        let budget = darkbot_solana::provider::Budget::configured(
            &darkbot_solana::runtime_config::RuntimeConfig::default(),
        );
        tap.publish_provider(&budget);
        let sample = reader
            .wait_changed(Duration::from_millis(5))
            .unwrap()
            .provider
            .unwrap();
        assert_eq!(sample.snapshot, budget.snapshot());
        assert_eq!(sample.cooldown, budget.cooldown());
        assert_eq!(sample.halt_code, None);
        assert!(sample.at_unix_ms > 1_600_000_000_000);
    }

    #[test]
    fn phase_updates_keep_earlier_fields() {
        let tap = StatusTap::new();
        let mut reader = tap.reader();
        tap.publish_phase(|p| {
            p.phase = CapturePhase::Drain;
            p.stop_reason = "TIMER";
        });
        tap.publish_phase(|p| {
            p.phase = CapturePhase::Ended;
            p.end_result = "CAPTURE_COMPLETE".into();
        });
        let p = reader
            .wait_changed(Duration::from_millis(5))
            .unwrap()
            .phase
            .unwrap();
        assert_eq!((p.phase, p.stop_reason), (CapturePhase::Ended, "TIMER"));
        assert_eq!(p.end_result, "CAPTURE_COMPLETE");
        assert!(p.changed_unix_ms > 0);
    }

    #[test]
    fn transport_health_and_state_are_independent() {
        let tap = StatusTap::new();
        let mut reader = tap.reader();
        tap.publish_transport_state("RECOVERING".into(), "WSS_HTTP_429".into(), 2, Some(9));
        tap.publish_transport(TransportSnapshot {
            connected: true,
            ..Default::default()
        });
        let s = reader.wait_changed(Duration::from_millis(5)).unwrap();
        assert!(s.transport.unwrap().connected);
        assert_eq!(s.transport_state.unwrap().code, "WSS_HTTP_429");
    }

    #[test]
    fn system_and_capture_versions_are_separate() {
        let tap = StatusTap::new();
        let mut reader = tap.reader();
        tap.publish_operational(operational_snapshot_of(&Limits::default(), None));
        tap.publish_market(MarketSnapshot::default());
        tap.publish_market(MarketSnapshot::default());
        let s = reader.wait_changed(Duration::from_millis(5)).unwrap();
        assert_eq!((s.system_version, s.capture_version), (1, 2));
    }

    #[test]
    fn operational_snapshot_lists_every_registered_parameter() {
        let o = operational_snapshot_of(&Limits::default(), None);
        assert_eq!(o.parameters.len(), runtime_config::PARAMETERS.len());
        assert!(o
            .parameters
            .iter()
            .any(|p| p.field == "rpc_rps" && p.env == "P0_RPC_RPS"));
        assert!(o.parameters.iter().all(|p| p.source == "builtin_default"));
    }

    #[test]
    fn flush_waits_for_delivery_and_never_without_reader() {
        let tap = StatusTap::new();
        tap.publish_market(MarketSnapshot::default());
        assert!(!tap.flush(Duration::from_millis(1)));
        let mut reader = tap.reader();
        assert!(!tap.flush(Duration::from_millis(20)));
        let s = reader.wait_changed(Duration::from_millis(5)).unwrap();
        reader.delivered(s.seq);
        assert!(tap.flush(Duration::from_millis(1)));
        drop(reader);
        tap.publish_market(MarketSnapshot::default());
        assert!(!tap.flush(Duration::from_millis(1)));
    }
}
