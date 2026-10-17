//! G0.8 reads the G0.7 evidence. The real G0.8 binary gets a binary G0.7
//! evidence record (DBREC001) holding every required field of the G0.7
//! schema, the nested preflight record included: final and provisional
//! evidence pass G0.8's evidence checks up to its first RPC (it then stops
//! for the missing RPC URL, so no network is touched); defective records
//! are refused before any RPC.

use darkbot_bsc_record::schema::{g06a, g07::evidence};
use darkbot_bsc_record::{Draft, Kind, Presence, Schema, Value, sha256};
use std::{fs, path::PathBuf, process::Command};

const G08: &str = env!("CARGO_BIN_EXE_g08_mainnet_verify");
const BEFORE_FIRST_RPC: &str = "BSC_MAINNET_RPC_HTTPS is required";

fn address(hex_digits: &str) -> Value {
    let raw: [u8; 20] = hex::decode(hex_digits).unwrap().try_into().unwrap();
    Value::Address(raw)
}

/// Every required field of `schema` with a neutral value, nested records
/// included.
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

fn evidence_draft(classification: &str) -> Draft {
    let mut d = filled(&evidence::SCHEMA);
    d.set(evidence::GATE, "G0.7")
        .set(evidence::ATTEMPT_ID, "g07-compat-test")
        .set(evidence::CLASSIFICATION, classification)
        .set(evidence::CHAIN_ID, 56u64)
        .set(
            evidence::PORTAL,
            address("e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0"),
        )
        .set(evidence::CAPTURE_COMPLETE, false)
        .set(evidence::LAUNCH_TX_HASH, Value::B256([0x01; 32]))
        .set(evidence::BUY_TX_HASH, Value::B256([0x02; 32]))
        .set(
            evidence::TOKEN,
            address("e51c4deb1038afcd95ef12d4a649cdbcf6fa7777"),
        )
        .set(
            evidence::LAUNCHER,
            address("3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd"),
        )
        .set(
            evidence::BUYER,
            address("9c8a0db41914beee52dd49790c4f087e252743de"),
        )
        .set(evidence::MIN_OUTPUT, Value::u128(1))
        .set(evidence::BUY_VALUE_WEI, Value::u128(1_000_000_000_000));
    d
}

/// Recomputes the trailing checksum of hand-edited bytes, so G0.8 reaches
/// the check under test.
fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let end = bytes.len() - 32;
    let digest = sha256(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
    bytes
}

fn g08_stderr(tag: &str, contents: &[u8]) -> String {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("darkbot-g08-compat-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("g07.dbrec");
    fs::write(&path, contents).unwrap();
    let out = Command::new(G08)
        .env_clear()
        .env("DARKBOT_G08_G07_EVIDENCE", &path)
        .env("DARKBOT_G08_CHAIN_JOURNAL", dir.join("chain.journal"))
        .env("DARKBOT_G08_EVIDENCE_PATH", dir.join("result.dbrec"))
        .output()
        .unwrap();
    // Nothing was captured: the journal is only created after the RPC reads.
    assert!(!dir.join("chain.journal").exists(), "{tag}");
    fs::remove_dir_all(&dir).unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn g08_reads_final_and_provisional_g07_evidence_up_to_its_first_rpc() {
    for (tag, classification, capture_complete) in [
        (
            "final",
            "SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY",
            true,
        ),
        (
            "incomplete",
            "STOP_CAPTURE_INCOMPLETE__RECONCILE_G08__NO_AUTOMATIC_RETRY",
            false,
        ),
        (
            "provisional",
            "PROVISIONAL_RELEASED__RECEIPT_WINDOW_OPEN__RECONCILE_G08__NO_AUTOMATIC_RETRY",
            false,
        ),
        (
            "at-release",
            "PROVISIONAL_RELEASED__PROVIDER_RESULTS_PENDING__RECONCILE_G08__NO_AUTOMATIC_RETRY",
            false,
        ),
    ] {
        let mut d = evidence_draft(classification);
        d.set(evidence::CAPTURE_COMPLETE, capture_complete);
        if tag == "final" {
            // Final evidence carries the optional window fields as well.
            d.set(evidence::BOTH_RECEIPTS_OBSERVED_NS, 1u64)
                .set(evidence::COVERAGE_END_NS, 2u64)
                .set(evidence::CANONICAL_WINDOW_RECONCILED_IN_G08, false)
                .set(evidence::LAUNCH_RECEIPT_OBSERVED, true)
                .set(evidence::BUY_RECEIPT_OBSERVED, true);
        }
        let err = g08_stderr(tag, &d.encode().unwrap());
        assert!(err.contains(BEFORE_FIRST_RPC), "{tag}: {err}");
    }
}

#[test]
fn g08_refuses_defective_g07_evidence_before_any_rpc() {
    let full = evidence_draft("SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY")
        .encode()
        .unwrap();
    // Header: magic 0..8, format version 8..10, record type 10..12,
    // schema version 12..16; the trailing 32 bytes are the checksum.
    let mut record_type = full.clone();
    record_type[10] = 9;
    let mut schema_version = full.clone();
    schema_version[12] = 2;
    let mut flipped = full.clone();
    flipped[full.len() / 2] ^= 0x01;
    let mut checksum = full.clone();
    *checksum.last_mut().unwrap() ^= 0x80;
    let other_schema = filled(&g06a::preflight::SCHEMA).encode().unwrap();
    let mut testnet =
        evidence_draft("SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY");
    testnet.set(evidence::CHAIN_ID, 97u64);
    let mut other_portal =
        evidence_draft("SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY");
    other_portal.set(evidence::PORTAL, Value::Address([0x11; 20]));
    let legacy_json = br#"{"schema":"darkbot.g07.mainnet-self-launch-buy.v2","chain_id":56}"#;
    for (tag, broken, expected) in [
        ("empty", Vec::new(), "truncated"),
        ("truncated", full[..full.len() / 2].to_vec(), "checksum"),
        ("tail", full[..full.len() - 1].to_vec(), "checksum"),
        ("record-type", reseal(record_type), "record type 9"),
        ("schema-version", reseal(schema_version), "version 2"),
        ("flipped", flipped, "checksum"),
        ("checksum", checksum, "checksum"),
        ("other-schema", other_schema, "record type 3"),
        ("legacy-json", legacy_json.to_vec(), "not a DBREC001 record"),
        ("chain", testnet.encode().unwrap(), "56"),
        ("portal", other_portal.encode().unwrap(), "assertion"),
    ] {
        let err = g08_stderr(tag, &broken);
        assert!(
            !err.contains(BEFORE_FIRST_RPC),
            "{tag} reached the RPC: {err}"
        );
        assert!(err.contains(expected), "{tag}: {err}");
    }
}
