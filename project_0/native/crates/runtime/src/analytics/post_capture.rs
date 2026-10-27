//! C0E1-P5-AUTO (PHASENPLAN L.5): the one production chain after a capture's drain and manifest —
//! the selected verification (`off`: identities only, no chain RPC; `strict`: the native driver,
//! online only with provider evidence), the native report and the separate master qualification.
//! A failed or incomplete verification never suppresses the report or the qualification.
use crate::analytics::{qualification, report};
use crate::market::observer::strict_driver;
use std::path::Path;

/// Flags of the verification request (`strict_driver::cli`); every other flag is the report's.
const VERIFY_VALUE_FLAGS: &[&str] = &[
    "--verify-rps",
    "--verify-calls",
    "--verify-seconds",
    "--verify-bytes",
    "--verify-disk-bytes",
    "--verify-ram-bytes",
    "--verify-min-free-bytes",
    "--provider-evidence",
];
const VERIFY_SWITCHES: &[&str] = &["--offline", "--resume-verification"];

/// What the chain did: its terminal text and each stage's outcome.
pub struct Outcome {
    pub text: String,
    /// `off` (NOT_RUN recorded) or a CANONICAL_VERIFIED strict result.
    pub verification_ok: bool,
    pub report_ok: bool,
    /// The master verdict, `None` if the qualification could not run.
    pub verdict: Option<String>,
}

pub fn post_capture(run: &Path, mode: &str, args: &[String]) -> Result<Outcome, String> {
    if !matches!(mode, "off" | "strict") {
        return Err(format!(
            "CHAIN_VERIFY_{mode}_BLOCKED: index/crosscheck stay with the pinned Python verifier"
        ));
    }
    // DEC-16: an unbounded run is measured window by window, never as one capture.
    let windows = crate::run_layout::RunPaths::new(run, crate::run_layout::RunLayout::V2)
        .snapshot(crate::analytics::econ::ECON_WINDOWS_JOURNAL);
    if windows.is_file() {
        return Ok(unbounded(&windows));
    }
    let path = run.display().to_string();
    let mut verify = vec!["--run".into(), path.clone(), "--mode".into(), mode.into()];
    let mut rest = vec!["--run".into(), path];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if VERIFY_VALUE_FLAGS.contains(&a.as_str()) {
            verify.push(a.clone());
            verify.extend(it.next().cloned());
        } else if VERIFY_SWITCHES.contains(&a.as_str()) {
            verify.push(a.clone());
        } else {
            rest.push(a.clone());
        }
    }
    let inv = report::parse_args(&rest)?;
    let mut text = String::new();
    let verification_ok = match strict_driver::cli(&verify) {
        Ok((t, verified)) => {
            text.push_str(&t);
            verified || mode == "off"
        }
        Err(e) => {
            text.push_str(&format!("verification HISTORICAL_INCOMPLETE: {e}\n"));
            false
        }
    };
    let report_ok = match report::report(run, &inv.options, &inv.query) {
        Ok(out) => {
            text.push_str(&out.meta);
            text.push_str(&out.data);
            true
        }
        Err(e) => {
            text.push_str(&format!("report INCOMPLETE: {e}\n"));
            false
        }
    };
    let verdict = match qualification::qualify(run, &inv.options) {
        Ok((q, path)) => {
            text.push_str(&qualification::render(&q));
            text.push_str(&format!("record={}\n", path.display()));
            Some(q.verdict)
        }
        Err(e) => {
            text.push_str(&format!("qualification NOT_RUN: {e}\n"));
            None
        }
    };
    Ok(Outcome {
        text,
        verification_ok,
        report_ok,
        verdict,
    })
}

/// DEC-16: the post-capture chain of an unbounded run — its windows as recorded (each provisional,
/// no canonical finalization); verification, report and master qualification of a run of windows
/// are NOT_RUN (no per-window scopes yet). Nothing is written; the chain ends unqualified.
fn unbounded(windows: &Path) -> Outcome {
    use crate::analytics::econ::{EconWindow, WindowStatus};
    let mut text = String::new();
    let (mut closed, mut partial) = (0u64, 0u64);
    match crate::storage::segments::ChainReader::open(windows) {
        Ok(mut r) => {
            while let Ok(Some(f)) = r.next_frame() {
                if f.kind != crate::analytics::journal::kind::ECON_WINDOW {
                    continue;
                }
                let Ok(w) = f.typed::<EconWindow>() else {
                    text.push_str("window UNREADABLE\n");
                    continue;
                };
                let status = match &w.status {
                    WindowStatus::Closed { verdict } => {
                        closed += 1;
                        format!("CLOSED {verdict}")
                    }
                    WindowStatus::Partial { reason } => {
                        partial += 1;
                        format!("PARTIAL {reason}")
                    }
                };
                text.push_str(&format!(
                    "window {} {status} snapshots={} opportunities={} late={} reports={}\n",
                    w.window, w.snapshots, w.opportunities, w.late_evidence, w.report_journal
                ));
            }
        }
        Err(e) => text.push_str(&format!("windows journal UNREADABLE: {e}\n")),
    }
    let chain = crate::storage::segments::verify_chain(windows)
        .map(|c| format!("sealed={} complete={}", c.sealed, c.complete))
        .unwrap_or_else(|e| format!("INCOMPLETE ({e})"));
    text.push_str(&format!(
        "until-stopped run: {} windows ({closed} closed, {partial} partial); windows journal {chain}\n\
         verification NOT_RUN: UNTIL_STOPPED_WINDOW_VERIFY_NOT_RUN (an unbounded run is not verified window by window)\n\
         report NOT_RUN: UNTIL_STOPPED_REPORT_NOT_RUN (its windows stay provisional; no run report)\n\
         qualification NOT_QUALIFIED: UNTIL_STOPPED_NO_MASTER (an unbounded run is no C0-E1 master)\n",
        closed + partial
    ));
    Outcome {
        text,
        verification_ok: false,
        report_ok: false,
        verdict: Some("NOT_QUALIFIED".into()),
    }
}

#[cfg(test)]
mod tests {
    /// DEC-16: post-capture of an unbounded run lists its windows as recorded and runs no
    /// verification, report or master qualification of it — nothing is written, it ends
    /// unqualified (the controller exits 2).
    #[test]
    fn dec16_post_capture_of_an_unbounded_run_lists_its_windows_and_qualifies_nothing() {
        use crate::analytics::econ::{EconWindow, WindowStatus};
        let run = std::env::temp_dir().join(format!("p0_post_unbounded_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&run);
        let paths = crate::run_layout::RunPaths::new(&run, crate::run_layout::RunLayout::V2);
        let journal = paths.snapshot(crate::analytics::econ::ECON_WINDOWS_JOURNAL);
        std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
        let mut w = crate::storage::segments::SegmentedWriter::create(&journal, 0).unwrap();
        let segment = crate::storage::segments::SegmentRef {
            file: "e.p0j".into(),
            index: 1,
            frames: 1,
            root_sha256: [0; 32],
            events_through: 1,
        };
        for (window, status) in [
            (
                1,
                WindowStatus::Closed {
                    verdict: "CONTINUE".into(),
                },
            ),
            (
                2,
                WindowStatus::Partial {
                    reason: "OPERATOR_STOP".into(),
                },
            ),
        ] {
            let record = EconWindow {
                window,
                from_ns: 0,
                end_ns: 1,
                from_unix_ns: 0,
                end_unix_ns: 1,
                status,
                snapshots: 4,
                opportunities: 2,
                report_journal: format!("snapshots/windows/w{window:06}/economics_reports.p0j"),
                report_root_sha256: "00".into(),
                evaluations: segment.clone(),
                late_evidence: 1,
                pending_overflow: 0,
                analytics_drops: 0,
                retention_overflow: 0,
                writer_errors: 0,
            };
            w.append_typed(crate::analytics::journal::Provenance::default(), &record)
                .unwrap();
        }
        w.seal(true).unwrap();
        let before = std::fs::read_dir(journal.parent().unwrap())
            .unwrap()
            .count();
        let out = super::post_capture(&run, "off", &[]).unwrap();
        assert!(!out.verification_ok && !out.report_ok);
        assert_eq!(out.verdict.as_deref(), Some("NOT_QUALIFIED"));
        for line in [
            "window 1 CLOSED CONTINUE snapshots=4 opportunities=2 late=1",
            "window 2 PARTIAL OPERATOR_STOP",
            "until-stopped run: 2 windows (1 closed, 1 partial); windows journal sealed=true complete=true",
            "verification NOT_RUN: UNTIL_STOPPED_WINDOW_VERIFY_NOT_RUN",
            "report NOT_RUN: UNTIL_STOPPED_REPORT_NOT_RUN",
            "qualification NOT_QUALIFIED: UNTIL_STOPPED_NO_MASTER",
        ] {
            assert!(out.text.contains(line), "{line}\n{}", out.text);
        }
        assert_eq!(
            std::fs::read_dir(journal.parent().unwrap())
                .unwrap()
                .count(),
            before,
            "nothing written"
        );
        let _ = std::fs::remove_dir_all(run);
    }
}
