//! Process-level regression of the D1 salt handoff (plan v9.1): a synthetic
//! SELF_B candidate (OS-random salt, created and selected by the real
//! `g07_candidate`, never used for anything else) and a recording stand-in
//! for `node` on PATH. Only the allowlisted consumer receives the salt, only
//! signing consumers keep the (synthetic) keys, nothing prints it, and every
//! wrong, stale or foreign input STOPs without running the consumer.

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
    let source = include_str!("../src/bin/g06a_dry_handoff.rs").replace("eprintln!", "");
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
