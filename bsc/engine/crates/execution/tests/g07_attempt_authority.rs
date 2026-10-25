//! BSC-005: the user's grant of the shared one-attempt authority, against a
//! local bare `origin` (no network). Only an approved, execution-ready v5
//! with its attempt ID typed out is granted, once; the runner's binding
//! (v5 file SHA-256, the bound DRY's signed hashes) consumes it once.

#[path = "support/requalification.rs"]
mod requalification_fixture;

use darkbot_bsc_execution::acceptance_contract::{
    Authorization, WITNESS_SOURCE_RECORD, attempt1_v5, attempt1_v5_evidence,
};
use darkbot_bsc_execution::acceptance_guard::{V3_RELATIVE_PATH, V5_RELATIVE_PATH};
use darkbot_bsc_execution::attempt_authority::{self, Binding, Git};
use darkbot_bsc_record::schema::g06a::dry;
use darkbot_bsc_record::{decode, sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TOOL: &str = env!("CARGO_BIN_EXE_g07_attempt_authority");
const FROZEN: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016";
const DIR: &str = "bsc/native/evidence/g06a/requal_authority";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn git(at: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(at)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A Git checkout with a bare origin, the stand-in requalification run and
/// its v5 contract in both variants (written by the caller).
fn root() -> (PathBuf, Vec<u8>, Vec<u8>) {
    let repo = repo();
    let base = std::env::temp_dir().join(format!("g07-grant-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("checkout");
    std::fs::create_dir_all(&root).unwrap();
    git(&base, &["init", "-q", "--bare", "origin.git"]);
    git(&root, &["init", "-q"]);
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            base.join("origin.git").to_str().unwrap(),
        ],
    );
    std::fs::create_dir_all(root.join("bsc/engine")).unwrap();
    std::fs::write(root.join("bsc/engine/Cargo.toml"), "").unwrap();
    let policy = format!("{FROZEN}/builder_policy.dbrec");
    for path in [WITNESS_SOURCE_RECORD, V3_RELATIVE_PATH, policy.as_str()] {
        std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        std::fs::copy(repo.join(path), root.join(path)).unwrap();
    }
    requalification_fixture::stand_in_records(&root, DIR, &repo);
    requalification_fixture::fixture(&root, DIR, &policy, &repo);
    requalification_fixture::upgrade_to_v5(&root, DIR, &policy, &repo);
    let evidence = attempt1_v5_evidence(
        &root,
        DIR,
        &policy,
        requalification_fixture::CANDIDATE_RECORD,
    )
    .unwrap();
    let proposal = attempt1_v5(&evidence, Authorization::Proposed).unwrap();
    let approved = attempt1_v5(&evidence, Authorization::Approved).unwrap();
    (root, proposal, approved)
}

fn tool(root: &Path, args: &[&str]) -> Output {
    Command::new(TOOL)
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn only_an_approved_v5_is_granted_once_and_the_runner_binding_consumes_it_once() {
    let (root, proposal, approved) = root();
    let v5 = root.join(V5_RELATIVE_PATH);
    std::fs::write(&v5, &proposal).unwrap();
    let status = tool(&root, &["status"]);
    assert!(status.status.success(), "{}", text(&status));
    assert!(text(&status).contains("v5_approved=false\nauthority=MISSING"));
    let attempt = text(&status)
        .lines()
        .find_map(|l| l.strip_prefix("attempt_id="))
        .unwrap()
        .to_string();
    let refused = tool(&root, &["grant", "--attempt", &attempt]);
    assert_eq!(refused.status.code(), Some(2));
    assert!(text(&refused).contains("not the approved"));

    std::fs::write(&v5, &approved).unwrap();
    assert_eq!(
        tool(&root, &["grant", "--attempt", "another-attempt"])
            .status
            .code(),
        Some(2)
    );
    let granted = tool(&root, &["grant", "--attempt", &attempt]);
    assert!(granted.status.success(), "{}", text(&granted));
    assert!(text(&granted).contains("authority=AUTHORIZED "));
    let again = tool(&root, &["grant", "--attempt", &attempt]);
    assert_eq!(again.status.code(), Some(2));
    assert!(text(&again).contains("exists already"));
    assert!(text(&tool(&root, &["status"])).contains("authority=AUTHORIZED"));

    // The runner's binding: the v5 file's SHA-256 and the DRY's signed hashes.
    let d = decode(
        &dry::NATIVE,
        &std::fs::read(root.join(DIR).join("dry_result.dbrec")).unwrap(),
    )
    .unwrap();
    let binding = Binding {
        attempt_id: attempt.clone(),
        v5_sha256: sha256(&approved),
        launch_tx_hash: d.b256(dry::LAUNCH_TX_HASH).unwrap(),
        buy_tx_hash: d.b256(dry::BUY_TX_HASH).unwrap(),
    };
    let checkout = Git::new(root.clone(), "origin");
    attempt_authority::consume(&checkout, &binding, "test-run").unwrap();
    assert!(attempt_authority::consume(&checkout, &binding, "second").is_err());
    assert!(text(&tool(&root, &["status"])).contains("authority=CONSUMED_OR_FOREIGN "));
    assert_eq!(
        tool(&root, &["grant", "--attempt", &attempt]).status.code(),
        Some(2)
    );
}
