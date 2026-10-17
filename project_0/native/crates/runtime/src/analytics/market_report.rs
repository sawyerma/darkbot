//! Pure text rendering of a finalized market summary (`final_report.txt`, terminal); typed, no
//! JSON (JFI-01/PERSIST-04: human-readable reports are text). No capture or trading admission.
use crate::market_observer::MarketFinal;
use std::path::Path;

/// `key=value` lines of the final market report, in the historical key order. Decimal values
/// carry six places; absent values are `null`; lists are comma separated (`none` when empty).
pub fn report_lines(m: &MarketFinal) -> Vec<(&'static str, String)> {
    let s = &m.summary;
    let f = |x: f64| format!("{x:.6}");
    let of = |x: Option<f64>| x.map_or_else(|| "null".to_string(), f);
    let ou = |x: Option<u64>| x.map_or_else(|| "null".to_string(), |n| n.to_string());
    let os = |x: &Option<String>| x.clone().unwrap_or_else(|| "null".to_string());
    let list = |v: &[String]| {
        if v.is_empty() {
            "none".to_string()
        } else {
            v.join(",")
        }
    };
    let (plan, window) = match &m.econ {
        Some((p, w)) => (
            format!(
                "duration_seconds={} snapshot_offsets_ns={:?}",
                p.duration_seconds, p.snapshot_offsets_ns
            ),
            w.as_ref().map_or_else(
                || "null".to_string(),
                |w| format!("{}..{}", w.capture_start_utc, w.capture_cutoff_utc),
            ),
        ),
        None => ("null".to_string(), "null".to_string()),
    };
    let reasons: Vec<String> = s
        .truncated_unqualified_reasons
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let n = |v: u64| v.to_string();
    vec![
        ("run_id", s.run_id.clone()),
        ("source", s.source.clone()),
        ("commitment", s.commitment.clone()),
        ("program", s.program.clone()),
        (
            "configured_duration_seconds",
            f(s.configured_duration_seconds),
        ),
        ("measurement_plan", plan),
        ("capture_window", window),
        (
            "actual_observation_seconds",
            f(s.actual_observation_seconds),
        ),
        ("verdict", s.verdict.clone()),
        (
            "market_capture_complete",
            s.market_capture_complete.to_string(),
        ),
        ("coverage_complete", s.coverage_complete.to_string()),
        ("decode_complete", s.decode_complete.to_string()),
        ("drain_complete", s.drain_complete.to_string()),
        (
            "source_coverage_verified",
            s.source_coverage_verified.to_string(),
        ),
        ("source_coverage_note", s.source_coverage_note.clone()),
        ("incomplete_reasons", list(&s.incomplete_reasons)),
        ("first_observed_slot", ou(s.first_observed_slot)),
        ("last_observed_slot", ou(s.last_observed_slot)),
        ("transactions_received", n(s.transactions_received)),
        ("unique_transactions", n(s.unique_transactions)),
        ("duplicate_transactions", n(s.duplicate_transactions)),
        ("successful_transactions", n(s.successful_transactions)),
        ("failed_transactions", n(s.failed_transactions)),
        ("liquidation_transactions", n(s.liquidation_transactions)),
        ("liquidation_events", n(s.liquidation_events)),
        ("sample_status", s.sample_status.clone()),
        ("liquidations_per_hour", of(s.liquidations_per_hour)),
        ("unique_targets", n(s.unique_targets)),
        ("gross_edge_total_usd", f(s.gross_edge_total_usd)),
        ("gross_edge_per_hour_usd", of(s.gross_edge_per_hour_usd)),
        (
            "gross_edge_per_liquidation_usd",
            of(s.gross_edge_per_liquidation_usd),
        ),
        ("gross_edge_median_usd", of(s.gross_edge_median_usd)),
        ("gross_edge_p90_usd", of(s.gross_edge_p90_usd)),
        ("gross_edge_p99_usd", of(s.gross_edge_p99_usd)),
        ("gross_edge_max_usd", of(s.gross_edge_max_usd)),
        ("gross_edge_min_usd", of(s.gross_edge_min_usd)),
        (
            "protocol_flat_fee_total_lamports",
            n(s.protocol_flat_fee_total_lamports),
        ),
        ("tx_fee_total_lamports", n(s.tx_fee_total_lamports)),
        ("fee_totals_complete", s.fee_totals_complete.to_string()),
        ("enrichment_available", s.enrichment_available.to_string()),
        ("enrichment_complete", s.enrichment_complete.to_string()),
        ("enrichment_requested", n(s.enrichment_requested)),
        ("enrichment_succeeded", n(s.enrichment_succeeded)),
        ("enrichment_failed", n(s.enrichment_failed)),
        ("unique_receivers", n(s.unique_receivers)),
        ("top_receiver", os(&s.top_receiver)),
        (
            "top_receiver_liquidation_share",
            of(s.top_receiver_liquidation_share),
        ),
        (
            "top_receiver_gross_edge_share",
            of(s.top_receiver_gross_edge_share),
        ),
        (
            "top3_receiver_liquidation_share",
            of(s.top3_receiver_liquidation_share),
        ),
        (
            "top3_receiver_gross_edge_share",
            of(s.top3_receiver_gross_edge_share),
        ),
        ("gap_count", n(s.gap_count)),
        ("reconnect_count", n(s.reconnect_count)),
        ("ingress_overflow_count", n(s.ingress_overflow_count)),
        ("decoder_error_count", n(s.decoder_error_count)),
        ("logs_truncated_total", n(s.logs_truncated_total)),
        ("logs_truncated_qualified", n(s.logs_truncated_qualified)),
        (
            "logs_truncated_unqualified",
            n(s.logs_truncated_unqualified),
        ),
        (
            "logs_truncated_liquidation_mismatch",
            n(s.logs_truncated_liquidation_mismatch),
        ),
        (
            "truncated_instruction_end_liquidations",
            n(s.truncated_instruction_end_liquidations),
        ),
        (
            "truncated_log_end_liquidations",
            n(s.truncated_log_end_liquidations),
        ),
        ("truncated_unqualified_reasons", list(&reasons)),
        ("rpc_fallback_count", n(s.rpc_fallback_count)),
        ("events_after_cutoff", n(s.events_after_cutoff)),
        ("max_stream_silence_ms", n(s.max_stream_silence_ms)),
    ]
}

pub fn render_report(m: &MarketFinal, files: &[(&str, &Path)]) -> String {
    let rule = "=".repeat(72);
    let mut out = format!("{rule}\nPROJECT 0 MARKET OBSERVER — FINAL\n\n");
    for (k, v) in report_lines(m) {
        out.push_str(&format!("{k}={v}\n"));
    }
    out.push('\n');
    for (name, path) in files {
        out.push_str(&format!("{name}={}\n", path.display()));
    }
    out.push_str(&rule);
    out.push('\n');
    out
}
