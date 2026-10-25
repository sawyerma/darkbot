#[path = "support/requalification.rs"]
mod requalification_fixture;
use std::path::{Path, PathBuf};
use std::process::Command;

use darkbot_bsc_execution::acceptance_contract::{
    Authorization, WITNESS_SOURCE_RECORD, attempt1_v5, attempt1_v5_evidence,
};
use darkbot_bsc_execution::acceptance_guard::{V3_RELATIVE_PATH, V5_RELATIVE_PATH};
use darkbot_bsc_record::schema::{g06a, g07::contract as c};
use darkbot_bsc_record::{Draft, Value, decode};

const BIN: &str = env!("CARGO_BIN_EXE_g07_pre_live_readiness");
const FROZEN: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016";
const REQUAL: &str = "bsc/native/evidence/g06a/requal_test";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn run(root: &Path, args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(BIN)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/no/commands/available")
        .env("BSC_MAINNET_RPC_HTTPS", "https://127.0.0.1:1")
        .env("DARKBOT_G07_MAINNET_SEND", "YES")
        .env("DARKBOT_G07_PROBE_SEND", "YES")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

/// A temporary repository root with the committed inputs, their legacy
/// sources, stand-in requalification records (with the real launch-path
/// snapshot and a candidate record) and a v5 proposal binding them; `edit`
/// may alter the contract before it is written.
fn v5_root(tag: &str, edit: impl FnOnce(&mut Draft)) -> PathBuf {
    let repo = repo_root();
    let root = std::env::temp_dir().join(format!("darkbot-readiness-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let policy = format!("{FROZEN}/builder_policy.dbrec");
    for path in [
        WITNESS_SOURCE_RECORD,
        V3_RELATIVE_PATH,
        policy.as_str(),
        "bsc/config/g07_producer_witness_source_attempt1.json",
        "bsc/config/g06a_builder_floor_addendum_2026-10-11.json",
    ] {
        let to = root.join(path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(repo.join(path), to).unwrap();
    }
    requalification_fixture::stand_in_records(&root, REQUAL, &repo);
    requalification_fixture::fixture(&root, REQUAL, &policy, &repo);
    requalification_fixture::upgrade_to_v5(&root, REQUAL, &policy, &repo);
    let evidence = attempt1_v5_evidence(
        &root,
        REQUAL,
        &policy,
        requalification_fixture::CANDIDATE_RECORD,
    )
    .unwrap();
    let proposal = attempt1_v5(&evidence, Authorization::Proposed).unwrap();
    let mut draft = Draft::new(&c::V5);
    for (field, value) in decode(&c::V5, &proposal).unwrap().fields() {
        draft.push(*field, value.clone());
    }
    edit(&mut draft);
    std::fs::write(root.join(V5_RELATIVE_PATH), draft.encode().unwrap()).unwrap();
    root
}

#[test]
fn without_a_v5_contract_readiness_is_blocked_and_never_invokes_rpc() {
    // Never depend on an operator's untracked proposal in the real checkout;
    // a v4 alone (superseded, no launch-path binding) is no contract either.
    let root = std::env::temp_dir().join(format!("darkbot-readiness-no-v5-{}", std::process::id()));
    std::fs::create_dir_all(root.join("bsc/config")).unwrap();
    std::fs::write(
        root.join("bsc/config/g07_attempt1_acceptance_v4.dbrec"),
        b"offline-test",
    )
    .unwrap();
    let (code, text) = run(&root, &[]);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(code, Some(2));
    for expected in [
        "OFFLINE_ZERO_EGRESS",
        "V5_CONTRACT",
        "requalification pending",
        "LIVE=BLOCKED",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(!text.contains("PRE_LIVE_READINESS=PASS"));
}

#[test]
fn offline_readiness_evaluates_the_v5_bindings_without_rpc() {
    let root = v5_root("offline", |_| {});
    let (code, text) = run(&root, &[]);
    assert_eq!(code, Some(2));
    for expected in [
        "OFFLINE_ZERO_EGRESS",
        "V5_GUARD",
        "V5_INPUT_BINDINGS",
        "LAUNCH_PATH_BINDING",
        "22 code entries and 4 storage selectors",
        "DRY1_DRY4",
        "BUILDER_FRESHNESS",
        "CURRENT_DEPLOYMENTS_NONCES_FUNDING_GAS",
        "CURRENT_LAUNCH_PATH",
        "CLOCK_HEAD",
        "SIGNED_EGRESS",
        "LIVE=BLOCKED",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(!text.contains("VALIDATION"), "{text}");
    assert!(!text.contains("PRE_LIVE_READINESS=PASS"));
    std::fs::remove_dir_all(root).unwrap();
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
    let out = Command::new(BIN)
        .current_dir(repo_root())
        .env_clear()
        .arg("--binary")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    use darkbot_bsc_record::schema::g07::readiness as r;
    let report = decode(&r::SCHEMA, &out.stdout).unwrap();
    assert!(!report.bool(r::LIVE_AUTHORIZED).unwrap());
    assert_eq!(report.text(r::MODE).unwrap(), "OFFLINE_ZERO_EGRESS");
}

#[test]
fn an_active_or_widened_v5_and_wrong_input_hashes_fail_closed() {
    for (tag, value) in [
        (c::APPROVED, Value::Bool(true)),
        (c::MAX_ATTEMPTS, Value::U64(2)),
        (c::DRY_RECORD_SHA256, Value::B256([0; 32])),
        (c::LOCAL_SIMULATION_RECORD_SHA256, Value::B256([0; 32])),
        (c::DRY_LEGACY_SHA256, Value::B256([1; 32])),
        (c::LAUNCH_PATH_RECORD_SHA256, Value::B256([0; 32])),
    ] {
        let root = v5_root(&format!("negative-{tag}"), |d| {
            d.set(tag, value.clone());
        });
        let (code, text) = run(&root, &[]);
        assert_eq!(code, Some(2));
        assert!(text.contains("VALIDATION"), "{tag}: {text}");
        assert!(!text.contains("V5_INPUT_BINDINGS"), "{tag}: {text}");
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn a_rebound_but_stale_or_contradictory_launch_path_or_candidate_binding_fails_closed() {
    use g06a::launch_path as lp;
    let snapshot =
        std::fs::read(repo_root().join(requalification_fixture::LAUNCH_PATH_EVIDENCE)).unwrap();
    let record = decode(&lp::SCHEMA, &snapshot).unwrap();
    let edited = |tag: u16, value: Value| {
        let mut d = Draft::new(&lp::SCHEMA);
        for (t, v) in record.fields() {
            d.push(*t, v.clone());
        }
        d.set(tag, value);
        d.encode().unwrap()
    };
    let stale = edited(
        lp::BLOCK_NUMBER,
        Value::U64(record.u64(lp::BLOCK_NUMBER).unwrap() - 1),
    );
    let failed = edited(lp::LAUNCH_SUCCESS, Value::Bool(false));
    for (name, bytes, expected) in [
        ("stale", stale, "stale or foreign"),
        ("failed", failed, "did not succeed"),
    ] {
        // The contract re-binds the replaced snapshot: only the guard's own
        // check against the bound preflight can refuse it.
        let sha = darkbot_bsc_record::sha256(&bytes);
        let root = v5_root(&format!("launch-path-{name}"), |d| {
            d.set(c::LAUNCH_PATH_RECORD_SHA256, Value::B256(sha));
        });
        std::fs::write(root.join(REQUAL).join("launch_path.dbrec"), &bytes).unwrap();
        let (code, text) = run(&root, &[]);
        assert_eq!(code, Some(2));
        assert!(
            text.contains("V5_INPUT_BINDINGS") && text.contains(expected),
            "{name}: {text}"
        );
        assert!(!text.contains("LAUNCH_PATH_BINDING"), "{name}: {text}");
        std::fs::remove_dir_all(root).unwrap();
    }
    let root = v5_root("candidate-profile", |d| {
        d.set(c::CANDIDATE_PROFILE, "SELF_C");
    });
    let (code, text) = run(&root, &[]);
    assert_eq!(code, Some(2));
    assert!(
        text.contains("VALIDATION") && text.contains("candidate"),
        "{text}"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// The status of the readiness row `name` in a rendered record.
fn row(text: &str, name: &str) -> Option<String> {
    let index = text.lines().find_map(|l| {
        let (key, value) = l.split_once(" = ")?;
        (key.ends_with("].name") && value.trim_matches('"') == name)
            .then(|| key.trim_end_matches(".name").to_string())
    })?;
    text.lines().find_map(|l| {
        l.strip_prefix(&format!("{index}.status = "))
            .map(|v| v.trim_matches('"').to_string())
    })
}

fn run_with(root: &Path, args: &[&str], env: &[(&str, &Path)]) -> (Option<i32>, String) {
    let mut cmd = Command::new(BIN);
    cmd.current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("BSC_MAINNET_RPC_HTTPS", "https://127.0.0.1:1")
        .args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
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

/// BSC-005 / BSC-050 A-scan: a GUI snapshot (no Git checkout of its own)
/// reads the attempt latch and the shared attempt authority of the main
/// checkout it was synced from; before approval no authority may exist.
#[test]
fn a_gui_snapshot_reads_the_latch_and_the_shared_authority_of_its_checkout() {
    let root = v5_root("authority", |_| {});
    let attempt = darkbot_bsc_execution::acceptance_guard::parse_v5(
        &std::fs::read(root.join(V5_RELATIVE_PATH)).unwrap(),
    )
    .unwrap()
    .attempt_id;
    let (_, text) = run_with(&root, &[], &[]);
    assert_eq!(
        row(&text, "ATTEMPT_LATCH").as_deref(),
        Some("BLOCKED"),
        "{text}"
    );
    assert_eq!(
        row(&text, "ATTEMPT_AUTHORITY").as_deref(),
        Some("BLOCKED"),
        "{text}"
    );
    // The main checkout: Git, an origin, ignored bsc/.local.
    let base = std::env::temp_dir().join(format!("darkbot-readiness-co-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let co = base.join("checkout");
    std::fs::create_dir_all(co.join("bsc/engine")).unwrap();
    git(&base, &["init", "-q", "--bare", "origin.git"]);
    git(&co, &["init", "-q"]);
    git(
        &co,
        &[
            "remote",
            "add",
            "origin",
            base.join("origin.git").to_str().unwrap(),
        ],
    );
    std::fs::write(co.join("bsc/engine/Cargo.toml"), "").unwrap();
    std::fs::write(co.join("bsc/.gitignore"), "/.local/\n").unwrap();
    let env = [
        ("DARKBOT_BSC_CHECKOUT", co.as_path()),
        ("HOME", base.as_path()),
    ];
    let (_, text) = run_with(&root, &["--current-rpc"], &env);
    assert_eq!(
        row(&text, "ATTEMPT_LATCH").as_deref(),
        Some("PASS"),
        "{text}"
    );
    assert_eq!(
        row(&text, "ATTEMPT_AUTHORITY").as_deref(),
        Some("PASS"),
        "{text}"
    );
    // Any authority of this attempt on origin before approval: STOP.
    git(
        &co,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.invalid",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ],
    );
    git(
        &co,
        &[
            "push",
            "-q",
            "origin",
            &format!("HEAD:refs/darkbot/g07-attempt-authority/{attempt}"),
        ],
    );
    let (code, text) = run_with(&root, &["--current-rpc"], &env);
    assert_eq!(
        row(&text, "ATTEMPT_AUTHORITY").as_deref(),
        Some("STOP"),
        "{text}"
    );
    assert_eq!(code, Some(2));
    assert!(text.contains("PRE_LIVE_READINESS=STOP"), "{text}");
    // A latch in the checkout: BLOCKED.
    std::fs::create_dir_all(co.join("bsc/.local/g07").join(&attempt)).unwrap();
    let (_, text) = run_with(&root, &[], &env);
    assert_eq!(
        row(&text, "ATTEMPT_LATCH").as_deref(),
        Some("BLOCKED"),
        "{text}"
    );
    std::fs::remove_dir_all(base).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

/// BSC-050 A-scan: the snapshot sync copies exactly what v5 binds; the
/// readiness binary itself names those files (records and hashed legacy
/// sources), nothing else.
#[test]
fn bound_inputs_are_exactly_the_files_the_v5_contract_binds() {
    let root = v5_root("bound", |_| {});
    let (code, text) = run_with(&root, &["--bound-inputs"], &[]);
    assert_eq!(code, Some(0), "{text}");
    let mut listed: Vec<&str> = text.lines().collect();
    listed.sort();
    let contract = decode(&c::V5, &std::fs::read(root.join(V5_RELATIVE_PATH)).unwrap()).unwrap();
    let mut expected: Vec<String> = [
        c::DRY_RECORD_PATH,
        c::PREFLIGHT_RECORD_PATH,
        c::BUILDER_POLICY_RECORD_PATH,
        c::WITNESS_SOURCE_RECORD_PATH,
        c::LOCAL_SIMULATION_RECORD_PATH,
        c::ECONOMICS_RECORD_PATH,
        c::PORTAL_UPGRADE_RECORD_PATH,
        c::LAUNCH_PATH_RECORD_PATH,
    ]
    .iter()
    .map(|t| contract.text(*t).unwrap().to_string())
    .chain([
        "bsc/config/g06a_builder_floor_addendum_2026-10-11.json".to_string(),
        "bsc/config/g07_producer_witness_source_attempt1.json".to_string(),
    ])
    .collect();
    expected.sort();
    assert_eq!(listed, expected);
    assert!(listed.iter().all(|p| root.join(p).is_file()));
    std::fs::remove_dir_all(root).unwrap();
}

/// Review 25.10. #7: a snapshot without a configured checkout never reads
/// the shared authority of whatever Git work tree it happens to sit in.
#[test]
fn a_snapshot_without_its_checkout_never_reads_a_foreign_work_tree() {
    let snapshot = v5_root("foreign", |_| {});
    let base = std::env::temp_dir().join(format!("darkbot-readiness-wt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let parent = base.join("other-work-tree");
    std::fs::create_dir_all(&parent).unwrap();
    git(&base, &["init", "-q", "--bare", "origin.git"]);
    git(&parent, &["init", "-q"]);
    git(
        &parent,
        &[
            "remote",
            "add",
            "origin",
            base.join("origin.git").to_str().unwrap(),
        ],
    );
    let inside = parent.join("snapshot");
    let copied = Command::new("cp")
        .args(["-R"])
        .arg(&snapshot)
        .arg(&inside)
        .status()
        .unwrap();
    assert!(copied.success());
    std::fs::remove_dir_all(&snapshot).unwrap();
    let (_, text) = run_with(&inside, &["--current-rpc"], &[("HOME", base.as_path())]);
    assert_eq!(
        row(&text, "ATTEMPT_AUTHORITY").as_deref(),
        Some("BLOCKED"),
        "{text}"
    );
    std::fs::remove_dir_all(base).unwrap();
}
