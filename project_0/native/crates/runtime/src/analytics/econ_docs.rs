//! Typed C0-E1 analytics documents (JFI-FIX-05). The hourly snapshots (`ECON_SNAPSHOT`) and the
//! final report (`ECON_FINAL`) are these values in the report journal; `hour_0N.json` and
//! `economics_final.json` are their renders (`serde_json::to_value`: the historical keys and
//! numbers). Text (human-readable) serialization renders the historical document wherever a
//! section merges keys; the binary journal stores the plain fields.
use super::*;
use crate::analytics::measurement::CaptureWindow;
use crate::market::observer::truth::TruthSummary;
use crate::market::observer::{rows::ReconciliationRow, MarketFinal};
use serde::ser::Error as _;

/// One receiver of the market ranking.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RankedReceiver {
    pub receiver: String,
    pub count: u64,
    pub gross_edge_usd: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketMetrics {
    pub capture_counters: BTreeMap<String, u64>,
    pub liquidation_events: u64,
    pub gross_edge_usd: Distribution,
    pub gross_edge_per_hour_usd: f64,
    pub repaid_equity_usd: Distribution,
    pub seized_equity_usd: Distribution,
    pub gross_roi: Distribution,
    pub unique_receivers: u64,
    pub top_receiver: Option<String>,
    pub top_receiver_liquidation_share: Option<f64>,
    pub top_receiver_gross_edge_share: Option<f64>,
    pub top3_liquidation_share: Option<f64>,
    pub top3_gross_edge_share: Option<f64>,
    pub ranked_receivers: Vec<RankedReceiver>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShadowMetrics {
    pub evaluations: u64,
    pub opportunities: u64,
    pub known_executable: EdgeCount,
    pub stage_intervals_ns: BTreeMap<String, Distribution>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Matching {
    pub market_liquidations_total: u64,
    pub market_with_shadow_match: u64,
    pub market_without_shadow_match: u64,
    pub shadow_winner_resolved: u64,
    pub shadow_winner_unresolved: u64,
    pub winner_live_observation_matched: u64,
    pub winner_live_observation_missing: u64,
    pub winner_identity_mismatch: u64,
    pub same_slot_ambiguous: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EconomicsMetrics {
    pub known_executable: EdgeCount,
    pub known_executable_net_edge_per_hour_usd: f64,
    pub economics_unknown_count: u64,
    /// Never known from Shadow evidence (always `null`).
    pub realized_pnl: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapitalReport {
    pub budget_usd: u64,
    pub market_matched_executable_opportunities: u64,
    pub fundable_known_opportunities: u64,
    pub known_executable_net_edge_usd_micro: i128,
    pub known_executable_net_edge_usd: f64,
    pub known_executable_net_edge_per_hour_usd: f64,
    pub race_classes: RacePool,
    pub capital_unknown_count: u64,
    pub economics_unknown_count: u64,
    pub required_working_capital_usd: Distribution,
    pub profit_per_capital_bps: Distribution,
    pub profit_per_capital_zero_denominator_count: u64,
    pub concurrent_capital_allocation_proven: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sensitivity {
    pub speedup_ms: u64,
    pub winner_rx_boundary: EdgeCount,
    pub winner_slot_first_seen_boundary: EdgeCount,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RaceTiming {
    pub race_classes: RacePool,
    pub margin_to_winner_rx_ns: Distribution,
    pub margin_to_winner_rx_ms: Distribution,
    pub margin_to_winner_slot_seen_ns: Distribution,
    pub margin_to_winner_slot_seen_ms: Distribution,
    pub required_speedup_to_winner_rx_ns: Distribution,
    pub required_speedup_to_winner_rx_ms: Distribution,
    pub required_speedup_to_winner_slot_seen_ns: Distribution,
    pub required_speedup_to_winner_slot_seen_ms: Distribution,
    pub observed_speedup_sensitivity: Vec<Sensitivity>,
    pub late_stage_intervals_ns: BTreeMap<String, Distribution>,
    pub late_stage_net_edge_weighted_ns_micro_usd: BTreeMap<String, f64>,
    pub stage_latency_budget_sufficient_if_fully_removed: BTreeMap<String, EdgeCount>,
    pub largest_measured_late_stage: Option<String>,
    pub stage_removal_is_mathematical_ceiling_only: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Bottleneck {
    pub opportunity_count: u64,
    pub market_matched_count: u64,
    pub known_candidate_net_edge_usd_micro: i128,
    pub matched_market_gross_edge_usd: Option<f64>,
    pub percentage_of_known_rejected_executable_edge: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bottlenecks {
    pub by_reason: BTreeMap<String, Bottleneck>,
    pub ranking: Vec<String>,
    pub known_rejected_candidate_edge_pool_usd_micro: i128,
    pub denominator_note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unknowns {
    pub economics_unknown: u64,
    pub winner_unresolved: u64,
    pub live_winner_missing: u64,
    pub true_competitor_submission_deadline: String,
}

/// The legacy-receiver analytics of one window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub market: MarketMetrics,
    pub shadow: ShadowMetrics,
    pub matching: Matching,
    pub economics: EconomicsMetrics,
    pub capital: Vec<CapitalReport>,
    pub competition: RacePool,
    pub race_timing: RaceTiming,
    pub bottlenecks: Bottlenecks,
    pub unknowns: Unknowns,
}

// ------------------------------------------------------------------------------------------
// Remote and backlog telemetry (MT0 snapshot)
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hist {
    pub count: u64,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
    pub max: Option<u64>,
}
impl From<&darkbot_common::mt0::HistSnapshot> for Hist {
    fn from(h: &darkbot_common::mt0::HistSnapshot) -> Self {
        Self {
            count: h.count,
            p50: h.p50,
            p95: h.p95,
            p99: h.p99,
            max: h.max,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mt0Remote {
    pub class: String,
    pub provider: String,
    pub requests: u64,
    pub ok: u64,
    pub rate_limited_429: u64,
    pub timeout: u64,
    pub error: u64,
    pub unclassified: u64,
    pub slow_ge_500ms: u64,
    pub latency_ns: Hist,
}

/// MT0 registry snapshot (`mt0_version` 2), typed; ratios kept as `(num, den)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self")]
pub struct Mt0Telemetry {
    pub unix_s: u64,
    pub owner_busy_ns: Vec<(String, u64)>,
    pub owner_busy_total_ns: u64,
    pub owner_wait_total_ns: u64,
    pub owner_duty_cycle_total: (u64, u64),
    pub owner_duty_cycle_permille_1s: Hist,
    pub queue_wait_latest_ns: Vec<(String, Hist)>,
    pub queue_wait_first_ns: Vec<(String, Hist)>,
    pub ingress_queue_wait_ns: Hist,
    pub pending_current: u64,
    pub pending_max: u64,
    pub backlog_age_ns: Option<u64>,
    pub backlog_age_max_ns: Option<u64>,
    pub remote: Vec<Mt0Remote>,
    pub health_calls_sweep: u64,
    pub health_calls_detect_consumer: u64,
    pub health_calls_pipeline: u64,
    pub health_calls_per_pipeline_evaluation: (u64, u64),
    pub counters: Vec<(String, u64)>,
    pub jupiter_requests_per_routed_evaluation: (u64, u64),
    pub route_requotes: u64,
    pub fee_rpc_requests: u64,
    pub simulation_requests: u64,
    pub projection_requests: u64,
    pub winner_requests: u64,
    pub reconcile_requests: u64,
}

impl From<&darkbot_common::mt0::Snapshot> for Mt0Telemetry {
    fn from(s: &darkbot_common::mt0::Snapshot) -> Self {
        let named = |v: &[(&str, u64)]| v.iter().map(|(k, n)| (k.to_string(), *n)).collect();
        let hists = |v: &[(&str, darkbot_common::mt0::HistSnapshot)]| {
            v.iter().map(|(k, h)| (k.to_string(), h.into())).collect()
        };
        let ratio = |r: darkbot_common::mt0::Ratio| (r.num, r.den);
        Self {
            unix_s: s.unix_s,
            owner_busy_ns: named(&s.owner_busy_ns),
            owner_busy_total_ns: s.owner_busy_total_ns,
            owner_wait_total_ns: s.owner_wait_total_ns,
            owner_duty_cycle_total: ratio(s.owner_duty_cycle_total),
            owner_duty_cycle_permille_1s: (&s.owner_duty_cycle_permille_1s).into(),
            queue_wait_latest_ns: hists(&s.queue_wait_latest_ns),
            queue_wait_first_ns: hists(&s.queue_wait_first_ns),
            ingress_queue_wait_ns: (&s.ingress_queue_wait_ns).into(),
            pending_current: s.pending_current,
            pending_max: s.pending_max,
            backlog_age_ns: s.backlog_age_ns,
            backlog_age_max_ns: s.backlog_age_max_ns,
            remote: s
                .remote
                .iter()
                .map(|r| Mt0Remote {
                    class: r.class.into(),
                    provider: r.provider.into(),
                    requests: r.requests,
                    ok: r.ok,
                    rate_limited_429: r.rate_limited_429,
                    timeout: r.timeout,
                    error: r.error,
                    unclassified: r.unclassified,
                    slow_ge_500ms: r.slow_ge_500ms,
                    latency_ns: (&r.latency_ns).into(),
                })
                .collect(),
            health_calls_sweep: s.health_calls_sweep,
            health_calls_detect_consumer: s.health_calls_detect_consumer,
            health_calls_pipeline: s.health_calls_pipeline,
            health_calls_per_pipeline_evaluation: ratio(s.health_calls_per_pipeline_evaluation),
            counters: named(&s.counters),
            jupiter_requests_per_routed_evaluation: ratio(s.jupiter_requests_per_routed_evaluation),
            route_requotes: s.route_requotes,
            fee_rpc_requests: s.fee_rpc_requests,
            simulation_requests: s.simulation_requests,
            projection_requests: s.projection_requests,
            winner_requests: s.winner_requests,
            reconcile_requests: s.reconcile_requests,
        }
    }
}

impl Mt0Telemetry {
    /// The historical object: the MT0 text parsed as JSON (ratios keep their six-decimal text).
    pub fn legacy_json(&self) -> Result<Value, String> {
        let ratio = |(num, den): (u64, u64)| -> Result<Value, String> {
            if den == 0 {
                return Ok(Value::Null);
            }
            format!("{:.6}", num as f64 / den as f64)
                .parse::<serde_json::Number>()
                .map(Value::Number)
                .map_err(|e| e.to_string())
        };
        let hist = |h: &Hist| serde_json::to_value(h).map_err(|e| e.to_string());
        let hists = |v: &[(String, Hist)]| -> Result<Value, String> {
            let mut m = serde_json::Map::new();
            for (k, h) in v {
                m.insert(k.clone(), hist(h)?);
            }
            Ok(Value::Object(m))
        };
        let mut m = serde_json::Map::new();
        m.insert("mt0_version".into(), 2u64.into());
        m.insert("unix_s".into(), self.unix_s.into());
        m.insert(
            "owner_busy_ns".into(),
            Value::Object(
                self.owner_busy_ns
                    .iter()
                    .map(|(k, n)| (k.clone(), (*n).into()))
                    .collect(),
            ),
        );
        m.insert(
            "owner_busy_total_ns".into(),
            self.owner_busy_total_ns.into(),
        );
        m.insert(
            "owner_wait_total_ns".into(),
            self.owner_wait_total_ns.into(),
        );
        m.insert(
            "owner_duty_cycle_total".into(),
            ratio(self.owner_duty_cycle_total)?,
        );
        m.insert(
            "owner_duty_cycle_permille_1s".into(),
            hist(&self.owner_duty_cycle_permille_1s)?,
        );
        m.insert(
            "queue_wait_latest_ns".into(),
            hists(&self.queue_wait_latest_ns)?,
        );
        m.insert(
            "queue_wait_first_ns".into(),
            hists(&self.queue_wait_first_ns)?,
        );
        m.insert(
            "ingress_queue_wait_ns".into(),
            hist(&self.ingress_queue_wait_ns)?,
        );
        m.insert("pending_current".into(), self.pending_current.into());
        m.insert("pending_max".into(), self.pending_max.into());
        m.insert("backlog_age_ns".into(), self.backlog_age_ns.into());
        m.insert("backlog_age_max_ns".into(), self.backlog_age_max_ns.into());
        let mut remote = serde_json::Map::new();
        for r in &self.remote {
            let mut v = serde_json::to_value(r).map_err(|e| e.to_string())?;
            v.as_object_mut().map(|o| o.remove("class"));
            remote.insert(r.class.clone(), v);
        }
        m.insert("remote".into(), Value::Object(remote));
        m.insert(
            "health_calls".into(),
            json!({"sweep": self.health_calls_sweep, "detect_consumer": self.health_calls_detect_consumer,
                "pipeline": self.health_calls_pipeline}),
        );
        m.insert(
            "health_calls_per_pipeline_evaluation".into(),
            ratio(self.health_calls_per_pipeline_evaluation)?,
        );
        for (k, n) in &self.counters {
            m.insert(k.clone(), (*n).into());
        }
        m.insert(
            "jupiter_requests_per_routed_evaluation".into(),
            ratio(self.jupiter_requests_per_routed_evaluation)?,
        );
        for (k, n) in [
            ("route_requotes", self.route_requotes),
            ("fee_rpc_requests", self.fee_rpc_requests),
            ("simulation_requests", self.simulation_requests),
            ("projection_requests", self.projection_requests),
            ("winner_requests", self.winner_requests),
            ("reconcile_requests", self.reconcile_requests),
        ] {
            m.insert(k.into(), n.into());
        }
        m.insert(
            "not_measured".into(),
            Value::Object(
                darkbot_common::mt0::NOT_MEASURED
                    .iter()
                    .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
                    .collect(),
            ),
        );
        Ok(Value::Object(m))
    }
}

/// Text formats render the historical document; the binary journal stores the fields.
macro_rules! text_render {
    ($t:ty, $name:literal) => {
        impl Serialize for $t {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                if s.is_human_readable() {
                    self.legacy_json().map_err(S::Error::custom)?.serialize(s)
                } else {
                    <$t>::serialize(self, s)
                }
            }
        }
        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                use serde::de::Error;
                if d.is_human_readable() {
                    return Err(D::Error::custom($name));
                }
                <$t>::deserialize(d)
            }
        }
    };
}
text_render!(Mt0Telemetry, "MT0_TELEMETRY_TEXT_NOT_SUPPORTED");

// ------------------------------------------------------------------------------------------
// Technical qualification
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Technical {
    pub verdict: String,
    pub requested_duration_seconds: u64,
    pub actual_duration_seconds: Option<f64>,
    pub clean_automatic_shutdown: bool,
    pub market_capture_complete: bool,
    pub four_snapshots_present: bool,
    pub snapshot_write_success: u64,
    pub snapshot_write_failures: u64,
    pub analytics_queue_capacity: u64,
    pub analytics_queue_max_depth: u64,
    pub analytics_queue_drops: u64,
    pub shadow_evaluations_received: u64,
    pub shadow_evaluations_streamed: u64,
    pub shadow_evaluations_coalesced: u64,
    pub shadow_semantic_entries_current: u64,
    pub shadow_semantic_entries_max: u64,
    pub shadow_semantic_evictions: u64,
    pub shadow_journal_target_heads: u64,
    pub retention_capacity: u64,
    pub retention_overflow: u64,
    pub retention_bytes_current: u64,
    pub retention_bytes_peak: u64,
    pub retention_byte_limit: u64,
    pub shadow_ready_at_ns: u64,
    pub startup_excluded_from_measurement: bool,
    pub shadow_evaluations_in_window: u64,
    pub shadow_runtime_terminated_early: bool,
    pub shadow_runtime_finished: bool,
    pub program_upgrade_halt: bool,
    pub submission_attempts_blocked: u64,
    pub submissions: u64,
    pub live_ready: bool,
    pub source_coverage_verified: bool,
    pub final_reconciliation_complete: bool,
    pub shadow_telemetry: Option<ShadowStats>,
    pub remote_and_backlog_telemetry: Option<Mt0Telemetry>,
}

// ------------------------------------------------------------------------------------------
// Snapshot / final report document
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    pub schema_version: String,
    pub run_id: String,
    pub source_commit: String,
    pub marginfi_pin: String,
    pub mode: String,
    pub market_source: String,
    pub provider_class: String,
    pub commitment: String,
    pub started_at_utc: String,
    pub ended_at_utc: String,
    pub actual_elapsed_seconds: f64,
    pub snapshot_index: u64,
    pub measurement_plan: MeasurementPlan,
    pub snapshot_interval_seconds: f64,
    pub complete: bool,
    pub live_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub from_offset_ns: u64,
    pub through_offset_ns: u64,
    pub from_seconds: f64,
    pub through_seconds: f64,
    pub provisional: bool,
    pub reconciled_after_drain: bool,
    pub join_assignment: String,
    pub coalescing: String,
    pub backfill_hour_assignment: String,
}

/// One analytics section: the snapshot window and everything since the capture start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section<T> {
    pub window_delta: T,
    pub cumulative: T,
}
impl<T> Section<T> {
    /// The same section in another (migrated) layout.
    fn migrate<U: From<T>>(self) -> Section<U> {
        Section {
            window_delta: self.window_delta.into(),
            cumulative: self.cumulative.into(),
        }
    }
}

/// Market window delta: the metrics plus the capture-counter delta between checkpoints.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(remote = "Self")]
pub struct MarketDelta {
    pub metrics: MarketMetrics,
    pub c0_mo_counter_delta: Option<BTreeMap<String, u64>>,
    pub counter_attribution: String,
}
impl MarketDelta {
    pub fn legacy_json(&self) -> Result<Value, String> {
        let mut v = serde_json::to_value(&self.metrics).map_err(|e| e.to_string())?;
        v["c0_mo_counter_delta"] =
            serde_json::to_value(&self.c0_mo_counter_delta).map_err(|e| e.to_string())?;
        v["counter_attribution"] = Value::String(self.counter_attribution.clone());
        Ok(v)
    }
}
text_render!(MarketDelta, "MARKET_DELTA_TEXT_NOT_SUPPORTED");

/// Market cumulative: the metrics plus the market summary at the snapshot boundary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(remote = "Self")]
pub struct MarketCumulative {
    pub metrics: MarketMetrics,
    pub c0_mo_r6: Option<MarketFinal>,
    pub c0_mo_checkpoint_qualification: String,
}
impl MarketCumulative {
    pub fn legacy_json(&self) -> Result<Value, String> {
        let mut v = serde_json::to_value(&self.metrics).map_err(|e| e.to_string())?;
        v["c0_mo_r6"] = serde_json::to_value(&self.c0_mo_r6).map_err(|e| e.to_string())?;
        v["c0_mo_checkpoint_qualification"] =
            Value::String(self.c0_mo_checkpoint_qualification.clone());
        Ok(v)
    }
}
text_render!(MarketCumulative, "MARKET_CUMULATIVE_TEXT_NOT_SUPPORTED");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketSection {
    pub scope: String,
    pub window_delta: MarketDelta,
    pub cumulative: MarketCumulative,
}

/// The market's own canonical view at the final summary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalTruthRef {
    pub full_window_summary: TruthSummary,
    pub reconciliation: ReconciliationRow,
    pub hourly_attribution_qualified: bool,
    pub market_shadow_join_qualified: bool,
}

/// One hourly snapshot or the final report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotReport {
    pub measurement_plan: MeasurementPlan,
    pub capture_window: Option<CaptureWindow>,
    pub identity: Identity,
    pub window: Window,
    pub technical_qualification: Technical,
    pub non_claims: Vec<String>,
    pub economic_sample_status: String,
    pub market_edge_status: String,
    pub executable_edge_status: String,
    pub competition_evidence_status: String,
    pub live_ready: bool,
    pub market: MarketSection,
    pub shadow: Section<ShadowMetrics>,
    pub matching: Section<Matching>,
    pub economics: Section<EconomicsMetrics>,
    pub capital: Section<Vec<CapitalReport>>,
    pub competition: Section<RacePool>,
    pub race_timing: Section<RaceTiming>,
    pub bottlenecks: Section<Bottlenecks>,
    pub unknowns: Section<Unknowns>,
    pub canonical_market_truth: Option<CanonicalTruthRef>,
    pub canonical: Section<CanonicalSummary>,
    pub authoritative_analytics: String,
    /// A2: race figures (winner-slot margin as the main figure, rx margin status).
    pub race_margins: Section<RaceMargins>,
}
/// Layout of schema 2 (before A2): no `race_margins` section.
#[derive(Deserialize)]
pub(super) struct SnapshotReportV2 {
    measurement_plan: MeasurementPlan,
    capture_window: Option<CaptureWindow>,
    identity: Identity,
    window: Window,
    technical_qualification: Technical,
    non_claims: Vec<String>,
    economic_sample_status: String,
    market_edge_status: String,
    executable_edge_status: String,
    competition_evidence_status: String,
    live_ready: bool,
    market: MarketSection,
    shadow: Section<ShadowMetrics>,
    matching: Section<Matching>,
    economics: Section<EconomicsMetrics>,
    capital: Section<Vec<CapitalReport>>,
    competition: Section<RacePool>,
    race_timing: Section<RaceTiming>,
    bottlenecks: Section<Bottlenecks>,
    unknowns: Section<Unknowns>,
    canonical_market_truth: Option<CanonicalTruthRef>,
    canonical: Section<CanonicalSummaryV3>,
    authoritative_analytics: String,
}
impl From<SnapshotReportV2> for SnapshotReport {
    fn from(v: SnapshotReportV2) -> Self {
        Self {
            measurement_plan: v.measurement_plan,
            capture_window: v.capture_window,
            identity: v.identity,
            window: v.window,
            technical_qualification: v.technical_qualification,
            non_claims: v.non_claims,
            economic_sample_status: v.economic_sample_status,
            market_edge_status: v.market_edge_status,
            executable_edge_status: v.executable_edge_status,
            competition_evidence_status: v.competition_evidence_status,
            live_ready: v.live_ready,
            market: v.market,
            shadow: v.shadow,
            matching: v.matching,
            economics: v.economics,
            capital: v.capital,
            competition: v.competition,
            race_timing: v.race_timing,
            bottlenecks: v.bottlenecks,
            unknowns: v.unknowns,
            canonical_market_truth: v.canonical_market_truth,
            canonical: v.canonical.migrate(),
            authoritative_analytics: v.authoritative_analytics,
            race_margins: Section {
                window_delta: RaceMargins::earlier_schema(),
                cumulative: RaceMargins::earlier_schema(),
            },
        }
    }
}
/// Layout of schema 3 (A2, before N1/N5): canonical summary and race margins without the
/// attribution model and the technical figures.
#[derive(Deserialize)]
pub(super) struct SnapshotReportV3 {
    measurement_plan: MeasurementPlan,
    capture_window: Option<CaptureWindow>,
    identity: Identity,
    window: Window,
    technical_qualification: Technical,
    non_claims: Vec<String>,
    economic_sample_status: String,
    market_edge_status: String,
    executable_edge_status: String,
    competition_evidence_status: String,
    live_ready: bool,
    market: MarketSection,
    shadow: Section<ShadowMetrics>,
    matching: Section<Matching>,
    economics: Section<EconomicsMetrics>,
    capital: Section<Vec<CapitalReport>>,
    competition: Section<RacePool>,
    race_timing: Section<RaceTiming>,
    bottlenecks: Section<Bottlenecks>,
    unknowns: Section<Unknowns>,
    canonical_market_truth: Option<CanonicalTruthRef>,
    canonical: Section<CanonicalSummaryV3>,
    authoritative_analytics: String,
    race_margins: Section<RaceMarginsV3>,
}
impl From<SnapshotReportV3> for SnapshotReport {
    fn from(v: SnapshotReportV3) -> Self {
        Self {
            measurement_plan: v.measurement_plan,
            capture_window: v.capture_window,
            identity: v.identity,
            window: v.window,
            technical_qualification: v.technical_qualification,
            non_claims: v.non_claims,
            economic_sample_status: v.economic_sample_status,
            market_edge_status: v.market_edge_status,
            executable_edge_status: v.executable_edge_status,
            competition_evidence_status: v.competition_evidence_status,
            live_ready: v.live_ready,
            market: v.market,
            shadow: v.shadow,
            matching: v.matching,
            economics: v.economics,
            capital: v.capital,
            competition: v.competition,
            race_timing: v.race_timing,
            bottlenecks: v.bottlenecks,
            unknowns: v.unknowns,
            canonical_market_truth: v.canonical_market_truth,
            canonical: v.canonical.migrate(),
            authoritative_analytics: v.authoritative_analytics,
            race_margins: v.race_margins.migrate(),
        }
    }
}
/// N1/N5: schema 4; schema 2 and 3 frames are read through their own explicit layouts.
fn decode_report_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<SnapshotReport>> {
    use crate::analytics::payload::{decode_wire, TYPED};
    match schema {
        TYPED => Some(decode_wire::<SnapshotReportV2>(bytes).map(Into::into)),
        3 => Some(decode_wire::<SnapshotReportV3>(bytes).map(Into::into)),
        _ => None,
    }
}
impl crate::analytics::payload::Payload for SnapshotReport {
    const KIND: u16 = kind::ECON_FINAL;
    const SCHEMA: u16 = 4;
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<Self>> {
        decode_report_earlier(schema, bytes)
    }
}

/// `ECON_SNAPSHOT` frame: the report of one boundary (the newest frame per index is the view).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotFrame {
    pub index: u64,
    pub finalizing: bool,
    pub report: SnapshotReport,
}
/// Layout of schema 2 (before A2) and 3 (before N1/N5): the frame around that report layout.
#[derive(Deserialize)]
struct SnapshotFrameEarlier<R> {
    index: u64,
    finalizing: bool,
    report: R,
}
impl<R: Into<SnapshotReport>> SnapshotFrameEarlier<R> {
    fn migrate(self) -> SnapshotFrame {
        SnapshotFrame {
            index: self.index,
            finalizing: self.finalizing,
            report: self.report.into(),
        }
    }
}
impl crate::analytics::payload::Payload for SnapshotFrame {
    const KIND: u16 = kind::ECON_SNAPSHOT;
    const SCHEMA: u16 = 4;
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<Self>> {
        use crate::analytics::payload::{decode_wire, TYPED};
        match schema {
            TYPED => Some(
                decode_wire::<SnapshotFrameEarlier<SnapshotReportV2>>(bytes)
                    .map(SnapshotFrameEarlier::migrate),
            ),
            3 => Some(
                decode_wire::<SnapshotFrameEarlier<SnapshotReportV3>>(bytes)
                    .map(SnapshotFrameEarlier::migrate),
            ),
            _ => None,
        }
    }
}

/// Render of a typed report document (`hour_0N.json`, `economics_final.json`).
pub fn render<T: Serialize>(doc: &T) -> Result<Value, String> {
    serde_json::to_value(doc).map_err(|e| e.to_string())
}
