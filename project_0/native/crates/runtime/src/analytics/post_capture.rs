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
