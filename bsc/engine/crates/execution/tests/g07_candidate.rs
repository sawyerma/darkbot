//! The real `g07_candidate` binary in a throwaway main-checkout root: A and
//! B are created, reopened and switched; no output ever carries the B salt;
//! the sources have no network, key, signing or send path.

use darkbot_bsc_record::{decode, schema::g07_candidate::secret};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_g07_candidate");
const MAIN: &str = include_str!("../src/bin/g07_candidate/main.rs");
const CANDIDATE: &str = include_str!("../src/bin/g07_candidate/candidate.rs");

fn frozen(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../native/evidence/g06a/g07_binary_import_20261016")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn fake_root() -> PathBuf {
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("g07-candidate-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("bsc/config")).unwrap();
    std::fs::write(root.join("bsc/.gitignore"), "/.local/\n").unwrap();
    // The checkout check only needs the v4 path to exist.
    std::fs::write(
        root.join("bsc/config/g07_attempt1_acceptance_v4.dbrec"),
        b"unused",
    )
    .unwrap();
    root
}

fn run(root: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(root)
        .env_clear()
        .output()
        .unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn a_and_b_are_managed_offline_without_leaking_the_b_salt() {
    let root = fake_root();
    let (econ, dry, pre) = (
        frozen("economics_crosscheck.dbrec"),
        frozen("dry_result.dbrec"),
        frozen("preflight.dbrec"),
    );
    let mut outputs = Vec::new();
    let mut step = |args: &[&str], code: i32, expect: &str| {
        let (got, text) = run(&root, args);
        assert_eq!(got, code, "{args:?}: {text}");
        assert!(text.contains(expect), "{args:?}: missing {expect}: {text}");
        outputs.push(text);
    };
    step(
        &[
            "init-a",
            "--economics",
            &econ,
            "--dry",
            &dry,
            "--preflight",
            &pre,
        ],
        0,
        "SELF_A.token = 0xe51c4dEb1038AfcD95ef12d4a649CdBCF6FA7777",
    );
    step(
        &["create-b", "--dry", &dry, "--preflight", &pre],
        0,
        "SELF_B.qualification = UNQUALIFIED",
    );
    let salt_file = root.join("bsc/.local/g07_candidates/b.salt.dbrec");
    let salt_bytes = std::fs::read(&salt_file).unwrap();
    assert_eq!(
        std::fs::metadata(&salt_file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(root.join("bsc/.local/g07_candidates"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let salt = decode(&secret::SCHEMA, &salt_bytes)
        .unwrap()
        .b256(secret::SALT)
        .unwrap();
    step(
        &["create-b", "--dry", &dry, "--preflight", &pre],
        2,
        "never re-rolled",
    );
    assert_eq!(std::fs::read(&salt_file).unwrap(), salt_bytes);
    step(&["show"], 0, "selection = NONE");
    step(
        &["select", "B", "--expect-generation", "0"],
        0,
        "selection.generation = 1",
    );
    step(
        &["select", "A", "--expect-generation", "0"],
        2,
        "stale selection",
    );
    step(
        &["select", "A", "--expect-generation", "1"],
        0,
        "selection.profile = SELF_A",
    );
    step(&["show"], 0, "guard = UNARMED");
    std::fs::create_dir_all(root.join("bsc/.local/g07/attempt-x")).unwrap();
    step(
        &["select", "B", "--expect-generation", "2"],
        2,
        "switching is forbidden",
    );
    step(
        &["select", "C", "--expect-generation", "2"],
        2,
        "unknown candidate profile",
    );

    let hex: String = salt.iter().map(|b| format!("{b:02x}")).collect();
    for text in &outputs {
        let lower = text.to_lowercase();
        assert!(!lower.contains(&hex), "B salt leaked: {text}");
        assert!(!lower.contains("salt ="), "salt field printed: {text}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn outside_a_main_checkout_nothing_is_created() {
    let dir = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("g07-candidate-noroot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (code, text) = run(&dir, &["show"]);
    assert_eq!(code, 2, "{text}");
    assert!(!dir.join("bsc").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_candidate_sources_have_no_network_key_signing_or_send_path() {
    for source in [MAIN, CANDIDATE] {
        for forbidden in [
            "rpc_",
            "run_current_preflight",
            "sign_legacy",
            "SigningKey",
            "take_startup_keys",
            "_PRIVATE_KEY",
            "DARKBOT_G07_SALT",
            "TcpStream",
            "Command::new",
            "submit_",
            "latch_attempt",
            "FROZEN_APPROVED_V4_SHA256 =",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
    }
    // The salt value only reaches the private record writer.
    assert_eq!(CANDIDATE.matches("Value::B256(salt.0.0)").count(), 1);
}
