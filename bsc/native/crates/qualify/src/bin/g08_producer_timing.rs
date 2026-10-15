//! Offline producer-witness evaluator for the strict T0 -> T_buy product gate.
//!
//! It accepts only timestamps from the original successful producing execution
//! attempt. Receipt arrival, block timestamp, follower/archive replay and local
//! builder-send timing are explicitly not accepted as producer timing authority.

use darkbot_runner::acceptance_guard::sha256_hex;
use serde_json::{Value, json};
use std::{env, fs};

const LIMIT_NS: u128 = 1_000_000;
const WITNESS_SCHEMA: &str = "darkbot.producer-execution-witness.v1";
const G08_SCHEMA: &str = "darkbot.g08.mainnet-acquisition-ordering.v2";

fn req_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn load_bytes(path: &str) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn parse_json(bytes: &[u8], path: &str) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

fn req_str<'a>(v: &'a Value, ptr: &str) -> &'a str {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing/bad string at {ptr}"))
}

fn req_bool(v: &Value, ptr: &str) -> bool {
    v.pointer(ptr)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("missing/bad bool at {ptr}"))
}

fn req_u64(v: &Value, ptr: &str) -> u64 {
    v.pointer(ptr)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing/bad u64 at {ptr}"))
}

fn req_u128_string(v: &Value, ptr: &str) -> u128 {
    req_str(v, ptr)
        .parse::<u128>()
        .unwrap_or_else(|_| panic!("bad decimal u128 at {ptr}"))
}

fn norm(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn timing_bounds(
    t0_before: u128,
    t0_after: u128,
    tbuy_before: u128,
    tbuy_after: u128,
    remaining_uncertainty_ns: u128,
) -> Result<(u128, u128, u128), &'static str> {
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

fn main() {
    let g08_path = req_env("DARKBOT_G08_EVIDENCE");
    let witness_path = req_env("DARKBOT_G08_PRODUCER_WITNESS");
    let g08_bytes = load_bytes(&g08_path);
    let witness_bytes = load_bytes(&witness_path);
    let g08 = parse_json(&g08_bytes, &g08_path);
    let witness = parse_json(&witness_bytes, &witness_path);

    assert_eq!(req_str(&g08, "/schema"), G08_SCHEMA);
    assert_eq!(req_str(&witness, "/schema"), WITNESS_SCHEMA);
    assert_eq!(
        witness.pointer("/schema_version").and_then(Value::as_u64),
        Some(1),
        "producer witness schema_version must be 1"
    );

    let attempt_id = req_str(&g08, "/attempt_id");
    assert_eq!(
        req_str(&witness, "/attempt_id"),
        attempt_id,
        "producer witness belongs to another G0.7/G0.8 attempt"
    );
    assert_eq!(
        req_str(&g08, "/verdict"),
        "PASS",
        "strict timing PASS is only evaluated after G0.8 acquisition/ordering PASS"
    );
    assert!(req_bool(
        &g08,
        "/acquisition/positive_attributable_acquisition"
    ));
    assert!(req_bool(&g08, "/ordering/canonical"));
    assert!(req_bool(&g08, "/ordering/same_block"));
    assert!(req_bool(&g08, "/ordering/adjacent"));
    assert_eq!(
        g08.pointer("/ordering/successful_external_buys_between_count")
            .and_then(Value::as_u64),
        Some(0)
    );

    assert_eq!(
        norm(req_str(&witness, "/block_hash")),
        norm(req_str(&g08, "/ordering/block_hash")),
        "producer witness block does not match canonical G0.8 block"
    );
    assert_eq!(
        norm(req_str(&witness, "/launch_tx_hash")),
        norm(req_str(&g08, "/transactions/launch_tx")),
        "producer witness launch hash mismatch"
    );
    assert_eq!(
        norm(req_str(&witness, "/buy_tx_hash")),
        norm(req_str(&g08, "/transactions/buy_tx")),
        "producer witness BUY hash mismatch"
    );
    assert_eq!(
        req_u64(&witness, "/launch_tx_index"),
        req_u64(&g08, "/ordering/launch_index"),
        "producer witness launch index mismatch"
    );
    assert_eq!(
        req_u64(&witness, "/buy_tx_index"),
        req_u64(&g08, "/ordering/buy_index"),
        "producer witness BUY index mismatch"
    );
    assert_eq!(req_u64(&witness, "/launch_status"), 1);
    assert_eq!(req_u64(&witness, "/buy_status"), 1);
    assert_eq!(req_str(&witness, "/canonical_status"), "CANONICAL");
    assert_eq!(req_str(&witness, "/reorg_status"), "NONE");

    let parent_hash = req_str(&witness, "/parent_hash");
    assert!(
        parent_hash.starts_with("0x") && parent_hash.len() == 66,
        "producer witness parent_hash must be a 32-byte hash"
    );
    let acquisition_refs = witness
        .pointer("/acquisition_evidence_refs")
        .and_then(Value::as_array)
        .expect("producer witness acquisition_evidence_refs must be an array");
    assert!(
        !acquisition_refs.is_empty()
            && acquisition_refs
                .iter()
                .all(|v| v.as_str().is_some_and(|s| !s.is_empty())),
        "producer witness acquisition_evidence_refs must contain at least one non-empty reference"
    );

    let node_id = req_str(&witness, "/node_id");
    let producer_boot_id = req_str(&witness, "/producer_boot_id");
    let client_version = req_str(&witness, "/client_version");
    let client_commit = req_str(&witness, "/client_commit");
    let clock_id = req_str(&witness, "/clock_id");
    let clock_source = req_str(&witness, "/clock_source");
    let clock_raw_unit = req_str(&witness, "/clock_raw_unit");
    let clock_raw_resolution = req_str(&witness, "/clock_raw_resolution");
    let normalization_method = req_str(&witness, "/normalization_to_ns_method");
    let execution_attempt_id = req_str(&witness, "/execution_attempt_id");
    let t0_hook = req_str(&witness, "/semantic_hook_ids/T0");
    let tbuy_hook = req_str(&witness, "/semantic_hook_ids/T_buy");
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

    for ptr in [
        "/original_raw_timestamps/T0_before",
        "/original_raw_timestamps/T0_after",
        "/original_raw_timestamps/T_buy_before",
        "/original_raw_timestamps/T_buy_after",
    ] {
        assert!(
            !req_str(&witness, ptr).trim().is_empty(),
            "empty raw producer timestamp at {ptr}"
        );
    }
    let t0_before = req_u128_string(&witness, "/normalized_timestamps_ns/T0_before_ns");
    let t0_after = req_u128_string(&witness, "/normalized_timestamps_ns/T0_after_ns");
    let tbuy_before = req_u128_string(&witness, "/normalized_timestamps_ns/T_buy_before_ns");
    let tbuy_after = req_u128_string(&witness, "/normalized_timestamps_ns/T_buy_after_ns");
    let clock_resolution_ns = req_u128_string(&witness, "/clock_resolution_ns");
    let hook_read_overhead_ns = req_u128_string(&witness, "/hook_read_overhead_ns");
    let normalization_error_ns = req_u128_string(&witness, "/normalization_error_ns");
    let remaining_uncertainty_ns =
        req_u128_string(&witness, "/remaining_non_overlapping_uncertainty_ns");
    assert!(clock_resolution_ns > 0, "clock resolution must be positive");
    assert!(req_bool(
        &witness,
        "/remaining_uncertainty_excludes_bracketed_uncertainty"
    ));
    assert!(req_bool(
        &witness,
        "/clock_resolution_accounted_in_brackets_or_remaining_uncertainty"
    ));
    assert!(req_bool(
        &witness,
        "/hook_overhead_accounted_in_brackets_or_remaining_uncertainty"
    ));
    assert!(req_bool(
        &witness,
        "/normalization_error_accounted_in_remaining_uncertainty"
    ));

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

    let result = json!({
        "schema":"darkbot.g08.producer-timing-verdict.v1",
        "attempt_id":attempt_id,
        "verdict":verdict,
        "strict_limit_ns":LIMIT_NS.to_string(),
        "producer_identity":{
            "node_id":node_id,
            "producer_boot_id":producer_boot_id,
            "client_version":client_version,
            "client_commit":client_commit,
            "execution_attempt_id":execution_attempt_id
        },
        "clock":{
            "clock_id":clock_id,
            "clock_source":clock_source,
            "raw_unit":clock_raw_unit,
            "raw_resolution":clock_raw_resolution,
            "normalization_to_ns_method":normalization_method,
            "normalization_error_ns":normalization_error_ns.to_string(),
            "clock_resolution_ns":clock_resolution_ns.to_string(),
            "hook_read_overhead_ns":hook_read_overhead_ns.to_string(),
            "remaining_non_overlapping_uncertainty_ns":remaining_uncertainty_ns.to_string()
        },
        "hooks":{
            "T0":t0_hook,
            "T_buy":tbuy_hook
        },
        "original_raw_timestamps":witness["original_raw_timestamps"].clone(),
        "normalized_brackets_ns":{
            "T0_before_ns":t0_before.to_string(),
            "T0_after_ns":t0_after.to_string(),
            "T_buy_before_ns":tbuy_before.to_string(),
            "T_buy_after_ns":tbuy_after.to_string()
        },
        "derived":{
            "lower_bound_ns":lower_bound_ns.to_string(),
            "bracket_upper_ns":bracket_upper_ns.to_string(),
            "conservative_upper_bound_ns":upper_bound_ns.to_string(),
            "formula":"T_buy_after - T0_before + remaining_non_overlapping_uncertainty_ns",
            "pass_rule":"conservative_upper_bound_ns < 1000000"
        },
        "evidence_hashes":{
            "g08_sha256":sha256_hex(&g08_bytes),
            "producer_witness_sha256":sha256_hex(&witness_bytes)
        },
        "authority":"ORIGINAL_SUCCESSFUL_PRODUCER_EXECUTION_ATTEMPT",
        "explicitly_not_authority":[
            "receipt first-seen time",
            "block timestamp",
            "builder ACK time",
            "local T_send",
            "follower/archive replay duration",
            "OHLC/price interpolation"
        ]
    });

    let pretty = serde_json::to_string_pretty(&result).unwrap();
    println!("{pretty}");
    if let Ok(path) = env::var("DARKBOT_G08_TIMING_EVIDENCE_PATH") {
        fs::write(&path, format!("{pretty}\n"))
            .unwrap_or_else(|e| panic!("write timing evidence {path}: {e}"));
    }
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
}
