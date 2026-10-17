//! Market summary and report serialization.
use super::*;

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

/// Finalise: write any still-deferred liquidation, STOPPED, seal, `summary.json`, `final_report.txt`.
pub fn finalize(
    core: &mut ObserverCore,
    sink: &mut dyn MarketSink,
    ctx: &SummaryContext,
    run_dir: &Path,
    now_unix_s: u64,
    stopped: super::rows::WindowEvent,
) -> Result<(Value, String), String> {
    core.write_deferred_unenriched(sink, "NOT_ENRICHED")
        .map_err(|e| format!("liquidations write: {e}"))?;
    core.fail_truncated_pending("NOT_QUALIFIED");
    core.coverage(sink, now_unix_s, None, stopped)
        .map_err(|e| format!("coverage write: {e}"))?;
    // A journal-backed sink seals here, before summary.json exists; no view is rendered.
    let binding = sink
        .finish()
        .map_err(|e| format!("evidence journal: {e}"))?;
    let mut summary = core.summary(ctx);
    if let Some(h) = &core.econ {
        summary["measurement_plan"] =
            serde_json::to_value(&h.cfg.plan).map_err(|e| e.to_string())?;
        summary["capture_window"] = if let Some(cutoff) = h.cutoff() {
            h.cfg.plan.capture_window(
                h.clock,
                cutoff - h.cfg.plan.duration_seconds * crate::analytics::measurement::NS,
            )?
        } else {
            Value::Null
        };
    }
    if let Some(binding) = &binding {
        summary["evidence_journal"] = serde_json::to_value(binding).map_err(|e| e.to_string())?;
    }
    let summary_path = run_dir.join("summary.json");
    let pretty = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    fs::write(&summary_path, pretty).map_err(|e| format!("summary.json: {e}"))?;
    let mut files = vec![("summary_json", summary_path.clone())];
    if binding.is_some() {
        files.insert(0, ("market_journal", run_dir.join(MARKET_JOURNAL)));
    }
    let refs: Vec<(&str, &Path)> = files.iter().map(|(n, p)| (*n, p.as_path())).collect();
    let report = render_report(&summary, &refs);
    fs::write(run_dir.join("final_report.txt"), &report)
        .map_err(|e| format!("final_report.txt: {e}"))?;
    Ok((summary, report))
}
