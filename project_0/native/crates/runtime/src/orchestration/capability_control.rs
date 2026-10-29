//! DEC-13 operator capability control (`p0ctl capability set LIVE LIVE|OFF`, DEC-04
//! `p0ctl capability set HOLD_B OFF|OBSERVE|ACTIVE`, DEC-06 `… THRESHOLD_TRIGGER OFF|OBSERVE|ACTIVE`,
//! hot, OFF → ACTIVE refused by the owner): typed,
//! append-only request files beside the source-control log, rung by the same doorbell and handed
//! to the state owner by the same control thread (`source_control::spawn`). The operator's value
//! is the *requested* one; only the owner decides the *effective* one and records it
//! (`outcome_<seq>.p0j`). LIVE stays BLOCKED until a separate FE-06 authorization exists — this
//! build constructs none (`Fe06Authorization`), so no request ever makes a candidate reach a send.
use crate::analytics::journal;
use crate::analytics::payload::{CapabilityControlOutcome, CapabilityControlRequest};
use std::path::{Path, PathBuf};

/// The controllable capability of FE-06 (DEC-13).
pub const LIVE: &str = "LIVE";
/// LIVE not requested — the value of every start.
pub const LIVE_OFF: &str = "OFF";
/// The values LIVE may be requested with.
pub const LIVE_VALUES: [&str; 2] = [LIVE_OFF, LIVE];
/// Why LIVE is not effective (B10).
pub const LIVE_BLOCK_REASON: &str = "FE-06/P8-LT: LIVE is not authorized (B10)";

pub fn request_dir(project: &Path) -> PathBuf {
    project.join("data").join("capability-control")
}

fn request_path(dir: &Path, seq: u64) -> PathBuf {
    dir.join(format!("request_{seq:08}.p0j"))
}

fn outcome_path(dir: &Path, seq: u64) -> PathBuf {
    dir.join(format!("outcome_{seq:08}.p0j"))
}

fn sequences(dir: &Path) -> Result<Vec<u64>, String> {
    super::source_control::numbered(dir, "request_")
        .map_err(|e| e.replace("SOURCE_CONTROL_IO", "CAPABILITY_CONTROL_IO"))
}

/// The controllable capabilities and their values: LIVE (DEC-13), the B hold mode (DEC-04), the
/// bundle path (DEC-11), the threshold mode (DEC-06).
pub const CAPABILITIES: [(&str, &[&str]); 4] = [
    (LIVE, &LIVE_VALUES),
    (
        crate::execution::hold_b::HOLD_B,
        &crate::execution::hold_b::HOLD_B_VALUES,
    ),
    (
        crate::execution::bundle_path::JITO_BUNDLE,
        &crate::execution::bundle_path::JITO_BUNDLE_VALUES,
    ),
    (
        super::threshold_mode::THRESHOLD_TRIGGER,
        &super::threshold_mode::VALUES,
    ),
];

/// A request names a known capability and one of its values.
pub fn validate(capability: &str, requested: &str) -> Result<(), String> {
    let Some((_, values)) = CAPABILITIES.iter().find(|(c, _)| *c == capability) else {
        return Err(format!(
            "CAPABILITY_UNKNOWN: {capability} (LIVE|HOLD_B|JITO_BUNDLE|THRESHOLD_TRIGGER)"
        ));
    };
    if !values.contains(&requested) {
        return Err(format!(
            "CAPABILITY_VALUE_INVALID: {capability} {requested} ({})",
            values.join("|")
        ));
    }
    Ok(())
}

/// The capability configuration's version: the request log's head (0 before the first request).
pub fn head(dir: &Path) -> Result<u64, String> {
    Ok(sequences(dir)?.last().copied().unwrap_or(0))
}

/// A request, written only when the log's head is exactly `expected` (CTRL-03 CAS) and published
/// without overwriting (of two writers of one version exactly one wins).
pub fn write_request_expecting(
    dir: &Path,
    capability: &str,
    requested: &str,
    now_unix_s: u64,
    expected: Option<u64>,
) -> Result<u64, String> {
    validate(capability, requested)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("CAPABILITY_CONTROL_IO: {e}"))?;
    let current = head(dir)?;
    let conflict = |current: u64| {
        format!(
            "CAPABILITY_VERSION_CONFLICT: expected {}, current {current}; reload and retry",
            expected.map_or("none".into(), |e| e.to_string())
        )
    };
    if expected.is_some_and(|e| e != current) {
        return Err(conflict(current));
    }
    let seq = current + 1;
    let request = CapabilityControlRequest {
        seq,
        capability: capability.into(),
        requested: requested.into(),
        requested_unix_s: now_unix_s,
    };
    journal::create_typed_value(
        &request_path(dir, seq),
        journal::source::CAPABILITY_CONTROL,
        &request,
    )
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            conflict(head(dir).unwrap_or(seq))
        } else {
            format!("CAPABILITY_CONTROL_IO: {e}")
        }
    })?;
    Ok(seq)
}

/// Request `seq` as written (the control thread's input).
pub fn request_at(dir: &Path, seq: u64) -> Result<CapabilityControlRequest, String> {
    match journal::read_typed_value::<CapabilityControlRequest>(&request_path(dir, seq)) {
        Ok(r) if r.seq == seq => Ok(r),
        Ok(_) => Err("CAPABILITY_CONTROL_REQUEST_SEQ".into()),
        Err(e) => Err(format!("CAPABILITY_CONTROL_REQUEST_INVALID: {e}")),
    }
}

/// The sequence numbers of the requests after `handed`, ascending.
pub fn requests_after(dir: &Path, handed: u64) -> Result<Vec<u64>, String> {
    Ok(sequences(dir)?
        .into_iter()
        .filter(|s| *s > handed)
        .collect())
}

/// The newest request, if any.
pub fn latest_request(dir: &Path) -> Result<Option<CapabilityControlRequest>, String> {
    match sequences(dir)?.last() {
        None => Ok(None),
        Some(seq) => request_at(dir, *seq).map(Some),
    }
}

/// The owner: records its decision on request `outcome.seq`.
pub fn write_outcome(dir: &Path, outcome: &CapabilityControlOutcome) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("CAPABILITY_CONTROL_IO: {e}"))?;
    journal::write_typed_value(
        &outcome_path(dir, outcome.seq),
        journal::source::CAPABILITY_CONTROL,
        outcome,
    )
    .map_err(|e| format!("CAPABILITY_CONTROL_IO: {e}"))
}

/// The newest decision of an owner, if any.
pub fn latest_outcome(dir: &Path) -> Result<Option<CapabilityControlOutcome>, String> {
    match super::source_control::numbered(dir, "outcome_")
        .map_err(|e| e.replace("SOURCE_CONTROL_IO", "CAPABILITY_CONTROL_IO"))?
        .last()
    {
        None => Ok(None),
        Some(seq) => journal::read_typed_value(&outcome_path(dir, *seq))
            .map(Some)
            .map_err(|e| format!("CAPABILITY_CONTROL_OUTCOME_INVALID: {e}")),
    }
}

/// DEC-13: the separate FE-06 authorization that opens LIVE — the pipeline mode, Safety LIVE, the
/// asset allowlist and the submitter together. No request, FE-04 approval or configuration grants
/// it; its format and source are the user's decision (BUGS DEC-13), so this build reads none and
/// no production path constructs one: LIVE stays BLOCKED (only the offline tests hold one).
#[derive(Debug, PartialEq, Eq)]
pub struct Fe06Authorization {
    _sealed: (),
}

impl Fe06Authorization {
    /// The offline tests' stand-in of an FE-06 authorization (never part of a build).
    #[cfg(test)]
    pub(crate) fn offline_test() -> Self {
        Self { _sealed: () }
    }
}

/// DEC-13: the one decision of LIVE's effective value — BLOCKED whatever is requested without an
/// FE-06 authorization (this build holds none, so the owner evaluates every candidate in Shadow
/// and hands no FE-04 admission to the pipeline); with one, the requested value.
pub fn live_effective(
    requested: &str,
    fe06: Option<&Fe06Authorization>,
) -> (&'static str, &'static str) {
    match (fe06, requested) {
        (Some(_), LIVE) => (LIVE, ""),
        (Some(_), _) => (LIVE_OFF, ""),
        (None, _) => ("BLOCKED", LIVE_BLOCK_REASON),
    }
}

/// A request's operands as they may be shown (log, GUI): bounded, control characters escaped —
/// the request file is operator input, valid or not.
pub fn shown(capability: &str, requested: &str) -> String {
    let text = format!("{capability} {requested}");
    let mut shown: String = text
        .chars()
        .take(64)
        .flat_map(char::escape_default)
        .collect();
    if text.chars().count() > 64 {
        shown.push('…');
    }
    shown
}

/// DEC-13: the pipeline mode of a detection — LIVE only while LIVE is effective, so a candidate
/// reaches no send boundary in this build, whatever was requested.
pub fn pipeline_mode(
    requested: &str,
    fe06: Option<&Fe06Authorization>,
) -> crate::execution::pipeline::Mode {
    if live_effective(requested, fe06).0 == LIVE {
        crate::execution::pipeline::Mode::Live
    } else {
        crate::execution::pipeline::Mode::Shadow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DEC-13: requests are validated, CAS-versioned and never overwritten; LIVE is effective
    /// for no request (BLOCKED with the FE-06 reason).
    #[test]
    fn dec13_live_requests_are_versioned_and_never_effective() {
        let dir = std::env::temp_dir().join(format!("p0-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(head(&dir).unwrap(), 0);
        assert!(validate("JITO", "LIVE")
            .unwrap_err()
            .starts_with("CAPABILITY_UNKNOWN"));
        assert!(validate(LIVE, "ON")
            .unwrap_err()
            .starts_with("CAPABILITY_VALUE_INVALID"));
        // DEC-11: the bundle path is a capability of its own values.
        for v in crate::execution::bundle_path::JITO_BUNDLE_VALUES {
            assert_eq!(
                validate(crate::execution::bundle_path::JITO_BUNDLE, v),
                Ok(())
            );
        }
        assert!(validate(crate::execution::bundle_path::JITO_BUNDLE, "LIVE")
            .unwrap_err()
            .starts_with("CAPABILITY_VALUE_INVALID"));
        // DEC-04: the B hold mode is a capability of its own values.
        for v in crate::execution::hold_b::HOLD_B_VALUES {
            assert_eq!(validate(crate::execution::hold_b::HOLD_B, v), Ok(()));
        }
        assert!(validate(crate::execution::hold_b::HOLD_B, "LIVE")
            .unwrap_err()
            .starts_with("CAPABILITY_VALUE_INVALID"));
        assert!(validate(LIVE, "OBSERVE")
            .unwrap_err()
            .starts_with("CAPABILITY_VALUE_INVALID"));
        assert_eq!(
            write_request_expecting(&dir, LIVE, "LIVE", 1, Some(0)).unwrap(),
            1
        );
        assert!(write_request_expecting(&dir, LIVE, "OFF", 2, Some(0))
            .unwrap_err()
            .starts_with("CAPABILITY_VERSION_CONFLICT"));
        assert_eq!(
            write_request_expecting(&dir, LIVE, "OFF", 3, Some(1)).unwrap(),
            2
        );
        assert_eq!(requests_after(&dir, 0).unwrap(), vec![1, 2]);
        assert_eq!(request_at(&dir, 1).unwrap().requested, "LIVE");
        assert_eq!(latest_request(&dir).unwrap().unwrap().requested, "OFF");
        assert_eq!(shown("LIVE", "LIVE"), "LIVE LIVE");
        assert_eq!(shown("LIVE\nX", "OFF"), "LIVE\\nX OFF");
        assert_eq!(shown(&"A".repeat(80), "OFF").chars().count(), 65);
        for requested in LIVE_VALUES {
            assert_eq!(
                live_effective(requested, None),
                ("BLOCKED", LIVE_BLOCK_REASON)
            );
            assert_eq!(
                pipeline_mode(requested, None),
                crate::execution::pipeline::Mode::Shadow
            );
        }
        // DEC-13b: only an FE-06 authorization makes the requested value effective.
        let fe06 = Fe06Authorization::offline_test();
        assert_eq!(live_effective(LIVE, Some(&fe06)), (LIVE, ""));
        assert_eq!(live_effective(LIVE_OFF, Some(&fe06)), (LIVE_OFF, ""));
        assert_eq!(
            pipeline_mode(LIVE, Some(&fe06)),
            crate::execution::pipeline::Mode::Live
        );
        assert_eq!(
            pipeline_mode(LIVE_OFF, Some(&fe06)),
            crate::execution::pipeline::Mode::Shadow
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
