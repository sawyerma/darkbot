//! One `bsc/tools` test file (Python unittest or the Node test runner) in the
//! cargo regression: exit code 0, exactly the expected number of tests, every
//! one passed, none failed, errored, skipped, left to do or cancelled. A
//! missing interpreter, npm dependency or binary fails; nothing is skipped.
//! Shared by the `bsc_tools` tests of execution and qualification.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// The repository root (every crate sits at `bsc/engine/crates/<crate>`).
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .expect("repository root")
}

/// The debug binary the tools call by their fixed path is `built`, this
/// build's own `CARGO_BIN_EXE_*` (also missing ⇒ failure).
pub fn same_binary(name: &str, built: &str) {
    let used = repo().join("bsc/engine/target/debug").join(name);
    let (a, b) = (used.canonicalize(), Path::new(built).canonicalize());
    assert!(
        matches!((&a, &b), (Ok(a), Ok(b)) if a == b),
        "{name}: the tools call {} but this build is {built} ({a:?} vs {b:?})",
        used.display()
    );
}

/// A binary of another package (no `CARGO_BIN_EXE_*` here) is current in
/// cargo's own sense: no input in its dep-info (`target/debug/<name>.d`,
/// every source and included file it was built from) is newer than it, so
/// cargo would not rebuild it (qualification's use).
#[allow(dead_code)]
pub fn current(name: &str) {
    fn mtime(path: &Path) -> Option<SystemTime> {
        fs::metadata(path).and_then(|m| m.modified()).ok()
    }
    let debug = repo().join("bsc/engine/target/debug");
    let built = mtime(&debug.join(name)).unwrap_or_else(|| {
        panic!("{name} is not built: cargo build the package that owns it first")
    });
    let info = fs::read_to_string(debug.join(format!("{name}.d")))
        .unwrap_or_else(|e| panic!("{name}.d (cargo's dep-info): {e}"));
    let (_, inputs) = info
        .split_once(": ")
        .unwrap_or_else(|| panic!("{name}.d is not a dep-info rule"));
    let inputs = inputs.replace("\\\n", " ").replace("\\ ", "\u{0}");
    let mut seen = 0;
    for input in inputs.split_whitespace() {
        let input = PathBuf::from(input.replace('\u{0}', " "));
        let changed = mtime(&input).unwrap_or_else(|| panic!("{} unreadable", input.display()));
        assert!(
            changed <= built,
            "{name} is older than its input {}: rebuild it",
            input.display()
        );
        seen += 1;
    }
    assert!(seen > 0, "{name}.d names no input");
}

/// Every npm dependency a tool directory declares is installed (`npm ci`).
fn npm_dependencies() {
    for dir in ["abi-tools", "evm-audit-tools"] {
        let tools = repo().join("bsc/tools").join(dir);
        let manifest = fs::read_to_string(tools.join("package.json"))
            .unwrap_or_else(|e| panic!("bsc/tools/{dir}/package.json: {e}"));
        let deps = manifest
            .split("\"dependencies\"")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .unwrap_or_else(|| panic!("bsc/tools/{dir}/package.json declares no dependencies"));
        let modules = tools.join("node_modules");
        assert!(
            modules.join(".package-lock.json").is_file(),
            "bsc/tools/{dir}: npm dependencies not installed (npm ci there)"
        );
        for name in deps.split('"').skip(1).step_by(4) {
            assert!(
                modules.join(name).join("package.json").is_file(),
                "bsc/tools/{dir}: {name} not installed (npm ci there)"
            );
        }
    }
}

pub struct Outcome {
    pub tests: u32,
    pub pass: u32,
    pub fail: u32,
    pub error: u32,
    pub skip: u32,
}

/// Runs `bsc/tools/<file>` from the repository root with `env` set (every
/// other `DARKBOT_TEST_*` removed) and checks its outcome.
pub fn run(file: &str, expected: u32, env: &[(&str, &str)]) {
    let root = repo();
    let path = root.join("bsc/tools").join(file);
    assert!(path.is_file(), "bsc/tools/{file} missing");
    let node = file.ends_with(".cjs");
    let mut cmd = if node {
        npm_dependencies();
        let mut c = Command::new("node");
        c.args(["--test", "--test-reporter=tap"]).arg(&path);
        c
    } else {
        let mut c = Command::new("python3");
        c.arg(&path);
        c
    };
    cmd.current_dir(&root);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("DARKBOT_TEST_") {
            cmd.env_remove(key);
        }
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    let interpreter = if node { "node" } else { "python3" };
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("{file}: {interpreter} is not runnable: {e}"));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let o = if node { tap(&text) } else { unittest(&text) }
        .unwrap_or_else(|| panic!("{file}: no test summary\n{text}"));
    println!(
        "bsc_tools {file}: exit {:?}, tests {}, pass {}, fail {}, error {}, skip {}",
        out.status.code(),
        o.tests,
        o.pass,
        o.fail,
        o.error,
        o.skip
    );
    assert!(
        out.status.success()
            && o.tests == expected
            && o.pass == expected
            && o.fail == 0
            && o.error == 0
            && o.skip == 0,
        "{file}: expected {expected} passing tests with exit 0 and nothing failed, errored or skipped\n{text}"
    );
}

/// The Node test runner's TAP summary (`# tests N` … at column 0).
pub fn tap(text: &str) -> Option<Outcome> {
    let n = |key: &str| {
        let prefix = format!("# {key} ");
        text.lines()
            .filter_map(|l| l.strip_prefix(prefix.as_str())?.trim().parse::<u32>().ok())
            .next_back()
    };
    Some(Outcome {
        tests: n("tests")?,
        pass: n("pass")?,
        fail: n("fail")?,
        error: n("cancelled")?,
        skip: n("skipped")? + n("todo")?,
    })
}

/// unittest's `Ran N tests` and its verdict `OK` / `FAILED (failures=…, …)`.
pub fn unittest(text: &str) -> Option<Outcome> {
    let tests = text.lines().rev().find_map(|l| {
        l.strip_prefix("Ran ")?
            .split_whitespace()
            .next()?
            .parse::<u32>()
            .ok()
    })?;
    let verdict = text
        .lines()
        .rev()
        .find(|l| *l == "OK" || l.starts_with("OK (") || l.starts_with("FAILED"))?;
    let mut counts = std::collections::HashMap::new();
    if let Some(list) = verdict
        .split_once('(')
        .map(|(_, r)| r.trim_end_matches(')'))
    {
        for item in list.split(", ") {
            let (key, value) = item.split_once('=')?;
            counts.insert(key.to_string(), value.parse::<u32>().ok()?);
        }
    }
    let count = |key: &str| counts.get(key).copied().unwrap_or(0);
    let fail = count("failures") + count("unexpected successes");
    let error = count("errors");
    let skip = count("skipped") + count("expected failures");
    Some(Outcome {
        tests,
        pass: tests.saturating_sub(fail + error + skip),
        fail,
        error,
        skip,
    })
}
