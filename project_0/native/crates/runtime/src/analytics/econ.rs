//! C0-E1: observation only. Both producers use one Clock; analytics never call RPC.
//! Arrival-time hourly snapshots are provisional; final drain atomically reconciles them.
use crate::analytics::journal::{self, kind, Provenance};
use crate::analytics::measurement::{MeasurementPlan, SNAPSHOT_COUNT};
use crate::analytics::record::{CandidateCostRecord, Decision, OpportunityRecord};
use crate::market::winner::{Winner, WinnerIdentity};
use crate::market_observer::{utc_compact, MarketConfig, ReceiverEvent};
use darkbot_common::channel::{bounded, Consumer, Producer};
use darkbot_common::timing::Clock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[path = "econ_canonical.rs"]
mod canonical;
#[path = "econ_docs.rs"]
mod docs;
pub use canonical::{
    canonical_legacy_view, join as canonical_join, joined_facts, race_margins, record_digest,
    CanonicalSummary, CaptureFilter, JoinContext, JoinOutcome, Joined as CanonicalJoin,
    JoinedFacts, RaceMargins, ReplayScope,
};
pub use docs::*;

pub const CAPITAL_USD: [u64; 6] = [200, 500, 1000, 2500, 5000, 10000];

/// The capital scenarios (USD) of a run: `P0_ECON_CAPITAL_USD` and the replay's argument.
pub fn parse_capital(v: &str) -> Result<Vec<u64>, String> {
    let capital = v
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<u64>()
                .map_err(|_| "invalid capital bucket".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if capital.is_empty()
        || capital.len() > 32
        || capital
            .iter()
            .any(|x| *x == 0 || x.checked_mul(1_000_000).is_none())
    {
        return Err("invalid capital list".into());
    }
    Ok(capital)
}
pub const SPEEDUP_MS: [u64; 9] = [1, 5, 10, 25, 50, 100, 200, 500, 1000];
pub const NON_CLAIMS: [&str; 8] = [
    "Shadow evidence is not realized PnL or LIVE qualification",
    "Pre-winner slot is not a guarantee of capture if submitted",
    "True competitor submission deadline UNKNOWN",
    "Private relay arrival deadline UNKNOWN",
    "Local provider/host observation margins are not competitor send deadlines",
    "Observed speedup sensitivity is not guaranteed capture probability",
    "No daily/monthly/yearly extrapolation from one run",
    "Source coverage remains unverified; capital scenarios are per-opportunity, not concurrent portfolio returns",
];

#[derive(Clone)]
pub struct EconConfig {
    pub plan: MeasurementPlan,
    pub queue_capacity: usize,
    pub retention: usize,
    pub capital: Vec<u64>,
    pub run_dir: PathBuf,
    pub layout: crate::run_layout::RunLayout,
    pub run_id: String,
    pub provider_class: String,
}
impl EconConfig {
    pub fn paths(&self) -> crate::run_layout::RunPaths {
        crate::run_layout::RunPaths::new(&self.run_dir, self.layout)
    }
    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>, unix_s: u64) -> Result<Self, String> {
        for key in ["P0_LIVE_ENABLED", "P0_SUBMISSION_ENABLED"] {
            if get(key).is_some_and(|v| !matches!(v.as_str(), "false" | "0" | "")) {
                return Err(format!("{key} must be false in econ4h"));
            }
        }
        let num = |key: &str, default| -> Result<u64, String> {
            get(key).map_or(Ok(default), |v| {
                v.parse().map_err(|_| format!("invalid {key}"))
            })
        };
        let plan = MeasurementPlan::from_lookup(get)?;
        let layout = crate::run_layout::RunLayout::from_lookup(get)?;
        let queue_capacity = usize::try_from(num("P0_ECON_QUEUE_CAPACITY", 8192)?)
            .map_err(|_| "queue capacity overflow")?;
        let retention =
            usize::try_from(num("P0_ECON_RETENTION", 262144)?).map_err(|_| "retention overflow")?;
        if queue_capacity == 0
            || retention == 0
            || queue_capacity > 1_000_000
            || retention > 2_000_000
        {
            return Err("invalid econ bounded capacity/drain".into());
        }
        let capital = match get("P0_ECON_CAPITAL_USD") {
            Some(v) => parse_capital(&v)?,
            None => CAPITAL_USD.to_vec(),
        };
        let run_id =
            get("P0_ECON_RUN_ID").unwrap_or_else(|| format!("econ4h_{}", utc_compact(unix_s)));
        if !run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            || run_id.is_empty()
        {
            return Err("invalid econ run id".into());
        }
        let run_dir =
            PathBuf::from(get("P0_ECON_OUT_DIR").unwrap_or_else(|| "project_0/data/econ4h".into()))
                .join(&run_id);
        Ok(Self {
            plan,
            queue_capacity,
            retention,
            capital,
            run_dir,
            layout,
            run_id,
            provider_class: get("P0_PROVIDER_CLASS").unwrap_or_else(|| "UNKNOWN".into()),
        })
    }
}

// Published API retained; the adapter is owned and compiled once by execution.
pub use crate::execution::observe_only::ObserveOnly;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketEvidenceSource {
    Live,
    GapBackfill,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketEvent {
    pub signature: String,
    pub slot: u64,
    pub t_rx_ns: Option<u64>,
    pub source: MarketEvidenceSource,
    pub transaction_index: Option<u32>,
    pub event: ReceiverEvent,
}
#[derive(Clone, Debug)]
pub enum Evidence {
    Market(MarketEvent),
    CanonicalMarket(Box<crate::market_observer::truth::TruthEvent>),
    Slot {
        slot: u64,
        t_rx_ns: u64,
    },
    Shadow(Box<OpportunityRecord>),
    Winner {
        target: String,
        rx_slot: u64,
        winner: Winner,
    },
    MarketCheckpoint {
        index: usize,
        summary: Box<crate::market_observer::MarketSummary>,
    },
    MarketFinal(Box<crate::market_observer::MarketFinal>),
    MarketCounters {
        at_ns: u64,
        before: Box<crate::market_observer::MarketStats>,
        after: Box<crate::market_observer::MarketStats>,
    },
    ShadowStats(ShadowStats),
}
/// Position of one evaluation frame in the segmented evaluation journal (SK-07).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvalRef {
    pub segment: u32,
    pub offset: u64,
}

/// `ECON_SHADOW_EVALUATION` frame (JFI-FIX-05, typed): one Shadow evaluation and the position
/// of the previous in-window evaluation of the same target (its on-disk chain).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShadowEvaluation {
    pub previous: Option<EvalRef>,
    pub record: OpportunityRecord,
}
impl crate::analytics::payload::Payload for ShadowEvaluation {
    const KIND: u16 = kind::ECON_SHADOW_EVALUATION;
}

/// Shadow owner counters at shutdown. Built on the owner thread without JSON (PERSIST-02);
/// the analytics worker turns it into the `shadow_telemetry` value of its reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowStats {
    pub winner_watch_current: u64,
    pub winner_watch_max: u64,
    pub winner_watch_overflow: u64,
    pub winner_watch_errors: u64,
    pub winner_watch_resolved: u64,
    pub winner_watch_expired: u64,
    pub winner_queue_full_backoffs: u64,
    pub ingress_overflow: u64,
    pub winner_pending_at_deadline: u64,
    pub shadow_state_ready_at_shutdown: bool,
    pub remote_winner_queue_depth: u64,
    pub remote_projection_queue_depth: u64,
}
#[derive(Default)]
pub struct Shared {
    pub start_ns: AtomicU64,
    pub cutoff_ns: AtomicU64,
    pub abort: AtomicBool,
    /// Set with `abort` when the operator requested the stop (SIGTERM/SIGINT, `p0ctl run stop`).
    pub operator_stop: AtomicBool,
    pub drops: AtomicU64,
    pub max_depth: AtomicU64,
    pub retention_overflow: AtomicU64,
    pub writer_errors: AtomicU64,
    pub submission_attempts: AtomicU64,
    pub upgrade: AtomicBool,
    pub shadow_done: AtomicBool,
    pub shadow_ready: AtomicBool,
    pub shadow_ready_ns: AtomicU64,
    pub shadow_failed: AtomicBool,
    pub market_done: AtomicBool,
    pub market_failed: AtomicBool,
    pub finish: AtomicBool,
}
#[derive(Clone)]
pub struct Handle {
    tx: Producer<Evidence>,
    pub shared: Arc<Shared>,
    pub clock: Clock,
    pub cfg: EconConfig,
}
impl Handle {
    pub fn emit(&self, ev: Evidence) {
        if self.tx.try_push(ev).is_err() {
            self.shared.drops.fetch_add(1, Ordering::Relaxed);
        }
        let s = self.tx.stats();
        self.shared
            .max_depth
            .fetch_max(s.sent.saturating_sub(s.received), Ordering::Relaxed);
    }
    pub fn start(&self, at: u64, cutoff: u64) {
        self.shared.start_ns.store(at, Ordering::Release);
        self.shared.cutoff_ns.store(cutoff, Ordering::Release);
    }
    pub fn cutoff(&self) -> Option<u64> {
        let n = self.shared.cutoff_ns.load(Ordering::Acquire);
        (n > 0).then_some(n)
    }
    pub fn admission_closed(&self) -> bool {
        self.shared.abort.load(Ordering::Acquire)
            || self.cutoff().is_some_and(|c| self.clock.now_ns() >= c)
    }
    pub fn drain_expired(&self) -> bool {
        self.shared.abort.load(Ordering::Acquire)
            || self.cutoff().is_some_and(|c| {
                self.clock.now_ns()
                    >= c.saturating_add(self.cfg.plan.budgets.shadow_drain_seconds * 1_000_000_000)
            })
    }
    /// The shared economic measurement must not spend its window bootstrapping.
    /// This does not bypass Recovery: only the owner publishes actual readiness.
    pub fn wait_shadow_ready(&self, timeout: Duration) -> Result<(), String> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if self.shared.abort.load(Ordering::Acquire)
                || self.shared.shadow_done.load(Ordering::Acquire)
                || self.shared.shadow_failed.load(Ordering::Acquire)
            {
                return Err("SHADOW_STARTUP_FAILED".into());
            }
            if self.shared.shadow_ready.load(Ordering::Acquire) {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err("SHADOW_STARTUP_TIMEOUT".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// End of capture alone does not end Shadow's winner/projection RPC drain.
    pub fn wait_shadow_quiescent(&self, timeout: Duration) -> Result<(), String> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if self.shared.shadow_done.load(Ordering::Acquire) {
                return if self.shared.shadow_failed.load(Ordering::Acquire) {
                    Err("SHADOW_FAILED_BEFORE_MARKET_RECONCILIATION".into())
                } else {
                    Ok(())
                };
            }
            if std::time::Instant::now() >= deadline {
                return Err("SHADOW_DRAIN_BARRIER_TIMEOUT".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn capacity(&self) -> usize {
        self.cfg.retention
    }
}

/// Only accept a clean real Git checkout. Archive builds report UNKNOWN; never borrow a parent
/// repository identity or the export's claimed base as the implementation commit.
pub fn source_commit(repo: &Path) -> String {
    if !repo.join(".git").exists() {
        return "UNKNOWN".into();
    }
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    if git(&["status", "--porcelain", "--untracked-files=no"]).as_deref() != Some("") {
        return "UNKNOWN".into();
    }
    git(&["rev-parse", "HEAD"])
        .filter(|s| s.len() == 40 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or_else(|| "UNKNOWN".into())
}

pub fn spawn(
    cfg: EconConfig,
    clock: Clock,
    market: &MarketConfig,
) -> Result<(Handle, std::thread::JoinHandle<Result<(), String>>), String> {
    cfg.paths().create().map_err(|e| e.to_string())?;
    crate::analytics::measurement::persist_manifest(
        &cfg.paths(),
        &cfg.run_id,
        &cfg.provider_class,
        &cfg.plan,
        clock,
    )?;
    let (tx, rx) = bounded("econ-analytics", cfg.queue_capacity);
    let handle = Handle {
        tx,
        shared: Arc::new(Shared::default()),
        clock,
        cfg: cfg.clone(),
    };
    let h = handle.clone();
    let source = market.source.as_str().to_string();
    let commitment = market.commitment.to_string();
    let min_sample = market.min_sample_liquidations;
    // Startup provenance only, never executed per opportunity.
    let repo = crate::analytics::measurement::repo_root();
    let commit = source_commit(&repo);
    let worker = std::thread::Builder::new()
        .name("econ-analytics".into())
        .spawn(move || Worker::new(h, rx, source, commitment, commit, min_sample)?.run())
        .map_err(|e| e.to_string())?;
    Ok((handle, worker))
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Distribution {
    pub count_known: usize,
    pub total: Option<f64>,
    pub average: Option<f64>,
    pub p10: Option<f64>,
    pub median: Option<f64>,
    pub p50: Option<f64>,
    pub p90: Option<f64>,
    pub p99: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}
pub fn distribution(mut values: Vec<f64>) -> Distribution {
    values.retain(|x| x.is_finite());
    values.sort_by(f64::total_cmp);
    let q = |p| crate::market_observer::quantile_nearest_rank(&values, p);
    let sum: f64 = values.iter().sum();
    Distribution {
        count_known: values.len(),
        total: (!values.is_empty()).then_some(sum),
        average: (!values.is_empty()).then_some(sum / values.len().max(1) as f64),
        p10: q(0.1),
        median: q(0.5),
        p50: q(0.5),
        p90: q(0.9),
        p99: q(0.99),
        min: values.first().copied(),
        max: values.last().copied(),
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RaceClass {
    PreWinnerSlotProven,
    SameSlotUnknown,
    PostWinnerSlotProvenLate,
    NoSign,
    WinnerUnresolved,
    WinnerIdentityAmbiguous,
    LiveTimestampUnqualified,
    MarketMatchMissing,
}
#[derive(Clone, Debug, Serialize)]
pub struct Race {
    pub class: RaceClass,
    pub chain_slot_class: RaceClass,
    pub live_timestamp_qualified: bool,
    pub identity_mismatch: bool,
    pub margin_to_winner_rx_ns: Option<i128>,
    pub margin_to_winner_rx_ms: Option<f64>,
    pub margin_to_winner_slot_seen_ns: Option<i128>,
    pub margin_to_winner_slot_seen_ms: Option<f64>,
    pub required_speedup_to_winner_rx_ns: Option<u64>,
    pub required_speedup_to_winner_slot_seen_ns: Option<u64>,
    pub required_speedup_to_winner_rx_ms: Option<f64>,
    pub required_speedup_to_winner_slot_seen_ms: Option<f64>,
}
/// The live snapshot bucket of a monotone receipt time; `None` outside `[start, cutoff]` or the
/// plan. A3: the one window rule of the live evaluation chain and of the offline join.
pub fn live_hour(plan: &MeasurementPlan, start: u64, cutoff: u64, ns: u64) -> Option<usize> {
    if ns < start || ns > cutoff {
        return None;
    }
    plan.live_bucket(ns - start)
}

/// A2: the winner's live receipt is a race observation only under `confirmed`. Under
/// `finalized` (and an unknown commitment) it is the finalized notification, roughly a
/// finalization later than the winner slot, so every margin against it would be too optimistic.
pub fn rx_margin_valid(market_commitment: &str) -> bool {
    market_commitment == "confirmed"
}

/// Report status of the margins against the winner's live receipt.
pub fn rx_margin_status(market_commitment: &str) -> String {
    if rx_margin_valid(market_commitment) {
        "COMPUTED".into()
    } else {
        format!("NOT_COMPUTED: market commitment {market_commitment}; the winner's live receipt is its finalized notification, not a race observation; the winner-slot margin is the race figure")
    }
}

pub fn race(
    rec: &OpportunityRecord,
    winner: Option<&Winner>,
    market: Option<&MarketEvent>,
    slot_seen: Option<u64>,
    rx_valid: bool,
) -> Race {
    let chain = match (rec.sign_slot, rec.timing.t_sign_ns, winner) {
        (None, _, _) | (_, None, _) => RaceClass::NoSign,
        (_, _, None) => RaceClass::WinnerUnresolved,
        (
            _,
            _,
            Some(Winner {
                identity: WinnerIdentity::SameSlotAmbiguous { .. },
                ..
            }),
        ) => RaceClass::WinnerIdentityAmbiguous,
        (Some(s), _, Some(w)) if s < w.slot => RaceClass::PreWinnerSlotProven,
        (Some(s), _, Some(w)) if s == w.slot => RaceClass::SameSlotUnknown,
        _ => RaceClass::PostWinnerSlotProvenLate,
    };
    let identity_ok = match (winner, market) {
        (
            Some(Winner {
                slot,
                identity:
                    WinnerIdentity::Unique {
                        signature,
                        receiver,
                        ..
                    },
            }),
            Some(m),
        ) => {
            *signature == m.signature
                && *receiver == m.event.receiver.to_string()
                && *slot == m.slot
                && rec.account == m.event.target.to_string()
        }
        _ => false,
    };
    let mismatch = winner.is_some()
        && market.is_some()
        && !identity_ok
        && !matches!(chain, RaceClass::WinnerIdentityAmbiguous);
    let qualified = identity_ok
        && rec.timing.t_sign_ns.is_some()
        && market
            .is_some_and(|m| matches!(m.source, MarketEvidenceSource::Live) && m.t_rx_ns.is_some());
    let class = if mismatch {
        RaceClass::LiveTimestampUnqualified
    } else if winner.is_some()
        && market.is_none()
        && !matches!(
            chain,
            RaceClass::NoSign | RaceClass::WinnerIdentityAmbiguous
        )
    {
        RaceClass::MarketMatchMissing
    } else {
        chain.clone()
    };
    let rx_margin = (qualified && rx_valid)
        .then(|| market.unwrap().t_rx_ns.unwrap() as i128 - rec.timing.t_sign_ns.unwrap() as i128);
    let slot_margin = if qualified {
        slot_seen.map(|s| s as i128 - rec.timing.t_sign_ns.unwrap() as i128)
    } else {
        None
    };
    let speed = |m: Option<i128>| m.map(|m| (-m).max(0) as u64);
    Race {
        class,
        chain_slot_class: chain,
        live_timestamp_qualified: qualified,
        identity_mismatch: mismatch,
        margin_to_winner_rx_ns: rx_margin,
        margin_to_winner_rx_ms: rx_margin.map(|v| v as f64 / 1e6),
        margin_to_winner_slot_seen_ns: slot_margin,
        margin_to_winner_slot_seen_ms: slot_margin.map(|v| v as f64 / 1e6),
        required_speedup_to_winner_rx_ns: speed(rx_margin),
        required_speedup_to_winner_slot_seen_ns: speed(slot_margin),
        required_speedup_to_winner_rx_ms: speed(rx_margin).map(|v| v as f64 / 1e6),
        required_speedup_to_winner_slot_seen_ms: speed(slot_margin).map(|v| v as f64 / 1e6),
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeCount {
    pub count: u64,
    pub net_usd_micro: i128,
}
impl EdgeCount {
    fn add(&mut self, net: i128) {
        self.count += 1;
        self.net_usd_micro += net;
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RacePool {
    pub pre: EdgeCount,
    pub same: EdgeCount,
    pub post: EdgeCount,
    pub unresolved: EdgeCount,
}
impl RacePool {
    fn add(&mut self, class: &RaceClass, net: i128) {
        match class {
            RaceClass::PreWinnerSlotProven => self.pre.add(net),
            RaceClass::SameSlotUnknown => self.same.add(net),
            RaceClass::PostWinnerSlotProvenLate => self.post.add(net),
            _ => self.unresolved.add(net),
        }
    }
}
/// `ECON_OPPORTUNITY` frame (JFI-FIX-05, typed): the inputs of one opportunity row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpportunityRow {
    pub rec: OpportunityRecord,
    pub winner: Option<Winner>,
    pub market: Option<MarketEvent>,
    /// First local receipt of the winner slot.
    pub slot_seen: Option<u64>,
    /// Capital scenarios of the run (USD).
    pub capital: Vec<u64>,
    /// A2: commitment of the market capture (decides whether the winner rx margin is valid).
    pub market_commitment: String,
}
/// Layout of schema 2 (before A2): no market commitment.
#[derive(Deserialize)]
struct OpportunityRowV2 {
    rec: OpportunityRecord,
    winner: Option<Winner>,
    market: Option<MarketEvent>,
    slot_seen: Option<u64>,
    capital: Vec<u64>,
}
impl crate::analytics::payload::Payload for OpportunityRow {
    const KIND: u16 = kind::ECON_OPPORTUNITY;
    const SCHEMA: u16 = 3;
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<Self>> {
        (schema == crate::analytics::payload::TYPED).then(|| {
            crate::analytics::payload::decode_wire::<OpportunityRowV2>(bytes).map(|v| Self {
                rec: v.rec,
                winner: v.winner,
                market: v.market,
                slot_seen: v.slot_seen,
                capital: v.capital,
                market_commitment: "UNKNOWN".into(),
            })
        })
    }
}

/// Historical `opportunity_economics.jsonl` row of an opportunity (same function live and
/// offline, so the render of a typed frame is the row the run produced).
pub fn opportunity_view(row: &OpportunityRow) -> Value {
    let rec = &row.rec;
    let (w, m) = (row.winner.as_ref(), row.market.as_ref());
    let r = race(
        rec,
        w,
        m,
        row.slot_seen,
        rx_margin_valid(&row.market_commitment),
    );
    let strong = r.live_timestamp_qualified
        || (m.is_some()
            && !r.identity_mismatch
            && matches!(w.map(|w| &w.identity), Some(WinnerIdentity::Unique { .. })));
    let cost = rec.selected_costs.as_ref();
    let member: Vec<u64> = row
        .capital
        .iter()
        .copied()
        .filter(|usd| {
            executable(rec)
                && cost
                    .and_then(CandidateCostRecord::total_budget)
                    .is_some_and(|b| b <= usd * 1_000_000)
        })
        .collect();
    json!({"target":rec.account,"winner":w,"market":if strong {m} else {None},"strong_join":strong,
        "our":rec,"race":r,"required_working_capital_usd_micro":cost.and_then(|c|c.required_working_capital_usd_micro),
        "required_working_capital_usd":cost.and_then(|c|c.required_working_capital_usd_micro).map(|v|v as f64/1e6),
        "required_fee_tip_reserve_usd":cost.and_then(CandidateCostRecord::fee_tip_reserve).map(|v|v as f64/1e6),
        "required_total_budget_usd_lower_bound":cost.and_then(CandidateCostRecord::total_budget).map(|v|v as f64/1e6),
        "required_fee_tip_reserve_usd_micro":cost.and_then(CandidateCostRecord::fee_tip_reserve),
        "required_total_budget_usd_lower_bound_micro":cost.and_then(CandidateCostRecord::total_budget),
        "capital_bucket_membership_usd":member,"true_competitor_submission_deadline":"UNKNOWN",
        "stage_intervals_ns":stage_intervals(rec),
        "stage_latency_budget_sufficient_if_fully_removed":stage_intervals(rec).into_iter().map(|(k,v)|(k,
            v.zip(r.required_speedup_to_winner_rx_ns).map(|(stage,speed)|speed>0 && speed<=stage))).collect::<BTreeMap<_,_>>(),
        "t_winner_rx_ns":if r.live_timestamp_qualified {m.and_then(|m|m.t_rx_ns)}else{None},
        "t_winner_slot_first_seen_ns":if r.live_timestamp_qualified {row.slot_seen}else{None}})
}

fn bottleneck(rec: &OpportunityRecord) -> String {
    if matches!(
        rec.decision,
        Decision::ShadowSimFailed | Decision::ShadowTechnicalSimFailed
    ) {
        "Simulation".into()
    } else {
        rec.loss
            .map(|l| format!("{l:?}"))
            .unwrap_or_else(|| "Unknown".into())
    }
}
fn executable(rec: &OpportunityRecord) -> bool {
    rec.decision == Decision::ShadowReady
        && rec.economics_known
        && rec.net_profit.is_some_and(|n| n > 0)
        && rec.simulation_result.as_deref() == Some("PASS")
        && rec.timing.t_sign_ns.is_some()
        && rec.timing.t_handoff_ns.is_none()
}

/// Only the offline replay (`p0-econ-replay`, input of the pinned `market_report.py`, JFI-03)
/// writes JSON documents; the productive run writes typed journal frames only.
pub fn atomic_json(path: &Path, value: &Value) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut f, value).map_err(|e| e.to_string())?;
    f.write_all(b"\n")
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}
fn line(file: &mut BufWriter<File>, value: &impl Serialize) -> Result<(), String> {
    serde_json::to_writer(&mut *file, value).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())
}

// The arrival sequence makes the original forward selected() reduction reproducible
// even when a late winner requires reverse traversal of a target's disk journal.
struct RetainedShadow {
    hour: usize,
    sequence: u64,
    rec: OpportunityRecord,
    /// Accounted retention bytes of this entry (released exactly on removal).
    bytes: u64,
}

/// Fixed per-entry allowance for map nodes and allocations on top of the typed encoding.
const RETAINED_OVERHEAD: u64 = 96;

/// SK-03: accounted bytes of one retained entry — its typed (bincode) encoding plus the fixed
/// node allowance. Deterministic, never zero, never an unbounded estimate.
fn retained_size<T: Serialize + ?Sized>(v: &T) -> u64 {
    bincode::serialized_size(v).unwrap_or(u64::MAX / 4) + RETAINED_OVERHEAD
}
/// Name of the analytics report journal (snapshots, final report, derived rows).
pub const REPORT_JOURNAL: &str = "economics_reports.p0j";

struct Worker {
    h: Handle,
    rx: Consumer<Evidence>,
    source: String,
    commitment: String,
    commit: String,
    min_sample: u64,
    market: Vec<MarketEvent>,
    canonical_market: Vec<crate::market_observer::truth::TruthEvent>,
    canonical_joined: Vec<canonical::Joined>,
    canonical_finalized: bool,
    slots: BTreeMap<u64, u64>,
    // One best record per target/winner-slot or unresolved target/hour.
    shadow: BTreeMap<String, RetainedShadow>,
    // One bounded head per target: the offset of its newest in-window evaluation frame. Each
    // frame names the previous one, so all revisions live on disk in the binary journal.
    shadow_heads: BTreeMap<String, EvalRef>,
    // Every evaluation, in arrival order (`shadow_evaluations.p0j` and its linked segments),
    // and read handles per segment for chain walks; frames are written unbuffered, so the
    // readers see every appended frame.
    evaluations: crate::storage::segments::SegmentedWriter,
    evaluations_first: std::path::PathBuf,
    evaluations_at: BTreeMap<u32, File>,
    // Snapshots, final report and derived rows; the legacy JSON files are renders of it.
    reports: journal::Writer,
    shadow_evaluations_received: u64,
    shadow_evaluations_streamed: u64,
    shadow_evaluations_coalesced: u64,
    shadow_semantic_entries_max: usize,
    shadow_semantic_evictions: u64,
    market_counters: [BTreeMap<String, u64>; SNAPSHOT_COUNT],
    winners: BTreeMap<String, Vec<(u64, Winner)>>,
    checkpoints: BTreeMap<usize, crate::market_observer::MarketSummary>,
    market_final: Option<crate::market_observer::MarketFinal>,
    shadow_stats: Option<ShadowStats>,
    final_technical: Option<Technical>,
    evaluation_counts: [u64; SNAPSHOT_COUNT],
    snapshots: usize,
    /// SK-03: byte budget of every retained collection of this worker (market, canonical, slots,
    /// Shadow heads and entries, winners, checkpoints). Admission beyond it is refused and
    /// counted as retention overflow (technical INCOMPLETE); nothing grows past the limit.
    retained_bytes: u64,
    retained_bytes_peak: u64,
    retention_byte_limit: u64,
}
impl Worker {
    fn new(
        h: Handle,
        rx: Consumer<Evidence>,
        source: String,
        commitment: String,
        commit: String,
        min_sample: u64,
    ) -> Result<Self, String> {
        let paths = h.cfg.paths();
        let evaluations_path = journal::binary_path(&paths.capture("shadow_evaluations.jsonl"));
        let segment_bytes = darkbot_solana::runtime_config::active()
            .limits
            .econ_segment_mib
            * crate::storage::budget::MIB;
        let evaluations =
            crate::storage::segments::SegmentedWriter::create(&evaluations_path, segment_bytes)
                .map_err(|e| e.to_string())?;
        let reports =
            journal::Writer::create(&paths.snapshot(REPORT_JOURNAL)).map_err(|e| e.to_string())?;
        Ok(Self {
            h,
            rx,
            source,
            commitment,
            commit,
            min_sample,
            market: vec![],
            canonical_market: vec![],
            canonical_joined: vec![],
            canonical_finalized: false,
            slots: BTreeMap::new(),
            shadow: BTreeMap::new(),
            shadow_heads: BTreeMap::new(),
            evaluations,
            evaluations_first: evaluations_path,
            evaluations_at: BTreeMap::new(),
            reports,
            shadow_evaluations_received: 0,
            shadow_evaluations_streamed: 0,
            shadow_evaluations_coalesced: 0,
            shadow_semantic_entries_max: 0,
            shadow_semantic_evictions: 0,
            market_counters: Default::default(),
            winners: BTreeMap::new(),
            checkpoints: BTreeMap::new(),
            market_final: None,
            shadow_stats: None,
            final_technical: None,
            evaluation_counts: [0; SNAPSHOT_COUNT],
            snapshots: 0,
            retained_bytes: 0,
            retained_bytes_peak: 0,
            retention_byte_limit: darkbot_solana::runtime_config::active()
                .limits
                .econ_retention_mib
                * crate::storage::budget::MIB,
        })
    }
    /// Reserves `bytes` of the retention budget, or records an overflow and refuses.
    fn reserve(&mut self, bytes: u64) -> bool {
        match self.retained_bytes.checked_add(bytes) {
            Some(total) if total <= self.retention_byte_limit => {
                self.retained_bytes = total;
                self.retained_bytes_peak = self.retained_bytes_peak.max(total);
                true
            }
            _ => {
                self.over_limit();
                false
            }
        }
    }
    fn release(&mut self, bytes: u64) {
        self.retained_bytes = self.retained_bytes.saturating_sub(bytes);
    }
    /// Inside the capture window `[start, cutoff]` (the market core's `in_window`).
    fn in_window(&self, ns: u64) -> bool {
        let start = self.h.shared.start_ns.load(Ordering::Acquire);
        self.h
            .cutoff()
            .is_some_and(|cutoff| ns >= start && ns <= cutoff)
    }
    fn hour(&self, ns: u64) -> Option<usize> {
        let start = self.h.shared.start_ns.load(Ordering::Acquire);
        live_hour(&self.h.cfg.plan, start, self.h.cutoff()?, ns)
    }
    fn over_limit(&self) {
        self.h
            .shared
            .retention_overflow
            .fetch_add(1, Ordering::Relaxed);
    }
    fn accept(&mut self, e: Evidence) -> Result<(), String> {
        match e {
            Evidence::CanonicalMarket(event) => {
                if self.canonical_market.len() >= self.h.capacity() {
                    self.over_limit();
                } else if self
                    .canonical_market
                    .iter()
                    .any(|e| e.signature == event.signature && e.event_index == event.event_index)
                {
                    return Err("duplicate canonical event".into());
                } else if self.reserve(retained_size(&*event)) {
                    self.canonical_market.push(*event);
                }
            }
            Evidence::Market(e) => {
                if self.market.len() >= self.h.capacity() {
                    self.over_limit();
                } else if self.reserve(retained_size(&e)) {
                    self.market.push(e);
                }
            }
            // A2: only in-window receipts, exactly the `SlotBoundary` rows the capture journals,
            // so the live and the offline winner-slot margin read the same first receipt.
            Evidence::Slot { t_rx_ns, .. } if !self.in_window(t_rx_ns) => {}
            Evidence::Slot { slot, t_rx_ns } => {
                if let Some(t) = self.slots.get_mut(&slot) {
                    *t = (*t).min(t_rx_ns);
                } else if self.slots.len() >= self.h.capacity() {
                    self.over_limit();
                } else if self.reserve(16 + RETAINED_OVERHEAD) {
                    self.slots.insert(slot, t_rx_ns);
                }
            }
            Evidence::Shadow(rec) => {
                self.shadow_evaluations_received += 1;
                let hour = rec.timing.t_rx_ns.and_then(|t| self.hour(t));
                let full = self.shadow_heads.len() >= self.h.capacity()
                    && !self.shadow_heads.contains_key(&rec.account);
                let chained = hour.is_some() && !full;
                // Every evaluation is streamed; only in-window ones join their target's chain.
                let previous = chained
                    .then(|| self.shadow_heads.get(&rec.account).copied())
                    .flatten();
                let value = ShadowEvaluation {
                    previous,
                    record: (*rec).clone(),
                };
                let provenance = Provenance {
                    source: journal::source::ECON,
                    source_seq: self.shadow_evaluations_received,
                    slot: rec.rx_slot,
                    rx_ns: rec.timing.t_rx_ns,
                    ..Default::default()
                };
                let sequence = provenance.source_seq;
                let (segment, frame) = self
                    .evaluations
                    .append_typed(provenance, &value)
                    .map_err(|e| e.to_string())?;
                self.evaluations
                    .rotate_if_due()
                    .map_err(|e| e.to_string())?;
                let at = EvalRef {
                    segment,
                    offset: frame.offset,
                };
                self.shadow_evaluations_streamed += 1;
                if let Some(hour) = hour {
                    self.evaluation_counts[hour] += 1;
                    if full {
                        self.shadow_semantic_evictions += 1;
                        self.over_limit();
                    } else if self.shadow_heads.contains_key(&rec.account)
                        || self.reserve(retained_size(rec.account.as_str()) + 12)
                    {
                        self.shadow_heads.insert(rec.account.clone(), at);
                        self.retain_shadow(hour, sequence, *rec, true);
                    } else {
                        // Head refused by the byte budget (overflow recorded): the evaluation
                        // stays streamed on disk, it just cannot join a retained chain.
                        self.shadow_semantic_evictions += 1;
                    }
                }
            }
            Evidence::Winner {
                target,
                rx_slot,
                winner,
            } => {
                let duplicate = self
                    .winners
                    .get(&target)
                    .is_some_and(|v| v.iter().any(|(r, w)| *r == rx_slot && *w == winner));
                if !duplicate {
                    let count: usize = self.winners.values().map(Vec::len).sum();
                    if count >= self.h.capacity() {
                        self.over_limit();
                    } else if self.reserve(retained_size(&winner) + target.len() as u64 + 8) {
                        self.winners
                            .entry(target.clone())
                            .or_default()
                            .push((rx_slot, winner));
                        self.recorrelate_shadow(&target)?;
                    }
                }
            }
            Evidence::MarketCheckpoint { index, summary } => {
                let summary = *summary;
                let bytes = retained_size(&summary);
                if let Some(old) = self.checkpoints.remove(&index) {
                    self.release(retained_size(&old));
                }
                if self.reserve(bytes) {
                    self.checkpoints.insert(index, summary);
                }
            }
            Evidence::MarketCounters {
                at_ns,
                before,
                after,
            } => {
                if let Some(hour) = self.hour(at_ns) {
                    let b: BTreeMap<&str, u64> = before.counters().into_iter().collect();
                    for (key, n) in after.counters() {
                        if key == "first_observed_slot" {
                            self.market_counters[hour].entry(key.into()).or_insert(n);
                        } else if key == "last_observed_slot" || key == "max_stream_silence_ms" {
                            let old = self.market_counters[hour].entry(key.into()).or_default();
                            *old = (*old).max(n);
                        } else {
                            *self.market_counters[hour].entry(key.into()).or_default() +=
                                n.saturating_sub(b.get(key).copied().unwrap_or(0));
                        }
                    }
                }
            }
            Evidence::MarketFinal(v) => self.market_final = Some(*v),
            Evidence::ShadowStats(v) => self.shadow_stats = Some(v),
        }
        Ok(())
    }
    fn winner_for(&self, rec: &OpportunityRecord) -> Option<&Winner> {
        let rx = rec.rx_slot?;
        self.winners
            .get(&rec.account)?
            .iter()
            .filter(|(start, w)| *start <= rx && rx <= w.slot)
            .min_by_key(|(_, w)| w.slot)
            .map(|(_, w)| w)
    }
    fn market_for(&self, rec: &OpportunityRecord, winner: Option<&Winner>) -> Option<&MarketEvent> {
        let Winner {
            identity: WinnerIdentity::Unique { signature, .. },
            ..
        } = winner?
        else {
            return None;
        };
        // Signature match alone is insufficient; race() checks target/receiver/slot too.
        self.market
            .iter()
            .find(|m| &m.signature == signature && m.event.target.to_string() == rec.account)
            .or_else(|| self.market.iter().find(|m| &m.signature == signature))
    }
    /// The typed `ECON_OPPORTUNITY` row of one selected record (its inputs; the JSONL view is
    /// `opportunity_view`).
    fn opportunity(&self, rec: &OpportunityRecord) -> OpportunityRow {
        let w = self.winner_for(rec);
        OpportunityRow {
            rec: rec.clone(),
            winner: w.cloned(),
            market: self.market_for(rec, w).cloned(),
            slot_seen: w.and_then(|w| self.slots.get(&w.slot).copied()),
            capital: self.h.cfg.capital.clone(),
            market_commitment: self.commitment.clone(),
        }
    }
    fn retain_shadow(&mut self, hour: usize, sequence: u64, rec: OpportunityRecord, count: bool) {
        let key = match self.winner_for(&rec) {
            Some(w) => format!("{}:winner:{}", rec.account, w.slot),
            None => format!("{}:unresolved:{hour}", rec.account),
        };
        let replace = match self.shadow.get(&key) {
            None => {
                if self.shadow.len() >= self.h.capacity() {
                    self.shadow_semantic_evictions += 1;
                    self.over_limit();
                    return;
                }
                true
            }
            Some(old) => {
                if count {
                    self.shadow_evaluations_coalesced += 1;
                }
                match (executable(&old.rec), executable(&rec)) {
                    (true, true) => {
                        (rec.timing.t_sign_ns, sequence) < (old.rec.timing.t_sign_ns, old.sequence)
                    }
                    (true, false) => false,
                    (false, true) => true,
                    (false, false) => sequence > old.sequence,
                }
            }
        };
        if replace {
            let bytes = retained_size(&rec) + key.len() as u64;
            let old = self.shadow.get(&key).map_or(0, |e| e.bytes);
            if bytes > old && !self.reserve(bytes - old) {
                // Byte budget exhausted (overflow recorded): keep the retained entry unchanged.
                self.shadow_semantic_evictions += 1;
                return;
            }
            self.release(old.saturating_sub(bytes));
            self.shadow.insert(
                key,
                RetainedShadow {
                    hour,
                    sequence,
                    rec,
                    bytes,
                },
            );
            self.shadow_semantic_entries_max =
                self.shadow_semantic_entries_max.max(self.shadow.len());
        }
    }
    /// One hop of `target`'s evaluation chain: (arrival sequence, previous hop, record).
    /// Every hop is a CRC32/SHA256-verified typed frame of this run's evaluation journal; a hop
    /// may live in an earlier segment, never in a later position.
    fn chain_frame(
        &mut self,
        target: &str,
        at: EvalRef,
    ) -> Result<(u64, Option<EvalRef>, OpportunityRecord), String> {
        if at.offset < journal::HEADER_LEN
            || at.segment == 0
            || at.segment > self.evaluations.segments()
        {
            return Err("invalid shadow journal offset".into());
        }
        let file = match self.evaluations_at.entry(at.segment) {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(e) => e.insert(
                File::open(crate::storage::segments::segment_path(
                    &self.evaluations_first,
                    at.segment,
                ))
                .map_err(|e| format!("invalid shadow journal segment: {e}"))?,
            ),
        };
        let frame = journal::read_at(file, at.offset)
            .map_err(|e| format!("invalid shadow journal frame: {e}"))?;
        if frame.kind != kind::ECON_SHADOW_EVALUATION {
            return Err("invalid shadow journal frame kind".into());
        }
        let value: ShadowEvaluation = frame
            .typed()
            .map_err(|e| format!("invalid shadow journal frame: {e}"))?;
        if value.previous.is_some_and(|p| p >= at) {
            return Err("invalid shadow journal chain".into());
        }
        if value.record.account != target {
            return Err("shadow journal target mismatch".into());
        }
        Ok((frame.provenance.source_seq, value.previous, value.record))
    }
    fn recorrelate_shadow(&mut self, target: &str) -> Result<(), String> {
        let Some(head) = self.shadow_heads.get(target).copied() else {
            return Ok(());
        };
        let freed: u64 = self
            .shadow
            .values()
            .filter(|entry| entry.rec.account == target)
            .map(|entry| entry.bytes)
            .sum();
        self.release(freed);
        self.shadow.retain(|_, entry| entry.rec.account != target);
        // Stream only this target's chain, one record at a time. Replaying just the
        // old representative would lose opportunities when a winner splits a group.
        let mut cursor = Some(head);
        while let Some(at) = cursor {
            let (sequence, previous, rec) = self.chain_frame(target, at)?;
            let hour = rec
                .timing
                .t_rx_ns
                .and_then(|t| self.hour(t))
                .ok_or("shadow journal timestamp mismatch")?;
            self.retain_shadow(hour, sequence, rec, false);
            cursor = previous;
        }
        Ok(())
    }
    fn market_hour(&self, market: &MarketEvent) -> Option<usize> {
        match market.source {
            MarketEvidenceSource::Live => market.t_rx_ns.and_then(|t| self.hour(t)),
            MarketEvidenceSource::GapBackfill => Some(SNAPSHOT_COUNT - 1),
        }
    }
    fn selected(&self) -> Vec<(usize, &OpportunityRecord)> {
        self.shadow
            .values()
            .map(|entry| {
                let m = self.market_for(&entry.rec, self.winner_for(&entry.rec));
                (
                    m.and_then(|m| self.market_hour(m)).unwrap_or(entry.hour),
                    &entry.rec,
                )
            })
            .collect()
    }
    fn metrics(&self, from: usize, through: usize) -> Metrics {
        let seconds = self.h.cfg.plan.span_seconds(from, through);
        let markets: Vec<_> = self
            .market
            .iter()
            .filter(|m| {
                self.market_hour(m)
                    .is_some_and(|h| h >= from && h < through)
            })
            .collect();
        let selected = self.selected();
        let rows: Vec<_> = selected
            .into_iter()
            .filter(|(h, _)| *h >= from && *h < through)
            .map(|(_, r)| r)
            .collect();
        let mut capture_counters: BTreeMap<String, u64> =
            crate::market_observer::MarketStats::default()
                .counters()
                .into_iter()
                .map(|(k, n)| (k.to_string(), n))
                .collect();
        for counts in &self.market_counters[from..through] {
            for (key, n) in counts {
                let v = capture_counters.entry(key.clone()).or_insert(*n);
                if key == "first_observed_slot" {
                    *v = (*v).min(*n);
                } else if key == "last_observed_slot" || key == "max_stream_silence_ms" {
                    *v = (*v).max(*n);
                } else { /* handled below using a sum to avoid counting the first item twice */
                }
            }
        }
        for (key, v) in &mut capture_counters {
            if key != "first_observed_slot"
                && key != "last_observed_slot"
                && key != "max_stream_silence_ms"
            {
                *v = self.market_counters[from..through]
                    .iter()
                    .map(|m| m.get(key).copied().unwrap_or(0))
                    .sum();
            }
        }
        let gross = distribution(markets.iter().map(|m| m.event.gross_edge_usd()).collect());
        let repaid = distribution(markets.iter().map(|m| m.event.repaid_equity_usd).collect());
        let seized = distribution(markets.iter().map(|m| m.event.seized_equity_usd).collect());
        let roi = distribution(
            markets
                .iter()
                .filter(|m| m.event.repaid_equity_usd > 0.0)
                .map(|m| m.event.gross_edge_usd() / m.event.repaid_equity_usd)
                .collect(),
        );
        let mut receivers: BTreeMap<String, (usize, f64)> = BTreeMap::new();
        for m in &markets {
            let e = receivers.entry(m.event.receiver.to_string()).or_default();
            e.0 += 1;
            e.1 += m.event.gross_edge_usd();
        }
        let mut ranking: Vec<_> = receivers.into_iter().collect();
        ranking.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
        let total_gross = gross.total.unwrap_or(0.0);
        let share = |x: f64, y: f64| if y > 0.0 { Some(x / y) } else { None };
        let top = ranking.first();
        let mut by_gross: Vec<_> = ranking.iter().map(|r| r.1 .1).collect();
        by_gross.sort_by(|a, b| b.total_cmp(a));
        let mut market_matched = BTreeSet::new();
        let (
            mut resolved,
            mut unresolved,
            mut live_matched,
            mut missing,
            mut mismatch,
            mut ambiguous,
        ) = (0, 0, 0, 0, 0, 0);
        let (mut known, mut economics_unknown) = (EdgeCount::default(), 0);
        let mut races = RacePool::default();
        let (mut margins, mut slot_margins, mut speeds, mut slot_speeds) =
            (vec![], vec![], vec![], vec![]);
        let mut sensitivity: Vec<(EdgeCount, EdgeCount)> = SPEEDUP_MS
            .iter()
            .map(|_| (EdgeCount::default(), EdgeCount::default()))
            .collect();
        let mut stage_all: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
        let mut stage_late: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
        let mut stage_weighted: BTreeMap<&str, f64> = BTreeMap::new();
        let mut stage_sufficient: BTreeMap<&str, EdgeCount> = BTreeMap::new();
        let mut bottlenecks: BTreeMap<String, Bottleneck> = [
            "State",
            "Health",
            "CircuitBreaker",
            "Bankruptcy",
            "IntegrationRefresh",
            "Funding",
            "MissingAta",
            "UnsupportedIntegration",
            "Sizing",
            "Economics",
            "Routing",
            "Build",
            "Simulation",
            "EconomicSkip",
            "ThresholdTooHigh",
            "RaceLate",
            "RaceSameSlotUnknown",
            "RaceUnresolved",
            "Capital",
            "Unknown",
        ]
        .into_iter()
        .map(|s| (s.into(), Bottleneck::default()))
        .collect();
        let mut capital: Vec<Capital> = self
            .h
            .cfg
            .capital
            .iter()
            .map(|usd| Capital::new(*usd))
            .collect();
        for rec in &rows {
            let w = self.winner_for(rec);
            let m = self.market_for(rec, w);
            let r = race(
                rec,
                w,
                m,
                w.and_then(|w| self.slots.get(&w.slot).copied()),
                rx_margin_valid(&self.commitment),
            );
            let strong = m.is_some()
                && !r.identity_mismatch
                && matches!(w.map(|w| &w.identity), Some(WinnerIdentity::Unique { .. }));
            if strong {
                let m = m.unwrap();
                market_matched.insert((m.signature.clone(), m.event.event_index));
            }
            if w.is_some() {
                resolved += 1;
            } else {
                unresolved += 1;
            }
            if r.identity_mismatch {
                mismatch += 1;
            }
            if matches!(
                w.map(|w| &w.identity),
                Some(WinnerIdentity::SameSlotAmbiguous { .. })
            ) {
                ambiguous += 1;
            }
            if strong
                && m.is_some_and(|m| {
                    matches!(m.source, MarketEvidenceSource::Live) && m.t_rx_ns.is_some()
                })
            {
                live_matched += 1;
            } else if w.is_some() {
                missing += 1;
            }
            let net = if rec.economics_known {
                rec.net_profit
            } else {
                None
            };
            if net.is_none() {
                economics_unknown += 1;
            }
            let exec = executable(rec);
            let stages = stage_intervals(rec);
            for (name, ns) in &stages {
                if let Some(ns) = ns {
                    stage_all.entry(name).or_default().push(*ns as f64);
                }
            }
            if exec {
                let n = net.unwrap();
                known.add(n);
                if strong {
                    races.add(&r.class, n);
                } else {
                    races.unresolved.add(n);
                }
                if r.live_timestamp_qualified {
                    if let Some(v) = r.margin_to_winner_rx_ns {
                        margins.push(v as f64);
                    }
                    if let Some(v) = r.margin_to_winner_slot_seen_ns {
                        slot_margins.push(v as f64);
                    }
                    if let Some(v) = r.required_speedup_to_winner_rx_ns.filter(|v| *v > 0) {
                        speeds.push(v as f64);
                    }
                    if let Some(v) = r.required_speedup_to_winner_slot_seen_ns.filter(|v| *v > 0) {
                        slot_speeds.push(v as f64);
                    }
                    for (i, ms) in SPEEDUP_MS.iter().enumerate() {
                        if r.required_speedup_to_winner_rx_ns
                            .is_some_and(|s| s > 0 && s <= ms * 1_000_000)
                        {
                            sensitivity[i].0.add(n);
                        }
                        if r.required_speedup_to_winner_slot_seen_ns
                            .is_some_and(|s| s > 0 && s <= ms * 1_000_000)
                        {
                            sensitivity[i].1.add(n);
                        }
                    }
                    if r.required_speedup_to_winner_rx_ns.is_some_and(|s| s > 0)
                        || r.class == RaceClass::PostWinnerSlotProvenLate
                    {
                        for (name, ns) in &stages {
                            if let Some(ns) = ns {
                                stage_late.entry(name).or_default().push(*ns as f64);
                                *stage_weighted.entry(name).or_default() += *ns as f64 * n as f64;
                                if r.required_speedup_to_winner_rx_ns
                                    .is_some_and(|s| s > 0 && s <= *ns)
                                {
                                    stage_sufficient.entry(name).or_default().add(n);
                                }
                            }
                        }
                    }
                }
            }
            for b in &mut capital {
                b.add(rec, exec, strong, &r.class);
            }
            let reason = if !exec {
                bottleneck(rec)
            } else {
                match r.class {
                    RaceClass::PostWinnerSlotProvenLate => "RaceLate".into(),
                    RaceClass::SameSlotUnknown => "RaceSameSlotUnknown".into(),
                    RaceClass::PreWinnerSlotProven => {
                        if rec
                            .selected_costs
                            .as_ref()
                            .and_then(CandidateCostRecord::total_budget)
                            .is_none()
                        {
                            "Capital".into()
                        } else {
                            continue;
                        }
                    }
                    _ => "RaceUnresolved".into(),
                }
            };
            let b = bottlenecks.entry(reason).or_default();
            b.opportunity_count += 1;
            if let Some(n) = net.filter(|n| *n > 0) {
                b.known_candidate_net_edge_usd_micro += n;
            }
            if strong {
                b.market_matched_count += 1;
                *b.matched_market_gross_edge_usd.get_or_insert(0.0) +=
                    m.unwrap().event.gross_edge_usd();
            }
        }
        let blocked: i128 = bottlenecks
            .values()
            .map(|b| b.known_candidate_net_edge_usd_micro)
            .sum();
        for b in bottlenecks.values_mut() {
            if blocked > 0 {
                b.percentage_of_known_rejected_executable_edge =
                    Some(100.0 * b.known_candidate_net_edge_usd_micro as f64 / blocked as f64);
            }
        }
        let mut bottleneck_rank: Vec<_> = bottlenecks.iter().collect();
        bottleneck_rank.sort_by(|a, b| {
            b.1.known_candidate_net_edge_usd_micro
                .cmp(&a.1.known_candidate_net_edge_usd_micro)
                .then(b.1.opportunity_count.cmp(&a.1.opportunity_count))
                .then(a.0.cmp(b.0))
        });
        let mut stage_rank: Vec<_> = stage_weighted
            .iter()
            .filter(|(k, _)| **k != "rx_to_sign" && **k != "rx_to_state")
            .collect();
        stage_rank.sort_by(|a, b| b.1.total_cmp(a.1).then(a.0.cmp(b.0)));
        let distributions = |m: BTreeMap<&'static str, Vec<f64>>| {
            m.into_iter()
                .map(|(k, v)| (k.to_string(), distribution(v)))
                .collect::<BTreeMap<_, _>>()
        };
        let ms = |v: &[f64]| distribution(v.iter().map(|x| x / 1e6).collect());
        let observed_speedup_sensitivity = SPEEDUP_MS
            .iter()
            .zip(sensitivity)
            .map(|(ms, (rx, slot))| Sensitivity {
                speedup_ms: *ms,
                winner_rx_boundary: rx,
                winner_slot_first_seen_boundary: slot,
            })
            .collect();
        let largest_measured_late_stage = stage_rank.first().map(|(k, _)| k.to_string());
        let ranking_names: Vec<String> =
            bottleneck_rank.iter().map(|(k, _)| (*k).clone()).collect();
        let market_count = markets.len() as u64;
        let top_receiver_liquidation_share =
            top.and_then(|r| share(r.1 .0 as f64, markets.len() as f64));
        let top_receiver_gross_edge_share = top.and_then(|r| share(r.1 .1, total_gross));
        Metrics {
            market: MarketMetrics {
                capture_counters,
                liquidation_events: market_count,
                gross_edge_usd: gross,
                gross_edge_per_hour_usd: total_gross * 3600.0 / seconds,
                repaid_equity_usd: repaid,
                seized_equity_usd: seized,
                gross_roi: roi,
                unique_receivers: ranking.len() as u64,
                top_receiver: top.map(|r| r.0.clone()),
                top_receiver_liquidation_share,
                top_receiver_gross_edge_share,
                top3_liquidation_share: share(
                    ranking.iter().take(3).map(|r| r.1 .0).sum::<usize>() as f64,
                    markets.len() as f64,
                ),
                top3_gross_edge_share: share(by_gross.iter().take(3).sum(), total_gross),
                ranked_receivers: ranking
                    .iter()
                    .take(20)
                    .map(|(id, (count, gross))| RankedReceiver {
                        receiver: id.clone(),
                        count: *count as u64,
                        gross_edge_usd: *gross,
                    })
                    .collect(),
            },
            shadow: ShadowMetrics {
                evaluations: self.evaluation_counts[from..through].iter().sum::<u64>(),
                opportunities: rows.len() as u64,
                known_executable: known.clone(),
                stage_intervals_ns: distributions(stage_all),
            },
            matching: Matching {
                market_liquidations_total: market_count,
                market_with_shadow_match: market_matched.len() as u64,
                market_without_shadow_match: markets.len().saturating_sub(market_matched.len())
                    as u64,
                shadow_winner_resolved: resolved,
                shadow_winner_unresolved: unresolved,
                winner_live_observation_matched: live_matched,
                winner_live_observation_missing: missing,
                winner_identity_mismatch: mismatch,
                same_slot_ambiguous: ambiguous,
            },
            economics: EconomicsMetrics {
                known_executable_net_edge_per_hour_usd: known.net_usd_micro as f64 / 1e6 * 3600.0
                    / seconds,
                known_executable: known,
                economics_unknown_count: economics_unknown,
                realized_pnl: None,
            },
            capital: capital.into_iter().map(|b| b.report(seconds)).collect(),
            competition: races.clone(),
            race_timing: RaceTiming {
                race_classes: races,
                margin_to_winner_rx_ns: distribution(margins.clone()),
                margin_to_winner_rx_ms: ms(&margins),
                margin_to_winner_slot_seen_ns: distribution(slot_margins.clone()),
                margin_to_winner_slot_seen_ms: ms(&slot_margins),
                required_speedup_to_winner_rx_ns: distribution(speeds.clone()),
                required_speedup_to_winner_rx_ms: ms(&speeds),
                required_speedup_to_winner_slot_seen_ns: distribution(slot_speeds.clone()),
                required_speedup_to_winner_slot_seen_ms: ms(&slot_speeds),
                observed_speedup_sensitivity,
                late_stage_intervals_ns: distributions(stage_late),
                late_stage_net_edge_weighted_ns_micro_usd: stage_weighted
                    .iter()
                    .map(|(k, v)| (k.to_string(), *v))
                    .collect(),
                stage_latency_budget_sufficient_if_fully_removed: stage_sufficient
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                largest_measured_late_stage,
                stage_removal_is_mathematical_ceiling_only: true,
            },
            bottlenecks: Bottlenecks {
                by_reason: bottlenecks,
                ranking: ranking_names,
                known_rejected_candidate_edge_pool_usd_micro: blocked,
                denominator_note:
                    "positive known candidate edge; simulation failures are not executable claims"
                        .into(),
            },
            unknowns: Unknowns {
                economics_unknown,
                winner_unresolved: unresolved,
                live_winner_missing: missing,
                true_competitor_submission_deadline: "UNKNOWN".into(),
            },
        }
    }
    fn technical(&self, finalizing: bool) -> Technical {
        if finalizing {
            if let Some(t) = &self.final_technical {
                return t.clone();
            }
        }
        let s = &self.h.shared;
        let market_ok = self
            .market_final
            .as_ref()
            .is_some_and(|m| m.summary.market_capture_complete && m.summary.drain_complete);
        let writer_ok = s.writer_errors.load(Ordering::Relaxed) == 0;
        let duration_ok = self
            .market_final
            .as_ref()
            .map(|m| m.summary.actual_observation_seconds)
            == Some(self.h.cfg.plan.duration_seconds as f64);
        let shadow_ok = self.shadow_stats.as_ref().is_some_and(|s| {
            s.ingress_overflow == 0
                && s.winner_watch_overflow == 0
                && s.shadow_state_ready_at_shutdown
        });
        let ready_ns = s.shadow_ready_ns.load(Ordering::Acquire);
        let start_ns = s.start_ns.load(Ordering::Acquire);
        let startup_ok = ready_ns > 0 && ready_ns <= start_ns;
        let measured_evaluations: u64 = self.evaluation_counts.iter().sum();
        let pass = finalizing
            && startup_ok
            && measured_evaluations > 0
            && self.canonical_finalized
            && shadow_ok
            && market_ok
            && duration_ok
            && self.snapshots == SNAPSHOT_COUNT
            && writer_ok
            && self.shadow_evaluations_received == self.shadow_evaluations_streamed
            && self.shadow_semantic_evictions == 0
            && s.drops.load(Ordering::Relaxed) == 0
            && s.retention_overflow.load(Ordering::Relaxed) == 0
            && !s.shadow_failed.load(Ordering::Relaxed)
            && s.shadow_done.load(Ordering::Relaxed)
            && !s.market_failed.load(Ordering::Relaxed)
            && s.market_done.load(Ordering::Relaxed)
            && !s.upgrade.load(Ordering::Relaxed)
            && s.submission_attempts.load(Ordering::Relaxed) == 0;
        let mt0 = darkbot_common::mt0::MT0.snapshot(
            (self.h.clock.unix_ns(self.h.clock.now_ns()) / 1_000_000_000) as u64,
            darkbot_common::mt0::mono_ns(),
            &Default::default(),
        );
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        Technical {
            verdict: if pass {
                "PASS"
            } else if finalizing {
                "INCOMPLETE"
            } else {
                "IN_PROGRESS"
            }
            .into(),
            requested_duration_seconds: self.h.cfg.plan.duration_seconds,
            actual_duration_seconds: self
                .market_final
                .as_ref()
                .map(|m| m.summary.actual_observation_seconds),
            clean_automatic_shutdown: finalizing && !s.abort.load(Ordering::Relaxed),
            market_capture_complete: market_ok,
            four_snapshots_present: self.snapshots == SNAPSHOT_COUNT,
            snapshot_write_success: self.snapshots as u64,
            snapshot_write_failures: load(&s.writer_errors),
            analytics_queue_capacity: self.h.cfg.queue_capacity as u64,
            analytics_queue_max_depth: load(&s.max_depth),
            analytics_queue_drops: load(&s.drops),
            shadow_evaluations_received: self.shadow_evaluations_received,
            shadow_evaluations_streamed: self.shadow_evaluations_streamed,
            shadow_evaluations_coalesced: self.shadow_evaluations_coalesced,
            shadow_semantic_entries_current: self.shadow.len() as u64,
            shadow_semantic_entries_max: self.shadow_semantic_entries_max as u64,
            shadow_semantic_evictions: self.shadow_semantic_evictions,
            shadow_journal_target_heads: self.shadow_heads.len() as u64,
            retention_capacity: self.h.cfg.retention as u64,
            retention_overflow: load(&s.retention_overflow),
            retention_bytes_current: self.retained_bytes,
            retention_bytes_peak: self.retained_bytes_peak,
            retention_byte_limit: self.retention_byte_limit,
            shadow_ready_at_ns: s.shadow_ready_ns.load(Ordering::Acquire),
            startup_excluded_from_measurement: startup_ok,
            shadow_evaluations_in_window: measured_evaluations,
            shadow_runtime_terminated_early: s.shadow_failed.load(Ordering::Relaxed),
            shadow_runtime_finished: s.shadow_done.load(Ordering::Relaxed),
            program_upgrade_halt: s.upgrade.load(Ordering::Relaxed),
            submission_attempts_blocked: load(&s.submission_attempts),
            submissions: 0,
            live_ready: false,
            source_coverage_verified: self
                .market_final
                .as_ref()
                .is_some_and(|m| m.summary.source_coverage_verified),
            final_reconciliation_complete: finalizing && self.canonical_finalized,
            shadow_telemetry: self.shadow_stats.clone(),
            remote_and_backlog_telemetry: Some((&mt0).into()),
        }
    }
    fn report(&self, index: usize, finalizing: bool) -> SnapshotReport {
        let cumulative = self.metrics(0, index);
        let delta = self.metrics(index - 1, index);
        let start = self.h.shared.start_ns.load(Ordering::Acquire);
        let end = start + self.h.cfg.plan.offset_ns(index);
        let elapsed = if finalizing {
            self.market_final
                .as_ref()
                .map(|m| m.summary.actual_observation_seconds)
                .unwrap_or(0.0)
                .min(self.h.cfg.plan.seconds(index))
        } else {
            self.h.clock.now_ns().saturating_sub(start) as f64 / 1e9
        };
        let complete = finalizing && elapsed >= self.h.cfg.plan.seconds(index);
        let capture_window = if self.h.cutoff().is_some() {
            self.h.cfg.plan.capture_window(self.h.clock, start).ok()
        } else {
            None
        };
        let r6 = if index == SNAPSHOT_COUNT && finalizing {
            self.market_final.clone()
        } else {
            self.checkpoints
                .get(&index)
                .cloned()
                .map(crate::market_observer::MarketFinal::checkpoint)
        };
        let counter_delta = counter_delta(
            r6.as_ref().map(|m| &m.summary),
            self.checkpoints.get(&(index - 1)),
        );
        let canonical = Section {
            window_delta: self.canonical_metrics(index - 1, index),
            cumulative: self.canonical_metrics(0, index),
        };
        let canonical_count = canonical
            .cumulative
            .market
            .get("ALL")
            .map_or(0, |g| g.events);
        let executable_edge_status = if cumulative.economics.known_executable.count > 0 {
            "KNOWN_SHADOW_EDGE"
        } else {
            "NOT_PROVEN"
        };
        let competition_evidence_status = if cumulative.matching.winner_live_observation_matched > 0
        {
            "MATCHED_EVIDENCE"
        } else {
            "INSUFFICIENT_EVIDENCE"
        };
        let (d, c) = (delta, cumulative);
        SnapshotReport {
            measurement_plan: self.h.cfg.plan.clone(),
            capture_window,
            identity: Identity {
                schema_version: "C0-E1/1".into(),
                run_id: self.h.cfg.run_id.clone(),
                source_commit: self.commit.clone(),
                marginfi_pin: "33c67987a661f59c6683a65485ebb4f0ef030b77".into(),
                mode: "econ4h".into(),
                market_source: self.source.clone(),
                provider_class: self.h.cfg.provider_class.clone(),
                commitment: self.commitment.clone(),
                started_at_utc: crate::analytics::measurement::utc_ns(self.h.clock.unix_ns(start)),
                ended_at_utc: crate::analytics::measurement::utc_ns(self.h.clock.unix_ns(end)),
                actual_elapsed_seconds: elapsed,
                snapshot_index: index as u64,
                measurement_plan: self.h.cfg.plan.clone(),
                snapshot_interval_seconds: self.h.cfg.plan.span_seconds(index - 1, index),
                complete,
                live_enabled: false,
            },
            window: Window {
                from_offset_ns: self.h.cfg.plan.offset_ns(index - 1),
                through_offset_ns: self.h.cfg.plan.offset_ns(index),
                from_seconds: self.h.cfg.plan.seconds(index - 1),
                through_seconds: self.h.cfg.plan.seconds(index),
                provisional: !finalizing,
                reconciled_after_drain: finalizing,
                join_assignment: "matched event receive hour; unmatched candidate trigger hour".into(),
                coalescing: "earliest executable sign per target/winner; unresolved per target/hour; all input evaluations streamed to disk; bounded semantic representatives; disk-indexed late-winner recorrelation".into(),
                backfill_hour_assignment: "final resolution window (hour 4), not a live receive time".into(),
            },
            technical_qualification: self.technical(finalizing),
            non_claims: NON_CLAIMS.iter().map(|s| s.to_string()).collect(),
            economic_sample_status: if canonical_count >= self.min_sample {
                "SAMPLE_SUFFICIENT"
            } else {
                "INSUFFICIENT_SAMPLE"
            }
            .into(),
            market_edge_status: if canonical_count > 0 {
                "OBSERVED"
            } else {
                "NO_OBSERVED_LIQUIDATIONS"
            }
            .into(),
            executable_edge_status: executable_edge_status.into(),
            competition_evidence_status: competition_evidence_status.into(),
            live_ready: false,
            market: MarketSection {
                scope: "LEGACY_RECEIVERSHIP_ANALYTICS; not total Marginfi market; canonical is the authoritative all-path market/economics join".into(),
                window_delta: MarketDelta {
                    metrics: d.market,
                    c0_mo_counter_delta: counter_delta,
                    counter_attribution: "capture-boundary checkpoints; post-capture RPC/truncation qualification charged to final resolution window".into(),
                },
                cumulative: MarketCumulative {
                    metrics: c.market,
                    c0_mo_r6: r6,
                    c0_mo_checkpoint_qualification: if index == SNAPSHOT_COUNT && finalizing {
                        "FINAL_AFTER_ENRICHMENT"
                    } else {
                        "CAPTURE_CHECKPOINT_ENRICHMENT_PENDING"
                    }
                    .into(),
                },
            },
            shadow: Section {
                window_delta: d.shadow,
                cumulative: c.shadow,
            },
            matching: Section {
                window_delta: d.matching,
                cumulative: c.matching,
            },
            economics: Section {
                window_delta: d.economics,
                cumulative: c.economics,
            },
            capital: Section {
                window_delta: d.capital,
                cumulative: c.capital,
            },
            competition: Section {
                window_delta: d.competition,
                cumulative: c.competition,
            },
            race_timing: Section {
                window_delta: d.race_timing,
                cumulative: c.race_timing,
            },
            bottlenecks: Section {
                window_delta: d.bottlenecks,
                cumulative: c.bottlenecks,
            },
            unknowns: Section {
                window_delta: d.unknowns,
                cumulative: c.unknowns,
            },
            canonical_market_truth: self.market_final.as_ref().map(|m| CanonicalTruthRef {
                full_window_summary: m.summary.canonical_market_truth.clone(),
                reconciliation: m.summary.full_slot_reconciliation.clone(),
                hourly_attribution_qualified: self.canonical_finalized,
                market_shadow_join_qualified: self.canonical_finalized,
            }),
            canonical,
            authoritative_analytics:
                "canonical; other top-level analytics retain legacy receiver scope for compatibility"
                    .into(),
            race_margins: Section {
                window_delta: self.race_margin_metrics(index - 1, index),
                cumulative: self.race_margin_metrics(0, index),
            },
        }
    }
    fn write_snapshot(&mut self, index: usize, finalizing: bool) -> Result<(), String> {
        let frame = SnapshotFrame {
            index: index as u64,
            finalizing,
            report: self.report(index, finalizing),
        };
        // The journal frame is the snapshot; `hour_0N.json` exists only as an explicit offline
        // render of the newest frame of this index (JFI-03).
        self.append_typed_report(&frame).inspect_err(|_| {
            self.h.shared.writer_errors.fetch_add(1, Ordering::Relaxed);
        })
    }
    /// A typed report frame (JFI-FIX-05) with the same provenance as `append_report`.
    fn append_typed_report<P: crate::analytics::payload::Payload>(
        &mut self,
        value: &P,
    ) -> Result<(), String> {
        let provenance = Provenance {
            source: journal::source::ECON,
            source_seq: self.reports.next_seq(),
            unix_ns: u64::try_from(self.h.clock.unix_ns(self.h.clock.now_ns())).ok(),
            ..Default::default()
        };
        self.reports
            .append_typed(provenance, value)
            .map(|_| ())
            .map_err(|e| format!("REPORT_JOURNAL: {e}"))
    }
    /// Terminal text of the final report (typed values; the documents are journal frames).
    fn terminal_summary(&self, r: &SnapshotReport) -> String {
        let market = &r.market.cumulative;
        let usd = |micro: i128| format!("{:.6}", micro as f64 / 1e6);
        let dec = |v: Option<f64>| v.map_or_else(|| "UNKNOWN".to_string(), |n| format!("{n:.6}"));
        let races = &r.competition.cumulative;
        let speed = &r.race_timing.cumulative.required_speedup_to_winner_rx_ms;
        let margins = &r.race_margins.cumulative;
        let unknowns = &r.unknowns.cumulative;
        let canonical = &r.canonical.cumulative;
        let mut lines = vec![
            format!(
                "C0-E1 technical={} market={} sample={} liquidations={} gross_total_usd={} matched={} executable={} executable_net_usd={}",
                r.technical_qualification.verdict,
                market.c0_mo_r6.as_ref().map_or("UNKNOWN", |m| m.summary.verdict.as_str()),
                r.economic_sample_status,
                market.metrics.liquidation_events,
                dec(market.metrics.gross_edge_usd.total),
                r.matching.cumulative.market_with_shadow_match,
                r.economics.cumulative.known_executable.count,
                usd(r.economics.cumulative.known_executable.net_usd_micro),
            ),
            format!(
                "capital: {}",
                r.capital
                    .cumulative
                    .iter()
                    .map(|b| format!(
                        "{}usd fundable={} net_usd={:.6}",
                        b.budget_usd, b.fundable_known_opportunities, b.known_executable_net_edge_usd
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            format!(
                "race: pre={} same={} post={} unresolved={}",
                races.pre.count, races.same.count, races.post.count, races.unresolved.count
            ),
            // A2: the main race figure is the canonical margin to the first local receipt of the
            // winner slot; the winner rx margin exists only under `confirmed`.
            format!(
                "race_margin_to_winner_slot_ms (canonical, first local slot receipt): count={} median={} p90={} required_speedup_median={} status={}",
                margins.canonical_slot_margin_ms.count_known,
                dec(margins.canonical_slot_margin_ms.median),
                dec(margins.canonical_slot_margin_ms.p90),
                dec(margins.required_canonical_slot_speedup_ms.median),
                margins
                    .winner_slot_margin_status
                    .split(':')
                    .next()
                    .unwrap_or("UNKNOWN")
            ),
            if rx_margin_valid(&margins.market_commitment) {
                format!(
                    "winner_rx_required_speedup_ms: count={} median={} p90={}",
                    speed.count_known,
                    dec(speed.median),
                    dec(speed.p90)
                )
            } else {
                format!(
                    "winner_rx_required_speedup_ms: NOT_COMPUTED (market commitment {})",
                    margins.market_commitment
                )
            },
            format!(
                "bottlenecks: {}",
                r.bottlenecks.cumulative.ranking.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
            ),
            format!(
                "unknowns: economics={} winner_unresolved={} live_winner_missing={} submission_deadline={}",
                unknowns.economics_unknown,
                unknowns.winner_unresolved,
                unknowns.live_winner_missing,
                unknowns.true_competitor_submission_deadline
            ),
            format!(
                "journal: {} (offline views: p0-journal renders RUN --create)",
                self.reports.path.display()
            ),
        ];
        let events = canonical
            .market
            .iter()
            .map(|(k, g)| format!("{k}={}", g.events))
            .collect::<Vec<_>>()
            .join(" ");
        let losses = canonical
            .loss_attribution
            .iter()
            .filter(|(_, b)| b.count > 0)
            .map(|(k, b)| format!("{k}={}", b.count))
            .collect::<Vec<_>>()
            .join(" ");
        let fundable = canonical
            .capital
            .iter()
            .map(|c| format!("{}usd={}", c.budget_usd, c.fundable))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!(
            "C0-E1 CANONICAL ALL-PATH events: {events}; fundable: {fundable}; losses: {}; completeness={}",
            if losses.is_empty() { "none".to_string() } else { losses },
            r.technical_qualification.verdict
        ));
        lines.join("\n")
    }
    /// Human-readable authoritative MARKET conclusion next to the C0-E1 console
    /// summary. Read existing report data only; no RPC, scanner, or parallel ledger.
    fn market_terminal_summary(report: &SnapshotReport) -> String {
        let final_market = report
            .market
            .cumulative
            .c0_mo_r6
            .as_ref()
            .map(|m| &m.summary);
        let scan = final_market.map(|m| &m.full_slot_reconciliation);
        let count = |v: Option<u64>| v.map_or_else(|| "UNKNOWN".into(), |n| n.to_string());
        let canonical_complete = scan.is_some_and(|s| s.complete)
            && final_market.is_some_and(|m| m.full_slot_reconciliation_complete);
        let mut lines = vec![
            "================ MARKET FINAL (canonical all-path) ================".to_string(),
            format!(
                "status={} canonical_complete={} sample={}",
                final_market.map_or("UNKNOWN", |m| m.verdict.as_str()),
                canonical_complete,
                report.economic_sample_status
            ),
            format!(
                "capture: received={} success={} failed={}",
                count(final_market.map(|m| m.unique_transactions)),
                count(final_market.map(|m| m.successful_transactions)),
                count(final_market.map(|m| m.failed_transactions))
            ),
            format!(
                "finalized_slots: expected={} fetched={} skipped_confirmed={} failed={}",
                count(scan.map(|s| s.slots_expected)),
                count(scan.map(|s| s.slots_fetched)),
                count(scan.map(|s| s.slots_skipped_confirmed)),
                count(scan.map(|s| s.slots_failed))
            ),
        ];
        for key in [
            "ALL",
            "CLASSIC",
            "RECEIVERSHIP",
            "FORCED_DELEVERAGE",
            "BANKRUPTCY_SETTLEMENT",
        ] {
            let g = report.canonical.cumulative.market.get(key);
            let gross = g.map(|g| &g.gross_edge_usd);
            lines.push(format!(
                "{key}: events={} gross_known_usd={} gross_known_events={} gross_unknown_events={}",
                count(g.map(|g| g.events)),
                match gross {
                    Some(v) if v.count_known > 0 => v
                        .distribution
                        .total
                        .map_or_else(|| "UNKNOWN".into(), |n| format!("{n:.6}")),
                    _ => "UNKNOWN".to_string(),
                },
                count(gross.map(|v| v.count_known)),
                count(gross.map(|v| v.count_unknown))
            ));
        }
        lines.push(
            "gross_known_usd is NOT realized profit; unknown values are not zero.".to_string(),
        );
        lines.push("===============================================================".to_string());
        lines.join("\n")
    }

    fn run(mut self) -> Result<(), String> {
        loop {
            if let Some(ev) = self.rx.pop_timeout(Duration::from_millis(100)) {
                self.accept(ev)?;
            }
            while let Some(ev) = self.rx.try_pop() {
                self.accept(ev)?;
            }
            if let Some(cutoff) = self.h.cutoff() {
                let start = self.h.shared.start_ns.load(Ordering::Acquire);
                let n = self
                    .h
                    .cfg
                    .plan
                    .due(self.h.clock.now_ns().min(cutoff).saturating_sub(start));
                while self.snapshots < n {
                    let i = self.snapshots + 1;
                    self.write_snapshot(i, false)?;
                    self.snapshots = i;
                }
            }
            if self.h.shared.finish.load(Ordering::Acquire) {
                while let Some(ev) = self.rx.try_pop() {
                    self.accept(ev)?;
                }
                break;
            }
        }
        // The evaluation stream is complete only if no evidence was dropped before this worker.
        let complete = self.h.shared.drops.load(Ordering::Relaxed) == 0;
        self.evaluations
            .seal(complete)
            .map_err(|e| format!("SHADOW_JOURNAL_SEAL: {e}"))?;
        self.finalize_canonical()?;
        // Freeze one final telemetry snapshot, so hour 4 and final reconcile byte-for-byte.
        self.final_technical = Some(self.technical(true));
        // Reconcile delayed F8/qualification evidence atomically. No fifth snapshot.
        for i in 1..=self.snapshots {
            self.write_snapshot(i, true)?;
        }
        let final_report = self.report(SNAPSHOT_COUNT, true);
        self.append_typed_report(&final_report)?;
        let rows: Vec<OpportunityRow> = self
            .selected()
            .into_iter()
            .map(|(_, rec)| self.opportunity(rec))
            .collect();
        for row in &rows {
            self.append_typed_report(row)?;
        }
        self.reports
            .seal(true)
            .map_err(|e| format!("REPORT_JOURNAL_SEAL: {e}"))?;
        println!("{}", self.terminal_summary(&final_report));
        println!("{}", Self::market_terminal_summary(&final_report));
        crate::analytics::measurement::journal(
            &self.h.cfg.paths(),
            self.h.clock,
            crate::analytics::measurement::Phase::ReportsFinalized {
                technical_qualification: final_report.technical_qualification.verdict.clone(),
                snapshot_count: self.snapshots as u64,
                report: "economics_final.json".into(),
            },
        )?;
        if final_report.technical_qualification.verdict != "PASS" {
            return Err(
                "C0-E1 technical qualification incomplete; see economics_final.json".into(),
            );
        }
        Ok(())
    }
}

pub fn stage_intervals(rec: &OpportunityRecord) -> BTreeMap<&'static str, Option<u64>> {
    let t = &rec.timing;
    let sub = |a: Option<u64>, b: Option<u64>| b?.checked_sub(a?);
    BTreeMap::from([
        ("rx_to_state", sub(t.t_rx_ns, t.t_state_ns)),
        ("rx_to_detect", sub(t.t_rx_ns, t.t_detect_ns)),
        ("detect_to_ready", sub(t.t_detect_ns, t.t_ready_ns)),
        ("ready_to_sign", sub(t.t_ready_ns, t.t_sign_ns)),
        ("rx_to_sign", sub(t.t_rx_ns, t.t_sign_ns)),
    ])
}
/// Capture-counter delta between two market summaries (`c0_mo_counter_delta`).
fn counter_delta(
    current: Option<&crate::market_observer::MarketSummary>,
    prev: Option<&crate::market_observer::MarketSummary>,
) -> Option<BTreeMap<String, u64>> {
    let c = current?;
    let p: BTreeMap<&str, u64> = prev
        .map(|p| p.delta_counters().into_iter().collect())
        .unwrap_or_default();
    Some(
        c.delta_counters()
            .into_iter()
            .map(|(k, n)| {
                (
                    k.to_string(),
                    n.saturating_sub(p.get(k).copied().unwrap_or(0)),
                )
            })
            .collect(),
    )
}
struct Capital {
    usd: u64,
    matched: u64,
    fundable: EdgeCount,
    races: RacePool,
    capital_unknown: u64,
    economics_unknown: u64,
    working: Vec<f64>,
    ppc: Vec<f64>,
    ppc_zero_denominator: u64,
}
impl Capital {
    fn new(usd: u64) -> Self {
        Self {
            usd,
            matched: 0,
            fundable: EdgeCount::default(),
            races: RacePool::default(),
            capital_unknown: 0,
            economics_unknown: 0,
            working: vec![],
            ppc: vec![],
            ppc_zero_denominator: 0,
        }
    }
    fn add(&mut self, r: &OpportunityRecord, exec: bool, strong: bool, race: &RaceClass) {
        if !r.economics_known {
            self.economics_unknown += 1;
        }
        if exec && strong {
            self.matched += 1;
        }
        let cost = r.selected_costs.as_ref();
        let Some(budget) = cost.and_then(CandidateCostRecord::total_budget) else {
            self.capital_unknown += 1;
            return;
        };
        if !exec {
            return;
        }
        let cost = cost.unwrap();
        let working = cost.required_working_capital_usd_micro.unwrap();
        self.working.push(working as f64 / 1e6);
        if working == 0 {
            self.ppc_zero_denominator += 1;
        } else if let Some(v) = cost.profit_per_capital_bps {
            self.ppc.push(v as f64);
        }
        if budget <= self.usd * 1_000_000 {
            let net = r.net_profit.unwrap();
            self.fundable.add(net);
            self.races.add(
                if strong {
                    race
                } else {
                    &RaceClass::WinnerUnresolved
                },
                net,
            );
        }
    }
    fn report(self, seconds: f64) -> CapitalReport {
        CapitalReport {
            budget_usd: self.usd,
            market_matched_executable_opportunities: self.matched,
            fundable_known_opportunities: self.fundable.count,
            known_executable_net_edge_usd_micro: self.fundable.net_usd_micro,
            known_executable_net_edge_usd: self.fundable.net_usd_micro as f64 / 1e6,
            known_executable_net_edge_per_hour_usd: self.fundable.net_usd_micro as f64 / 1e6
                * 3600.0
                / seconds,
            race_classes: self.races,
            capital_unknown_count: self.capital_unknown,
            economics_unknown_count: self.economics_unknown,
            required_working_capital_usd: distribution(self.working),
            profit_per_capital_bps: distribution(self.ppc),
            profit_per_capital_zero_denominator_count: self.ppc_zero_denominator,
            concurrent_capital_allocation_proven: false,
        }
    }
}

#[cfg(test)]
#[path = "econ_tests.rs"]
mod tests;

/// Exposes the existing canonical selector/economics to offline reporting only.
pub fn replay_canonical(
    market: &Path,
    shadow: &Path,
    out: &Path,
    plan: &MeasurementPlan,
    start_ns: u64,
    max_seconds: u64,
    scope: &ReplayScope,
) -> Result<(), String> {
    canonical::replay(market, shadow, out, plan, start_ns, max_seconds, scope)
}
