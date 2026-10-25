//! Process-level regression of the D1 salt handoff (plan v9.1): a synthetic
//! SELF_B candidate (OS-random salt, created and selected by the real
//! `g07_candidate`, never used for anything else) and a recording stand-in
//! for `node` on PATH. Only the allowlisted consumer receives the salt, only
//! signing consumers keep the (synthetic) keys, nothing prints it, and every
//! wrong, stale or foreign input STOPs without running the consumer.

#[path = "support/requalification.rs"]
mod requalification_fixture;

use darkbot_bsc_record::schema::{g06a::preflight as pf, g07_candidate::secret};
use darkbot_bsc_record::{Draft, Value, decode};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CANDIDATE: &str = env!("CARGO_BIN_EXE_g07_candidate");
const HANDOFF: &str = env!("CARGO_BIN_EXE_g06a_dry_handoff");
const FROZEN: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016";
const TEST_KEY: &str = "0x00000000000000000000000000000000000000000000000000000000000000aa";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

struct Fixture {
    root: PathBuf,
    bin: PathBuf,
    id: String,
    generation: String,
    salt: String,
}

fn cmd(root: &Path, program: &str, args: &[&str], path: &str) -> Output {
    Command::new(program)
        .current_dir(root)
        .env_clear()
        .env("PATH", path)
        .env("DARKBOT_G06A_LAUNCHER_PRIVATE_KEY", TEST_KEY)
        .env("DARKBOT_G06A_SALT", "0xSTALE_INHERITED")
        .args(args)
        .output()
        .unwrap()
}

fn fixture(tag: &str) -> Fixture {
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("g06a-handoff-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in [".git", "bsc/engine", "bsc/tools", "inputs", "fakebin"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("bsc/.gitignore"), "/.local/\n").unwrap();
    std::fs::write(root.join("bsc/engine/Cargo.toml"), "").unwrap();
    for tool in [
        "g06a_launch_path_trace.cjs",
        "g06a_local_simulate.cjs",
        "g06a_economics_evm_crosscheck.cjs",
    ] {
        std::fs::write(root.join("bsc/tools").join(tool), "// stand-in").unwrap();
    }
    for name in ["preflight.dbrec", "dry_result.dbrec"] {
        std::fs::copy(
            repo().join(FROZEN).join(name),
            root.join("inputs").join(name),
        )
        .unwrap();
    }
    // The stand-in node records its argv and environment.
    let node = root.join("fakebin/node");
    std::fs::write(
        &node,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}/argv'\nenv > '{0}/env'\nexit 7\n",
            root.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = "/usr/bin:/bin";
    let create = cmd(
        &root,
        CANDIDATE,
        &[
            "create-b",
            "--dry",
            "inputs/dry_result.dbrec",
            "--preflight",
            "inputs/preflight.dbrec",
        ],
        path,
    );
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    let select = cmd(
        &root,
        CANDIDATE,
        &["select", "B", "--expect-generation", "0"],
        path,
    );
    assert!(
        select.status.success(),
        "{}",
        String::from_utf8_lossy(&select.stderr)
    );
    let shown = String::from_utf8_lossy(&select.stdout).to_string();
    let value = |key: &str| {
        shown
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{key} = ")))
            .unwrap()
            .to_string()
    };
    let salt = decode(
        &secret::SCHEMA,
        &std::fs::read(root.join("bsc/.local/g07_candidates/b.salt.dbrec")).unwrap(),
    )
    .unwrap()
    .b256(secret::SALT)
    .unwrap();
    Fixture {
        bin: root.join("fakebin"),
        id: value("selection.candidate_id"),
        generation: value("selection.generation"),
        salt: format!("0x{}", darkbot_bsc_record::hex(&salt)),
        root,
    }
}

impl Fixture {
    fn path(&self) -> String {
        format!("{}:/usr/bin:/bin", self.bin.display())
    }

    fn exec(&self, own: &[&str], consumer: &[&str]) -> Output {
        let _ = std::fs::remove_file(self.root.join("env"));
        let mut args = vec!["exec-salt"];
        args.extend_from_slice(own);
        args.push("--");
        args.extend_from_slice(consumer);
        cmd(&self.root, HANDOFF, &args, &self.path())
    }

    fn own<'a>(&'a self, preflight: &'a str) -> [&'a str; 6] {
        [
            "--candidate",
            &self.id,
            "--generation",
            &self.generation,
            "--preflight",
            preflight,
        ]
    }

    fn seen(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(name)).ok()
    }
}

const TRACE: [&str; 4] = [
    "node",
    "bsc/tools/g06a_launch_path_trace.cjs",
    "inputs/preflight.dbrec",
    "out",
];

#[test]
fn only_the_allowlisted_consumer_gets_the_salt_and_only_signers_keep_keys() {
    let f = fixture("allow");
    let printed = |o: &Output| {
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    };
    // A non-signing consumer: the salt, no key, no stale inherited salt.
    let out = f.exec(&f.own("inputs/preflight.dbrec"), &TRACE);
    assert_eq!(out.status.code(), Some(7), "{}", printed(&out));
    let env = f.seen("env").unwrap();
    assert!(env.contains(&format!("DARKBOT_G06A_SALT={}", f.salt)));
    assert!(
        !env.contains("PRIVATE_KEY") && !env.contains("STALE"),
        "{env}"
    );
    let argv = f.seen("argv").unwrap();
    assert!(!argv.contains(&f.salt[2..]), "salt on argv");
    assert!(!printed(&out).contains(&f.salt[2..]), "salt printed");
    // A signing consumer keeps the caller's (synthetic) key.
    let out = f.exec(
        &f.own("inputs/preflight.dbrec"),
        &["node", "bsc/tools/g06a_local_simulate.cjs", "out"],
    );
    assert_eq!(out.status.code(), Some(7));
    let env = f.seen("env").unwrap();
    assert!(env.contains(TEST_KEY) && env.contains(&f.salt));
    std::fs::remove_dir_all(&f.root).unwrap();
}

#[test]
fn a_wrong_stale_or_foreign_candidate_consumer_or_preflight_stops_before_the_consumer() {
    let f = fixture("refuse");
    // Another deployment: the same preflight with another Portal version.
    let bytes = std::fs::read(f.root.join("inputs/preflight.dbrec")).unwrap();
    let record = decode(&pf::SCHEMA, &bytes).unwrap();
    let mut d = Draft::new(&pf::SCHEMA);
    for (tag, value) in record.fields() {
        d.push(*tag, value.clone());
    }
    d.set(pf::PORTAL_VERSION, Value::Text("v9.9.9".into()));
    std::fs::write(f.root.join("inputs/other.dbrec"), d.encode().unwrap()).unwrap();
    let wrong_id = format!("0x{}", "11".repeat(32));
    let next = (f.generation.parse::<u64>().unwrap() + 1).to_string();
    let pre = "inputs/preflight.dbrec";
    let cases: Vec<(Vec<&str>, Vec<&str>, &str)> = vec![
        (
            vec![
                "--candidate",
                &wrong_id,
                "--generation",
                &f.generation,
                "--preflight",
                pre,
            ],
            TRACE.to_vec(),
            "stale",
        ),
        (
            vec![
                "--candidate",
                &f.id,
                "--generation",
                &next,
                "--preflight",
                pre,
            ],
            TRACE.to_vec(),
            "stale",
        ),
        (
            f.own("inputs/other.dbrec").to_vec(),
            TRACE.to_vec(),
            "deployment",
        ),
        (
            f.own(pre).to_vec(),
            vec!["node", "-e", "console.log(process.env)"],
            "allowlisted",
        ),
        (
            f.own(pre).to_vec(),
            vec![
                "node",
                "bsc/tools/g06a_economics_evm_crosscheck.cjs",
                pre,
                "out",
            ],
            "allowlisted",
        ),
        (f.own(pre).to_vec(), vec!["sh", "-c", "env"], "allowlisted"),
    ];
    for (own, consumer, expected) in cases {
        let out = f.exec(&own, &consumer);
        let error = String::from_utf8_lossy(&out.stderr).to_string();
        assert_eq!(out.status.code(), Some(2), "{own:?} {consumer:?}: {error}");
        assert!(error.contains(expected), "{expected}: {error}");
        assert!(
            f.seen("env").is_none(),
            "consumer ran for {own:?} {consumer:?}"
        );
        assert!(!error.contains(&f.salt[2..]));
    }
    // No selection of B any more (A selected): never a fallback to A's salt.
    let a = cmd(
        &f.root,
        CANDIDATE,
        &[
            "init-a",
            "--economics",
            &repo()
                .join(FROZEN)
                .join("economics_crosscheck.dbrec")
                .display()
                .to_string(),
            "--dry",
            "inputs/dry_result.dbrec",
            "--preflight",
            "inputs/preflight.dbrec",
        ],
        "/usr/bin:/bin",
    );
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    let switch = cmd(
        &f.root,
        CANDIDATE,
        &["select", "A", "--expect-generation", &f.generation],
        "/usr/bin:/bin",
    );
    assert!(
        switch.status.success(),
        "{}",
        String::from_utf8_lossy(&switch.stderr)
    );
    let out = f.exec(&f.own(pre), &TRACE);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not SELF_B"));
    assert!(f.seen("env").is_none());
    // Without any candidate state.
    std::fs::remove_dir_all(f.root.join("bsc/.local")).unwrap();
    let out = f.exec(&f.own(pre), &TRACE);
    assert_eq!(out.status.code(), Some(2));
    assert!(f.seen("env").is_none());
    std::fs::remove_dir_all(&f.root).unwrap();
}

#[test]
fn the_handoff_source_never_reads_keys_rpc_or_writes_the_salt() {
    let source = [
        include_str!("../src/bin/g06a_dry_handoff.rs"),
        include_str!("../src/bin/g06a_dry_handoff/archive.rs"),
    ]
    .concat()
    .replace("eprintln!", "");
    for forbidden in [
        "rpc(",
        "run_current_preflight",
        "sign_legacy",
        "println!",
        "print!(",
        "fs::write",
        "take_startup_keys",
    ] {
        assert!(!source.contains(forbidden), "{forbidden}");
    }
    assert_eq!(source.matches("Command::new(").count(), 1);
    assert_eq!(source.matches(".env_clear()").count(), 1);
}

/// A PREPARE run directory in the fixture: an H1 preflight (the frozen one
/// moved to block `to` with hash `last`) and a coherent R0 record from the
/// frozen block to it, `edit` applied to the R0 draft.
fn prepare_dir(f: &Fixture, name: &str, edit: impl FnOnce(&mut Draft)) -> PathBuf {
    use darkbot_bsc_record::schema::g06a::portal_upgrade as up;
    use darkbot_bsc_record::schema::{PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL};
    let dir = f.root.join("bsc/native/evidence/g06a").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let frozen = decode(
        &pf::SCHEMA,
        &std::fs::read(f.root.join("inputs/preflight.dbrec")).unwrap(),
    )
    .unwrap();
    let (to, last) = (128_000_000u64, [0x4c; 32]);
    let mut h1 = Draft::new(&pf::SCHEMA);
    for (tag, value) in frozen.fields() {
        h1.push(*tag, value.clone());
    }
    h1.set(pf::LATEST_BLOCK_NUMBER, to)
        .set(pf::LATEST_BLOCK_HASH, Value::B256(last));
    std::fs::write(dir.join("preflight.dbrec"), h1.encode().unwrap()).unwrap();
    let old = frozen.address(pf::PORTAL_IMPLEMENTATION).unwrap();
    let new = [0xe1; 20];
    let mut u = Draft::new(&up::upgrade::SCHEMA);
    u.set(up::upgrade::BLOCK, 127_900_000u64)
        .set(up::upgrade::BLOCK_HASH, Value::B256([3; 32]))
        .set(up::upgrade::TRANSACTION_HASH, Value::B256([4; 32]))
        .set(up::upgrade::LOG_INDEX, 1u64)
        .set(up::upgrade::IMPLEMENTATION, Value::Address(new))
        .set(up::upgrade::RECEIPT_STATUS, 1u64)
        .set(up::upgrade::SLOT_BEFORE, Value::Address(old))
        .set(up::upgrade::SLOT_AFTER, Value::Address(new));
    let from = frozen.u64(pf::LATEST_BLOCK_NUMBER).unwrap() + 1;
    let mut s = Draft::new(&up::SCHEMA);
    s.set(PROVENANCE_TOOL, "test")
        .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(up::PROXY, frozen.get(pf::PORTAL).unwrap().clone())
        .set(up::SCANNED_FROM_BLOCK, from)
        .set(up::SCANNED_TO_BLOCK, to)
        .push(up::UPGRADES, u.into_nested())
        .set(up::CURRENT_IMPLEMENTATION, Value::Address(new))
        .set(up::BASELINE_IMPLEMENTATION, Value::Address(old))
        .set(up::COVERAGE_METHOD, up::METHOD)
        .set(up::CHAIN_SOURCE, "test")
        .set(up::ARCHIVE_SOURCE, "test")
        .set(up::HEADERS_VERIFIED, to - from + 1)
        .set(up::BLOOM_CANDIDATE_BLOCKS, 1u64)
        .set(
            up::CURRENT_IMPLEMENTATION_CODE_SHA256,
            Value::B256([0x42; 32]),
        )
        .set(up::CURRENT_IMPLEMENTATION_CODE_BYTES, 1u64)
        .set(
            up::FIRST_PARENT_HASH,
            frozen.get(pf::LATEST_BLOCK_HASH).unwrap().clone(),
        )
        .set(up::LAST_BLOCK_HASH, Value::B256(last));
    edit(&mut s);
    std::fs::write(dir.join("portal_upgrade.dbrec"), s.encode().unwrap()).unwrap();
    dir
}

fn run_ok(f: &Fixture, args: &[&str]) -> Output {
    cmd(&f.root, HANDOFF, args, "/usr/bin:/bin")
}

const FROZEN_ARG: &str = "inputs";

#[test]
fn prepare_binds_only_a_coherent_r0_record_and_the_candidate_and_verify_hands_it_to_a_new_private_dir()
 {
    use darkbot_bsc_record::schema::g06a::{portal_upgrade as up, prepare_handoff as h};
    use std::os::unix::fs::MetadataExt;
    let f = fixture("d3");
    let run = prepare_dir(&f, "g06a_prepare_test", |_| {});
    let run_arg = run.display().to_string();
    let out = run_ok(
        &f,
        &[
            "prepare",
            "--run",
            &run_arg,
            "--profile",
            "SELF_B",
            "--frozen",
            FROZEN_ARG,
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let handoff = decode(
        &h::SCHEMA,
        &std::fs::read(run.join("prepare_handoff.dbrec")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        handoff.text(h::MODE).unwrap(),
        "PREPARE_ONLY / NOT_REQUALIFIED"
    );
    assert_eq!(handoff.text(h::CANDIDATE_PROFILE).unwrap(), "SELF_B");
    // Nothing of the salt in the handoff.
    let raw = std::fs::read(run.join("prepare_handoff.dbrec")).unwrap();
    assert!(!darkbot_bsc_record::hex(&raw).contains(&f.salt[2..]));
    let before = std::fs::read(run.join("portal_upgrade.dbrec")).unwrap();
    let sign_root = f.root.join("bsc/.local/g06a_sign");
    std::fs::create_dir_all(&sign_root).unwrap();
    let sign = sign_root.join("run1");
    let sign_arg = sign.display().to_string();
    let verify = |out: &str| {
        run_ok(
            &f,
            &[
                "verify",
                "--prepare",
                &run_arg,
                "--frozen",
                FROZEN_ARG,
                "--out",
                out,
            ],
        )
    };
    let ok = verify(&sign_arg);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let printed = String::from_utf8_lossy(&ok.stdout).to_string();
    assert!(
        printed.contains("profile=SELF_B")
            && printed.contains(&format!("generation={}", f.generation))
    );
    assert_eq!(
        std::fs::read(sign.join("portal_upgrade_scan.dbrec")).unwrap(),
        before
    );
    assert!(sign.join("candidate.dbrec").is_file());
    assert_eq!(std::fs::metadata(&sign).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        std::fs::read_dir(&sign).unwrap().count(),
        2,
        "only the R0 record and the candidate cross"
    );
    // Never into an existing directory; the PREPARE run is untouched.
    assert_eq!(verify(&sign_arg).status.code(), Some(2));
    assert_eq!(
        std::fs::read(run.join("portal_upgrade.dbrec")).unwrap(),
        before
    );

    // Manipulated, substituted, incomplete or missing R0 records STOP.
    let stop = |case: &str, expected: &str| {
        let target = sign_root.join(format!("case-{case}"));
        let out = verify(&target.display().to_string());
        let error = String::from_utf8_lossy(&out.stderr).to_string();
        assert_eq!(out.status.code(), Some(2), "{case}: {error}");
        assert!(error.contains(expected), "{case}: {error}");
        assert!(!target.exists(), "{case}: SIGN dir created");
    };
    let mut flipped = before.clone();
    flipped[40] ^= 1;
    std::fs::write(run.join("portal_upgrade.dbrec"), &flipped).unwrap();
    stop("flipped", "modified or substituted");
    std::fs::remove_file(run.join("portal_upgrade.dbrec")).unwrap();
    stop("missing", "portal_upgrade.dbrec");
    // A substitute re-bound in a rewritten handoff is still checked for
    // its meaning: wrong start, a gap in the headers, a broken chain.
    for (case, edit, expected) in [
        (
            "start",
            (up::SCANNED_FROM_BLOCK, Value::U64(127_865_762)),
            "right after the frozen head",
        ),
        ("gap", (up::HEADERS_VERIFIED, Value::U64(5)), "do not cover"),
        (
            "anchor",
            (up::FIRST_PARENT_HASH, Value::B256([9; 32])),
            "frozen DRY's block hash",
        ),
        (
            "chain",
            (up::CURRENT_IMPLEMENTATION, Value::Address([0xee; 20])),
            "does not end at the current",
        ),
    ] {
        let dir = prepare_dir(&f, "g06a_prepare_test", |d| {
            d.set(edit.0, edit.1.clone());
        });
        let bytes = std::fs::read(dir.join("portal_upgrade.dbrec")).unwrap();
        let record = decode(&h::SCHEMA, &raw).unwrap();
        let mut rebound = Draft::new(&h::SCHEMA);
        for (tag, value) in record.fields() {
            rebound.push(*tag, value.clone());
        }
        rebound.set(
            h::R0_RECORD_SHA256,
            Value::B256(darkbot_bsc_record::sha256(&bytes)),
        );
        std::fs::write(dir.join("prepare_handoff.dbrec"), rebound.encode().unwrap()).unwrap();
        stop(case, expected);
    }
    // A coherent R0 record, but the handoff states another chain end.
    prepare_dir(&f, "g06a_prepare_test", |_| {});
    let record = decode(&h::SCHEMA, &raw).unwrap();
    let mut other = Draft::new(&h::SCHEMA);
    for (tag, value) in record.fields() {
        other.push(*tag, value.clone());
    }
    other.set(h::R0_TO_BLOCK, 127_999_999u64);
    std::fs::write(run.join("prepare_handoff.dbrec"), other.encode().unwrap()).unwrap();
    stop("stated", "chain differs from the handoff");
    std::fs::write(run.join("prepare_handoff.dbrec"), &raw).unwrap();
    // The candidate changed after PREPARE: switch to A and back to B.
    let a = cmd(
        &f.root,
        CANDIDATE,
        &[
            "init-a",
            "--economics",
            &repo()
                .join(FROZEN)
                .join("economics_crosscheck.dbrec")
                .display()
                .to_string(),
            "--dry",
            "inputs/dry_result.dbrec",
            "--preflight",
            "inputs/preflight.dbrec",
        ],
        "/usr/bin:/bin",
    );
    assert!(a.status.success());
    let g = f.generation.parse::<u64>().unwrap();
    assert!(
        cmd(
            &f.root,
            CANDIDATE,
            &["select", "A", "--expect-generation", &g.to_string()],
            "/usr/bin:/bin"
        )
        .status
        .success()
    );
    stop("profile", "not SELF_B");
    assert!(
        cmd(
            &f.root,
            CANDIDATE,
            &["select", "B", "--expect-generation", &(g + 1).to_string()],
            "/usr/bin:/bin"
        )
        .status
        .success()
    );
    stop("generation", "stale");
    std::fs::remove_dir_all(&f.root).unwrap();
}

#[test]
fn prepare_refuses_an_r0_record_not_closed_at_its_own_preflight_or_incoherent() {
    use darkbot_bsc_record::schema::g06a::portal_upgrade as up;
    let f = fixture("d3-prepare");
    for (name, edit, expected) in [
        (
            "short",
            (up::SCANNED_TO_BLOCK, Value::U64(127_999_999)),
            "coherent",
        ),
        (
            "end",
            (up::LAST_BLOCK_HASH, Value::B256([8; 32])),
            "H1 preflight block",
        ),
        (
            "method",
            (up::COVERAGE_METHOD, Value::Text("eth_getLogs".into())),
            "coverage method",
        ),
    ] {
        let run = prepare_dir(&f, name, |d| {
            d.set(edit.0, edit.1.clone());
        });
        let out = run_ok(
            &f,
            &[
                "prepare",
                "--run",
                &run.display().to_string(),
                "--profile",
                "SELF_B",
                "--frozen",
                FROZEN_ARG,
            ],
        );
        let error = String::from_utf8_lossy(&out.stderr).to_string();
        assert_eq!(out.status.code(), Some(2), "{name}: {error}");
        assert!(error.contains(expected), "{name}: {error}");
        assert!(!run.join("prepare_handoff.dbrec").exists());
    }
    // Another profile than the selected one, or a foreign frozen start.
    let run = prepare_dir(&f, "profile", |_| {});
    let out = run_ok(
        &f,
        &[
            "prepare",
            "--run",
            &run.display().to_string(),
            "--profile",
            "SELF_A",
            "--frozen",
            FROZEN_ARG,
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    std::fs::write(
        f.root.join("inputs/preflight.dbrec"),
        b"not the frozen preflight",
    )
    .unwrap();
    let out = run_ok(
        &f,
        &[
            "prepare",
            "--run",
            &run.display().to_string(),
            "--profile",
            "SELF_B",
            "--frozen",
            FROZEN_ARG,
        ],
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("pinned"));
    std::fs::remove_dir_all(&f.root).unwrap();
}

// BSC-006 (BSC_Bug_V2 P3-F5): the private SIGN evidence a v5 contract binds
// survives the loss of its checkout only through a verified private archive,
// restored bit-identical at exactly its bound path.

const SIGN_RUN: &str = "g06a_sign_20261025T000000Z";

/// The run's manifest as the harness writes it (`shasum -a 256`).
fn sums(dir: &Path) {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n != "SHA256SUMS")
        .collect();
    names.sort();
    let text: String = names
        .iter()
        .map(|n| {
            let digest = darkbot_bsc_record::sha256(&std::fs::read(dir.join(n)).unwrap());
            format!("{}  ./{n}\n", darkbot_bsc_record::hex(&digest))
        })
        .collect();
    std::fs::write(dir.join("SHA256SUMS"), text).unwrap();
}

/// The harness's permissions (umask 077): the run 0700, its files 0600.
fn private(dir: &Path) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if !std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
        {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// A repository root with a SELF_A SIGN run of stand-in records laid out as
/// `--sign` writes it (builder policy and candidate inside the run), a view,
/// a comparison, its manifest and the v5 proposal binding the run.
fn sign_root(tag: &str) -> (PathBuf, String) {
    use darkbot_bsc_execution::acceptance_contract::{
        Authorization, WITNESS_SOURCE_RECORD, attempt1_v5, attempt1_v5_evidence,
    };
    use darkbot_bsc_execution::acceptance_guard::{V3_RELATIVE_PATH, V5_RELATIVE_PATH};
    let repo = repo();
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("g06a-archive-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in [".git", "bsc/engine"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("bsc/engine/Cargo.toml"), "").unwrap();
    std::fs::write(root.join("bsc/.gitignore"), "/.local/\n").unwrap();
    let dir = format!("bsc/.local/g06a_sign/{SIGN_RUN}");
    let policy = format!("{dir}/builder_policy.dbrec");
    requalification_fixture::stand_in_records(&root, &dir, &repo);
    for (from, to) in [
        (
            WITNESS_SOURCE_RECORD.to_string(),
            WITNESS_SOURCE_RECORD.to_string(),
        ),
        (V3_RELATIVE_PATH.to_string(), V3_RELATIVE_PATH.to_string()),
        (
            format!("{FROZEN}/builder_policy.dbrec"),
            format!("{FROZEN}/builder_policy.dbrec"),
        ),
        (format!("{FROZEN}/builder_policy.dbrec"), policy.clone()),
    ] {
        std::fs::create_dir_all(root.join(&to).parent().unwrap()).unwrap();
        std::fs::copy(repo.join(from), root.join(to)).unwrap();
    }
    requalification_fixture::fixture(&root, &dir, &policy, &repo);
    // A real quote record (the committed PREPARE run's) for the text stand-in.
    std::fs::copy(
        repo.join("bsc/native/evidence/g06a/g06a_prepare_20261023T211704Z/native_quote.dbrec"),
        root.join(&dir).join("native_quote.dbrec"),
    )
    .unwrap();
    requalification_fixture::refresh(&root, &dir, &policy);
    requalification_fixture::upgrade_to_v5(&root, &dir, &policy, &repo);
    let candidate = format!("{dir}/candidate.dbrec");
    std::fs::copy(
        root.join(requalification_fixture::CANDIDATE_RECORD),
        root.join(&candidate),
    )
    .unwrap();
    let evidence = attempt1_v5_evidence(&root, &dir, &policy, &candidate).unwrap();
    let v5 = attempt1_v5(&evidence, Authorization::Proposed).unwrap();
    std::fs::create_dir_all(root.join(V5_RELATIVE_PATH).parent().unwrap()).unwrap();
    std::fs::write(root.join(V5_RELATIVE_PATH), v5).unwrap();
    let run = root.join(&dir);
    std::fs::write(
        run.join("dry_result.view.txt"),
        "derived view of dry_result.dbrec\n",
    )
    .unwrap();
    std::fs::write(run.join("COMPARISON_dry_result.md"), "no differences\n").unwrap();
    sums(&run);
    private(&run);
    (root, dir)
}

fn line<'a>(stdout: &'a str, key: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("{key} missing in {stdout}"))
}

#[test]
fn a_sign_run_is_archived_privately_and_restored_bit_identical_only_at_its_bound_path() {
    let (root, dir) = sign_root("roundtrip");
    let run_dir = root.join(&dir);
    let backup = root.with_extension("backup");
    let _ = std::fs::remove_dir_all(&backup);
    std::fs::create_dir_all(&backup).unwrap();
    let run = |args: &[&str]| cmd(&root, HANDOFF, args, "/usr/bin:/bin");
    let stderr = |o: &Output| String::from_utf8_lossy(&o.stderr).to_string();

    // Never into the repository outside its ignored private area.
    std::fs::create_dir_all(root.join("bsc/native")).unwrap();
    let inside = root.join("bsc/native").display().to_string();
    let out = run(&["archive", "--sign", &dir, "--to", &inside]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("repository"), "{}", stderr(&out));

    let to = backup.display().to_string();
    let out = run(&["archive", "--sign", &dir, "--to", &to]);
    assert!(out.status.success(), "{}", stderr(&out));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let manifest = std::fs::read(run_dir.join("SHA256SUMS")).unwrap();
    let anchor = darkbot_bsc_record::hex(&darkbot_bsc_record::sha256(&manifest));
    assert_eq!(line(&stdout, "SUMS_SHA256"), anchor);
    let archived = backup.join(SIGN_RUN);
    assert_eq!(mode(&archived), 0o700);
    let names: Vec<_> = std::fs::read_dir(&run_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(std::fs::read_dir(&archived).unwrap().count(), names.len());
    for name in &names {
        assert_eq!(
            std::fs::read(archived.join(name)).unwrap(),
            std::fs::read(run_dir.join(name)).unwrap()
        );
        assert_eq!(mode(&archived.join(name)), 0o400, "{name:?}");
    }
    // An archive is never overwritten.
    assert_eq!(
        run(&["archive", "--sign", &dir, "--to", &to]).status.code(),
        Some(2)
    );

    // A fresh checkout without the run; only the exact manifest restores it.
    std::fs::remove_dir_all(root.join("bsc/.local")).unwrap();
    let from = archived.display().to_string();
    let out = run(&["restore", "--from", &from, "--expect", &"00".repeat(32)]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!run_dir.exists());
    let out = run(&["restore", "--from", &from, "--expect", &anchor]);
    assert!(out.status.success(), "{}", stderr(&out));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(line(&stdout, "RESTORED"), dir);
    assert!(
        line(&stdout, "V5_REPRODUCED").starts_with("PROPOSAL "),
        "{stdout}"
    );
    for private_dir in [
        root.join("bsc/.local"),
        root.join("bsc/.local/g06a_sign"),
        run_dir.clone(),
    ] {
        assert_eq!(mode(&private_dir), 0o700);
    }
    for name in &names {
        assert_eq!(
            std::fs::read(run_dir.join(name)).unwrap(),
            std::fs::read(archived.join(name)).unwrap()
        );
    }
    let out = run(&["restore", "--from", &from, "--expect", &anchor]);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(line(&stdout, "ALREADY_PRESENT_IDENTICAL"), dir);
    assert!(line(&stdout, "V5_REPRODUCED").starts_with("PROPOSAL "));

    // A consistent forgery (a bound record and the manifest rewritten
    // together) does not reproduce v5: nothing stays at the bound path.
    std::fs::remove_dir_all(&run_dir).unwrap();
    let forged = backup.join("forged").join(SIGN_RUN);
    std::fs::create_dir_all(&forged).unwrap();
    for name in &names {
        std::fs::copy(archived.join(name), forged.join(name)).unwrap();
    }
    private(&forged);
    let sim = std::fs::read(forged.join("local_simulation.dbrec")).unwrap();
    std::fs::write(
        forged.join("local_simulation.dbrec"),
        requalification_fixture::with(
            &sim,
            &darkbot_bsc_record::schema::g06a::local_simulation::NATIVE,
            &[("buy_execution_gas", Value::U64(1))],
        ),
    )
    .unwrap();
    sums(&forged);
    private(&forged);
    let forged_anchor = darkbot_bsc_record::hex(&darkbot_bsc_record::sha256(
        &std::fs::read(forged.join("SHA256SUMS")).unwrap(),
    ));
    let out = run(&[
        "restore",
        "--from",
        &forged.display().to_string(),
        "--expect",
        &forged_anchor,
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("v5"), "{}", stderr(&out));
    assert!(!run_dir.exists());
    assert!(
        std::fs::read_dir(run_dir.parent().unwrap())
            .unwrap()
            .any(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{SIGN_RUN}.unbound-"))
            })
    );

    // An archive that lost its access protection is refused.
    std::fs::set_permissions(&archived, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = run(&["restore", "--from", &from, "--expect", &anchor]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("private"), "{}", stderr(&out));
    assert!(!run_dir.exists());
}

#[test]
fn archive_refuses_unlisted_changed_linked_or_open_files_raw_bytes_wallet_keys_and_the_b_salt() {
    use darkbot_bsc_record::schema::g06a::local_simulation as ls;
    let f = fixture("archive");
    let dir = format!("bsc/.local/g06a_sign/{SIGN_RUN}");
    let run_dir = f.root.join(&dir);
    let backup = f.root.with_extension("backup");
    let salt_hex = f.salt.trim_start_matches("0x").to_ascii_lowercase();
    // A SIGN run of the selected SELF_B candidate: its public record, a
    // simulation and a text view; then one deviation.
    let archive = |edit: &dyn Fn(&Path)| -> Output {
        let _ = std::fs::remove_dir_all(&run_dir);
        let _ = std::fs::remove_dir_all(&backup);
        std::fs::create_dir_all(&run_dir).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::copy(
            f.root.join("bsc/.local/g07_candidates/b.candidate.dbrec"),
            run_dir.join("candidate.dbrec"),
        )
        .unwrap();
        std::fs::copy(
            repo().join(FROZEN).join("local_simulation.dbrec"),
            run_dir.join("local_simulation.dbrec"),
        )
        .unwrap();
        std::fs::write(
            run_dir.join("local_simulation.out.txt"),
            "LOCAL_SIMULATION=WRITTEN\n",
        )
        .unwrap();
        sums(&run_dir);
        private(&run_dir);
        edit(&run_dir);
        cmd(
            &f.root,
            HANDOFF,
            &[
                "archive",
                "--sign",
                &dir,
                "--to",
                &backup.display().to_string(),
            ],
            "/usr/bin:/bin",
        )
    };
    let ok = archive(&|_| {});
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let write_resummed = |d: &Path, name: &str, bytes: &[u8]| {
        std::fs::write(d.join(name), bytes).unwrap();
        sums(d);
        private(d);
    };
    let key1 = format!("0x{}1", "0".repeat(63));
    let cases: Vec<Case> = vec![
        (
            "not in SHA256SUMS",
            Box::new(|d| std::fs::write(d.join("extra.txt"), "x").unwrap()),
        ),
        (
            "SHA-256",
            Box::new(|d| std::fs::write(d.join("local_simulation.out.txt"), "changed").unwrap()),
        ),
        (
            "regular file",
            Box::new(|d| {
                std::fs::remove_file(d.join("local_simulation.out.txt")).unwrap();
                std::os::unix::fs::symlink("candidate.dbrec", d.join("local_simulation.out.txt"))
                    .unwrap();
            }),
        ),
        (
            "private",
            Box::new(|d| {
                std::fs::set_permissions(
                    d.join("candidate.dbrec"),
                    std::fs::Permissions::from_mode(0o644),
                )
                .unwrap()
            }),
        ),
        (
            "private",
            Box::new(|d| {
                std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o750)).unwrap()
            }),
        ),
        (
            "raw",
            Box::new(|d| {
                write_resummed(
                    d,
                    "local_simulation.out.txt",
                    format!("RAW=0x{}\n", "f8".repeat(150)).as_bytes(),
                )
            }),
        ),
        (
            "salt",
            Box::new(|d| {
                write_resummed(
                    d,
                    "local_simulation.out.txt",
                    format!("X={}\n", f.salt.to_ascii_uppercase().replace("0X", "0x")).as_bytes(),
                )
            }),
        ),
        (
            "salt",
            Box::new(|d| {
                let raw = darkbot_bsc_record::schema::g06a::preflight::SCHEMA
                    .name
                    .as_bytes()
                    .to_vec();
                let mut bytes = raw;
                bytes.extend_from_slice(&hex_bytes(&salt_hex));
                write_resummed(d, "local_simulation.out.txt", &bytes)
            }),
        ),
        (
            "salt record",
            Box::new(|d| {
                let salt =
                    std::fs::read(f.root.join("bsc/.local/g07_candidates/b.salt.dbrec")).unwrap();
                write_resummed(d, "kept.dbrec", &salt)
            }),
        ),
        (
            "private key",
            Box::new(|d| {
                let sim = std::fs::read(d.join("local_simulation.dbrec")).unwrap();
                let buyer = Value::Address(
                    hex_bytes("7e5f4552091a69125d5dfcb7b8c2659029395bdf")
                        .try_into()
                        .unwrap(),
                );
                std::fs::write(
                    d.join("local_simulation.dbrec"),
                    requalification_fixture::with(&sim, &ls::SCHEMA, &[("buyer", buyer)]),
                )
                .unwrap();
                write_resummed(
                    d,
                    "local_simulation.out.txt",
                    format!("K={key1}\n").as_bytes(),
                )
            }),
        ),
    ];
    for (needle, edit) in &cases {
        let out = archive(edit.as_ref());
        let err = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
        assert_eq!(out.status.code(), Some(2), "{needle}: {err}");
        assert!(
            err.contains(&needle.to_ascii_lowercase()),
            "{needle}: {err}"
        );
        assert!(!err.contains(&salt_hex), "the salt is never printed");
        assert!(!backup.join(SIGN_RUN).exists(), "{needle}");
    }
}

/// A refusal case: the expected error text and the deviation it applies.
type Case<'a> = (&'a str, Box<dyn Fn(&Path) + 'a>);

fn hex_bytes(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
