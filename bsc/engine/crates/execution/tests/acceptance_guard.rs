//! Negative and identity tests of the G0.7 acceptance guard.
//!
//! The guard resolves `V3_RELATIVE_PATH` relative to the working directory,
//! exactly as the G0.7 tools do when they run from the repository root. This
//! file is its own test binary, so every test can move the process to the
//! repository root without affecting other test binaries; all tests set the
//! same directory.

use darkbot_bsc_execution::acceptance_contract::{
    Authorization, WITNESS_SOURCE_RECORD, attempt1_bindings, attempt1_v3, attempt1_v4,
    attempt1_v4_evidence,
};
use darkbot_bsc_execution::acceptance_guard::{
    FROZEN_APPROVED_V4_SHA256, InputHashes, LiveGuardContext, V3_RELATIVE_PATH, V4_RELATIVE_PATH,
    inspect_v3, parse_v4, sha256_hex, verify_live_authorization,
};
use darkbot_bsc_record::schema::{
    PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL, g06a, g07::contract,
};
use darkbot_bsc_record::{Draft, Schema, Value, decode};
use std::path::{Path, PathBuf};

/// SHA-256 of the committed binary v3 contract of attempt 1: the proposal
/// (approved=false, execution_ready=false); any changed value changes it.
const V3_PROPOSAL_SHA256: &str = "3966b227bfa969b0e74ce5913aec4142778a4607c0cae42fa0981eb38b774f6a";
/// SHA-256 of its approved variant: what the user approves, never written
/// without that approval.
const V3_APPROVED_VARIANT_SHA256: &str =
    "19fb9b389a50e83fc0c250ee37123af0d42ed16b25b41ba2a111b41ff61a5d80";
/// The JSON v2 proposal stays untouched as legacy evidence and is never read.
const V2_LEGACY_PATH: &str = "bsc/config/g01_mainnet_acceptance_proposal_v2.json";
const V2_LEGACY_SHA256: &str = "026b534f4a2e6e76b6eb816a8de84ef71b508aa28b6e24c5c471e9ee6f0b19ff";

fn at_repo_root() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    std::env::set_current_dir(root).unwrap();
}

/// A context that matches the contract in every field the guard compares,
/// so that only the arming state can refuse it.
fn matching_context() -> LiveGuardContext {
    let snap = inspect_v3(Path::new(V3_RELATIVE_PATH)).unwrap();
    LiveGuardContext {
        launcher_address: snap.launcher_address.clone(),
        buyer_address: snap.buyer_address.clone(),
        chain_id: snap.chain_id,
        portal_address: snap.portal_address.clone(),
        launch_value_wei: 0,
        buy_value_wei: 0,
        gas_price_wei: 0,
        launch_gas_limit: 0,
        buy_gas_limit: 0,
        direct_builder_tip_wei: 0,
        spent_budget_wei: 0,
        reserved_attempt_wei: 0,
        attempt_number: 1,
        dry_pass_all: true,
        capture_prearmed: true,
        producer_witness_contract_bound: true,
        dry_evidence: snap.dry_evidence.clone(),
        preflight_evidence: snap.preflight_evidence.clone(),
        builder_policy_evidence: snap.builder_policy_evidence.clone(),
        producer_witness_source: snap.producer_witness_source.clone(),
    }
}

#[test]
fn guard_is_compiled_unarmed() {
    assert_eq!(FROZEN_APPROVED_V4_SHA256, "UNARMED");
}

#[test]
fn contract_paths_are_the_binary_config_contracts() {
    assert_eq!(
        V3_RELATIVE_PATH,
        "bsc/config/g07_attempt1_acceptance_v3.dbrec"
    );
    assert_eq!(
        V4_RELATIVE_PATH,
        "bsc/config/g07_attempt1_acceptance_v4.dbrec"
    );
}

#[test]
fn committed_contract_is_reproduced_from_its_source_and_binds_the_import() {
    at_repo_root();
    let bindings = attempt1_bindings(Path::new(".")).unwrap();
    let bytes = attempt1_v3(&bindings, Authorization::Proposed).unwrap();
    assert_eq!(std::fs::read(V3_RELATIVE_PATH).unwrap(), bytes);
    let approved = attempt1_v3(&bindings, Authorization::Approved).unwrap();
    assert_eq!(sha256_hex(&approved), V3_APPROVED_VARIANT_SHA256);
    let snap = inspect_v3(Path::new(V3_RELATIVE_PATH)).unwrap();
    assert_eq!(snap.sha256, V3_PROPOSAL_SHA256);
    // The committed contract carries no authorization.
    assert!(!snap.approved && !snap.execution_ready);
    assert_eq!(
        snap.attempt_id,
        "g07-mainnet-attempt-1-dry-20261015T222241Z"
    );
    assert_eq!(snap.max_attempts, 1);
    assert_eq!(snap.max_total_per_attempt, 481_163_500_000_000);
    assert_eq!(snap.absolute_budget_wei, 481_163_500_000_000);
    assert_eq!(snap.max_gas_price_wei, 50_000_000);
    assert_ne!(snap.launcher_address, snap.buyer_address);
    // The legacy hashes are exactly the ones the JSON-era bindings froze.
    let legacy: Vec<&str> = [
        &snap.dry_evidence,
        &snap.preflight_evidence,
        &snap.builder_policy_evidence,
        &snap.producer_witness_source,
    ]
    .iter()
    .map(|h| h.legacy_sha256.as_deref().unwrap())
    .collect();
    assert_eq!(
        legacy,
        [
            "0efe78a7faf1d817fae3cbfd8893f858a4386a4c2770755465508c668eeded6f",
            "3eb5563d9e0bfea9ce8cc4e3750e3152d81a1b7ea7eff91de78091087bf77e69",
            "ca76a93c432a9cee308b07e59170bac390e801caaa8e0697b0b31d405b9bcc87",
            "78660c7da9fbb8a2140daf1bca4187881d71f55a914c9fcddfc0e47f287116c4",
        ]
    );
    // The legacy JSON proposal is untouched.
    assert_eq!(
        sha256_hex(&std::fs::read(V2_LEGACY_PATH).unwrap()),
        V2_LEGACY_SHA256
    );
}

#[test]
fn a_tampered_contract_does_not_decode() {
    at_repo_root();
    let mut bytes = std::fs::read(V3_RELATIVE_PATH).unwrap();
    assert!(decode(&contract::SCHEMA, &bytes).is_ok());
    let middle = bytes.len() / 2;
    bytes[middle] ^= 1;
    assert!(decode(&contract::SCHEMA, &bytes).is_err());
}

#[test]
fn any_other_contract_path_is_rejected() {
    at_repo_root();
    let legacy = inspect_v3(Path::new(V2_LEGACY_PATH)).unwrap_err();
    assert!(
        legacy.contains("wrong acceptance contract path"),
        "{legacy}"
    );
    // The same bytes under a non-identical spelling are not the contract.
    let absolute = std::env::current_dir().unwrap().join(V3_RELATIVE_PATH);
    let abs = inspect_v3(&absolute).unwrap_err();
    assert!(abs.contains("wrong acceptance contract path"), "{abs}");
}

#[test]
fn unarmed_guard_refuses_a_fully_matching_context_before_reading() {
    at_repo_root();
    // No v4 file exists yet: the UNARMED refusal comes before any read.
    let err =
        verify_live_authorization(Path::new(V4_RELATIVE_PATH), &matching_context()).unwrap_err();
    assert!(err.contains("UNARMED"), "{err}");
}

#[test]
fn only_the_v4_path_can_authorize() {
    at_repo_root();
    let mut ctx = matching_context();
    ctx.dry_evidence = InputHashes {
        record_sha256: "0".repeat(64),
        legacy_sha256: None,
    };
    for path in [V2_LEGACY_PATH, V3_RELATIVE_PATH] {
        let err = verify_live_authorization(Path::new(path), &ctx).unwrap_err();
        assert!(
            err.contains("wrong acceptance contract path"),
            "{path}: {err}"
        );
    }
}

const FROZEN: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016";

/// A native record of `native` with the fields of a frozen import record
/// (same tags), provenance instead of the import: offline stand-ins for a
/// requalification DRY. They test the v4 wiring only, never qualify.
fn native_copy(import: &str, schema: &'static Schema, native: &'static Schema) -> Vec<u8> {
    let record = decode(
        schema,
        &std::fs::read(format!("{FROZEN}/{import}")).unwrap(),
    )
    .unwrap();
    let mut d = Draft::new(native);
    for (tag, value) in record.fields() {
        if *tag > 2 {
            d.push(*tag, value.clone());
        }
    }
    if native.field(PROVENANCE_TOOL).is_some() {
        d.set(PROVENANCE_TOOL, "offline-test")
            .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64);
    }
    d.encode().unwrap()
}

/// A temporary repository root holding the committed inputs v4 reads and a
/// requalification directory of stand-in records.
fn requal_root() -> (PathBuf, &'static str) {
    use g06a::{dry, economics, local_simulation, portal_upgrade as up, preflight};
    let root = std::env::temp_dir().join(format!("darkbot-v4-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = "bsc/native/evidence/g06a/requal_test";
    for path in [
        WITNESS_SOURCE_RECORD,
        V3_RELATIVE_PATH,
        &format!("{FROZEN}/builder_policy.dbrec"),
    ] {
        let to = root.join(path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(path, to).unwrap();
    }
    std::fs::create_dir_all(root.join(dir)).unwrap();
    let write =
        |name: &str, bytes: Vec<u8>| std::fs::write(root.join(dir).join(name), bytes).unwrap();
    write(
        "dry_result.dbrec",
        native_copy("dry_result.dbrec", &dry::SCHEMA, &dry::NATIVE),
    );
    write(
        "preflight.dbrec",
        native_copy("preflight.dbrec", &preflight::SCHEMA, &preflight::SCHEMA),
    );
    write(
        "local_simulation.dbrec",
        native_copy(
            "local_simulation.dbrec",
            &local_simulation::SCHEMA,
            &local_simulation::NATIVE,
        ),
    );
    write(
        "economics_crosscheck.dbrec",
        native_copy(
            "economics_crosscheck.dbrec",
            &economics::SCHEMA,
            &economics::NATIVE,
        ),
    );
    let implementation = decode(
        &preflight::SCHEMA,
        &std::fs::read(root.join(dir).join("preflight.dbrec")).unwrap(),
    )
    .unwrap()
    .address(preflight::PORTAL_IMPLEMENTATION)
    .unwrap();
    let mut scan = Draft::new(&up::SCHEMA);
    scan.set(PROVENANCE_TOOL, "offline-test")
        .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(up::PROXY, Value::Address([0xe2; 20]))
        .set(up::SCANNED_FROM_BLOCK, 1u64)
        .set(up::SCANNED_TO_BLOCK, 2u64)
        .set(up::CURRENT_IMPLEMENTATION, Value::Address(implementation))
        .set(up::BASELINE_IMPLEMENTATION, Value::Address(implementation))
        .set(up::COVERAGE_METHOD, up::METHOD)
        .set(up::CHAIN_SOURCE, "offline-test")
        .set(up::ARCHIVE_SOURCE, "offline-test")
        .set(up::HEADERS_VERIFIED, 2u64)
        .set(up::BLOOM_CANDIDATE_BLOCKS, 0u64)
        .set(up::CURRENT_IMPLEMENTATION_CODE_SHA256, Value::B256([0x42; 32]))
        .set(up::CURRENT_IMPLEMENTATION_CODE_BYTES, 1u64)
        .set(up::FIRST_PARENT_HASH, Value::B256([1; 32]))
        .set(up::LAST_BLOCK_HASH, Value::B256([2; 32]));
    write("portal_upgrade.dbrec", scan.encode().unwrap());
    (root, dir)
}

#[test]
fn v4_binds_native_inputs_without_and_imports_with_legacy_hashes() {
    at_repo_root();
    let (root, dir) = requal_root();
    let policy = format!("{FROZEN}/builder_policy.dbrec");
    let evidence = attempt1_v4_evidence(&root, dir, &policy).unwrap();
    let proposal = parse_v4(&attempt1_v4(&evidence, Authorization::Proposed).unwrap()).unwrap();
    let approved = parse_v4(&attempt1_v4(&evidence, Authorization::Approved).unwrap()).unwrap();
    assert!(!proposal.approved && !proposal.execution_ready);
    assert!(approved.approved && approved.execution_ready);
    assert_ne!(proposal.sha256, approved.sha256);
    // Native requalification inputs carry no legacy hash; imports do.
    assert_eq!(proposal.dry_evidence.legacy_sha256, None);
    assert_eq!(proposal.preflight_evidence.legacy_sha256, None);
    assert_eq!(
        proposal.builder_policy_evidence.legacy_sha256.as_deref(),
        Some("ca76a93c432a9cee308b07e59170bac390e801caaa8e0697b0b31d405b9bcc87")
    );
    assert_eq!(
        proposal.producer_witness_source.legacy_sha256.as_deref(),
        Some("78660c7da9fbb8a2140daf1bca4187881d71f55a914c9fcddfc0e47f287116c4")
    );
    assert!(proposal.local_simulation.is_some() && proposal.economics.is_some());
    assert!(proposal.portal_upgrade_record_sha256.is_some());
    assert_eq!(
        proposal.supersedes_contract_sha256.as_deref(),
        Some(V3_PROPOSAL_SHA256)
    );
    // Economic terms are unchanged from v3.
    let v3 = inspect_v3(Path::new(V3_RELATIVE_PATH)).unwrap();
    assert_eq!(
        (
            proposal.max_total_per_attempt,
            proposal.absolute_budget_wei,
            proposal.max_attempts
        ),
        (
            v3.max_total_per_attempt,
            v3.absolute_budget_wei,
            v3.max_attempts
        )
    );
    // An import where a native record is required is refused.
    std::fs::copy(
        format!("{FROZEN}/preflight.dbrec"),
        root.join(dir).join("preflight.dbrec"),
    )
    .unwrap();
    let e = attempt1_v4_evidence(&root, dir, &policy).unwrap_err();
    assert!(e.contains("must be a native record"), "{e}");
    std::fs::remove_dir_all(&root).unwrap();
}
