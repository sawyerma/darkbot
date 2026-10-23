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

/// v9.1 D6: no G0.6a DRY evidence schema can hold a raw payload (signed RLP,
/// a bundle, a salt blob): no bytes field anywhere, nested included.
#[test]
fn no_g06a_evidence_schema_has_a_raw_bytes_field() {
    use darkbot_bsc_record::{Kind, Schema, schema};
    fn walk(s: &Schema) {
        for f in s.fields {
            assert!(f.kind != Kind::Bytes, "{}.{} holds bytes", s.name, f.name);
            if let Some(nested) = f.nested {
                walk(nested);
            }
        }
    }
    for s in schema::g06a::ALL {
        walk(s);
    }
}

/// v9.1 D6: every G0.6a evidence record on disk (committed, and locally kept
/// runs where present) decodes and carries no raw-transaction-sized hex.
#[test]
fn no_g06a_evidence_record_holds_signed_transaction_bytes() {
    use darkbot_bsc_record::schema::g06a::dry;
    let raw = |view: &str| {
        view.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
            w.len() > 200 && w.starts_with("0x") && w[2..].bytes().all(|b| b.is_ascii_hexdigit())
        })
    };
    // Control: a raw signed transaction planted in a text field is found.
    let frozen = decode(
        &dry::SCHEMA,
        &std::fs::read(evidence("g07_binary_import_20261016").join("dry_result.dbrec")).unwrap(),
    )
    .unwrap();
    let mut planted = darkbot_bsc_record::Draft::new(&dry::SCHEMA);
    for (tag, value) in frozen.fields() {
        planted.push(*tag, value.clone());
    }
    let note = dry::SCHEMA
        .fields
        .iter()
        .find(|f| f.name == "qualification_note")
        .unwrap()
        .tag;
    planted.set(note, format!("0x{}", "f8".repeat(150)));
    let planted = decode(&dry::SCHEMA, &planted.encode().unwrap()).unwrap();
    assert!(raw(&planted.render()) && !raw(&frozen.render()));
    let root = evidence("");
    let mut seen = 0;
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "dbrec") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let Ok(record) = darkbot_bsc_record::schema::decode_any(&bytes) else {
                panic!("{} is not a known record", path.display());
            };
            assert!(
                !raw(&record.render()),
                "{} holds raw-transaction-sized hex",
                path.display()
            );
            seen += 1;
        }
    }
    assert!(seen >= 8, "{seen}");
}
