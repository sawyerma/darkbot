use std::{path::Path, process::Command};
const BIN: &str = env!("CARGO_BIN_EXE_g07_pre_live_readiness");
#[test]
fn offline_readiness_uses_bound_binary_inputs_and_never_invokes_rpc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let out = Command::new(BIN)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/no/commands/available")
        .env("BSC_MAINNET_RPC_HTTPS", "https://127.0.0.1:1")
        .env("DARKBOT_G07_MAINNET_SEND", "YES")
        .env("DARKBOT_G07_PROBE_SEND", "YES")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    for expected in [
        "OFFLINE_ZERO_EGRESS",
        "FOUR_INPUT_BINDINGS",
        "DRY1_DRY4",
        "BUILDER_FRESHNESS",
        "CURRENT_DEPLOYMENTS_NONCES_FUNDING_GAS",
        "CLOCK_HEAD",
        "SIGNED_EGRESS",
        "LIVE=BLOCKED",
    ] {
        assert!(
            text.contains(expected),
            "missing {expected}: {text}; {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(!text.contains("VALIDATION"), "{text}");
    assert!(!text.contains("PRE_LIVE_READINESS=PASS"));
}
#[test]
fn unexpected_argument_refuses_before_reads_or_network() {
    let out = Command::new(BIN)
        .env_clear()
        .arg("--send")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

#[test]
fn readiness_binary_is_decodable_and_has_no_execution_authority() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let out = Command::new(BIN)
        .current_dir(root)
        .env_clear()
        .arg("--binary")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    use darkbot_bsc_record::{decode, schema::g07::readiness as r};
    let report = decode(&r::SCHEMA, &out.stdout).unwrap();
    assert!(!report.bool(r::LIVE_AUTHORIZED).unwrap());
    assert_eq!(report.text(r::MODE).unwrap(), "OFFLINE_ZERO_EGRESS");
}

#[test]
fn inactive_guard_and_input_hashes_fail_closed() {
    use darkbot_bsc_record::{Draft, Value, decode, schema::g07::contract as c};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let rel = "bsc/config/g07_attempt1_acceptance_v3.dbrec";
    let original = decode(&c::SCHEMA, &std::fs::read(root.join(rel)).unwrap()).unwrap();
    for (tag, value) in [
        (c::APPROVED, Value::Bool(true)),
        (c::MAX_ATTEMPTS, Value::U64(2)),
        (c::DRY_RECORD_SHA256, Value::B256([0; 32])),
    ] {
        let dir =
            std::env::temp_dir().join(format!("readiness-negative-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(dir.join("bsc/config")).unwrap();
        let mut draft = Draft::new(&c::SCHEMA);
        for (field, value) in original.fields() {
            draft.push(*field, value.clone());
        }
        // Absolute input path is only for this offline negative fixture.
        let dry_path = original.text(c::DRY_RECORD_PATH).unwrap();
        draft.set(c::DRY_RECORD_PATH, root.join(dry_path).to_str().unwrap());
        draft.set(tag, value);
        std::fs::write(dir.join(rel), draft.encode().unwrap()).unwrap();
        let out = Command::new(BIN)
            .current_dir(&dir)
            .env_clear()
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(text.contains("VALIDATION"), "{text}");
        assert!(!text.contains("FOUR_INPUT_BINDINGS"), "{text}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
