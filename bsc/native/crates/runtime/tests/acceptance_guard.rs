//! Negative and identity tests of the G0.7 acceptance guard.
//!
//! The guard resolves `V2_RELATIVE_PATH` relative to the working directory,
//! exactly as the G0.7/G0.6a tools do when they run from the repository
//! root. This file is its own test binary, so every test can move the
//! process to the repository root without affecting other test binaries;
//! all tests set the same directory.

use darkbot_runner::acceptance_guard::{
    FROZEN_APPROVED_V2_SHA256, LiveGuardContext, V2_RELATIVE_PATH, inspect_v2,
    verify_live_authorization,
};
use std::path::Path;

/// SHA-256 of the unchanged v2 proposal bytes (moved byte-identically from
/// `bsc/gates/gate0/evidence/` to `bsc/config/`).
const V2_PROPOSAL_SHA256: &str = "026b534f4a2e6e76b6eb816a8de84ef71b508aa28b6e24c5c471e9ee6f0b19ff";
const OLD_V2_RELATIVE_PATH: &str =
    "bsc/gates/gate0/evidence/g01_mainnet_acceptance_proposal_v2.json";

fn at_repo_root() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    std::env::set_current_dir(root).unwrap();
}

fn hex64(c: char) -> String {
    std::iter::repeat_n(c, 64).collect()
}

/// A context that matches the frozen proposal in every field the guard
/// compares, so that only the arming state can refuse it.
fn matching_context() -> LiveGuardContext {
    let snap = inspect_v2(Path::new(V2_RELATIVE_PATH)).unwrap();
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
        dry_evidence_sha256: hex64('a'),
        preflight_evidence_sha256: hex64('b'),
        builder_policy_evidence_sha256: hex64('c'),
        producer_witness_source_contract_sha256: hex64('d'),
    }
}

#[test]
fn guard_is_compiled_unarmed() {
    assert_eq!(FROZEN_APPROVED_V2_SHA256, "UNARMED");
}

#[test]
fn v2_path_is_the_migrated_config_contract() {
    assert_eq!(
        V2_RELATIVE_PATH,
        "bsc/config/g01_mainnet_acceptance_proposal_v2.json"
    );
}

#[test]
fn frozen_v2_proposal_is_unchanged_and_unapproved() {
    at_repo_root();
    let snap = inspect_v2(Path::new(V2_RELATIVE_PATH)).unwrap();
    assert_eq!(snap.sha256, V2_PROPOSAL_SHA256);
    assert!(!snap.approved);
    assert!(!snap.execution_ready);
    assert_ne!(
        snap.launcher_address.to_ascii_lowercase(),
        snap.buyer_address.to_ascii_lowercase()
    );
}

#[test]
fn any_other_contract_path_is_rejected() {
    at_repo_root();
    let old = inspect_v2(Path::new(OLD_V2_RELATIVE_PATH)).unwrap_err();
    assert!(old.contains("wrong acceptance contract path"), "{old}");
    // The same bytes under a non-identical spelling are not the contract.
    let absolute = std::env::current_dir().unwrap().join(V2_RELATIVE_PATH);
    let abs = inspect_v2(&absolute).unwrap_err();
    assert!(abs.contains("wrong acceptance contract path"), "{abs}");
}

#[test]
fn unarmed_guard_refuses_a_fully_matching_context() {
    at_repo_root();
    let err =
        verify_live_authorization(Path::new(V2_RELATIVE_PATH), &matching_context()).unwrap_err();
    assert!(err.contains("UNARMED"), "{err}");
}

#[test]
fn wrong_path_is_refused_before_anything_else() {
    at_repo_root();
    let err = verify_live_authorization(Path::new(OLD_V2_RELATIVE_PATH), &matching_context())
        .unwrap_err();
    assert!(err.contains("wrong acceptance contract path"), "{err}");
}
