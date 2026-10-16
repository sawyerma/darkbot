//! Pure rendering of finalized market summaries; no capture or trading admission.
use serde_json::Value;
use std::path::Path;

const REPORT_KEYS: &[&str] = &[
    "run_id",
    "source",
    "commitment",
    "program",
    "configured_duration_seconds",
    "measurement_plan",
    "capture_window",
    "actual_observation_seconds",
    "verdict",
    "market_capture_complete",
    "coverage_complete",
    "decode_complete",
    "drain_complete",
    "source_coverage_verified",
    "source_coverage_note",
    "incomplete_reasons",
    "first_observed_slot",
    "last_observed_slot",
    "transactions_received",
    "unique_transactions",
    "duplicate_transactions",
    "successful_transactions",
    "failed_transactions",
    "liquidation_transactions",
    "liquidation_events",
    "sample_status",
    "liquidations_per_hour",
    "unique_targets",
    "gross_edge_total_usd",
    "gross_edge_per_hour_usd",
    "gross_edge_per_liquidation_usd",
    "gross_edge_median_usd",
    "gross_edge_p90_usd",
    "gross_edge_p99_usd",
    "gross_edge_max_usd",
    "gross_edge_min_usd",
    "protocol_flat_fee_total_lamports",
    "tx_fee_total_lamports",
    "fee_totals_complete",
    "enrichment_available",
    "enrichment_complete",
    "enrichment_requested",
    "enrichment_succeeded",
    "enrichment_failed",
    "unique_receivers",
    "top_receiver",
    "top_receiver_liquidation_share",
    "top_receiver_gross_edge_share",
    "top3_receiver_liquidation_share",
    "top3_receiver_gross_edge_share",
    "gap_count",
    "reconnect_count",
    "ingress_overflow_count",
    "decoder_error_count",
    "logs_truncated_total",
    "logs_truncated_qualified",
    "logs_truncated_unqualified",
    "logs_truncated_liquidation_mismatch",
    "truncated_instruction_end_liquidations",
    "truncated_log_end_liquidations",
    "truncated_unqualified_reasons",
    "rpc_fallback_count",
    "events_after_cutoff",
    "max_stream_silence_ms",
];

pub fn render_report(summary: &Value, files: &[(&str, &Path)]) -> String {
    let rule = "=".repeat(72);
    let mut out = format!("{rule}\nPROJECT 0 MARKET OBSERVER — FINAL\n\n");
    for k in REPORT_KEYS {
        let v = summary.get(*k).cloned().unwrap_or(Value::Null);
        let shown = match v {
            Value::String(s) => s,
            Value::Number(n) => n
                .as_f64()
                .filter(|_| n.is_f64())
                .map(|f| format!("{f:.6}"))
                .unwrap_or_else(|| n.to_string()),
            Value::Null => "null".to_string(),
            other => other.to_string(),
        };
        out.push_str(&format!("{k}={shown}\n"));
    }
    out.push('\n');
    for (name, path) in files {
        out.push_str(&format!("{name}={}\n", path.display()));
    }
    out.push_str(&rule);
    out.push('\n');
    out
}
