//! Market summary and report serialization.
use super::*;
use serde::{Deserialize, Serialize};

/// Run facts the core cannot know itself.
#[derive(Clone, Debug, Default)]
pub struct SummaryContext {
    pub run_id: String,
    pub commitment: String,
    pub process_started_at: String,
    pub stream_ready_at: Option<String>,
    pub observation_started_at: Option<String>,
    pub observation_ended_at: Option<String>,
    pub configured_duration_seconds: f64,
    pub actual_observation_seconds: f64,
    pub stream_ready: bool,
    pub full_duration_observed: bool,
    pub drain_complete: bool,
    pub min_sample_liquidations: u64,
    pub enrichment_available: bool,
}

pub use crate::analytics::market_report::render_report;

/// One receiver in the summary ranking.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReceiverShare {
    pub receiver: String,
    pub liquidation_events: u64,
    pub gross_edge_total_usd: f64,
    pub liquidation_share: Option<f64>,
    pub gross_edge_share: Option<f64>,
}

/// Program deployment continuity across the capture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeploymentContinuity {
    pub before: Option<super::paths::Deployment>,
    pub after: Option<super::paths::Deployment>,
    pub error: Option<String>,
    pub source_pin: String,
    pub bytecode_source_equivalence: String,
}

/// The market observer summary (JFI-FIX-05): computed typed by the core, handed typed to the
/// analytics worker and rendered as `summary.json` (`serde_json::to_value`, identical keys).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketSummary {
    pub title: String,
    pub run_id: String,
    pub source: String,
    pub commitment: String,
    pub program: String,
    pub process_started_at: String,
    pub stream_ready_at: Option<String>,
    pub observation_started_at: Option<String>,
    pub observation_ended_at: Option<String>,
    pub configured_duration_seconds: f64,
    pub actual_observation_seconds: f64,
    pub verdict: String,
    pub market_capture_complete: bool,
    pub receivership_transport_capture_complete: bool,
    pub full_slot_reconciliation_complete: bool,
    pub full_slot_reconciliation: super::rows::ReconciliationRow,
    pub canonical_market_truth: super::truth::TruthSummary,
    pub liquidation_path_coverage_complete: bool,
    pub liquidation_count_scope: String,
    pub live_source_coverage_verified: bool,
    pub deployment_continuity: DeploymentContinuity,
    pub completeness_authority: String,
    pub transport_issues_are_telemetry: bool,
    pub transport_incomplete_reasons: Vec<String>,
    pub coverage_complete: bool,
    pub decode_complete: bool,
    pub drain_complete: bool,
    pub live_coverage_status: String,
    pub capture_clock_complete: bool,
    pub verification_status: String,
    pub source_coverage_verified: bool,
    pub source_coverage_note: String,
    pub incomplete_reasons: Vec<String>,
    pub first_observed_slot: Option<u64>,
    pub last_observed_slot: Option<u64>,
    pub transactions_received: u64,
    pub unique_transactions: u64,
    pub duplicate_transactions: u64,
    pub successful_transactions: u64,
    pub failed_transactions: u64,
    pub unknown_status_transactions: u64,
    pub liquidation_transactions: u64,
    pub liquidation_events: u64,
    pub liquidations_per_hour: Option<f64>,
    pub gross_edge_total_usd: f64,
    pub gross_edge_per_hour_usd: Option<f64>,
    pub gross_edge_avg_usd: Option<f64>,
    pub gross_edge_per_liquidation_usd: Option<f64>,
    pub sample_status: String,
    pub sample_min_liquidation_events: u64,
    pub unique_targets: u64,
    pub enrichment_available: bool,
    pub gross_edge_median_usd: Option<f64>,
    pub gross_edge_p90_usd: Option<f64>,
    pub gross_edge_p99_usd: Option<f64>,
    pub gross_edge_max_usd: Option<f64>,
    pub gross_edge_min_usd: Option<f64>,
    pub quantile_method: String,
    pub protocol_flat_fee_total_lamports: u64,
    pub tx_fee_total_lamports: u64,
    pub tx_fee_unknown_transactions: u64,
    pub fee_totals_complete: bool,
    pub enrichment_complete: bool,
    pub enrichment_requested: u64,
    pub enrichment_succeeded: u64,
    pub enrichment_failed: u64,
    pub enrichment_skipped_queue_full: u64,
    pub enrichment_init_error: Option<String>,
    pub rpc_fallback_count: u64,
    pub unique_receivers: u64,
    pub top_receiver: Option<String>,
    pub top_receiver_liquidation_share: Option<f64>,
    pub top_receiver_gross_edge_share: Option<f64>,
    pub top3_receiver_liquidation_share: Option<f64>,
    pub top3_receiver_gross_edge_share: Option<f64>,
    pub gap_count: u64,
    pub gap_intervals: u64,
    pub gap_unrecovered: u64,
    pub coverage_outage_seconds: Option<f64>,
    pub transport_covered_seconds: Option<f64>,
    pub transport_coverage_provenance: String,
    pub last_live_slot_age_s: Option<f64>,
    pub gap_count_raw: u64,
    pub gaps_merged: u64,
    pub provider_budget: darkbot_solana::provider::ProviderSnapshot,
    pub gap_transactions_deduplicated: u64,
    pub gap_transactions_recovered: u64,
    pub gap_liquidation_transactions: u64,
    pub gap_liquidation_events: u64,
    pub gaps: Vec<super::gap::MarketGap>,
    pub reconnect_count: u64,
    pub reconnect_attempts: u64,
    pub reconnect_successful: u64,
    pub wss_http_429_count: u64,
    pub ingress_overflow_count: u64,
    pub fatal_count: u64,
    pub decoder_error_count: u64,
    pub decoder_errors_successful_tx: u64,
    pub base64_error_count: u64,
    pub foreign_program_events: u64,
    pub failed_tx_receiver_events_ignored: u64,
    pub logs_truncated: u64,
    pub logs_truncated_total: u64,
    pub logs_truncated_qualified: u64,
    pub logs_truncated_unqualified: u64,
    pub logs_truncated_liquidation_mismatch: u64,
    pub truncated_instruction_end_liquidations: u64,
    pub truncated_log_end_liquidations: u64,
    pub truncated_unqualified_reasons: BTreeMap<String, u64>,
    pub log_messages_missing: u64,
    pub events_outside_observation: u64,
    pub events_after_cutoff: u64,
    pub slot_notifications: u64,
    pub max_stream_silence_ms: u64,
    pub receivers: Vec<ReceiverShare>,
    pub non_claims: Vec<String>,
}

impl MarketSummary {
    /// The capture counters whose window delta the analytics reports (`c0_mo_counter_delta`):
    /// counts, transaction totals, truncation, enrichment and cutoff counters.
    pub fn delta_counters(&self) -> [(&'static str, u64); 32] {
        [
            ("base64_error_count", self.base64_error_count),
            ("decoder_error_count", self.decoder_error_count),
            ("duplicate_transactions", self.duplicate_transactions),
            ("enrichment_failed", self.enrichment_failed),
            ("enrichment_requested", self.enrichment_requested),
            (
                "enrichment_skipped_queue_full",
                self.enrichment_skipped_queue_full,
            ),
            ("enrichment_succeeded", self.enrichment_succeeded),
            ("events_after_cutoff", self.events_after_cutoff),
            ("failed_transactions", self.failed_transactions),
            ("fatal_count", self.fatal_count),
            ("gap_count", self.gap_count),
            (
                "gap_liquidation_transactions",
                self.gap_liquidation_transactions,
            ),
            (
                "gap_transactions_deduplicated",
                self.gap_transactions_deduplicated,
            ),
            (
                "gap_transactions_recovered",
                self.gap_transactions_recovered,
            ),
            ("ingress_overflow_count", self.ingress_overflow_count),
            ("liquidation_events", self.liquidation_events),
            ("liquidation_transactions", self.liquidation_transactions),
            ("logs_truncated", self.logs_truncated),
            (
                "logs_truncated_liquidation_mismatch",
                self.logs_truncated_liquidation_mismatch,
            ),
            ("logs_truncated_qualified", self.logs_truncated_qualified),
            ("logs_truncated_total", self.logs_truncated_total),
            (
                "logs_truncated_unqualified",
                self.logs_truncated_unqualified,
            ),
            ("reconnect_count", self.reconnect_count),
            ("rpc_fallback_count", self.rpc_fallback_count),
            ("successful_transactions", self.successful_transactions),
            ("transactions_received", self.transactions_received),
            (
                "truncated_instruction_end_liquidations",
                self.truncated_instruction_end_liquidations,
            ),
            (
                "truncated_log_end_liquidations",
                self.truncated_log_end_liquidations,
            ),
            (
                "tx_fee_unknown_transactions",
                self.tx_fee_unknown_transactions,
            ),
            ("unique_transactions", self.unique_transactions),
            (
                "unknown_status_transactions",
                self.unknown_status_transactions,
            ),
            ("wss_http_429_count", self.wss_http_429_count),
        ]
    }
}

/// The summary a finished capture hands to the analytics worker and renders as `summary.json`:
/// the core summary plus, for an econ4h capture, its plan and window, and the journal binding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(remote = "Self")]
pub struct MarketFinal {
    pub summary: MarketSummary,
    /// Measurement plan and journaled capture window of an econ4h capture.
    pub econ: Option<(
        crate::analytics::measurement::MeasurementPlan,
        Option<crate::analytics::measurement::CaptureWindow>,
    )>,
    pub evidence_journal: Option<MarketJournalBinding>,
}

impl crate::analytics::payload::Payload for MarketFinal {
    const KIND: u16 = crate::analytics::journal::kind::MARKET_SUMMARY;
}

impl MarketFinal {
    /// A checkpoint summary (no plan, window or binding keys).
    pub fn checkpoint(summary: MarketSummary) -> Self {
        Self {
            summary,
            econ: None,
            evidence_journal: None,
        }
    }
    /// `summary.json`: the summary keys, plus `measurement_plan`/`capture_window` and
    /// `evidence_journal` only when present (the historical document).
    pub fn legacy_json(&self) -> Result<Value, String> {
        let mut v = serde_json::to_value(&self.summary).map_err(|e| e.to_string())?;
        if let Some((plan, window)) = &self.econ {
            v["measurement_plan"] = serde_json::to_value(plan).map_err(|e| e.to_string())?;
            v["capture_window"] = serde_json::to_value(window).map_err(|e| e.to_string())?;
        }
        if let Some(binding) = &self.evidence_journal {
            v["evidence_journal"] = serde_json::to_value(binding).map_err(|e| e.to_string())?;
        }
        Ok(v)
    }
}

/// Text formats render the historical document; the binary journal stores the fields.
impl Serialize for MarketFinal {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;
        if s.is_human_readable() {
            self.legacy_json().map_err(S::Error::custom)?.serialize(s)
        } else {
            MarketFinal::serialize(self, s)
        }
    }
}
impl<'de> Deserialize<'de> for MarketFinal {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        if d.is_human_readable() {
            return Err(D::Error::custom("MARKET_FINAL_TEXT_NOT_SUPPORTED"));
        }
        MarketFinal::deserialize(d)
    }
}

/// Single-value journal of the finished capture's typed market summary (`MARKET_SUMMARY`).
pub const MARKET_SUMMARY_JOURNAL: &str = "market_summary.p0j";

/// Finalise: write any still-deferred liquidation, STOPPED, seal, the typed market summary and
/// `final_report.txt` (text). `summary.json` is an explicit offline render only (JFI-03).
pub fn finalize(
    core: &mut ObserverCore,
    sink: &mut dyn MarketSink,
    ctx: &SummaryContext,
    run_dir: &Path,
    now_unix_s: u64,
    stopped: super::rows::WindowEvent,
) -> Result<(MarketFinal, String), String> {
    core.write_deferred_unenriched(sink, "NOT_ENRICHED")
        .map_err(|e| format!("liquidations write: {e}"))?;
    core.fail_truncated_pending("NOT_QUALIFIED");
    let write_ok = matches!(
        &stopped,
        super::rows::WindowEvent::Stopped {
            write_error: None,
            ..
        }
    );
    core.coverage(sink, now_unix_s, None, stopped)
        .map_err(|e| format!("coverage write: {e}"))?;
    // D1: the capture stream is complete only after the full TIMER window with a completed drain,
    // no write error and no persistent incomplete reason (gap, overflow, fatal, abort, ...).
    let complete = ctx.full_duration_observed
        && ctx.drain_complete
        && write_ok
        && core.incomplete_reasons.is_empty();
    // A journal-backed sink seals here; no view is rendered.
    let binding = sink
        .finish(complete)
        .map_err(|e| format!("evidence journal: {e}"))?;
    // Typed qualification axes first (B7: the native report never reads summary.json).
    crate::analytics::journal::write_typed_value(
        &run_dir.join(super::rows::MARKET_COMPLETION_JOURNAL),
        crate::analytics::journal::source::MARKET,
        &core.completion(ctx),
    )
    .map_err(|e| format!("market completion: {e}"))?;
    let econ = match &core.econ {
        Some(h) => Some((
            h.cfg.plan.clone(),
            match h.cutoff() {
                Some(cutoff) => Some(h.cfg.plan.capture_window(
                    h.clock,
                    cutoff - h.cfg.plan.duration_seconds * crate::analytics::measurement::NS,
                )?),
                None => None,
            },
        )),
        None => None,
    };
    let summary = MarketFinal {
        summary: core.summary(ctx),
        econ,
        evidence_journal: binding.clone(),
    };
    let summary_path = run_dir.join(MARKET_SUMMARY_JOURNAL);
    crate::analytics::journal::write_typed_value(
        &summary_path,
        crate::analytics::journal::source::MARKET,
        &summary,
    )
    .map_err(|e| format!("market summary: {e}"))?;
    let mut files = vec![("market_summary_journal", summary_path.clone())];
    if binding.is_some() {
        files.insert(0, ("market_journal", run_dir.join(MARKET_JOURNAL)));
    }
    let refs: Vec<(&str, &Path)> = files.iter().map(|(n, p)| (*n, p.as_path())).collect();
    let report = render_report(&summary, &refs);
    fs::write(run_dir.join("final_report.txt"), &report)
        .map_err(|e| format!("final_report.txt: {e}"))?;
    Ok((summary, report))
}
