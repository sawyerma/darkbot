//! REV.1.0 R2b: the LIVE campaign on the LIVE tab (`bsc.live.state`): the
//! campaign verdict and the attempt records `bsc_runtime live attempts`
//! wrote, read and projected typed. The evaluator (`live_attempts`)
//! decides; nothing is evaluated or inferred here. A record that is absent
//! is no row; one that is unreadable, a link, not of its schema, not named
//! by its own transaction or beyond its share of the state is a warning,
//! never a value. Every read is bounded: at most the campaign's total BUY
//! attempt limit and one more attempt record, every record within its
//! share of what the rest of the tab leaves of the state bound, every kind
//! of damage one counted warning.

use super::projection::{read, safe_text};
use super::publisher::MAX_STATE_BYTES;
use super::wire::bsc::*;
use darkbot_bsc_execution::live_attempts::{
    ATTEMPT_FILE_SUFFIX, ATTEMPTS_DIR, VERDICT_PATH, attempt_file_tx,
};
use darkbot_bsc_record::schema::bsc_runtime::{live_attempt as la, live_campaign_verdict as lv};
use darkbot_bsc_record::{self as record, Record, RecordError, Schema, hex};
use prost::Message;
use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::Path;

/// The attempts the LIVE tab shows: every real BUY attempt takes one of the
/// campaign's total slots (at most `buy_attempts_total_max`, Audit E 3.1),
/// so more records than slots are a warning; without a readable
/// configuration none is shown, and the tab says so (review E3 L4).
pub(super) fn max_attempts(out: &BscLiveState) -> Option<usize> {
    out.buy_attempts_total_max.map(|m| m as usize)
}

/// No attempt shown: the configured maximum is unknown.
pub const LIVE_ATTEMPTS_MAX_UNKNOWN: &str = "LIVE_ATTEMPTS_MAX_UNKNOWN: no attempt shown without the runtime configuration's BSC_BUY_ATTEMPTS_TOTAL_MAX";

/// One record's share of `left`, what the rest of the LIVE tab leaves of
/// the publisher's state bound (encoded bytes): the campaign verdict and
/// each of the `max` attempts takes at most one share, one more share is
/// this section's warnings and framing (review L-1).
pub(super) fn share(left: usize, max: usize) -> usize {
    left / (max + 2)
}

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

fn text(r: &Record, tag: u16) -> Result<String, RecordError> {
    r.text(tag).map(safe_text)
}

fn opt_text(r: &Record, tag: u16) -> Result<String, RecordError> {
    Ok(r.opt_text(tag)?.map(safe_text).unwrap_or_default())
}

fn opt_hex(r: &Record, tag: u16) -> Result<String, RecordError> {
    Ok(r.opt_b256(tag)?.map(|b| hex(&b)).unwrap_or_default())
}

fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", hex(bytes))
}

fn opt_hex0x(r: &Record, tag: u16) -> Result<String, RecordError> {
    Ok(r.opt_b256(tag)?.map(|b| hex0x(&b)).unwrap_or_default())
}

fn opt_decimal(r: &Record, tag: u16) -> Result<Option<String>, RecordError> {
    Ok(r.opt_u256(tag)?.map(|w| record::u256_to_decimal(&w)))
}

/// `list` of `msg` filled with the texts in their order while `msg` stays
/// within `share`; how many did not fit (counted, never copied).
fn fill<M: Message>(
    msg: &mut M,
    list: fn(&mut M) -> &mut Vec<String>,
    texts: &[&str],
    share: usize,
) -> usize {
    let mut used = msg.encoded_len();
    // The list's key: what an empty element adds beyond its length byte.
    list(msg).push(String::new());
    let key = msg.encoded_len() - used - prost::length_delimiter_len(0);
    list(msg).pop();
    for (kept, t) in texts.iter().enumerate() {
        let s = safe_text(t);
        let cost = key + prost::length_delimiter_len(s.len()) + s.len();
        if used + cost > share {
            return texts.len() - kept;
        }
        used += cost;
        list(msg).push(s);
    }
    0
}

/// The campaign verdict record as it holds it, and how many of its reasons
/// did not fit its share; none when the rest alone is beyond it.
fn campaign(
    r: &Record,
    sha256: &[u8; 32],
    share: usize,
) -> Result<Option<(LiveCampaign, usize)>, RecordError> {
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
    let mut c = LiveCampaign {
        verdict: text(r, lv::VERDICT)?,
        release_evidence: text(r, lv::RELEASE)?,
        reasons: Vec::new(),
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
        skip_reasons: Vec::new(),
        cells,
        manifest_sha256: opt_hex(r, lv::MANIFEST_SHA256)?,
        precommit_sha256: opt_hex(r, lv::PRECOMMIT_SHA256)?,
        build_sha256: hex(&r.b256(lv::BUILD_SHA256)?),
        evaluated_unix_ms: r.u64(lv::PROVENANCE_OBSERVED_UNIX_MS)?,
        record_sha256: hex(sha256),
    };
    if c.encoded_len() > share {
        return Ok(None);
    }
    let cut = fill(&mut c, |c| &mut c.reasons, &r.texts(lv::REASONS)?, share)
        + fill(
            &mut c,
            |c| &mut c.skip_reasons,
            &r.texts(lv::SKIP_REASONS)?,
            share,
        );
    Ok(Some((c, cut)))
}

/// One attempt record as it holds it, and how many of its reasons did not
/// fit its share; none when it is not the record of the transaction `name`
/// names, or the rest alone is beyond its share.
fn attempt(
    r: &Record,
    name: &str,
    share: usize,
) -> Result<Option<(LiveAttempt, usize)>, RecordError> {
    let tx = r.b256(la::TX_HASH)?;
    if attempt_file_tx(name) != Some(tx) {
        return Ok(None);
    }
    let adapters = r
        .nested_all(la::ADAPTERS)?
        .iter()
        .map(|a| {
            use la::adapter as k;
            Ok(LiveAttemptAdapter {
                builder: opt_text(a, k::BUILDER)?,
                outcome: text(a, k::OUTCOME)?,
                answer: opt_text(a, k::ANSWER)?,
                adapter: a.u64(k::ADAPTER)?,
                boundary: text(a, k::BOUNDARY)?,
                qualified: a.bool(k::QUALIFIED)?,
                t_write_check_ns: a.opt_u64(k::T_WRITE_CHECK_NS)?,
                t_boundary_ns: a.opt_u64(k::T_BOUNDARY_NS)?,
                t_queue_ns: a.opt_u64(k::T_QUEUE_NS)?,
                t_answer_ns: a.opt_u64(k::T_ANSWER_NS)?,
            })
        })
        .collect::<Result<_, RecordError>>()?;
    let mut a = LiveAttempt {
        ordinal: r.u64(la::ORDINAL)?,
        tx: hex0x(&tx),
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
        reasons: Vec::new(),
        window_why: opt_text(r, la::WINDOW_WHY)?,
        trades: r.u64(la::TRADES)?,
        buys: r.u64(la::BUYS)?,
        sells: r.u64(la::SELLS)?,
        first_external_price: opt_decimal(r, la::FIRST_EXTERNAL_PRICE)?,
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
        // The record's own words, COMPLETE or INCOMPLETE, never a reading.
        window: text(r, la::WINDOW)?,
        attempt: hex0x(&r.b256(la::ATTEMPT)?),
        run: r.opt_u64(la::RUN)?,
        token: hex0x(&r.address(la::TOKEN)?),
        launch_hash: opt_hex0x(r, la::LAUNCH_HASH)?,
        trigger: opt_text(r, la::TRIGGER)?,
        block_number: r.opt_u64(la::BLOCK_NUMBER)?,
        block_hash: opt_hex0x(r, la::BLOCK_HASH)?,
        block_ms: r.opt_u64(la::BLOCK_MS)?,
        tokens: opt_decimal(r, la::TOKENS)?,
        eth_wei: opt_decimal(r, la::ETH_WEI)?,
        gas_wei: opt_decimal(r, la::GAS_WEI)?,
        own_price: opt_decimal(r, la::OWN_PRICE)?,
        placed: opt_text(r, la::PLACEMENT)?,
        launch_index: r.opt_u64(la::LAUNCH_INDEX)?,
        own_index: r.opt_u64(la::OWN_INDEX)?,
        buys_before: r.opt_u64(la::BUYS_BEFORE)?,
        clock_id: r.opt_u64(la::CLOCK_ID)?,
        t_ingress_ns: r.opt_u64(la::T_INGRESS_NS)?,
        t_select_ns: r.opt_u64(la::T_SELECT_NS)?,
        t_send_ns: r.opt_u64(la::T_SEND_NS)?,
        t_ack_ns: r.opt_u64(la::T_ACK_NS)?,
        deadline_misses: r.opt_u64(la::DEADLINE_MISSES)?,
        window_ms: r.u64(la::WINDOW_MS)?,
        launch_block: r.opt_u64(la::LAUNCH_BLOCK)?,
        launch_ms: r.opt_u64(la::LAUNCH_MS)?,
        window_end_block: r.opt_u64(la::WINDOW_END_BLOCK)?,
        t0_exact: r.bool(la::T0_EXACT)?,
        first_external_buy_tx: opt_hex0x(r, la::FIRST_EXTERNAL_BUY_TX)?,
        first_external_buy_block: r.opt_u64(la::FIRST_EXTERNAL_BUY_BLOCK)?,
        first_external_buy_index: r.opt_u64(la::FIRST_EXTERNAL_BUY_INDEX)?,
        buy_wei: record::u256_to_decimal(&r.u256(la::BUY_WEI)?),
        sell_wei: record::u256_to_decimal(&r.u256(la::SELL_WEI)?),
        evaluated_unix_ms: r.u64(la::PROVENANCE_OBSERVED_UNIX_MS)?,
    };
    if a.encoded_len() > share {
        return Ok(None);
    }
    let cut = fill(&mut a, |a| &mut a.reasons, &r.texts(la::REASONS)?, share);
    Ok(Some((a, cut)))
}

/// One warning for every case of a kind: how many, and the first by name.
#[derive(Default)]
struct Counted {
    n: usize,
    first: Option<String>,
}

impl Counted {
    fn note(&mut self, what: &str, n: usize) {
        self.n += n;
        if self.first.as_deref().is_none_or(|f| what < f) {
            self.first = Some(what.to_owned());
        }
    }

    fn warn(self, code: &str, out: &mut Vec<String>) {
        if let Some(first) = self.first {
            out.push(format!("{code} {}, first {}", self.n, safe_text(&first)));
        }
    }
}

/// The campaign verdict and the attempts of `rundir` into `out`, after the
/// rest of the LIVE tab: in ordinal order, at most the campaign's total BUY
/// attempt limit (every real BUY attempt takes one of its slots), more is a
/// warning; within what the rest of the tab leaves of the state bound, and
/// none of it when the section as a whole does not fit (counted; when not even
/// that count fits, the rest of the tab alone, within the bound).
pub fn project(rundir: &Path, out: &mut BscLiveState) {
    let warned = out.warnings.len();
    let max = max_attempts(out);
    section(
        rundir,
        out,
        share(
            MAX_STATE_BYTES.saturating_sub(out.encoded_len()),
            max.unwrap_or(0),
        ),
        max,
    );
    if out.encoded_len() > MAX_STATE_BYTES {
        let records = usize::from(out.campaign.take().is_some()) + out.attempts.len();
        let warnings = out.warnings.len() - warned;
        out.attempts.clear();
        out.warnings.truncate(warned);
        out.warnings.push(format!(
            "LIVE_CAMPAIGN_BEYOND_STATE {records} records, {warnings} warnings"
        ));
        if out.encoded_len() > MAX_STATE_BYTES {
            out.warnings.pop();
        }
    }
}

fn section(rundir: &Path, out: &mut BscLiveState, share: usize, max: Option<usize>) {
    match record_at(&rundir.join(VERDICT_PATH), &lv::SCHEMA) {
        Ok(None) => {}
        Ok(Some((r, sha))) => match campaign(&r, &sha, share) {
            Ok(Some((c, cut))) => {
                if cut > 0 {
                    out.warnings
                        .push(format!("LIVE_CAMPAIGN_REASONS_BEYOND_LIMIT {cut}"));
                }
                out.campaign = Some(c);
            }
            Ok(None) | Err(_) => out.warnings.push("LIVE_CAMPAIGN_VERDICT_INVALID".into()),
        },
        Err(()) => out.warnings.push("LIVE_CAMPAIGN_VERDICT_INVALID".into()),
    }
    let dir = rundir.join(ATTEMPTS_DIR);
    // A real directory, never a link to one.
    match std::fs::symlink_metadata(&dir) {
        Err(e) if e.kind() == ErrorKind::NotFound => return,
        Ok(m) if m.is_dir() => {}
        _ => {
            out.warnings.push("LIVE_ATTEMPTS_UNREADABLE".into());
            return;
        }
    }
    let Some(max) = max else {
        out.warnings.push(LIVE_ATTEMPTS_MAX_UNKNOWN.into());
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        out.warnings.push("LIVE_ATTEMPTS_UNREADABLE".into());
        return;
    };
    // The names only; at most the limit and one more (the first by
    // transaction) are read, the others counted.
    let (mut names, mut total, mut unreadable) = (BTreeSet::new(), 0usize, 0usize);
    let mut invalid = Counted::default();
    for e in entries {
        let Ok(e) = e else {
            unreadable += 1;
            continue;
        };
        let name = e.file_name().to_string_lossy().into_owned();
        // Not an attempt record's name (the writer's `.dbrec.tmp` too).
        if !name.ends_with(ATTEMPT_FILE_SUFFIX) {
            continue;
        }
        if attempt_file_tx(&name).is_none() {
            invalid.note(&name, 1);
            continue;
        }
        total += 1;
        names.insert(name);
        if names.len() > max + 1 {
            names.pop_last();
        }
    }
    if unreadable > 0 {
        out.warnings
            .push(format!("LIVE_ATTEMPTS_UNREADABLE {unreadable} entries"));
    }
    let mut rows = Vec::new();
    for name in names {
        match record_at(&dir.join(&name), &la::SCHEMA)
            .map(|r| r.map(|(r, _)| attempt(&r, &name, share)))
        {
            Ok(None) => {}
            Ok(Some(Ok(Some((a, n))))) => rows.push((a, name, n)),
            Ok(Some(Ok(None) | Err(_))) | Err(()) => invalid.note(&name, 1),
        }
    }
    // The first by ordinal (a tie by transaction) of those read; the
    // warnings below are of the attempts shown (re-check F4).
    rows.sort_by(|(a, ..), (b, ..)| a.ordinal.cmp(&b.ordinal).then_with(|| a.tx.cmp(&b.tx)));
    rows.truncate(max);
    // One evaluation writes every record with one time: the verdict's, and
    // without a verdict the newest attempt's.
    let evaluation = out
        .campaign
        .as_ref()
        .map(|c| c.evaluated_unix_ms)
        .or_else(|| rows.iter().map(|(a, ..)| a.evaluated_unix_ms).max());
    let (mut cut, mut other, mut tie) =
        (Counted::default(), Counted::default(), Counted::default());
    for (a, name, n) in &rows {
        if *n > 0 {
            cut.note(name, *n);
        }
        if evaluation.is_some_and(|ms| ms != a.evaluated_unix_ms) {
            other.note(name, 1);
        }
    }
    for w in rows.windows(2) {
        if w[0].0.ordinal == w[1].0.ordinal {
            tie.note(&w[1].1, 1);
        }
    }
    invalid.warn("LIVE_ATTEMPT_INVALID", &mut out.warnings);
    cut.warn("LIVE_ATTEMPT_REASONS_BEYOND_LIMIT", &mut out.warnings);
    other.warn("LIVE_ATTEMPT_OF_ANOTHER_EVALUATION", &mut out.warnings);
    tie.warn("LIVE_ATTEMPT_DUPLICATE_ORDINAL", &mut out.warnings);
    if total > max {
        out.warnings.push(format!(
            "LIVE_ATTEMPTS_BEYOND_LIMIT {total}/{max}, shown: the first {max} by ordinal of the first {} by transaction",
            max + 1
        ));
    }
    out.attempts = rows.into_iter().map(|(a, ..)| a).collect();
}
