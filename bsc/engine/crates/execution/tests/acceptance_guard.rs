//! Negative and identity tests of the G0.7 acceptance guard.
//!
//! The guard resolves `V3_RELATIVE_PATH` relative to the working directory,
//! exactly as the G0.7 tools do when they run from the repository root. This
//! file is its own test binary, so every test can move the process to the
//! repository root without affecting other test binaries; all tests set the
//! same directory.

#[path = "support/requalification.rs"]
mod requalification_fixture;

use darkbot_bsc_execution::acceptance_contract::{
    Authorization, WITNESS_SOURCE_RECORD, attempt1_bindings, attempt1_v3, attempt1_v4,
    attempt1_v4_evidence, attempt1_v5, attempt1_v5_evidence,
};
use darkbot_bsc_execution::acceptance_guard::{
    DryPassProof, FROZEN_APPROVED_V5_SHA256, InputHashes, LiveGuardContext, V3_RELATIVE_PATH,
    V4_RELATIVE_PATH, V5_RELATIVE_PATH, WitnessSourceProof, inspect_v3, parse_v4, parse_v5,
    sha256_hex, verify_further_v4_evidence, verify_further_v5_evidence, verify_live_authorization,
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
        attempt_id: snap.attempt_id.clone(),
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
        dry_pass: DryPassProof::check(&native_copy(
            "dry_result.dbrec",
            &g06a::dry::SCHEMA,
            &g06a::dry::NATIVE,
        ))
        .unwrap(),
        capture: None,
        producer_witness: None,
        dry_evidence: snap.dry_evidence.clone(),
        preflight_evidence: snap.preflight_evidence.clone(),
        builder_policy_evidence: snap.builder_policy_evidence.clone(),
        producer_witness_source: snap.producer_witness_source.clone(),
        candidate_profile: "SELF_A".into(),
        candidate_id: "00".repeat(32),
        launch_path_current: None,
    }
}

#[test]
fn guard_is_compiled_unarmed() {
    assert_eq!(FROZEN_APPROVED_V5_SHA256, "UNARMED");
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
    assert_eq!(
        V5_RELATIVE_PATH,
        "bsc/config/g07_attempt1_acceptance_v5.dbrec"
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
    // No v5 file exists yet: the UNARMED refusal comes before any read.
    let err =
        verify_live_authorization(Path::new(V5_RELATIVE_PATH), &matching_context()).unwrap_err();
    assert!(err.contains("UNARMED"), "{err}");
}

#[test]
fn only_the_v5_path_can_authorize_and_v4_is_superseded() {
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
    let err = verify_live_authorization(Path::new(V4_RELATIVE_PATH), &ctx).unwrap_err();
    assert!(
        err.contains("superseded") && err.contains("launch-path"),
        "{err}"
    );
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
    requal_root_at("v4")
}

fn requal_root_at(tag: &str) -> (PathBuf, &'static str) {
    use g06a::{dry, economics, local_simulation, portal_upgrade as up, preflight};
    let root = std::env::temp_dir().join(format!("darkbot-{tag}-{}", std::process::id()));
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
    requalification_fixture::fixture(&root, dir, &format!("{FROZEN}/builder_policy.dbrec"), Path::new("."));
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
    // All CLI modes share this validation, before any hash/output/write.
    use g06a::requalification as r;
    let status_path = root.join(dir).join("requalification_status.dbrec");
    let original = std::fs::read(&status_path).unwrap();
    let original_record = decode(&r::SCHEMA, &original).unwrap();
    let copy = |record: &darkbot_bsc_record::Record| {
        let mut d = Draft::new(record.schema());
        for (tag, value) in record.fields() { d.push(*tag, value.clone()); }
        d
    };
    for case in 0..10 {
        let mut status = copy(&original_record);
        match case {
            0 => { std::fs::remove_file(&status_path).unwrap(); },
            1 => { status.set(r::VERDICT, "STOP"); },
            2 => { let mut row = Draft::new(&r::check::SCHEMA); row.set(r::check::NAME, "R0_UPGRADE").set(r::check::STATUS, "STOP").set(r::check::DETAIL, "offline-test"); status.clear(r::CHECKS); status.push(r::CHECKS, row.into_nested()); },
            3 => { status.clear(r::RECORDS); },
            4 => { status.push(r::RECORDS, original_record.get(r::RECORDS).unwrap().clone()); },
            5 => { status.set(r::REQUAL_DIR, "wrong/directory"); },
            6 => { status.set(r::FROZEN_DIR, "wrong/frozen"); },
            7 => { status.set(PROVENANCE_TOOL, "not-the-verifier"); },
            8 | 9 => {
                status.clear(r::RECORDS);
                for record in original_record.nested_all(r::RECORDS).unwrap() {
                    let mut row = copy(&record);
                    if record.text(r::bound::NAME).unwrap() == "native_quote.dbrec" {
                        if case == 8 { row.set(r::bound::NAME, "replacement.dbrec"); }
                        else { row.set(r::bound::SHA256, Value::B256([0;32])); }
                    }
                    status.push(r::RECORDS, row.into_nested());
                }
            },
            _ => unreachable!(),
        }
        if case != 0 { std::fs::write(&status_path, status.encode().unwrap()).unwrap(); }
        assert!(attempt1_v4_evidence(&root, dir, &policy).is_err(), "case {case}");
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_g07_acceptance_contract")).current_dir(&root).args([dir, policy.as_str(), "--check-v4"]).env_clear().output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        std::fs::write(&status_path, &original).unwrap();
    }
    assert!(attempt1_v4_evidence(&root, dir, &policy.replace("/builder_policy", "/./builder_policy")).is_err());
    // Even a matching status binding cannot make altered signed identity valid.
    let dry_path = root.join(dir).join("dry_result.dbrec");
    let dry_bytes = std::fs::read(&dry_path).unwrap();
    let dry = decode(&g06a::dry::NATIVE, &dry_bytes).unwrap();
    for tag in [g06a::dry::CHAIN_ID, g06a::dry::GAS_PRICE_WEI, g06a::dry::BUY_GAS_LIMIT, g06a::dry::LAUNCH_GAS_LIMIT, g06a::dry::BUYER, g06a::dry::PORTAL, g06a::dry::LAUNCH_TX_HASH, g06a::dry::BUY_VALUE_WEI] {
        let mut altered = copy(&dry);
        altered.set(tag, match dry.get(tag).unwrap() { Value::U64(n) => Value::U64(n+1), Value::Address(_) => Value::Address([0;20]), Value::B256(_) => Value::B256([0;32]), Value::U256(_) => Value::U256([0;32]), _ => unreachable!() });
        std::fs::write(&dry_path, altered.encode().unwrap()).unwrap();
        requalification_fixture::refresh(&root, dir, &policy);
        assert!(attempt1_v4_evidence(&root, dir, &policy).is_err(), "identity {tag}");
    }
    std::fs::write(&dry_path, &dry_bytes).unwrap();
    requalification_fixture::refresh(&root, dir, &policy);
    assert!(attempt1_v4_evidence(&root, dir, &policy).is_ok());
    for authorization in [Authorization::Proposed, Authorization::Approved] {
        let bytes = attempt1_v4(&evidence, authorization).unwrap();
        let text = decode(&contract::V4, &bytes).unwrap().render();
        for obsolete in ["authorization.evidence_paths.builder_requirements_refresh", "eth_estimateGas=5502725", "addendum stays bound"] { assert!(!text.contains(obsolete), "{obsolete}"); }
    }
    // An import where a native record is required is refused.
    std::fs::copy(
        format!("{FROZEN}/preflight.dbrec"),
        root.join(dir).join("preflight.dbrec"),
    )
    .unwrap();
    requalification_fixture::refresh(&root, dir, &policy);
    let e = attempt1_v4_evidence(&root, dir, &policy).unwrap_err();
    assert!(e.contains("must be a native record"), "{e}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn explicit_attempt_identity_and_individual_dry_flags_fail_closed() {
    use darkbot_bsc_execution::acceptance_contract::{ATTEMPT_ID, require_dry_pass};
    use darkbot_bsc_execution::acceptance_guard::verify_attempt_id;
    use g06a::dry as d;
    assert!(verify_attempt_id(ATTEMPT_ID, ATTEMPT_ID).is_ok());
    for wrong in ["", "other", "g07-mainnet-attempt-1-dry-20261015T222241Z "] { assert!(verify_attempt_id(ATTEMPT_ID, wrong).is_err()); }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let bytes = std::fs::read(root.join(darkbot_bsc_execution::acceptance_contract::DRY_RECORD)).unwrap();
    let good = decode(&d::SCHEMA, &bytes).unwrap();
    assert!(require_dry_pass(&good).is_ok());
    for flag in [d::DRY_1,d::DRY_2,d::DRY_3,d::DRY_4,d::DRY_ALL,d::ZERO_EGRESS_STRUCTURAL] {
        let mut record = Draft::new(&d::SCHEMA);
        for (tag, value) in good.fields() { record.push(*tag, value.clone()); }
        record.set(flag, false);
        assert!(require_dry_pass(&decode(&d::SCHEMA, &record.encode().unwrap()).unwrap()).is_err());
    }
}

/// The v4 bytes with one record hash re-bound, keeping every other field.
fn rebound(v4: &[u8], tag: u16, sha: [u8; 32]) -> Vec<u8> {
    let record = decode(&contract::V4, v4).unwrap();
    let mut d = Draft::new(&contract::V4);
    for (t, value) in record.fields() {
        d.push(*t, value.clone());
    }
    d.set(tag, Value::B256(sha));
    d.encode().unwrap()
}

/// A record of `schema` with one named field replaced.
fn edited(bytes: &[u8], schema: &'static Schema, name: &str, value: Value) -> Vec<u8> {
    let record = decode(schema, bytes).unwrap();
    let mut d = Draft::new(schema);
    for (t, v) in record.fields() {
        d.push(*t, v.clone());
    }
    d.set(
        schema.fields.iter().find(|f| f.name == name).unwrap().tag,
        value,
    );
    d.encode().unwrap()
}

#[test]
fn the_guard_itself_binds_local_simulation_economics_and_portal_upgrade() {
    use g06a::{economics, local_simulation, portal_upgrade};
    at_repo_root();
    let (root, dir) = requal_root_at("v4-further");
    let policy = format!("{FROZEN}/builder_policy.dbrec");
    let evidence = attempt1_v4_evidence(&root, dir, &policy).unwrap();
    let v4 = attempt1_v4(&evidence, Authorization::Proposed).unwrap();
    let verified = verify_further_v4_evidence(&root, &v4).unwrap();
    let file = |name: &str| root.join(dir).join(name);
    assert_eq!(
        verified.local_simulation_sha256,
        sha256_hex(&std::fs::read(file("local_simulation.dbrec")).unwrap())
    );
    let simulated = decode(
        &g06a::local_simulation::NATIVE,
        &std::fs::read(file("local_simulation.dbrec")).unwrap(),
    )
    .unwrap();
    let gas = simulated
        .fields()
        .iter()
        .find(|(t, _)| simulated.schema().field(*t).unwrap().name == "launch_gas_used")
        .map(|(_, v)| v.clone());
    assert_eq!(gas, Some(Value::U64(verified.local_launch_gas_used)));
    let cases: [(&str, u16, &'static Schema, &str, Value); 3] = [
        (
            "local_simulation.dbrec",
            contract::LOCAL_SIMULATION_RECORD_SHA256,
            &local_simulation::NATIVE,
            "token",
            Value::Address([0x11; 20]),
        ),
        (
            "economics_crosscheck.dbrec",
            contract::ECONOMICS_RECORD_SHA256,
            &economics::NATIVE,
            "token",
            Value::Address([0x11; 20]),
        ),
        (
            "portal_upgrade.dbrec",
            contract::PORTAL_UPGRADE_RECORD_SHA256,
            &portal_upgrade::SCHEMA,
            "current_implementation",
            Value::Address([0x11; 20]),
        ),
    ];
    for (name, tag, schema, field, value) in cases {
        let path = file(name);
        let original = std::fs::read(&path).unwrap();
        // Missing.
        std::fs::remove_file(&path).unwrap();
        assert!(
            verify_further_v4_evidence(&root, &v4).is_err(),
            "{name} missing"
        );
        // Mutated bytes.
        let mut mutated = original.clone();
        mutated[20] ^= 1;
        std::fs::write(&path, &mutated).unwrap();
        assert!(
            verify_further_v4_evidence(&root, &v4)
                .unwrap_err()
                .contains("SHA-256"),
            "{name} mutated"
        );
        // Swapped for another bound record.
        std::fs::write(&path, std::fs::read(file("dry_result.dbrec")).unwrap()).unwrap();
        assert!(
            verify_further_v4_evidence(&root, &v4).is_err(),
            "{name} swapped"
        );
        // Correctly bound, but another identity than the DRY / preflight.
        let other = edited(&original, schema, field, value);
        std::fs::write(&path, &other).unwrap();
        let v4_other = rebound(&v4, tag, darkbot_bsc_record::sha256(&other));
        let error = verify_further_v4_evidence(&root, &v4_other).unwrap_err();
        assert!(
            error.contains("differs") || error.contains("does not end"),
            "{name}: {error}"
        );
        std::fs::write(&path, &original).unwrap();
        assert!(verify_further_v4_evidence(&root, &v4).is_ok());
    }
    // A bound path may not leave the root.
    let record = decode(&contract::V4, &v4).unwrap();
    let mut d = Draft::new(&contract::V4);
    for (t, value) in record.fields() {
        d.push(*t, value.clone());
    }
    d.set(
        contract::ECONOMICS_RECORD_PATH,
        "../outside/economics_crosscheck.dbrec",
    );
    assert!(
        verify_further_v4_evidence(&root, &d.encode().unwrap())
            .unwrap_err()
            .contains("plain relative")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn guard_proofs_come_only_from_their_checks() {
    use g06a::dry as d;
    at_repo_root();
    let good = native_copy("dry_result.dbrec", &d::SCHEMA, &d::NATIVE);
    assert!(DryPassProof::check(&good).is_ok());
    // One failed DRY flag: no proof, so no guard context can be built.
    let record = decode(&d::NATIVE, &good).unwrap();
    let mut failed = Draft::new(&d::NATIVE);
    for (t, v) in record.fields() {
        failed.push(*t, v.clone());
    }
    failed.set(d::DRY_3, false);
    assert!(DryPassProof::check(&failed.encode().unwrap()).is_err());
    assert!(DryPassProof::check(b"not a record").is_err());
    let witness = std::fs::read(WITNESS_SOURCE_RECORD).unwrap();
    let attempt = decode(&darkbot_bsc_record::schema::g07::witness_source::SCHEMA, &witness)
        .unwrap()
        .text(darkbot_bsc_record::schema::g07::witness_source::ATTEMPT_ID)
        .unwrap()
        .to_string();
    let schema = "bsc/config/producer_execution_witness_v1.schema.json";
    assert!(WitnessSourceProof::check(&witness, &attempt, schema).is_ok());
    assert!(WitnessSourceProof::check(&witness, "another-attempt", schema).is_err());
    assert!(WitnessSourceProof::check(&witness, &attempt, "other.schema.json").is_err());
}

const CANDIDATE: &str = requalification_fixture::CANDIDATE_RECORD;

/// A v5-shaped stand-in run: the v4 stand-ins, their v4 proposal at the v4
/// path, the real launch-path snapshot, the candidate record and DRY5.
fn v5_root(tag: &str) -> (PathBuf, &'static str, String) {
    let (root, dir) = requal_root_at(tag);
    let policy = format!("{FROZEN}/builder_policy.dbrec");
    requalification_fixture::upgrade_to_v5(&root, dir, &policy, Path::new("."));
    (root, dir, policy)
}

/// `bytes` of `schema` with one field set, every other field kept.
fn with_field(bytes: &[u8], schema: &'static Schema, tag: u16, value: Value) -> Vec<u8> {
    let record = decode(schema, bytes).unwrap();
    let mut d = Draft::new(schema);
    for (t, v) in record.fields() {
        d.push(*t, v.clone());
    }
    d.set(tag, value);
    d.encode().unwrap()
}

/// The candidate record with one field set and its ID recomputed.
fn candidate_with(bytes: &[u8], tag: u16, value: Value) -> Vec<u8> {
    use darkbot_bsc_record::schema::g07_candidate::{candidate as c, candidate_id_of};
    let edited = with_field(bytes, &c::SCHEMA, tag, value);
    let id = candidate_id_of(&decode(&c::SCHEMA, &edited).unwrap()).unwrap();
    with_field(&edited, &c::SCHEMA, c::CANDIDATE_ID, Value::B256(id))
}

#[test]
fn v5_binds_the_launch_path_and_candidate_and_supersedes_the_v4_proposal() {
    at_repo_root();
    let (root, dir, policy) = v5_root("v5");
    let evidence = attempt1_v5_evidence(&root, dir, &policy, CANDIDATE).unwrap();
    let bytes = attempt1_v5(&evidence, Authorization::Proposed).unwrap();
    let proposal = parse_v5(&bytes).unwrap();
    let approved = parse_v5(&attempt1_v5(&evidence, Authorization::Approved).unwrap()).unwrap();
    assert!(!proposal.approved && !proposal.execution_ready);
    assert!(approved.approved && approved.execution_ready);
    assert_ne!(proposal.sha256, approved.sha256);
    let v4 = std::fs::read(root.join(V4_RELATIVE_PATH)).unwrap();
    let file = |name: &str| std::fs::read(root.join(dir).join(name)).unwrap();
    assert_eq!(proposal.supersedes_contract_sha256, Some(sha256_hex(&v4)));
    assert_eq!(
        proposal.launch_path_record_sha256,
        Some(sha256_hex(&file("launch_path.dbrec")))
    );
    assert_eq!(proposal.candidate_profile.as_deref(), Some("SELF_A"));
    assert_eq!(
        proposal.candidate_id.as_deref(),
        Some(darkbot_bsc_record::hex(&evidence.candidate_id).as_str())
    );
    // Every v4 binding and term carries over unchanged.
    let old = parse_v4(&v4).unwrap();
    assert_eq!(
        [
            &proposal.dry_evidence,
            &proposal.preflight_evidence,
            &proposal.builder_policy_evidence,
            &proposal.producer_witness_source
        ],
        [
            &old.dry_evidence,
            &old.preflight_evidence,
            &old.builder_policy_evidence,
            &old.producer_witness_source
        ]
    );
    assert_eq!(
        (&proposal.local_simulation, &proposal.economics),
        (&old.local_simulation, &old.economics)
    );
    assert_eq!(
        (
            proposal.max_total_per_attempt,
            proposal.absolute_budget_wei,
            proposal.max_attempts,
            &proposal.attempt_id
        ),
        (
            old.max_total_per_attempt,
            old.absolute_budget_wei,
            old.max_attempts,
            &old.attempt_id
        )
    );
    assert_eq!(proposal.absolute_budget_wei, 481_163_500_000_000);
    // Neither version decodes as the other.
    assert!(parse_v5(&v4).is_err() && parse_v4(&bytes).is_err());
    let text = decode(&contract::V5, &bytes).unwrap().render();
    for expected in [
        "bsc/config/g07_attempt1_acceptance_v5.dbrec",
        "FROZEN_APPROVED_V5_SHA256",
        "record type 34, schema version 5",
        "superseded proposal without launch-path binding",
        "22 code entries, 4 storage selectors",
    ] {
        assert!(text.contains(expected), "{expected}");
    }
    let (_, bound) = verify_further_v5_evidence(&root, &bytes).unwrap();
    assert_eq!(
        (bound.snapshot().code.len(), bound.snapshot().storage.len()),
        (22, 4)
    );
    // The tool writes exactly these bytes once and reproduces them; a v5 run
    // never reproduces the v4 proposal.
    let tool = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_g07_acceptance_contract"))
            .current_dir(&root)
            .args(args)
            .env_clear()
            .output()
            .unwrap()
    };
    let wrote = tool(&[dir, &policy, CANDIDATE]);
    assert!(
        wrote.status.success(),
        "{}",
        String::from_utf8_lossy(&wrote.stderr)
    );
    assert_eq!(std::fs::read(root.join(V5_RELATIVE_PATH)).unwrap(), bytes);
    assert!(tool(&[dir, &policy, CANDIDATE, "--check"]).status.success());
    assert!(!tool(&[dir, &policy, CANDIDATE]).status.success());
    assert!(!tool(&[dir, &policy, "--check-v4"]).status.success());
    let sha = tool(&[dir, &policy, CANDIDATE, "--approved-sha"]);
    assert!(String::from_utf8_lossy(&sha.stdout).contains(&approved.sha256));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_missing_manipulated_stale_or_contradictory_snapshot_or_candidate_makes_no_v5() {
    use darkbot_bsc_record::schema::g06a::launch_path as lp;
    use darkbot_bsc_record::schema::g07_candidate::candidate as c;
    at_repo_root();
    let (root, dir, policy) = v5_root("v5-negative");
    let refuse = |what: &str| attempt1_v5_evidence(&root, dir, &policy, CANDIDATE).expect_err(what);
    let lp_file = root.join(dir).join("launch_path.dbrec");
    let original = std::fs::read(&lp_file).unwrap();
    // Missing, or manipulated under the run's binding.
    std::fs::remove_file(&lp_file).unwrap();
    refuse("missing");
    let mut flipped = original.clone();
    flipped[40] ^= 1;
    std::fs::write(&lp_file, &flipped).unwrap();
    assert!(refuse("mutated").contains("binding mismatch"));
    // Re-bound by the run, but stale, foreign or contradicting the DRY.
    let block = decode(&lp::SCHEMA, &original)
        .unwrap()
        .u64(lp::BLOCK_NUMBER)
        .unwrap();
    let other_token = with_field(
        &with_field(
            &original,
            &lp::SCHEMA,
            lp::TOKEN,
            Value::Address([0x11; 20]),
        ),
        &lp::SCHEMA,
        lp::LAUNCH_RETURN_TOKEN,
        Value::Address([0x11; 20]),
    );
    for (bytes, expected) in [
        (
            with_field(
                &original,
                &lp::SCHEMA,
                lp::BLOCK_NUMBER,
                Value::U64(block + 1),
            ),
            "stale or foreign",
        ),
        (
            with_field(&original, &lp::SCHEMA, lp::BLOCK_HASH, Value::B256([9; 32])),
            "stale or foreign",
        ),
        (
            with_field(&original, &lp::SCHEMA, lp::BUY_SUCCESS, Value::Bool(false)),
            "did not succeed",
        ),
        (other_token, "another token"),
    ] {
        std::fs::write(&lp_file, &bytes).unwrap();
        requalification_fixture::refresh_v5(&root, dir, &policy);
        assert!(refuse(expected).contains(expected), "{expected}");
    }
    std::fs::write(&lp_file, &original).unwrap();
    // A v4-shaped run (no DRY5, no launch-path record) never makes a v5.
    requalification_fixture::refresh(&root, dir, &policy);
    refuse("v4-shaped run");
    requalification_fixture::refresh_v5(&root, dir, &policy);
    assert!(attempt1_v5_evidence(&root, dir, &policy, CANDIDATE).is_ok());
    // The candidate: tampered ID, another profile, deployment, token or
    // signed hash, or absent.
    let record = root.join(CANDIDATE);
    let good = std::fs::read(&record).unwrap();
    for (bytes, expected) in [
        (
            with_field(&good, &c::SCHEMA, c::CANDIDATE_ID, Value::B256([1; 32])),
            "identity",
        ),
        (
            candidate_with(&good, c::PROFILE, Value::Text("SELF_C".into())),
            "identity",
        ),
        (
            candidate_with(&good, c::DEPLOYMENT_RECORD_SHA256, Value::B256([2; 32])),
            "requalification preflight",
        ),
        (
            candidate_with(&good, c::PORTAL_VERSION, Value::Text("v5.26.0".into())),
            "requalification preflight",
        ),
        (
            candidate_with(&good, c::TOKEN, Value::Address([3; 20])),
            "launcher or token",
        ),
        (
            candidate_with(&good, c::LAUNCH_TX_HASH, Value::B256([4; 32])),
            "signed hashes",
        ),
    ] {
        std::fs::write(&record, &bytes).unwrap();
        assert!(refuse(expected).contains(expected), "{expected}");
    }
    std::fs::remove_file(&record).unwrap();
    refuse("no candidate");
    std::fs::write(&record, &good).unwrap();
    // The superseded v4 must be the inactive proposal.
    let v4_path = root.join(V4_RELATIVE_PATH);
    let v4 = std::fs::read(&v4_path).unwrap();
    std::fs::write(
        &v4_path,
        with_field(&v4, &contract::V4, contract::APPROVED, Value::Bool(true)),
    )
    .unwrap();
    assert!(refuse("approved v4").contains("inactive proposal"));
    std::fs::remove_file(&v4_path).unwrap();
    refuse("no v4");
    std::fs::write(&v4_path, &v4).unwrap();
    assert!(attempt1_v5_evidence(&root, dir, &policy, CANDIDATE).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_guard_itself_binds_the_v5_launch_path_and_refuses_every_deviation() {
    use darkbot_bsc_record::schema::g06a::launch_path as lp;
    at_repo_root();
    let (root, dir, policy) = v5_root("v5-guard");
    let v5 = attempt1_v5(
        &attempt1_v5_evidence(&root, dir, &policy, CANDIDATE).unwrap(),
        Authorization::Proposed,
    )
    .unwrap();
    assert!(verify_further_v5_evidence(&root, &v5).is_ok());
    // A v4 contract is no v5 for the guard.
    assert!(
        verify_further_v5_evidence(&root, &std::fs::read(root.join(V4_RELATIVE_PATH)).unwrap())
            .is_err()
    );
    let lp_file = root.join(dir).join("launch_path.dbrec");
    let original = std::fs::read(&lp_file).unwrap();
    let refused = |contract: &[u8]| verify_further_v5_evidence(&root, contract).unwrap_err();
    std::fs::remove_file(&lp_file).unwrap();
    assert!(refused(&v5).contains("launch_path.dbrec"));
    let mut flipped = original.clone();
    flipped[40] ^= 1;
    std::fs::write(&lp_file, &flipped).unwrap();
    assert!(refused(&v5).contains("SHA-256"));
    std::fs::write(
        &lp_file,
        std::fs::read(root.join(dir).join("dry_result.dbrec")).unwrap(),
    )
    .unwrap();
    assert!(verify_further_v5_evidence(&root, &v5).is_err());
    // Correctly re-bound in the contract, but stale or contradicting the DRY.
    let block = decode(&lp::SCHEMA, &original)
        .unwrap()
        .u64(lp::BLOCK_NUMBER)
        .unwrap();
    let other_token = with_field(
        &with_field(
            &original,
            &lp::SCHEMA,
            lp::TOKEN,
            Value::Address([0x11; 20]),
        ),
        &lp::SCHEMA,
        lp::LAUNCH_RETURN_TOKEN,
        Value::Address([0x11; 20]),
    );
    for (bytes, expected) in [
        (
            with_field(
                &original,
                &lp::SCHEMA,
                lp::BLOCK_NUMBER,
                Value::U64(block + 1),
            ),
            "stale or foreign",
        ),
        (other_token, "another token"),
    ] {
        std::fs::write(&lp_file, &bytes).unwrap();
        let rebound = with_field(
            &v5,
            &contract::V5,
            contract::LAUNCH_PATH_RECORD_SHA256,
            Value::B256(darkbot_bsc_record::sha256(&bytes)),
        );
        assert!(refused(&rebound).contains(expected), "{expected}");
    }
    std::fs::write(&lp_file, &original).unwrap();
    // A malformed candidate binding or an escaping path.
    for (tag, value, expected) in [
        (
            contract::CANDIDATE_PROFILE,
            Value::Text("SELF_C".into()),
            "candidate",
        ),
        (contract::CANDIDATE_ID, Value::B256([0; 32]), "candidate"),
        (
            contract::LAUNCH_PATH_RECORD_PATH,
            Value::Text("../outside/launch_path.dbrec".into()),
            "plain relative",
        ),
    ] {
        assert!(
            refused(&with_field(&v5, &contract::V5, tag, value)).contains(expected),
            "{expected}"
        );
    }
    assert!(verify_further_v5_evidence(&root, &v5).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

const B_CANDIDATE: &str = requalification_fixture::B_CANDIDATE_RECORD;

/// A SELF_B-shaped stand-in run (`v5_root` made B's by the fixture).
fn b_root(tag: &str) -> (PathBuf, &'static str, String) {
    let (root, dir, policy) = v5_root(tag);
    requalification_fixture::make_b(&root, dir, &policy);
    (root, dir, policy)
}

#[test]
fn a_secret_salt_v5_binds_its_own_run_and_only_the_private_cross_check() {
    use darkbot_bsc_record::schema::g06a::economics;
    at_repo_root();
    let (root, dir, policy) = b_root("v5-b");
    let evidence = attempt1_v5_evidence(&root, dir, &policy, B_CANDIDATE).unwrap();
    let bytes = attempt1_v5(&evidence, Authorization::Proposed).unwrap();
    let v5 = parse_v5(&bytes).unwrap();
    assert_eq!(v5.candidate_profile.as_deref(), Some("SELF_B"));
    let file = root.join(dir).join("economics_crosscheck.dbrec");
    let econ = std::fs::read(&file).unwrap();
    assert_eq!(
        v5.economics.as_ref().map(|h| h.record_sha256.clone()),
        Some(sha256_hex(&econ))
    );
    let (_, bound) = verify_further_v5_evidence(&root, &bytes).unwrap();
    assert_eq!(bound.snapshot().token, requalification_fixture::B_TOKEN);
    // A's candidate never takes B's run: the frozen A identity differs.
    assert!(attempt1_v5_evidence(&root, dir, &policy, CANDIDATE).is_err());
    let refuse =
        |what: &str| attempt1_v5_evidence(&root, dir, &policy, B_CANDIDATE).expect_err(what);
    // A plain v5 status (no DRY6, no candidate binding) is no B run.
    requalification_fixture::refresh_v5(&root, dir, &policy);
    refuse("no DRY6");
    // Another salt commitment than the candidate's.
    std::fs::write(
        &file,
        requalification_fixture::with(
            &econ,
            &economics::PRIVATE,
            &[("salt_commitment", Value::B256([0x5d; 32]))],
        ),
    )
    .unwrap();
    requalification_fixture::refresh_b(&root, dir, &policy);
    assert!(refuse("commitment").contains("candidate's salt"));
    // A cross-check with the salt in clear (the native record) is no B evidence.
    let native = requalification_fixture::with(
        &native_copy(
            "economics_crosscheck.dbrec",
            &economics::SCHEMA,
            &economics::NATIVE,
        ),
        &economics::NATIVE,
        &[("token", Value::Address(requalification_fixture::B_TOKEN))],
    );
    std::fs::write(&file, &native).unwrap();
    requalification_fixture::refresh_b(&root, dir, &policy);
    assert!(refuse("native cross-check").contains("not an accepted input"));
    std::fs::write(&file, &econ).unwrap();
    requalification_fixture::refresh_b(&root, dir, &policy);
    assert!(attempt1_v5_evidence(&root, dir, &policy, B_CANDIDATE).is_ok());
    // The guard pins the cross-check record to the profile: B re-bound to a
    // native record, or the B contract relabelled SELF_A.
    let other = format!("{dir}/economics_native.dbrec");
    std::fs::write(root.join(&other), &native).unwrap();
    let rebound = with_field(
        &with_field(
            &bytes,
            &contract::V5,
            contract::ECONOMICS_RECORD_PATH,
            Value::Text(other),
        ),
        &contract::V5,
        contract::ECONOMICS_RECORD_SHA256,
        Value::B256(darkbot_bsc_record::sha256(&native)),
    );
    let relabelled = with_field(
        &bytes,
        &contract::V5,
        contract::CANDIDATE_PROFILE,
        Value::Text("SELF_A".into()),
    );
    for contract_bytes in [rebound, relabelled] {
        assert!(
            verify_further_v5_evidence(&root, &contract_bytes)
                .unwrap_err()
                .contains("wrong cross-check record")
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
