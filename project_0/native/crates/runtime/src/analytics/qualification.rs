//! C0E1-P4-VC / BUGS V21 (PHASENPLAN L.2, last step): the separate C0-E1 master qualification
//! after the post-capture verification — capture → verification → report → qualification.
//!
//! It joins the capture axes and the selected strict result of the native report, the canonical
//! economics replay over that result, the run's final technical record (`ECON_FINAL`) and the
//! owners' end (phase journal) into one typed, identity-bound result. The run-end technical
//! verdict cannot pass: it requires canonical evidence that only the later verification delivers.
//! Here every other gate of that verdict is re-checked from the persisted record; the full-slot
//! reconciliation comes from the strict verification, while the capture journal's seal and the
//! audited deployment continuity (which the slot verification does not prove) stay their own
//! gates. Master PASS needs `CANONICAL_VERIFIED` (a final result), complete capture clock, live
//! coverage and capture journal, proven deployment continuity, a canonical economics replay,
//! every technical gate and no LIVE or submission; anything else is `C0_E1_NOT_QUALIFIED` with
//! each failed gate. The report itself never claims the master (`c0_e1_master_pass` stays false).
use crate::analytics::econ::{SnapshotReport, REPORT_JOURNAL as ECON_REPORT_JOURNAL};
use crate::analytics::journal::{self, kind};
use crate::analytics::measurement::{Phase, PhaseRecord};
use crate::analytics::payload::Payload;
use crate::analytics::report::{self, DatasetRecord, Economics};
use crate::run_layout::{RunLayout, RunPaths};
use bincode::Options as _;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "Project0/C0E1Qualification/1";
/// Directory of the qualification results inside a run.
pub const QUALIFICATION_DIR: &str = "qualification";
pub const MASTER_PASS: &str = "C0_E1_MASTER_PASS";
pub const NOT_QUALIFIED: &str = "C0_E1_NOT_QUALIFIED";

/// How the owners ended (`OWNERS_FINISHED` of the phase journal).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnersEnd {
    pub aborted: bool,
    pub shadow_failed: bool,
    pub market_failed: bool,
}

/// Everything a qualification is bound to: the same inputs give the same id and result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualificationInputs {
    pub capture_run_id: String,
    pub capture_evidence_digest: String,
    pub verification_spec_id: Option<String>,
    pub verification_result_id: Option<String>,
    /// The normalized dataset (records) the axes were read from.
    pub dataset_records_sha256: [u8; 32],
    /// Hash of the persisted `ECON_FINAL` frame; `None` without one.
    pub econ_final_frame_sha256: Option<[u8; 32]>,
    /// The selected result's post-capture deployment proof (file hash); `None` without one.
    pub deployment_proof_sha256: Option<[u8; 32]>,
    pub owners: Option<OwnersEnd>,
    /// The qualifying binary is the evaluation pin (a changed binary is a new qualification).
    pub qualifier_binary_sha256: [u8; 32],
}

/// The separate axes (PHASENPLAN L.2 status classes) and the master verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualificationRecord {
    pub schema: String,
    pub qualification_id: String,
    pub inputs: QualificationInputs,
    /// `CAPTURE_CLOCK_COMPLETE` or `CAPTURE_CLOCK_INCOMPLETE`.
    pub capture_clock: String,
    /// `LIVE_COVERAGE_COMPLETE` or `LIVE_COVERAGE_INCOMPLETE`.
    pub live_coverage: String,
    /// `CAPTURE_JOURNAL_COMPLETE` or `CAPTURE_JOURNAL_INCOMPLETE` (the market journal's seal).
    pub capture_journal: String,
    /// `DEPLOYMENT_CONTINUITY_PROVEN` or `DEPLOYMENT_CONTINUITY_UNPROVEN` (audited program
    /// deployment before and after the window; the strict verification does not prove it).
    pub deployment_continuity: String,
    /// The selected result's status (`CANONICAL_VERIFIED`, `HISTORICAL_INCOMPLETE`, …) or `NOT_RUN`.
    pub verification: String,
    /// `CANONICAL_ECONOMICS_REPLAYED` (over a final canonical result), `ECONOMICS_REPLAYED_PROVISIONAL`
    /// (over a non-canonical one), `ECONOMICS_REPLAY_INCOMPLETE` or `ECONOMICS_NOT_REPLAYED`.
    pub economics: String,
    /// Canonical events in the capture window of a replay. The run's minimum sample is not
    /// persisted, so the sample sufficiency is not judged here (it is no master gate).
    pub canonical_events_in_window: Option<u64>,
    /// `LIVE_DISABLED_NO_SUBMISSION` or `LIVE_OR_SUBMISSION_PRESENT`.
    pub trading: String,
    /// `C0_E1_MASTER_PASS` or `C0_E1_NOT_QUALIFIED`.
    pub verdict: String,
    /// Every failed master gate (stable codes, in check order); empty for a PASS.
    pub failed: Vec<String>,
}
impl Payload for QualificationRecord {
    const KIND: u16 = kind::C0E1_QUALIFICATION;
}

/// The evidence a qualification evaluates.
pub struct Evidence<'a> {
    pub dataset: &'a DatasetRecord,
    pub economics: &'a Economics,
    /// The run's measurement plan (pre-capture manifest).
    pub plan: &'a crate::analytics::measurement::MeasurementPlan,
    /// The capture's completion axes.
    pub capture: &'a crate::market::observer::rows::MarketCompletion,
    /// The market journal is sealed COMPLETE.
    pub capture_journal_complete: bool,
    /// The selected result's post-capture deployment proof is verified (C0E1-P4-RESUME).
    pub deployment_proven_after: bool,
    pub final_report: Option<&'a SnapshotReport>,
    pub owners: Option<OwnersEnd>,
}

/// The axes and failed gates of `e` (pure; `qualify` binds and persists the result).
pub struct Assessment {
    pub capture_clock: &'static str,
    pub live_coverage: String,
    pub capture_journal: &'static str,
    pub deployment_continuity: &'static str,
    pub verification: String,
    pub economics: &'static str,
    pub canonical_events_in_window: Option<u64>,
    pub trading: &'static str,
    pub failed: Vec<&'static str>,
}
impl Assessment {
    pub fn verdict(&self) -> &'static str {
        if self.failed.is_empty() {
            MASTER_PASS
        } else {
            NOT_QUALIFIED
        }
    }
}

pub fn assess(e: &Evidence) -> Assessment {
    let d = e.dataset;
    let mut failed = Vec::new();
    let capture_clock = if d.capture_clock_complete {
        "CAPTURE_CLOCK_COMPLETE"
    } else {
        failed.push("CAPTURE_CLOCK_INCOMPLETE");
        "CAPTURE_CLOCK_INCOMPLETE"
    };
    // An interrupted capture stays LIVE_COVERAGE_INCOMPLETE, whatever is verified later.
    if d.live_coverage_status != "LIVE_COVERAGE_COMPLETE" {
        failed.push("LIVE_COVERAGE_INCOMPLETE");
    }
    // The capture's own incompleteness reasons (retention, decoder, overflow, fatal, …), which
    // live coverage alone does not carry.
    let capture_journal = if e.capture_journal_complete {
        "CAPTURE_JOURNAL_COMPLETE"
    } else {
        failed.push("CAPTURE_JOURNAL_INCOMPLETE");
        "CAPTURE_JOURNAL_INCOMPLETE"
    };
    // Only a final canonical result counts; off/index/crosscheck never do.
    let canonical = d.verification_status == "CANONICAL_VERIFIED"
        && d.report_status == "FINAL_VERIFICATION_RESULT";
    if !canonical {
        failed.push("VERIFICATION_NOT_CANONICAL");
    }
    // Liquidation-path coverage needs the audited deployment before and after the window; the
    // strict slot verification does not prove it (PHASENPLAN: never silently validated).
    let deployment_continuity =
        if e.capture.liquidation_path_coverage_complete || e.deployment_proven_after {
            "DEPLOYMENT_CONTINUITY_PROVEN"
        } else {
            failed.push("DEPLOYMENT_CONTINUITY_UNPROVEN");
            "DEPLOYMENT_CONTINUITY_UNPROVEN"
        };
    let (economics, canonical_events_in_window) = match e.economics {
        Economics::Replayed { events, .. } if canonical => {
            ("CANONICAL_ECONOMICS_REPLAYED", Some(*events))
        }
        // A replay over a non-canonical result is no canonical evidence (the verification gate
        // already fails).
        Economics::Replayed { events, .. } => ("ECONOMICS_REPLAYED_PROVISIONAL", Some(*events)),
        Economics::Incomplete(_) => {
            failed.push("ECONOMICS_REPLAY_INCOMPLETE");
            ("ECONOMICS_REPLAY_INCOMPLETE", None)
        }
        Economics::NotRun => {
            failed.push("ECONOMICS_NOT_REPLAYED");
            ("ECONOMICS_NOT_REPLAYED", None)
        }
    };
    let mut trading = "LIVE_DISABLED_NO_SUBMISSION";
    match e.final_report {
        None => failed.push("ECON_FINAL_MISSING"),
        Some(r) => {
            if r.identity.run_id != d.identity.capture_run_id {
                failed.push("ECON_FINAL_OTHER_RUN");
            }
            if r.identity.measurement_plan != *e.plan {
                failed.push("ECON_FINAL_OTHER_PLAN");
            }
            let t = &r.technical_qualification;
            // Every gate of the run-end technical verdict except its capture-time market and
            // canonical terms, which the verification and the canonical replay replace.
            let gates: [(bool, &'static str); 13] = [
                (
                    t.startup_excluded_from_measurement,
                    "TECHNICAL_STARTUP_IN_WINDOW",
                ),
                (
                    t.shadow_evaluations_in_window > 0,
                    "TECHNICAL_NO_EVALUATIONS",
                ),
                (
                    t.shadow_telemetry.as_ref().is_some_and(|s| {
                        s.ingress_overflow == 0
                            && s.winner_watch_overflow == 0
                            && s.shadow_state_ready_at_shutdown
                    }),
                    "TECHNICAL_SHADOW_STATE",
                ),
                (
                    t.actual_duration_seconds == Some(t.requested_duration_seconds as f64),
                    "TECHNICAL_DURATION",
                ),
                (t.four_snapshots_present, "TECHNICAL_SNAPSHOTS"),
                (t.snapshot_write_failures == 0, "TECHNICAL_WRITER"),
                (
                    t.shadow_evaluations_received == t.shadow_evaluations_streamed,
                    "TECHNICAL_EVALUATIONS_NOT_STREAMED",
                ),
                (
                    t.shadow_semantic_evictions == 0,
                    "TECHNICAL_SEMANTIC_EVICTIONS",
                ),
                (t.analytics_queue_drops == 0, "TECHNICAL_QUEUE_DROPS"),
                (t.retention_overflow == 0, "TECHNICAL_RETENTION_OVERFLOW"),
                (
                    !t.shadow_runtime_terminated_early,
                    "TECHNICAL_SHADOW_FAILED",
                ),
                (t.shadow_runtime_finished, "TECHNICAL_SHADOW_NOT_FINISHED"),
                (!t.program_upgrade_halt, "TECHNICAL_PROGRAM_UPGRADE"),
            ];
            failed.extend(gates.iter().filter(|g| !g.0).map(|g| g.1));
            if t.submissions != 0
                || t.submission_attempts_blocked != 0
                || t.live_ready
                || r.identity.live_enabled
            {
                failed.push("TRADING_LIVE_OR_SUBMISSION");
                trading = "LIVE_OR_SUBMISSION_PRESENT";
            }
        }
    }
    match e.owners {
        None => failed.push("OWNERS_END_UNKNOWN"),
        Some(o) => {
            if o.aborted {
                failed.push("OWNERS_ABORTED");
            }
            if o.shadow_failed {
                failed.push("OWNERS_SHADOW_FAILED");
            }
            if o.market_failed {
                failed.push("OWNERS_MARKET_FAILED");
            }
        }
    }
    Assessment {
        capture_clock,
        live_coverage: d.live_coverage_status.clone(),
        capture_journal,
        deployment_continuity,
        verification: d.verification_status.clone(),
        economics,
        canonical_events_in_window,
        trading,
        failed,
    }
}

/// The run's final report (`ECON_FINAL`, the newest frame) and its frame hash.
fn final_report(run: &Path) -> Result<Option<(SnapshotReport, [u8; 32])>, String> {
    let path = run.join("snapshots").join(ECON_REPORT_JOURNAL);
    if !path.exists() {
        return Ok(None);
    }
    let mut r = journal::Reader::open(&path).map_err(|e| format!("ECON_REPORTS: {e}"))?;
    let mut last = None;
    while let Some(f) = r.next_frame().map_err(|e| format!("ECON_REPORTS: {e}"))? {
        if f.kind == kind::ECON_FINAL {
            let report = f
                .typed::<SnapshotReport>()
                .map_err(|e| format!("ECON_FINAL: {e}"))?;
            last = Some((report, f.hash));
        }
    }
    Ok(last)
}

/// The owners' end of the run (the newest `OWNERS_FINISHED` phase row).
fn owners_end(run: &Path) -> Result<Option<OwnersEnd>, String> {
    let path = journal::binary_path(&RunPaths::new(run, RunLayout::V2).phases());
    if !path.exists() {
        return Ok(None);
    }
    let mut r = journal::Reader::open(&path).map_err(|e| format!("PHASE_JOURNAL: {e}"))?;
    let mut end = None;
    while let Some(f) = r.next_frame().map_err(|e| format!("PHASE_JOURNAL: {e}"))? {
        if f.kind != kind::MEASUREMENT_PHASE {
            continue;
        }
        let row = f
            .typed::<PhaseRecord>()
            .map_err(|e| format!("PHASE_JOURNAL: {e}"))?;
        if let Phase::OwnersFinished {
            aborted,
            shadow_failed,
            market_failed,
        } = row.phase
        {
            end = Some(OwnersEnd {
                aborted,
                shadow_failed,
                market_failed,
            });
        }
    }
    Ok(end)
}

/// Qualifies `run` against the selected verification result (`opts`, as the report selects it):
/// binds every input, evaluates the axes and persists the result immutably under
/// `qualification/qualification-<id>.p0j`. The same inputs give the same file; a different
/// result for the same inputs is refused (`QUALIFICATION_RESULT_CHANGED`).
pub fn qualify(
    run: &Path,
    opts: &report::Options,
) -> Result<(QualificationRecord, PathBuf), String> {
    let dataset = report::normalize(run, opts)?;
    let final_report = final_report(run)?;
    let id = &dataset.record.identity;
    // The selected result's deployment proof, bound to exactly that result.
    let proof = match (&id.verification_spec_id, &id.verification_result_id) {
        (Some(spec), Some(result)) => {
            let path =
                crate::market::observer::strict_driver::deployment_proof_path(run, spec, result);
            if path.exists() {
                let p: crate::market::observer::strict_driver::DeploymentProof =
                    journal::read_typed_value(&path)
                        .map_err(|e| format!("DEPLOYMENT_PROOF: {e}"))?;
                if p.verification_result_id != *result {
                    return Err("DEPLOYMENT_PROOF_OTHER_RESULT".into());
                }
                Some((
                    p.verified,
                    journal::file_sha256(&path).map_err(|e| e.to_string())?,
                ))
            } else {
                None
            }
        }
        _ => None,
    };
    let owners = owners_end(run)?;
    let a = assess(&Evidence {
        dataset: &dataset.record,
        economics: &dataset.economics,
        plan: &dataset.plan,
        capture: &dataset.completion,
        capture_journal_complete: dataset.capture_journal_complete,
        deployment_proven_after: proof.is_some_and(|p| p.0),
        final_report: final_report.as_ref().map(|(r, _)| r),
        owners,
    });
    let inputs = QualificationInputs {
        capture_run_id: id.capture_run_id.clone(),
        capture_evidence_digest: id.capture_evidence_digest.clone(),
        verification_spec_id: id.verification_spec_id.clone(),
        verification_result_id: id.verification_result_id.clone(),
        dataset_records_sha256: dataset.record.records_sha256,
        econ_final_frame_sha256: final_report.as_ref().map(|(_, h)| *h),
        deployment_proof_sha256: proof.map(|p| p.1),
        owners,
        qualifier_binary_sha256: std::env::current_exe()
            .ok()
            .and_then(|p| journal::file_sha256(&p).ok())
            .unwrap_or([0; 32]),
    };
    let encoded = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialize(&inputs)
        .map_err(|e| e.to_string())?;
    let qualification_id = journal::hex(&solana_sdk::hash::hash(&encoded).to_bytes()[..16]);
    let record = QualificationRecord {
        schema: SCHEMA.into(),
        qualification_id: qualification_id.clone(),
        inputs,
        capture_clock: a.capture_clock.into(),
        live_coverage: a.live_coverage.clone(),
        capture_journal: a.capture_journal.into(),
        deployment_continuity: a.deployment_continuity.into(),
        verification: a.verification.clone(),
        economics: a.economics.into(),
        canonical_events_in_window: a.canonical_events_in_window,
        trading: a.trading.into(),
        verdict: a.verdict().into(),
        failed: a.failed.iter().map(|s| s.to_string()).collect(),
    };
    let dir = run.join(QUALIFICATION_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("qualification-{qualification_id}.p0j"));
    journal::write_typed_value(&path, journal::source::REPORT, &record).map_err(|e| {
        if e.to_string().contains("IMMUTABLE_CONFLICT") {
            "QUALIFICATION_RESULT_CHANGED".to_string()
        } else {
            format!("QUALIFICATION: {e}")
        }
    })?;
    Ok((record, path))
}

/// Terminal lines of a qualification (axes first, then the verdict and its failed gates).
pub fn render(r: &QualificationRecord) -> String {
    format!(
        "qualification_id={}\ncapture_run_id={}\nverification_result_id={}\ncapture_clock={}\nlive_coverage={}\ncapture_journal={}\nverification={}\ndeployment_continuity={}\neconomics={} canonical_events_in_window={}\ntrading={}\nverdict={}\nfailed={}\n",
        r.qualification_id,
        r.inputs.capture_run_id,
        r.inputs.verification_result_id.as_deref().unwrap_or("-"),
        r.capture_clock,
        r.live_coverage,
        r.capture_journal,
        r.verification,
        r.deployment_continuity,
        r.economics,
        r.canonical_events_in_window
            .map_or("-".to_string(), |n| n.to_string()),
        r.trading,
        r.verdict,
        if r.failed.is_empty() {
            "-".to_string()
        } else {
            r.failed.join(",")
        },
    )
}
