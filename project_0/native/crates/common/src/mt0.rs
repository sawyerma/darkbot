//! [P6-MT MT0] Measurement of the existing production core (H1–H6). Instrumentation only.
//!
//! - Fixed-size, lock-free registry (`MT0`): every metric is an array indexed by a closed enum,
//!   so nothing grows with traffic (bounded by construction).
//! - Recording costs a few relaxed atomic operations; no allocation, no I/O, no locks. Recording
//!   never feeds back into any business decision.
//! - Snapshots leave the process only through the reporter thread (`spawn_reporter`) and its
//!   sink, never on the owner. The sink (the binary journal) lives in the runtime crate.
//! - Metrics that the current architecture cannot measure correctly are reported as
//!   `N/A:DEFER_TO_MT<n>`, never as invented zeros.

use crate::lifecycle::Shutdown;
use std::sync::atomic::{fence, AtomicBool, AtomicU64, Ordering, Ordering::Relaxed};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

// ------------------------------------------------------------------------------------------
// Histogram: log2 buckets with 8 linear sub-buckets (relative bucket width <= 12.5 %)
// ------------------------------------------------------------------------------------------

const SUB_BITS: u32 = 3;
const SUB: u64 = 1 << SUB_BITS;
pub const HIST_BUCKETS: usize = 512;
#[allow(clippy::declare_interior_mutable_const)]
const ZERO: AtomicU64 = AtomicU64::new(0);

fn bucket_index(v: u64) -> usize {
    if v < SUB {
        return v as usize;
    }
    let msb = 63 - v.leading_zeros();
    let shift = msb - SUB_BITS;
    let sub = (v >> shift) - SUB;
    (SUB + u64::from(msb - SUB_BITS) * SUB + sub) as usize
}

/// Inclusive upper bound of a bucket (quantiles are reported conservatively as upper bounds).
fn bucket_upper(i: usize) -> u64 {
    let i = i as u64;
    if i < SUB {
        return i;
    }
    let e = (i - SUB) / SUB + u64::from(SUB_BITS);
    let sub = (i - SUB) % SUB;
    let width = 1u64 << (e - u64::from(SUB_BITS));
    ((SUB + sub) << (e - u64::from(SUB_BITS))).saturating_add(width - 1)
}

pub struct Hist {
    buckets: [AtomicU64; HIST_BUCKETS],
    count: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistSnapshot {
    pub count: u64,
    pub sum: u64,
    pub max: Option<u64>,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
}

impl Hist {
    pub const fn new() -> Self {
        Self {
            buckets: [ZERO; HIST_BUCKETS],
            count: ZERO,
            sum: ZERO,
            max: ZERO,
        }
    }

    pub fn record(&self, v: u64) {
        self.buckets[bucket_index(v)].fetch_add(1, Relaxed);
        self.count.fetch_add(1, Relaxed);
        self.sum.fetch_add(v, Relaxed);
        self.max.fetch_max(v, Relaxed);
    }

    pub fn snapshot(&self) -> HistSnapshot {
        let counts: Vec<u64> = self.buckets.iter().map(|b| b.load(Relaxed)).collect();
        let total: u64 = counts.iter().sum();
        if total == 0 {
            return HistSnapshot::default(); // no samples: quantiles are None, never 0
        }
        let max = self.max.load(Relaxed);
        let q = |p: f64| -> Option<u64> {
            let rank = ((p * total as f64).ceil() as u64).clamp(1, total);
            let mut seen = 0u64;
            for (i, c) in counts.iter().enumerate() {
                seen += c;
                if seen >= rank {
                    return Some(bucket_upper(i).min(max));
                }
            }
            Some(max)
        };
        HistSnapshot {
            count: total,
            sum: self.sum.load(Relaxed),
            max: Some(max),
            p50: q(0.50),
            p95: q(0.95),
            p99: q(0.99),
        }
    }
}

impl Default for Hist {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------------------------------
// Closed metric dimensions
// ------------------------------------------------------------------------------------------

/// Owner-loop sections. `Wait` is idle time inside the ingress pop and is not busy time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerCat {
    RemoteResults,
    Sweep,
    Wait,
    IngressApply,
    Detect,
    Stats,
}
pub const OWNER_CATS: [OwnerCat; 6] = [
    OwnerCat::RemoteResults,
    OwnerCat::Sweep,
    OwnerCat::Wait,
    OwnerCat::IngressApply,
    OwnerCat::Detect,
    OwnerCat::Stats,
];

impl OwnerCat {
    fn idx(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            OwnerCat::RemoteResults => "remote_results",
            OwnerCat::Sweep => "sweep",
            OwnerCat::Wait => "wait",
            OwnerCat::IngressApply => "ingress_apply",
            OwnerCat::Detect => "detect",
            OwnerCat::Stats => "stats",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteClass {
    Jupiter,
    FeeRpc,
    Simulation,
    IntegrationProjection,
    OracleProjection,
    Winner,
    Reconcile,
    Blockhash,
}
pub const REMOTE_CLASSES: [RemoteClass; 8] = [
    RemoteClass::Jupiter,
    RemoteClass::FeeRpc,
    RemoteClass::Simulation,
    RemoteClass::IntegrationProjection,
    RemoteClass::OracleProjection,
    RemoteClass::Winner,
    RemoteClass::Reconcile,
    RemoteClass::Blockhash,
];

impl RemoteClass {
    fn idx(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            RemoteClass::Jupiter => "jupiter",
            RemoteClass::FeeRpc => "fee_rpc",
            RemoteClass::Simulation => "simulation",
            RemoteClass::IntegrationProjection => "integration_projection",
            RemoteClass::OracleProjection => "oracle_projection",
            RemoteClass::Winner => "winner",
            RemoteClass::Reconcile => "reconcile",
            RemoteClass::Blockhash => "blockhash",
        }
    }
    pub fn provider(self) -> &'static str {
        match self {
            RemoteClass::Jupiter => "jupiter",
            _ => "solana_rpc",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteOutcome {
    Ok,
    RateLimited,
    Timeout,
    Error,
    /// The call site cannot see the error kind; counted separately, never as success.
    Unclassified,
}
const REMOTE_OUTCOMES: usize = 5;

impl RemoteOutcome {
    fn idx(self) -> usize {
        self as usize
    }

    pub fn from_http_status(status: u16) -> Self {
        match status {
            200..=299 => RemoteOutcome::Ok,
            429 => RemoteOutcome::RateLimited,
            408 | 504 => RemoteOutcome::Timeout,
            _ => RemoteOutcome::Error,
        }
    }

    /// Error-text classification for clients that only expose a message (Solana RPC client).
    pub fn classify_text(text: &str) -> Self {
        let t = text.to_ascii_lowercase();
        if t.contains("429") || t.contains("too many requests") || t.contains("rate limit") {
            RemoteOutcome::RateLimited
        } else if t.contains("timed out")
            || t.contains("timeout")
            || t.contains("deadline has elapsed")
        {
            RemoteOutcome::Timeout
        } else {
            RemoteOutcome::Error
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerClass {
    AccountUpdate,
    OracleDependency,
    BankUpdate,
    ForcedRearm,
    IntegrationProjection,
    IntegrationProbe,
    BootstrapProbe,
    Other,
}
pub const TRIGGER_CLASSES: [TriggerClass; 8] = [
    TriggerClass::AccountUpdate,
    TriggerClass::OracleDependency,
    TriggerClass::BankUpdate,
    TriggerClass::ForcedRearm,
    TriggerClass::IntegrationProjection,
    TriggerClass::IntegrationProbe,
    TriggerClass::BootstrapProbe,
    TriggerClass::Other,
];

impl TriggerClass {
    fn idx(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            TriggerClass::AccountUpdate => "account_update",
            TriggerClass::OracleDependency => "oracle_dependency",
            TriggerClass::BankUpdate => "bank_update",
            TriggerClass::ForcedRearm => "forced_rearm",
            TriggerClass::IntegrationProjection => "integration_projection",
            TriggerClass::IntegrationProbe => "integration_probe",
            TriggerClass::BootstrapProbe => "bootstrap_probe",
            TriggerClass::Other => "other",
        }
    }
    /// Map the runner's trigger labels onto the closed set (unknown labels -> Other).
    pub fn from_trigger(trigger: &str) -> Self {
        match trigger {
            "account_update" => TriggerClass::AccountUpdate,
            "integration_projection" => TriggerClass::IntegrationProjection,
            "integration_target_probe" => TriggerClass::IntegrationProbe,
            "bootstrap_snapshot_probe" => TriggerClass::BootstrapProbe,
            t if t.contains("rearm") || t.contains("forced") => TriggerClass::ForcedRearm,
            t if t.contains("oracle") || t.contains("dependency") => TriggerClass::OracleDependency,
            t if t.contains("bank") => TriggerClass::BankUpdate,
            _ => TriggerClass::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthSite {
    Sweep,
    DetectConsumer,
    Pipeline,
}
const HEALTH_SITES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    PipelineEvaluations,
    RoutedEvaluations,
    JupiterRequestsRouted,
    RouteFitCalls,
    RouteFitAttempts,
    FeeRpcForKnownNonActionable,
    ProbeCandidatesWithFeeUnknown,
    FeeParityMatch,
    FeeParityMismatch,
    FeeParityUnsupported,
}
const COUNTERS: usize = 10;
const COUNTER_LIST: [(Counter, &str); COUNTERS] = [
    (Counter::PipelineEvaluations, "pipeline_evaluations"),
    (Counter::RoutedEvaluations, "routed_evaluations"),
    (Counter::JupiterRequestsRouted, "jupiter_requests_routed"),
    (Counter::RouteFitCalls, "route_fit_calls"),
    (Counter::RouteFitAttempts, "route_fit_attempts"),
    (
        Counter::FeeRpcForKnownNonActionable,
        "avoidable_fee_rpc_for_known_non_actionable",
    ),
    (
        Counter::ProbeCandidatesWithFeeUnknown,
        "probe_candidates_with_fee_unknown",
    ),
    (Counter::FeeParityMatch, "fee_parity_match"),
    (Counter::FeeParityMismatch, "fee_parity_mismatch"),
    (Counter::FeeParityUnsupported, "fee_parity_unsupported"),
];

/// Latency at or above this is consistent with at least one hidden 500 ms 429-retry sleep of
/// the Solana RPC client (its internal retries are invisible until MT1).
pub const SUSPECTED_RETRY_LATENCY_NS: u64 = 500_000_000;

// ------------------------------------------------------------------------------------------
// Registry
// ------------------------------------------------------------------------------------------

#[allow(clippy::declare_interior_mutable_const)]
const HIST: Hist = Hist::new();
#[allow(clippy::declare_interior_mutable_const)]
const OUTCOME_ROW: [AtomicU64; REMOTE_OUTCOMES] = [ZERO; REMOTE_OUTCOMES];

pub struct Registry {
    owner_ns: [AtomicU64; 6],
    /// Seqlock over (`owner_ns`, `owner_current`, `owner_current_since_ns`); single writer = owner.
    owner_seq: AtomicU64,
    /// Section in progress (`OwnerCat` index + 1; 0 = none) and its start on the `mono_ns` clock.
    owner_current: AtomicU64,
    owner_current_since_ns: AtomicU64,
    remote_latency: [Hist; 8],
    remote_outcomes: [[AtomicU64; REMOTE_OUTCOMES]; 8],
    remote_slow: [AtomicU64; 8],
    queue_wait_latest: [Hist; 8],
    queue_wait_first: [Hist; 8],
    ingress_queue_wait: Hist,
    health_calls: [AtomicU64; HEALTH_SITES],
    counters: [AtomicU64; COUNTERS],
    pending_current: AtomicU64,
    pending_max: AtomicU64,
    backlog_age_ns: AtomicU64,
    backlog_age_max_ns: AtomicU64,
    backlog_age_known: AtomicU64,
}

/// Process-wide registry. Recording is lock-free and allocation-free.
pub static MT0: Registry = Registry::new();

impl Registry {
    pub const fn new() -> Self {
        Self {
            owner_ns: [ZERO; 6],
            owner_seq: ZERO,
            owner_current: ZERO,
            owner_current_since_ns: ZERO,
            remote_latency: [HIST; 8],
            remote_outcomes: [OUTCOME_ROW; 8],
            remote_slow: [ZERO; 8],
            queue_wait_latest: [HIST; 8],
            queue_wait_first: [HIST; 8],
            ingress_queue_wait: Hist::new(),
            health_calls: [ZERO; HEALTH_SITES],
            counters: [ZERO; COUNTERS],
            pending_current: ZERO,
            pending_max: ZERO,
            backlog_age_ns: ZERO,
            backlog_age_max_ns: ZERO,
            backlog_age_known: ZERO,
        }
    }

    pub fn owner_add(&self, cat: OwnerCat, d: Duration) {
        self.owner_ns[cat.idx()].fetch_add(d.as_nanos().min(u128::from(u64::MAX)) as u64, Relaxed);
    }

    pub fn owner_ns(&self, cat: OwnerCat) -> u64 {
        self.owner_ns[cat.idx()].load(Relaxed)
    }

    /// Owner only: book `booked_ns` to `from`, then publish `to` as the section in progress
    /// since `now_ns`. Seqlock-protected so readers never mix old and new state.
    pub fn owner_switch(&self, from: OwnerCat, booked_ns: u64, to: OwnerCat, now_ns: u64) {
        let seq = self.owner_seq.load(Relaxed);
        self.owner_seq.store(seq.wrapping_add(1), Relaxed);
        fence(Ordering::Release);
        self.owner_ns[from.idx()].fetch_add(booked_ns, Relaxed);
        self.owner_current.store(to.idx() as u64 + 1, Relaxed);
        self.owner_current_since_ns.store(now_ns, Relaxed);
        self.owner_seq.store(seq.wrapping_add(2), Ordering::Release);
    }

    /// `(busy_ns, wait_ns)` including the section still in progress at `now_ns`, so a section
    /// that runs for seconds is visible in every 1-s window, not only after it ends.
    pub fn owner_totals_at(&self, now_ns: u64) -> (u64, u64) {
        loop {
            let s1 = self.owner_seq.load(Ordering::Acquire);
            if s1 % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let booked: [u64; 6] = std::array::from_fn(|i| self.owner_ns[i].load(Relaxed));
            let current = self.owner_current.load(Relaxed);
            let since = self.owner_current_since_ns.load(Relaxed);
            fence(Ordering::Acquire);
            if self.owner_seq.load(Relaxed) != s1 {
                continue;
            }
            let wait_idx = OwnerCat::Wait.idx();
            let mut wait = booked[wait_idx];
            let mut busy: u64 = booked
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != wait_idx)
                .map(|(_, v)| *v)
                .sum();
            if current > 0 {
                let in_progress = now_ns.saturating_sub(since);
                if current - 1 == wait_idx as u64 {
                    wait = wait.saturating_add(in_progress);
                } else {
                    busy = busy.saturating_add(in_progress);
                }
            }
            return (busy, wait);
        }
    }

    pub fn record_remote(&self, class: RemoteClass, elapsed: Duration, outcome: RemoteOutcome) {
        let ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.remote_latency[class.idx()].record(ns);
        self.remote_outcomes[class.idx()][outcome.idx()].fetch_add(1, Relaxed);
        if ns >= SUSPECTED_RETRY_LATENCY_NS {
            self.remote_slow[class.idx()].fetch_add(1, Relaxed);
        }
    }

    pub fn remote_count(&self, class: RemoteClass, outcome: RemoteOutcome) -> u64 {
        self.remote_outcomes[class.idx()][outcome.idx()].load(Relaxed)
    }

    pub fn remote_requests(&self, class: RemoteClass) -> u64 {
        self.remote_outcomes[class.idx()]
            .iter()
            .map(|c| c.load(Relaxed))
            .sum()
    }

    /// Queue wait at dequeue: since the latest coalesced trigger and, when known, since the
    /// first still-unserved trigger (`t_rx_first`).
    pub fn record_queue_wait(
        &self,
        class: TriggerClass,
        since_latest_ns: u64,
        since_first_ns: Option<u64>,
    ) {
        self.queue_wait_latest[class.idx()].record(since_latest_ns);
        if let Some(first) = since_first_ns {
            self.queue_wait_first[class.idx()].record(first);
        }
    }

    /// Wait since the first still-unserved trigger of a dequeued token.
    pub fn record_queue_wait_first(&self, class: TriggerClass, ns: u64) {
        self.queue_wait_first[class.idx()].record(ns);
    }

    pub fn record_ingress_queue_wait(&self, ns: u64) {
        self.ingress_queue_wait.record(ns);
    }

    pub fn count_health(&self, site: HealthSite) {
        self.health_calls[site as usize].fetch_add(1, Relaxed);
    }

    pub fn health_calls(&self, site: HealthSite) -> u64 {
        self.health_calls[site as usize].load(Relaxed)
    }

    pub fn add(&self, counter: Counter, n: u64) {
        self.counters[counter as usize].fetch_add(n, Relaxed);
    }

    pub fn get(&self, counter: Counter) -> u64 {
        self.counters[counter as usize].load(Relaxed)
    }

    /// Owner-side backlog gauges (pending tokens and age of the oldest unserved trigger).
    pub fn set_backlog(&self, pending: u64, oldest_age_ns: Option<u64>) {
        self.pending_current.store(pending, Relaxed);
        self.pending_max.fetch_max(pending, Relaxed);
        match oldest_age_ns {
            Some(age) => {
                self.backlog_age_ns.store(age, Relaxed);
                self.backlog_age_max_ns.fetch_max(age, Relaxed);
                self.backlog_age_known.store(1, Relaxed);
            }
            None => self.backlog_age_ns.store(0, Relaxed),
        }
    }

    /// One JSON object (no external dependency); `duty` = per-second owner duty-cycle permille.
    pub fn snapshot_json(&self, unix_s: u64, now_ns: u64, duty: &HistSnapshot) -> String {
        let mut o = String::with_capacity(8_192);
        o.push_str(&format!("{{\"mt0_version\":2,\"unix_s\":{unix_s}"));
        // owner (totals include the section in progress)
        let (busy, wait) = self.owner_totals_at(now_ns);
        o.push_str(",\"owner_busy_ns\":{");
        o.push_str(
            &OWNER_CATS
                .iter()
                .map(|c| format!("\"{}\":{}", c.name(), self.owner_ns(*c)))
                .collect::<Vec<_>>()
                .join(","),
        );
        o.push_str(&format!(
            "}},\"owner_busy_total_ns\":{busy},\"owner_wait_total_ns\":{wait}"
        ));
        o.push_str(&format!(
            ",\"owner_duty_cycle_total\":{}",
            ratio_json(busy, busy.saturating_add(wait))
        ));
        o.push_str(&format!(
            ",\"owner_duty_cycle_permille_1s\":{}",
            hist_json(duty)
        ));
        // queues
        o.push_str(",\"queue_wait_latest_ns\":{");
        o.push_str(
            &TRIGGER_CLASSES
                .iter()
                .map(|c| {
                    format!(
                        "\"{}\":{}",
                        c.name(),
                        hist_json(&self.queue_wait_latest[c.idx()].snapshot())
                    )
                })
                .collect::<Vec<_>>()
                .join(","),
        );
        o.push_str("},\"queue_wait_first_ns\":{");
        o.push_str(
            &TRIGGER_CLASSES
                .iter()
                .map(|c| {
                    format!(
                        "\"{}\":{}",
                        c.name(),
                        hist_json(&self.queue_wait_first[c.idx()].snapshot())
                    )
                })
                .collect::<Vec<_>>()
                .join(","),
        );
        o.push_str(&format!(
            "}},\"ingress_queue_wait_ns\":{}",
            hist_json(&self.ingress_queue_wait.snapshot())
        ));
        o.push_str(&format!(
            ",\"pending_current\":{},\"pending_max\":{},\"backlog_age_ns\":{},\"backlog_age_max_ns\":{}",
            self.pending_current.load(Relaxed),
            self.pending_max.load(Relaxed),
            if self.backlog_age_known.load(Relaxed) == 1 { self.backlog_age_ns.load(Relaxed).to_string() } else { "null".into() },
            if self.backlog_age_known.load(Relaxed) == 1 { self.backlog_age_max_ns.load(Relaxed).to_string() } else { "null".into() },
        ));
        // remote
        o.push_str(",\"remote\":{");
        o.push_str(
            &REMOTE_CLASSES
                .iter()
                .map(|c| {
                    format!(
                        "\"{}\":{{\"provider\":\"{}\",\"requests\":{},\"ok\":{},\"rate_limited_429\":{},\"timeout\":{},\"error\":{},\"unclassified\":{},\"slow_ge_500ms\":{},\"latency_ns\":{}}}",
                        c.name(),
                        c.provider(),
                        self.remote_requests(*c),
                        self.remote_count(*c, RemoteOutcome::Ok),
                        self.remote_count(*c, RemoteOutcome::RateLimited),
                        self.remote_count(*c, RemoteOutcome::Timeout),
                        self.remote_count(*c, RemoteOutcome::Error),
                        self.remote_count(*c, RemoteOutcome::Unclassified),
                        self.remote_slow[c.idx()].load(Relaxed),
                        hist_json(&self.remote_latency[c.idx()].snapshot()),
                    )
                })
                .collect::<Vec<_>>()
                .join(","),
        );
        o.push('}');
        // health
        let h_sweep = self.health_calls(HealthSite::Sweep);
        let h_detect = self.health_calls(HealthSite::DetectConsumer);
        let h_pipe = self.health_calls(HealthSite::Pipeline);
        let evals = self.get(Counter::PipelineEvaluations);
        o.push_str(&format!(
            ",\"health_calls\":{{\"sweep\":{h_sweep},\"detect_consumer\":{h_detect},\"pipeline\":{h_pipe}}},\"health_calls_per_pipeline_evaluation\":{}",
            ratio_json(h_detect.saturating_add(h_pipe), evals)
        ));
        // counters
        for (c, name) in COUNTER_LIST {
            o.push_str(&format!(",\"{name}\":{}", self.get(c)));
        }
        o.push_str(&format!(
            ",\"jupiter_requests_per_routed_evaluation\":{},\"route_requotes\":{}",
            ratio_json(
                self.get(Counter::JupiterRequestsRouted),
                self.get(Counter::RoutedEvaluations)
            ),
            self.get(Counter::RouteFitAttempts)
                .saturating_sub(self.get(Counter::RouteFitCalls)),
        ));
        o.push_str(&format!(
            ",\"fee_rpc_requests\":{},\"simulation_requests\":{},\"projection_requests\":{},\"winner_requests\":{},\"reconcile_requests\":{}",
            self.remote_requests(RemoteClass::FeeRpc),
            self.remote_requests(RemoteClass::Simulation),
            self.remote_requests(RemoteClass::IntegrationProjection) + self.remote_requests(RemoteClass::OracleProjection),
            self.remote_requests(RemoteClass::Winner),
            self.remote_requests(RemoteClass::Reconcile),
        ));
        o.push_str(concat!(
            ",\"not_measured\":{",
            "\"remote_retry_sleep_ns\":\"N/A:DEFER_TO_MT1 (Solana client retries 429 internally; see slow_ge_500ms)\",",
            "\"owner_wakeup_latency_ns\":\"N/A:DEFER_TO_MT2 (polling owner, no inbox; see ingress_queue_wait_ns)\",",
            "\"prepare_calls\":\"N/A:DEFER_TO_MT2\",\"prepare_health_calls\":\"N/A:DEFER_TO_MT2\",\"prepare_ns\":\"N/A:DEFER_TO_MT2\",",
            "\"health_calls_per_candidate_revision\":\"N/A:DEFER_TO_MT5 (no revision ids; see health_calls_per_pipeline_evaluation)\"",
            "}}"
        ));
        o
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

fn ratio_json(num: u64, den: u64) -> String {
    if den == 0 {
        "null".to_string()
    } else {
        format!("{:.6}", num as f64 / den as f64)
    }
}

fn opt_json(v: Option<u64>) -> String {
    v.map_or_else(|| "null".to_string(), |x| x.to_string())
}

pub fn hist_json(h: &HistSnapshot) -> String {
    format!(
        "{{\"count\":{},\"p50\":{},\"p95\":{},\"p99\":{},\"max\":{}}}",
        h.count,
        opt_json(h.p50),
        opt_json(h.p95),
        opt_json(h.p99),
        opt_json(h.max)
    )
}

// ------------------------------------------------------------------------------------------
// Owner section timer (owner-local; flushes into the registry on every section change)
// ------------------------------------------------------------------------------------------

/// Monotonic nanoseconds since the first call in this process (shared by owner and reporter).
pub fn mono_ns() -> u64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    let origin = *ORIGIN.get_or_init(Instant::now);
    Instant::now()
        .saturating_duration_since(origin)
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
}

/// Attributes owner-loop time to sections. Time since the last `enter` always goes to the
/// section that was active, so early `continue`s stay attributed to their section. The section
/// in progress is published, so the reporter sees long-running sections while they run.
pub struct OwnerTimer {
    last_ns: u64,
    current: OwnerCat,
}

impl OwnerTimer {
    pub fn new() -> Self {
        Self {
            last_ns: mono_ns(),
            current: OwnerCat::Stats,
        }
    }

    pub fn enter(&mut self, cat: OwnerCat) {
        self.enter_at_ns(cat, mono_ns(), &MT0);
    }

    pub fn enter_at_ns(&mut self, cat: OwnerCat, now_ns: u64, reg: &Registry) {
        reg.owner_switch(
            self.current,
            now_ns.saturating_sub(self.last_ns),
            cat,
            now_ns,
        );
        self.last_ns = now_ns;
        self.current = cat;
    }
}

impl Default for OwnerTimer {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------------------------------
// Reporter thread (only place with file I/O)
// ------------------------------------------------------------------------------------------

/// Destination of the periodic snapshots (the runtime's binary journal). Called only on the
/// reporter thread. `store` returns `false` when the snapshot was not stored.
pub trait SnapshotSink: Send + 'static {
    fn store(&mut self, unix_ns: u64, mono_ns: u64, snapshot: &str) -> bool;
    /// Closes the sink after the final snapshot; `complete` is false once any snapshot was lost.
    fn finish(&mut self, complete: bool) -> bool;
}

/// Handle of the reporter thread. `finish` stops it, stores one final snapshot and closes the
/// sink; dropping the handle does the same, so the last interval is never silently lost.
pub struct Reporter {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<bool>>,
}
impl Reporter {
    /// `true` only if every snapshot, including the final one, was stored and the sink closed.
    pub fn finish(mut self) -> bool {
        self.join()
    }
    fn join(&mut self) -> bool {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .is_some_and(|worker| worker.join().unwrap_or(false))
    }
}
impl Drop for Reporter {
    fn drop(&mut self) {
        if self.worker.is_some() && !self.join() {
            eprintln!("mt0-reporter: MT0 evidence INCOMPLETE");
        }
    }
}

/// Samples the owner duty cycle every second (1-s windows) and hands a snapshot to `sink`
/// every `interval` and once more at the end. Runs off the owner thread.
pub fn spawn_reporter(
    interval: Duration,
    shutdown: Shutdown,
    mut sink: impl SnapshotSink,
) -> std::io::Result<Reporter> {
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let worker = std::thread::Builder::new()
        .name("mt0-reporter".into())
        .spawn(move || {
            let duty = Hist::new();
            let (mut last_busy, mut last_wait) = MT0.owner_totals_at(mono_ns());
            let mut last_write = Instant::now();
            let mut complete = true;
            let mut store = |sink: &mut dyn SnapshotSink, duty: &Hist| {
                let unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let now = mono_ns();
                let line = MT0.snapshot_json(unix.as_secs(), now, &duty.snapshot());
                let unix_ns = u64::try_from(unix.as_nanos()).unwrap_or(u64::MAX);
                complete &= sink.store(unix_ns, now, &line);
            };
            while !shutdown.is_requested() && !stopped.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_secs(1));
                let (busy, wait) = MT0.owner_totals_at(mono_ns());
                let (db, dw) = (
                    busy.saturating_sub(last_busy),
                    wait.saturating_sub(last_wait),
                );
                if db + dw > 0 {
                    duty.record(db.saturating_mul(1_000) / (db + dw));
                }
                (last_busy, last_wait) = (busy, wait);
                if last_write.elapsed() >= interval {
                    last_write = Instant::now();
                    store(&mut sink, &duty);
                }
            }
            store(&mut sink, &duty);
            sink.finish(complete) && complete
        })?;
    Ok(Reporter {
        stop,
        worker: Some(worker),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_is_deterministic_bounded_and_conservative() {
        let h = Hist::new();
        assert_eq!(h.snapshot(), HistSnapshot::default()); // no samples -> no invented zeros
        for v in 1..=100u64 {
            h.record(v * 1_000);
        }
        let s = h.snapshot();
        assert_eq!((s.count, s.max), (100, Some(100_000)));
        let (p50, p99) = (s.p50.unwrap(), s.p99.unwrap());
        assert!((50_000..=50_000 * 9 / 8 + 1).contains(&p50), "p50={p50}");
        assert!((99_000..=100_000).contains(&p99), "p99={p99}");
        // identical input -> identical snapshot
        let h2 = Hist::new();
        for v in 1..=100u64 {
            h2.record(v * 1_000);
        }
        assert_eq!(h2.snapshot(), s);
        // every u64 maps into the fixed bucket array
        assert!(bucket_index(u64::MAX) < HIST_BUCKETS);
        for v in [0, 1, 7, 8, 9, 15, 16, 1_000, 1 << 40, u64::MAX] {
            assert!(bucket_upper(bucket_index(v)) >= v);
        }
    }

    const MS: u64 = 1_000_000;

    #[test]
    fn owner_sections_sum_to_elapsed_time() {
        let reg = Registry::new();
        let mut t = OwnerTimer {
            last_ns: 0,
            current: OwnerCat::RemoteResults,
        };
        t.enter_at_ns(OwnerCat::Sweep, 3 * MS, &reg);
        t.enter_at_ns(OwnerCat::Wait, 5 * MS, &reg);
        t.enter_at_ns(OwnerCat::IngressApply, 15 * MS, &reg);
        t.enter_at_ns(OwnerCat::Detect, 16 * MS, &reg);
        t.enter_at_ns(OwnerCat::RemoteResults, 20 * MS, &reg);
        let total: u64 = OWNER_CATS.iter().map(|c| reg.owner_ns(*c)).sum();
        assert_eq!(total, 20 * MS);
        assert_eq!(reg.owner_ns(OwnerCat::Wait), 10 * MS);
        assert_eq!(reg.owner_ns(OwnerCat::Detect), 4 * MS);
        let json = reg.snapshot_json(0, 20 * MS, &HistSnapshot::default());
        assert!(json.contains("\"owner_duty_cycle_total\":0.500000"));
    }

    /// A section running for seconds must be visible in every 1-s reporter window while it runs.
    #[test]
    fn long_running_section_is_visible_before_it_ends() {
        let reg = Registry::new();
        let mut t = OwnerTimer {
            last_ns: 0,
            current: OwnerCat::Wait,
        };
        t.enter_at_ns(OwnerCat::Detect, 1_000 * MS, &reg); // 1 s idle, then Detect starts
        assert_eq!(reg.owner_totals_at(1_000 * MS), (0, 1_000 * MS));
        assert_eq!(reg.owner_totals_at(2_000 * MS), (1_000 * MS, 1_000 * MS)); // still inside Detect
        assert_eq!(reg.owner_totals_at(4_000 * MS), (3_000 * MS, 1_000 * MS));
        t.enter_at_ns(OwnerCat::Wait, 4_000 * MS, &reg); // Detect ends: booked == previously visible
        assert_eq!(reg.owner_totals_at(4_000 * MS), (3_000 * MS, 1_000 * MS));
        assert_eq!(reg.owner_totals_at(4_500 * MS), (3_000 * MS, 1_500 * MS));
    }

    #[test]
    fn remote_classes_and_outcomes_are_separate() {
        let reg = Registry::new();
        reg.record_remote(
            RemoteClass::FeeRpc,
            Duration::from_millis(80),
            RemoteOutcome::Ok,
        );
        reg.record_remote(
            RemoteClass::FeeRpc,
            Duration::from_millis(2_600),
            RemoteOutcome::RateLimited,
        );
        reg.record_remote(
            RemoteClass::Jupiter,
            Duration::from_millis(300),
            RemoteOutcome::Timeout,
        );
        reg.record_remote(
            RemoteClass::Reconcile,
            Duration::from_millis(10),
            RemoteOutcome::Unclassified,
        );
        assert_eq!(reg.remote_requests(RemoteClass::FeeRpc), 2);
        assert_eq!(
            reg.remote_count(RemoteClass::FeeRpc, RemoteOutcome::RateLimited),
            1
        );
        assert_eq!(
            reg.remote_count(RemoteClass::FeeRpc, RemoteOutcome::Timeout),
            0
        );
        assert_eq!(
            reg.remote_count(RemoteClass::Jupiter, RemoteOutcome::Timeout),
            1
        );
        assert_eq!(reg.remote_requests(RemoteClass::Simulation), 0);
        assert_eq!(
            reg.remote_count(RemoteClass::Reconcile, RemoteOutcome::Ok),
            0
        );
        assert_eq!(reg.remote_slow[RemoteClass::FeeRpc.idx()].load(Relaxed), 1);
    }

    #[test]
    fn error_classification_separates_429_timeout_and_other() {
        assert_eq!(RemoteOutcome::from_http_status(200), RemoteOutcome::Ok);
        assert_eq!(
            RemoteOutcome::from_http_status(429),
            RemoteOutcome::RateLimited
        );
        assert_eq!(RemoteOutcome::from_http_status(504), RemoteOutcome::Timeout);
        assert_eq!(RemoteOutcome::from_http_status(500), RemoteOutcome::Error);
        assert_eq!(
            RemoteOutcome::classify_text("HTTP status client error (429 Too Many Requests)"),
            RemoteOutcome::RateLimited
        );
        assert_eq!(
            RemoteOutcome::classify_text("operation timed out"),
            RemoteOutcome::Timeout
        );
        assert_eq!(
            RemoteOutcome::classify_text("connection refused"),
            RemoteOutcome::Error
        );
    }

    #[test]
    fn trigger_mapping_and_health_counters_count_actual_calls() {
        assert_eq!(
            TriggerClass::from_trigger("account_update"),
            TriggerClass::AccountUpdate
        );
        assert_eq!(
            TriggerClass::from_trigger("integration_projection"),
            TriggerClass::IntegrationProjection
        );
        assert_eq!(
            TriggerClass::from_trigger("integration_target_probe"),
            TriggerClass::IntegrationProbe
        );
        assert_eq!(
            TriggerClass::from_trigger("oracle_dependency"),
            TriggerClass::OracleDependency
        );
        assert_eq!(
            TriggerClass::from_trigger("forced_bank_rearm"),
            TriggerClass::ForcedRearm
        );
        assert_eq!(
            TriggerClass::from_trigger("something_new"),
            TriggerClass::Other
        );
        let reg = Registry::new();
        for _ in 0..3 {
            reg.count_health(HealthSite::Pipeline);
        }
        reg.count_health(HealthSite::DetectConsumer);
        reg.add(Counter::PipelineEvaluations, 2);
        assert_eq!(
            (
                reg.health_calls(HealthSite::Pipeline),
                reg.health_calls(HealthSite::Sweep)
            ),
            (3, 0)
        );
        assert!(reg
            .snapshot_json(0, 0, &HistSnapshot::default())
            .contains("\"health_calls_per_pipeline_evaluation\":2.000000"));
    }

    #[test]
    fn unmeasured_metrics_are_marked_not_zero_and_snapshot_is_bounded() {
        let reg = Registry::new();
        let a = reg.snapshot_json(0, 0, &HistSnapshot::default());
        assert!(a.contains("\"remote_retry_sleep_ns\":\"N/A:DEFER_TO_MT1"));
        assert!(a.contains("\"prepare_calls\":\"N/A:DEFER_TO_MT2\""));
        assert!(a.contains("\"backlog_age_ns\":null"));
        assert!(a.contains("\"jupiter_requests_per_routed_evaluation\":null"));
        assert!(a.contains("\"p50\":null"));
        for _ in 0..10_000 {
            reg.record_queue_wait(TriggerClass::Other, 5, Some(7));
            reg.record_remote(
                RemoteClass::Winner,
                Duration::from_micros(5),
                RemoteOutcome::Ok,
            );
        }
        let b = reg.snapshot_json(0, 0, &HistSnapshot::default());
        assert!(
            b.len() < a.len() + 512,
            "snapshot size must not grow with traffic"
        );
        reg.set_backlog(12, Some(3_000));
        assert!(reg
            .snapshot_json(0, 0, &HistSnapshot::default())
            .contains("\"pending_current\":12"));
    }
}
