//! REV.1.0 R2b: the LIVE campaign on the LIVE tab (`bsc.live.state`): the
//! campaign verdict and the attempt records `bsc_runtime live attempts`
//! wrote, read and projected typed. The evaluator (`live_attempts`)
//! decides; nothing is evaluated or inferred here. A record that is absent
//! is no row; one that is unreadable or not of its schema is a warning,
//! never a value.

use super::projection::{read, safe_text};
use super::wire::bsc::*;
use darkbot_bsc_execution::live_attempts::{ATTEMPTS_DIR, VERDICT_PATH};
use darkbot_bsc_execution::sources::MAX_BUY_ATTEMPTS_TOTAL;
use darkbot_bsc_record::schema::bsc_runtime::{live_attempt as la, live_campaign_verdict as lv};
use darkbot_bsc_record::{self as record, Record, RecordError, Schema, hex};
use std::io::ErrorKind;
use std::path::Path;

/// The record at `path` with the SHA-256 of its bytes: none when absent,
/// an error when it is not a readable record of `schema`.
fn record_at(path: &Path, schema: &'static Schema) -> Result<Option<(Record, [u8; 32])>, ()> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
        Ok(_) => {}
    }
    let bytes = read(path).ok_or(())?;
    let r = record::decode(schema, &bytes).map_err(|_| ())?;
    Ok(Some((r, record::sha256(&bytes))))
}

fn texts(r: &Record, tag: u16) -> Result<Vec<String>, RecordError> {
    Ok(r.texts(tag)?.into_iter().map(safe_text).collect())
}

fn text(r: &Record, tag: u16) -> Result<String, RecordError> {
    r.text(tag).map(safe_text)
}

fn opt_text(r: &Record, tag: u16) -> Result<String, RecordError> {
    Ok(r.opt_text(tag)?.map(safe_text).unwrap_or_default())
}

fn opt_hex(r: &Record, tag: u16) -> Result<String, RecordError> {
    Ok(r.opt_b256(tag)?.map(|b| hex(&b)).unwrap_or_default())
}

/// The campaign verdict record as it holds it.
fn campaign(r: &Record, sha256: &[u8; 32]) -> Result<LiveCampaign, RecordError> {
    let cells = r
        .nested_all(lv::CELLS)?
        .iter()
        .map(|c| {
            use lv::cell as k;
            Ok(LiveCampaignCell {
                cell: text(c, k::CELL)?,
                enabled: c.bool(k::ENABLED)?,
                min_attempts: c.opt_u64(k::MIN_ATTEMPTS)?,
                attempts: c.u64(k::ATTEMPTS)?,
                passed: c.u64(k::PASSED)?,
                failed: c.u64(k::FAILED)?,
                undecided: c.u64(k::UNDECIDED)?,
                min_duration_s: c.opt_u64(k::MIN_DURATION_S)?,
                duration_s: c.u64(k::DURATION_S)?,
                verdict: text(c, k::VERDICT)?,
            })
        })
        .collect::<Result<_, RecordError>>()?;
    Ok(LiveCampaign {
        verdict: text(r, lv::VERDICT)?,
        release_evidence: text(r, lv::RELEASE)?,
        reasons: texts(r, lv::REASONS)?,
        planned: r.u64(lv::PLANNED)?,
        observed: r.u64(lv::OBSERVED)?,
        selected: r.u64(lv::SELECTED)?,
        admitted: r.u64(lv::ADMITTED)?,
        skipped: r.u64(lv::SKIPPED)?,
        written: r.u64(lv::WRITTEN)?,
        acquired: r.u64(lv::ACQUIRED)?,
        reverted: r.u64(lv::REVERTED)?,
        dropped: r.u64(lv::DROPPED)?,
        late: r.u64(lv::LATE)?,
        unresolved: r.u64(lv::UNRESOLVED)?,
        passed: r.u64(lv::PASSED)?,
        failed: r.u64(lv::FAILED)?,
        undecided: r.u64(lv::UNDECIDED)?,
        consecutive_misses: r.u64(lv::CONSECUTIVE_MISSES)?,
        before_precommit: r.u64(lv::BEFORE_PRECOMMIT)?,
        unverifiable: r.u64(lv::UNVERIFIABLE)?,
        covered_s: r.u64(lv::COVERED_S)?,
        uncovered_s: r.u64(lv::UNCOVERED_S)?,
        covered_ppm: r.u64(lv::COVERED_PPM)?,
        skip_reasons: texts(r, lv::SKIP_REASONS)?,
        cells,
        manifest_sha256: opt_hex(r, lv::MANIFEST_SHA256)?,
        precommit_sha256: opt_hex(r, lv::PRECOMMIT_SHA256)?,
        build_sha256: hex(&r.b256(lv::BUILD_SHA256)?),
        evaluated_unix_ms: r.u64(lv::PROVENANCE_OBSERVED_UNIX_MS)?,
        record_sha256: hex(sha256),
    })
}

/// One attempt record as it holds it.
fn attempt(r: &Record) -> Result<LiveAttempt, RecordError> {
    let adapters = r
        .nested_all(la::ADAPTERS)?
        .iter()
        .map(|a| {
            use la::adapter as k;
            Ok(LiveAttemptAdapter {
                builder: opt_text(a, k::BUILDER)?,
                outcome: text(a, k::OUTCOME)?,
                answer: opt_text(a, k::ANSWER)?,
            })
        })
        .collect::<Result<_, RecordError>>()?;
    Ok(LiveAttempt {
        ordinal: r.u64(la::ORDINAL)?,
        tx: format!("0x{}", hex(&r.b256(la::TX_HASH)?)),
        kind: text(r, la::KIND)?,
        cell: text(r, la::CELL)?,
        state: text(r, la::STATE)?,
        verdict: text(r, la::VERDICT)?,
        acquisition: text(r, la::ACQUISITION)?,
        canonical: text(r, la::CANONICAL)?,
        timing: text(r, la::TIMING)?,
        placement: text(r, la::PLACEMENT_VERDICT)?,
        capture: text(r, la::CAPTURE)?,
        local: text(r, la::LOCAL)?,
        economics: text(r, la::ECONOMICS)?,
        reasons: texts(r, la::REASONS)?,
        // Only the record's own COMPLETE is complete.
        window_complete: r.text(la::WINDOW)? == "COMPLETE",
        window_why: opt_text(r, la::WINDOW_WHY)?,
        trades: r.u64(la::TRADES)?,
        buys: r.u64(la::BUYS)?,
        sells: r.u64(la::SELLS)?,
        first_external_price: r
            .opt_u256(la::FIRST_EXTERNAL_PRICE)?
            .map(|w| record::u256_to_decimal(&w)),
        price_vs_first_external_ppm: r.opt_u64(la::PRICE_VS_FIRST_EXTERNAL_PPM)?,
        ingress_to_send_ns: r.opt_u64(la::INGRESS_TO_SEND_NS)?,
        select_to_send_ns: r.opt_u64(la::SELECT_TO_SEND_NS)?,
        send_to_ack_ns: r.opt_u64(la::SEND_TO_ACK_NS)?,
        attempts: r.u64(la::ATTEMPTS)?,
        acquired: r.u64(la::ACQUIRED)?,
        passed: r.u64(la::PASSED)?,
        failed: r.u64(la::FAILED)?,
        undecided: r.u64(la::UNDECIDED)?,
        adapters,
    })
}

/// The campaign verdict and the attempts of `rundir` into `out`: in
/// ordinal order, at most the campaign's total BUY attempt limit (every
/// real BUY attempt takes one of its slots), more is a warning.
pub fn project(rundir: &Path, out: &mut BscLiveState) {
    match record_at(&rundir.join(VERDICT_PATH), &lv::SCHEMA) {
        Ok(None) => {}
        Ok(Some((r, sha))) => match campaign(&r, &sha) {
            Ok(c) => out.campaign = Some(c),
            Err(_) => out.warnings.push("LIVE_CAMPAIGN_VERDICT_INVALID".into()),
        },
        Err(()) => out.warnings.push("LIVE_CAMPAIGN_VERDICT_INVALID".into()),
    }
    let dir = rundir.join(ATTEMPTS_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return,
        Err(_) => {
            out.warnings.push("LIVE_ATTEMPTS_UNREADABLE".into());
            return;
        }
    };
    let mut names: Vec<String> = Vec::new();
    for e in entries {
        match e {
            Ok(e) => {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.ends_with(".dbrec") {
                    names.push(name);
                }
            }
            Err(_) => out.warnings.push("LIVE_ATTEMPTS_UNREADABLE".into()),
        }
    }
    names.sort();
    let mut rows = Vec::with_capacity(names.len());
    for name in names {
        match record_at(&dir.join(&name), &la::SCHEMA).map(|r| r.map(|(r, _)| attempt(&r))) {
            Ok(Some(Ok(a))) => rows.push(a),
            Ok(None) => {}
            Ok(Some(Err(_))) | Err(()) => out
                .warnings
                .push(format!("LIVE_ATTEMPT_INVALID {}", safe_text(&name))),
        }
    }
    rows.sort_by_key(|a| a.ordinal);
    let bound = usize::try_from(MAX_BUY_ATTEMPTS_TOTAL).unwrap_or(usize::MAX);
    if rows.len() > bound {
        out.warnings
            .push(format!("LIVE_ATTEMPTS_BEYOND_LIMIT {}/{bound}", rows.len()));
        rows.truncate(bound);
    }
    out.attempts = rows;
}
