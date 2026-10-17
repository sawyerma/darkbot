//! Offline producer-witness evaluator for the strict T0 -> T_buy product gate.
//!
//! It accepts only timestamps from the original successful producing execution
//! attempt. Receipt arrival, block timestamp, follower/archive replay and local
//! builder-send timing are explicitly not accepted as producer timing authority.
//!
//! Inputs and output are binary records (DBREC001): the G0.8 result, the
//! producer execution witness and the timing verdict. No JSON is read or
//! written.

use darkbot_bsc_record::schema::g08::{producer_witness as w, result, timing_verdict as v};
use darkbot_bsc_record::{Draft, Record, RecordError, Value, decode};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

const LIMIT_NS: u64 = 1_000_000;

fn req_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn load_bytes(path: &str) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// A field of a decoded input; decoding already checked that every required
/// field is present with its kind.
fn field<T>(value: Result<T, RecordError>) -> T {
    value.unwrap_or_else(|e| panic!("{e}"))
}

/// Writes `bytes` to a staging file beside `path`, syncs it and renames it
/// over `path`: a reader sees the previous file or the complete new one.
fn write_atomic(path: &str, bytes: &[u8]) -> Result<(), String> {
    let target = Path::new(path);
    let name = target
        .file_name()
        .ok_or_else(|| format!("{path} names no file"))?;
    let tmp = target.with_file_name(format!(
        ".{}.tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let staged = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .and_then(|()| fs::rename(&tmp, target));
    if let Err(e) = staged {
        let _ = fs::remove_file(&tmp);
        return Err(format!("{path}: {e}"));
    }
    let dir = match target.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("{path} replaced but not yet durable: {e}"))
}

fn timing_bounds(
    t0_before: u64,
    t0_after: u64,
    tbuy_before: u64,
    tbuy_after: u64,
    remaining_uncertainty_ns: u64,
) -> Result<(u64, u64, u64), &'static str> {
    if t0_before > t0_after {
        return Err("T0 bracket inverted");
    }
    if tbuy_before > tbuy_after {
        return Err("T_buy bracket inverted");
    }
    if t0_after > tbuy_before {
        return Err("producer completion brackets overlap or are out of order");
    }
    let lower = tbuy_before
        .checked_sub(t0_after)
        .ok_or("lower bound underflow")?;
    let bracket_upper = tbuy_after
        .checked_sub(t0_before)
        .ok_or("upper bracket underflow")?;
    let upper = bracket_upper
        .checked_add(remaining_uncertainty_ns)
        .ok_or("timing uncertainty overflow")?;
    Ok((lower, bracket_upper, upper))
}

/// The encoded verdict and its decoded record, whose view carries the
/// file hash.
fn checked(draft: &Draft) -> (Vec<u8>, Record) {
    let bytes = draft.encode().expect("encode timing verdict record");
    let record = decode(draft.schema(), &bytes).expect("decode timing verdict record");
    (bytes, record)
}

fn main() {
    let g08_path = req_env("DARKBOT_G08_EVIDENCE");
    let witness_path = req_env("DARKBOT_G08_PRODUCER_WITNESS");
    let g08_bytes = load_bytes(&g08_path);
    let witness_bytes = load_bytes(&witness_path);
    // The record types and versions replace the former schema-name and
    // schema_version checks.
    let g08 = decode(&result::SCHEMA, &g08_bytes)
        .unwrap_or_else(|e| panic!("G0.8 result {g08_path}: {e}"));
    let witness = decode(&w::SCHEMA, &witness_bytes)
        .unwrap_or_else(|e| panic!("producer witness {witness_path}: {e}"));

    let attempt_id = field(g08.text(result::ATTEMPT_ID));
    assert_eq!(
        field(witness.text(w::ATTEMPT_ID)),
        attempt_id,
        "producer witness belongs to another G0.7/G0.8 attempt"
    );
    assert_eq!(
        field(g08.text(result::VERDICT)),
        "PASS",
        "strict timing PASS is only evaluated after G0.8 acquisition/ordering PASS"
    );
    assert!(field(g08.bool(result::POSITIVE_ATTRIBUTABLE_ACQUISITION)));
    assert!(field(g08.bool(result::CANONICAL)));
    assert!(field(g08.bool(result::SAME_BLOCK)));
    assert!(field(g08.bool(result::ADJACENT)));
    assert_eq!(
        field(g08.opt_u64(result::EXTERNAL_BUYS_BETWEEN_COUNT)),
        Some(0)
    );

    assert_eq!(
        field(witness.b256(w::BLOCK_HASH)),
        field(g08.b256(result::BLOCK_HASH)),
        "producer witness block does not match canonical G0.8 block"
    );
    assert_eq!(
        field(witness.b256(w::LAUNCH_TX_HASH)),
        field(g08.b256(result::LAUNCH_TX)),
        "producer witness launch hash mismatch"
    );
    assert_eq!(
        field(witness.b256(w::BUY_TX_HASH)),
        field(g08.b256(result::BUY_TX)),
        "producer witness BUY hash mismatch"
    );
    assert_eq!(
        field(witness.u64(w::LAUNCH_TX_INDEX)),
        field(g08.u64(result::LAUNCH_INDEX)),
        "producer witness launch index mismatch"
    );
    assert_eq!(
        field(witness.u64(w::BUY_TX_INDEX)),
        field(g08.u64(result::BUY_INDEX)),
        "producer witness BUY index mismatch"
    );
    assert_eq!(field(witness.u64(w::LAUNCH_STATUS)), 1);
    assert_eq!(field(witness.u64(w::BUY_STATUS)), 1);
    assert_eq!(field(witness.text(w::CANONICAL_STATUS)), "CANONICAL");
    assert_eq!(field(witness.text(w::REORG_STATUS)), "NONE");

    // A 32-byte word by its kind: decoding refused any other length.
    let _parent_hash = field(witness.b256(w::PARENT_HASH));
    let acquisition_refs = field(witness.texts(w::ACQUISITION_EVIDENCE_REFS));
    assert!(
        !acquisition_refs.is_empty() && acquisition_refs.iter().all(|s| !s.is_empty()),
        "producer witness acquisition_evidence_refs must contain at least one non-empty reference"
    );

    let node_id = field(witness.text(w::NODE_ID));
    let producer_boot_id = field(witness.text(w::PRODUCER_BOOT_ID));
    let client_version = field(witness.text(w::CLIENT_VERSION));
    let client_commit = field(witness.text(w::CLIENT_COMMIT));
    let clock_id = field(witness.text(w::CLOCK_ID));
    let clock_source = field(witness.text(w::CLOCK_SOURCE));
    let clock_raw_unit = field(witness.text(w::CLOCK_RAW_UNIT));
    let clock_raw_resolution = field(witness.text(w::CLOCK_RAW_RESOLUTION));
    let normalization_method = field(witness.text(w::NORMALIZATION_TO_NS_METHOD));
    let execution_attempt_id = field(witness.text(w::EXECUTION_ATTEMPT_ID));
    let t0_hook = field(witness.text(w::T0_HOOK));
    let tbuy_hook = field(witness.text(w::T_BUY_HOOK));
    for (name, value) in [
        ("node_id", node_id),
        ("producer_boot_id", producer_boot_id),
        ("client_version", client_version),
        ("client_commit", client_commit),
        ("clock_id", clock_id),
        ("clock_source", clock_source),
        ("clock_raw_unit", clock_raw_unit),
        ("clock_raw_resolution", clock_raw_resolution),
        ("normalization_to_ns_method", normalization_method),
        ("execution_attempt_id", execution_attempt_id),
        ("T0 hook", t0_hook),
        ("T_buy hook", tbuy_hook),
    ] {
        assert!(!value.trim().is_empty(), "producer witness {name} is empty");
    }

    let raw_t0_before = field(witness.text(w::RAW_T0_BEFORE));
    let raw_t0_after = field(witness.text(w::RAW_T0_AFTER));
    let raw_tbuy_before = field(witness.text(w::RAW_T_BUY_BEFORE));
    let raw_tbuy_after = field(witness.text(w::RAW_T_BUY_AFTER));
    for (name, value) in [
        ("T0_before", raw_t0_before),
        ("T0_after", raw_t0_after),
        ("T_buy_before", raw_tbuy_before),
        ("T_buy_after", raw_tbuy_after),
    ] {
        assert!(
            !value.trim().is_empty(),
            "empty raw producer timestamp at original_raw_timestamps.{name}"
        );
    }
    let t0_before = field(witness.u64(w::T0_BEFORE_NS));
    let t0_after = field(witness.u64(w::T0_AFTER_NS));
    let tbuy_before = field(witness.u64(w::T_BUY_BEFORE_NS));
    let tbuy_after = field(witness.u64(w::T_BUY_AFTER_NS));
    let clock_resolution_ns = field(witness.u64(w::CLOCK_RESOLUTION_NS));
    let hook_read_overhead_ns = field(witness.u64(w::HOOK_READ_OVERHEAD_NS));
    let normalization_error_ns = field(witness.u64(w::NORMALIZATION_ERROR_NS));
    let remaining_uncertainty_ns = field(witness.u64(w::REMAINING_UNCERTAINTY_NS));
    assert!(clock_resolution_ns > 0, "clock resolution must be positive");
    assert!(field(witness.bool(w::REMAINING_EXCLUDES_BRACKETED)));
    assert!(field(witness.bool(w::CLOCK_RESOLUTION_ACCOUNTED)));
    assert!(field(witness.bool(w::HOOK_OVERHEAD_ACCOUNTED)));
    assert!(field(witness.bool(w::NORMALIZATION_ERROR_ACCOUNTED)));

    let (lower_bound_ns, bracket_upper_ns, upper_bound_ns) = timing_bounds(
        t0_before,
        t0_after,
        tbuy_before,
        tbuy_after,
        remaining_uncertainty_ns,
    )
    .expect("invalid producer timing brackets");

    let verdict = if upper_bound_ns < LIMIT_NS {
        "PASS"
    } else {
        "FAIL"
    };

    let mut out = Draft::new(&v::SCHEMA);
    out.set(v::ATTEMPT_ID, attempt_id)
        .set(v::VERDICT, verdict)
        .set(v::STRICT_LIMIT_NS, LIMIT_NS)
        .set(v::NODE_ID, node_id)
        .set(v::PRODUCER_BOOT_ID, producer_boot_id)
        .set(v::CLIENT_VERSION, client_version)
        .set(v::CLIENT_COMMIT, client_commit)
        .set(v::EXECUTION_ATTEMPT_ID, execution_attempt_id)
        .set(v::CLOCK_ID, clock_id)
        .set(v::CLOCK_SOURCE, clock_source)
        .set(v::RAW_UNIT, clock_raw_unit)
        .set(v::RAW_RESOLUTION, clock_raw_resolution)
        .set(v::NORMALIZATION_TO_NS_METHOD, normalization_method)
        .set(v::NORMALIZATION_ERROR_NS, normalization_error_ns)
        .set(v::CLOCK_RESOLUTION_NS, clock_resolution_ns)
        .set(v::HOOK_READ_OVERHEAD_NS, hook_read_overhead_ns)
        .set(v::REMAINING_UNCERTAINTY_NS, remaining_uncertainty_ns)
        .set(v::T0_HOOK, t0_hook)
        .set(v::T_BUY_HOOK, tbuy_hook)
        .set(v::RAW_T0_BEFORE, raw_t0_before)
        .set(v::RAW_T0_AFTER, raw_t0_after)
        .set(v::RAW_T_BUY_BEFORE, raw_tbuy_before)
        .set(v::RAW_T_BUY_AFTER, raw_tbuy_after)
        .set(v::T0_BEFORE_NS, t0_before)
        .set(v::T0_AFTER_NS, t0_after)
        .set(v::T_BUY_BEFORE_NS, tbuy_before)
        .set(v::T_BUY_AFTER_NS, tbuy_after)
        .set(v::LOWER_BOUND_NS, lower_bound_ns)
        .set(v::BRACKET_UPPER_NS, bracket_upper_ns)
        .set(v::CONSERVATIVE_UPPER_BOUND_NS, upper_bound_ns)
        .set(
            v::FORMULA,
            "T_buy_after - T0_before + remaining_non_overlapping_uncertainty_ns",
        )
        .set(v::PASS_RULE, "conservative_upper_bound_ns < 1000000")
        // SHA-256 of the exact input files, as decoded.
        .set(
            v::G08_SHA256,
            Value::B256(darkbot_bsc_record::sha256(&g08_bytes)),
        )
        .set(
            v::PRODUCER_WITNESS_SHA256,
            Value::B256(darkbot_bsc_record::sha256(&witness_bytes)),
        )
        .set(
            v::AUTHORITY,
            "ORIGINAL_SUCCESSFUL_PRODUCER_EXECUTION_ATTEMPT",
        );
    for not_authority in [
        "receipt first-seen time",
        "block timestamp",
        "builder ACK time",
        "local T_send",
        "follower/archive replay duration",
        "OHLC/price interpolation",
    ] {
        out.push(v::EXPLICITLY_NOT_AUTHORITY, not_authority);
    }

    let (bytes, record) = checked(&out);
    // Written before anything is printed: a verdict that could not be
    // written never appears on stdout.
    if let Ok(path) = env::var("DARKBOT_G08_TIMING_EVIDENCE_PATH") {
        write_atomic(&path, &bytes).unwrap_or_else(|e| panic!("write timing evidence {e}"));
    }
    print!("{}", record.render());
    if verdict != "PASS" {
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_boundary_accepts_999999_and_rejects_1000000() {
        let (_, _, pass) = timing_bounds(0, 0, 999_999, 999_999, 0).unwrap();
        let (_, _, fail) = timing_bounds(0, 0, 1_000_000, 1_000_000, 0).unwrap();
        assert!(pass < LIMIT_NS);
        assert!(fail >= LIMIT_NS);
    }

    #[test]
    fn broken_or_overlapping_brackets_give_no_verdict() {
        assert!(timing_bounds(10, 5, 20, 30, 0).is_err());
        assert!(timing_bounds(0, 0, 30, 20, 0).is_err());
        // T0 completion bracket ends after T_buy's starts: no ordering proof.
        assert!(timing_bounds(0, 30, 20, 40, 0).is_err());
    }

    #[test]
    fn uncertainty_turns_a_sub_ms_bracket_into_fail() {
        let (_, bracket, upper) = timing_bounds(0, 0, 900_000, 900_000, 100_000).unwrap();
        assert!(bracket < LIMIT_NS);
        assert!(upper >= LIMIT_NS);
    }

    #[test]
    fn remaining_uncertainty_is_added_once() {
        let (_, bracket, upper) = timing_bounds(100, 120, 700, 800, 50).unwrap();
        assert_eq!(bracket, 700);
        assert_eq!(upper, 750);
    }

    #[test]
    fn uncertainty_overflow_gives_no_verdict() {
        assert!(timing_bounds(0, 0, 0, u64::MAX, 1).is_err());
    }
}
