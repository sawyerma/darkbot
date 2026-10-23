//! v9.1 D5: A's historical identity is immutable across the D1–D4 split.
//! The signing algorithm of the frozen bytes, every frozen A evidence record
//! and A's deterministic public salt are pinned; B never shares them.

use alloy_primitives::keccak256;
use darkbot_bsc_record::schema::g06a::economics;
use darkbot_bsc_record::{decode, hex, sha256};
use std::path::{Path, PathBuf};

fn evidence(dir: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../native/evidence/g06a")
        .join(dir)
}

#[test]
fn the_signing_algorithm_of_a_is_byte_identical() {
    // transactions.rs builds and signs the exact frozen launch/BUY bytes.
    let source = include_bytes!("../src/bin/g07_mainnet_self_launch_buy/transactions.rs");
    assert_eq!(
        hex(&sha256(source)),
        "2da8c1dd3ee9f09bc8e481e72b129e8a65eb8cbcbd321a7489593a2da5e827a0"
    );
}

#[test]
fn every_frozen_a_evidence_record_matches_its_manifest() {
    for dir in [
        "g07_binary_import_20261016",
        "launch_path_frozen_a_20261023",
    ] {
        let manifest = std::fs::read_to_string(evidence(dir).join("SHA256SUMS")).unwrap();
        let mut files = 0;
        for line in manifest.lines().filter(|l| !l.trim().is_empty()) {
            let (digest, name) = line.split_once("  ").unwrap();
            let bytes = std::fs::read(evidence(dir).join(name.trim_start_matches("./"))).unwrap();
            assert_eq!(hex(&sha256(&bytes)), digest, "{dir}/{name}");
            files += 1;
        }
        assert!(files >= 2, "{dir}");
    }
    // The pinned identities the D3 handoff and the v4/v5 generators rely on.
    let manifest =
        std::fs::read_to_string(evidence("g07_binary_import_20261016").join("SHA256SUMS")).unwrap();
    for pinned in [
        "c7153a7da4c6154a2a6af384c9836d7141d16fd643662d385d20d3d18ba99b98  dry_result.dbrec",
        "2b018137099c69bfd1069abfe08f320d19ca0ab97d1ee7905d5c43c594de33e1  preflight.dbrec",
    ] {
        assert!(manifest.contains(pinned), "{pinned}");
    }
}

#[test]
fn a_salt_is_the_public_deterministic_one_and_never_a_secret() {
    let bytes =
        std::fs::read(evidence("g07_binary_import_20261016").join("economics_crosscheck.dbrec"))
            .unwrap();
    let record = decode(&economics::SCHEMA, &bytes).unwrap();
    let salt_tag = |name: &str| {
        economics::SCHEMA
            .fields
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .tag
    };
    let counter = record.u64(salt_tag("salt_counter")).unwrap();
    let salt = record.b256(salt_tag("salt")).unwrap();
    // g06a_economics_evm_crosscheck.cjs chooseSalt: the first unused vanity
    // salt of a public label sequence; anyone can recompute it.
    assert_eq!(
        keccak256(format!("darkbot-g06a-economics-v1:{counter}").as_bytes()).0,
        salt
    );
}
