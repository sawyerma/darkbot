//! G0.8 records are binary end to end. Source checks pin that JSON exists in
//! the G0.8 binaries only inside the JSON-RPC wire boundary of
//! `g08_mainnet_verify`, and that G0.8 reads exactly the twelve G0.7 fields.
//! The real `g08_producer_timing` binary reads a binary G0.8 result and a
//! binary producer witness and writes a binary timing verdict; JSON inputs
//! and mismatched witnesses are refused without output.

use darkbot_bsc_record::schema::g08::{producer_witness as w, result, timing_verdict as v};
use darkbot_bsc_record::{Draft, Kind, Presence, Schema, Value, decode, sha256};
use std::{fs, process::Command};

const VERIFY: &str = include_str!("../src/bin/g08_mainnet_verify.rs");
const TIMING: &str = include_str!("../src/bin/g08_producer_timing.rs");
const TIMING_BIN: &str = env!("CARGO_BIN_EXE_g08_producer_timing");

const WIRE_START: &str = "\nmod wire {";
const WIRE_END: &str = "// ---- end of the JSON-RPC wire boundary";

#[test]
fn serde_json_occurs_only_inside_the_rpc_wire_boundary() {
    let start = VERIFY.find(WIRE_START).expect("wire module");
    let end = VERIFY.find(WIRE_END).expect("wire boundary end marker");
    assert!(start < end);
    let uses: Vec<usize> = VERIFY.match_indices("serde_json").map(|(i, _)| i).collect();
    assert!(!uses.is_empty());
    for i in uses {
        assert!(
            start < i && i < end,
            "serde_json outside the wire boundary at byte {i}"
        );
    }
    // No JSON is built anywhere: request bodies are templates.
    assert!(!VERIFY.contains("json!"));
    // Nothing public of the boundary names a JSON type.
    for line in VERIFY[start..end].lines() {
        if line.trim_start().starts_with("pub ") {
            assert!(!line.contains("Json"), "{line}");
        }
    }
    assert!(!TIMING.contains("serde_json") && !TIMING.contains("json!"));
}

#[test]
fn g08_reads_exactly_the_twelve_g07_fields() {
    // The record type and version are the twelfth: one decode with the
    // G0.7 evidence schema.
    assert_eq!(VERIFY.matches("decode(&evidence::SCHEMA,").count(), 1);
    let mut reads: Vec<&str> = VERIFY
        .split("g07.")
        .skip(1)
        .filter_map(|rest| rest.split_once("(evidence::"))
        .filter(|(accessor, _)| {
            accessor
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .filter_map(|(_, rest)| rest.split(')').next())
        .collect();
    reads.sort_unstable();
    assert_eq!(
        reads,
        [
            "ATTEMPT_ID",
            "BUYER",
            "BUY_TX_HASH",
            "BUY_VALUE_WEI",
            "CAPTURE_COMPLETE",
            "CHAIN_ID",
            "LAUNCHER",
            "LAUNCH_TX_HASH",
            "MIN_OUTPUT",
            "PORTAL",
            "TOKEN",
        ]
    );
}

/// Every required field of `schema` with a neutral value.
fn filled(schema: &'static Schema) -> Draft {
    let mut d = Draft::new(schema);
    for f in schema
        .fields
        .iter()
        .filter(|f| f.presence == Presence::Required)
    {
        let value = match f.kind {
            Kind::Bool => Value::Bool(false),
            Kind::U64 => Value::U64(0),
            Kind::I64 => Value::I64(0),
            Kind::U256 => Value::U256([0; 32]),
            Kind::Address => Value::Address([0; 20]),
            Kind::B256 => Value::B256([0; 32]),
            Kind::Bytes => Value::Bytes(Vec::new()),
            Kind::Text => Value::Text("test".into()),
            Kind::Nested => filled(f.nested.unwrap()).into_nested(),
        };
        d.set(f.tag, value);
    }
    d
}

const BLOCK: [u8; 32] = [0xb1; 32];
const LAUNCH: [u8; 32] = [0x01; 32];
const BUY: [u8; 32] = [0x02; 32];

fn g08_pass() -> Draft {
    let mut d = filled(&result::SCHEMA);
    d.set(result::GATE, "G0.8")
        .set(result::ATTEMPT_ID, "g07-timing-test")
        .set(result::VERDICT, "PASS")
        .set(result::POSITIVE_ATTRIBUTABLE_ACQUISITION, true)
        .set(result::CANONICAL, true)
        .set(result::SAME_BLOCK, true)
        .set(result::ADJACENT, true)
        .set(result::EXTERNAL_BUYS_BETWEEN_COUNT, 0u64)
        .set(result::BLOCK_HASH, Value::B256(BLOCK))
        .set(result::LAUNCH_TX, Value::B256(LAUNCH))
        .set(result::BUY_TX, Value::B256(BUY))
        .set(result::LAUNCH_INDEX, 4u64)
        .set(result::BUY_INDEX, 5u64);
    d
}

fn witness() -> Draft {
    let mut d = filled(&w::SCHEMA);
    d.set(w::ATTEMPT_ID, "g07-timing-test")
        .set(w::BLOCK_HASH, Value::B256(BLOCK))
        .set(w::PARENT_HASH, Value::B256([0xb0; 32]))
        .set(w::LAUNCH_TX_HASH, Value::B256(LAUNCH))
        .set(w::BUY_TX_HASH, Value::B256(BUY))
        .set(w::LAUNCH_TX_INDEX, 4u64)
        .set(w::BUY_TX_INDEX, 5u64)
        .set(w::LAUNCH_STATUS, 1u64)
        .set(w::BUY_STATUS, 1u64)
        .set(w::CANONICAL_STATUS, "CANONICAL")
        .set(w::REORG_STATUS, "NONE")
        .push(w::ACQUISITION_EVIDENCE_REFS, "g08.result#acquisition")
        .set(w::CLOCK_RESOLUTION_NS, 1u64)
        .set(w::REMAINING_EXCLUDES_BRACKETED, true)
        .set(w::CLOCK_RESOLUTION_ACCOUNTED, true)
        .set(w::HOOK_OVERHEAD_ACCOUNTED, true)
        .set(w::NORMALIZATION_ERROR_ACCOUNTED, true)
        .set(w::T0_BEFORE_NS, 100u64)
        .set(w::T0_AFTER_NS, 120u64)
        .set(w::T_BUY_BEFORE_NS, 700u64)
        .set(w::T_BUY_AFTER_NS, 800u64)
        .set(w::REMAINING_UNCERTAINTY_NS, 50u64);
    d
}

/// Runs the timing binary; returns its output and the verdict file bytes,
/// if one was written.
fn timing(tag: &str, g08: &[u8], witness: &[u8]) -> (std::process::Output, Option<Vec<u8>>) {
    let dir = std::env::temp_dir().join(format!("darkbot-g08-timing-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let (g08_path, witness_path, out_path) = (
        dir.join("g08.dbrec"),
        dir.join("witness.dbrec"),
        dir.join("timing.dbrec"),
    );
    fs::write(&g08_path, g08).unwrap();
    fs::write(&witness_path, witness).unwrap();
    let out = Command::new(TIMING_BIN)
        .env_clear()
        .env("DARKBOT_G08_EVIDENCE", &g08_path)
        .env("DARKBOT_G08_PRODUCER_WITNESS", &witness_path)
        .env("DARKBOT_G08_TIMING_EVIDENCE_PATH", &out_path)
        .output()
        .unwrap();
    let written = out_path.exists().then(|| fs::read(&out_path).unwrap());
    fs::remove_dir_all(&dir).unwrap();
    (out, written)
}

#[test]
fn producer_timing_reads_and_writes_binary_records() {
    let g08 = g08_pass().encode().unwrap();
    let witness_bytes = witness().encode().unwrap();
    let (out, written) = timing("pass", &g08, &witness_bytes);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("verdict = \"PASS\""), "{stdout}");
    assert!(
        stdout.contains("derived.conservative_upper_bound_ns = 750"),
        "{stdout}"
    );
    let record = decode(&v::SCHEMA, &written.expect("timing verdict written")).unwrap();
    assert_eq!(record.text(v::VERDICT).unwrap(), "PASS");
    assert!(record.render().contains("SELF_LAUNCH_BUNDLE_ONLY; DOES_NOT_MEASURE_FOREIGN_LAUNCH_REACTION_T_SIGNAL_TO_T_SEND"));
    assert_eq!(record.u64(v::BRACKET_UPPER_NS).unwrap(), 700);
    assert_eq!(record.b256(v::G08_SHA256).unwrap(), sha256(&g08));
    assert_eq!(
        record.b256(v::PRODUCER_WITNESS_SHA256).unwrap(),
        sha256(&witness_bytes)
    );
    assert_eq!(record.texts(v::EXPLICITLY_NOT_AUTHORITY).unwrap().len(), 7);
}

#[test]
fn producer_timing_refuses_foreign_or_legacy_inputs_without_output() {
    let g08 = g08_pass().encode().unwrap();
    let mut other_attempt = witness();
    other_attempt.set(w::ATTEMPT_ID, "another-attempt");
    let mut other_block = witness();
    other_block.set(w::BLOCK_HASH, Value::B256([0xee; 32]));
    let mut no_refs = witness();
    no_refs.clear(w::ACQUISITION_EVIDENCE_REFS);
    let mut adapt = g08_pass();
    adapt.set(result::VERDICT, "ADAPT");
    let legacy = br#"{"schema":"darkbot.producer-execution-witness.v1","schema_version":1}"#;
    for (tag, g08, witness, expected) in [
        (
            "attempt",
            g08.clone(),
            other_attempt.encode().unwrap(),
            "another",
        ),
        ("block", g08.clone(), other_block.encode().unwrap(), "block"),
        (
            "refs",
            g08.clone(),
            no_refs.encode().unwrap(),
            "acquisition_evidence_refs",
        ),
        (
            "adapt",
            adapt.encode().unwrap(),
            witness().encode().unwrap(),
            "PASS",
        ),
        (
            "legacy-json",
            g08.clone(),
            legacy.to_vec(),
            "not a DBREC001 record",
        ),
        // A G0.8 result is not a witness: the record type decides.
        ("swapped", g08.clone(), g08.clone(), "record type 14"),
    ] {
        let (out, written) = timing(tag, &g08, &witness);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{tag}");
        assert!(out.stdout.is_empty(), "{tag}");
        assert!(written.is_none(), "{tag}");
        assert!(stderr.contains(expected), "{tag}: {stderr}");
    }
}

#[test]
fn timing_requires_writable_output_before_reporting_a_verdict() {
    let output = Command::new(TIMING_BIN).env_clear().output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("DARKBOT_G08_TIMING_EVIDENCE_PATH"));
    let dir = std::env::temp_dir().join(format!("g08-unwritable-{}",std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("g08"), g08_pass().encode().unwrap()).unwrap();
    fs::write(dir.join("witness"), witness().encode().unwrap()).unwrap();
    let out = Command::new(TIMING_BIN).env_clear().env("DARKBOT_G08_EVIDENCE",dir.join("g08"))
        .env("DARKBOT_G08_PRODUCER_WITNESS",dir.join("witness"))
        .env("DARKBOT_G08_TIMING_EVIDENCE_PATH",dir.join("missing/out")).output().unwrap();
    assert!(!out.status.success()); assert!(out.stdout.is_empty());
    fs::remove_dir_all(dir).unwrap();
}
