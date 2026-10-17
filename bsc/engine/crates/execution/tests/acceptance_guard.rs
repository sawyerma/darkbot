//! Negative and identity tests of the G0.7 acceptance guard.
//!
//! The guard resolves `V3_RELATIVE_PATH` relative to the working directory,
//! exactly as the G0.7 tools do when they run from the repository root. This
//! file is its own test binary, so every test can move the process to the
//! repository root without affecting other test binaries; all tests set the
//! same directory.

use darkbot_bsc_execution::acceptance_contract::{Authorization, attempt1_bindings, attempt1_v3};
use darkbot_bsc_execution::acceptance_guard::{
    FROZEN_APPROVED_V3_SHA256, InputHashes, LiveGuardContext, V3_RELATIVE_PATH, inspect_v3,
    sha256_hex, verify_live_authorization,
};
use darkbot_bsc_record::{decode, schema::g07::contract};
use std::path::Path;

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
    assert_eq!(FROZEN_APPROVED_V3_SHA256, "UNARMED");
}

#[test]
fn v3_path_is_the_binary_config_contract() {
    assert_eq!(V3_RELATIVE_PATH, "bsc/config/g07_attempt1_acceptance_v3.dbrec");
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
    assert_eq!(snap.attempt_id, "g07-mainnet-attempt-1-dry-20261015T222241Z");
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
    .map(|h| h.legacy_sha256.as_str())
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
    assert!(legacy.contains("wrong acceptance contract path"), "{legacy}");
    // The same bytes under a non-identical spelling are not the contract.
    let absolute = std::env::current_dir().unwrap().join(V3_RELATIVE_PATH);
    let abs = inspect_v3(&absolute).unwrap_err();
    assert!(abs.contains("wrong acceptance contract path"), "{abs}");
}

#[test]
fn unarmed_guard_refuses_a_fully_matching_context() {
    at_repo_root();
    let err =
        verify_live_authorization(Path::new(V3_RELATIVE_PATH), &matching_context()).unwrap_err();
    assert!(err.contains("UNARMED"), "{err}");
}

#[test]
fn wrong_path_is_refused_before_anything_else() {
    at_repo_root();
    let mut ctx = matching_context();
    ctx.dry_evidence = InputHashes {
        record_sha256: "0".repeat(64),
        legacy_sha256: "0".repeat(64),
    };
    let err = verify_live_authorization(Path::new(V2_LEGACY_PATH), &ctx).unwrap_err();
    assert!(err.contains("wrong acceptance contract path"), "{err}");
}
