//! The real `bsc_runtime` process answers SIGTERM with its own stop path
//! (review Paket 3, MEDIUM-4): it exits cleanly within its configured stop
//! bound, commits its journal with the clean end record, and the census
//! reports the window COMPLETE. Offline: every source OFF, no canonical
//! endpoint, so nothing connects.

use darkbot_bsc_execution::g04_census::census;
use darkbot_bsc_execution::runtime::{RuntimeConfig, journal_max_payload, stop_file};
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};

const CONFIG: &str = include_str!("../../../../config/ingress.public.env");
const CRITERIA: &str = include_str!("../../../../config/g04_criteria.env");

#[test]
fn sigterm_is_a_clean_stop_with_a_committed_journal() {
    let base = std::env::temp_dir().join(format!("bsc-runtime-sigterm-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let text: String = CONFIG
        .replace("STATUS_MS=1000", "STATUS_MS=100")
        .lines()
        // No canonical follower, so neither the C2 owner nor the C3 path
        // that need it.
        .filter(|l| {
            !["BSC_CANONICAL_", "BSC_STATE_", "BSC_EXEC_"]
                .iter()
                .any(|p| l.starts_with(p))
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let config_path = base.join("ingress.env");
    fs::write(&config_path, &text).unwrap();
    let config = RuntimeConfig::parse(&text).unwrap();
    let bin = env!("CARGO_BIN_EXE_bsc_runtime");
    let sources = base.join("bsc-sources");
    let init = Command::new(bin)
        .args(["source", "init", "--state"])
        .arg(&sources)
        .output()
        .unwrap();
    assert!(init.status.success(), "{init:?}");
    // BSC3-033: confirmed criteria committed before the run; the run names
    // the pre-commitment in its journal (and the CLI never replaces it).
    let criteria = base.join("g04_criteria.env");
    fs::write(&criteria, CRITERIA.replace("=PROPOSED", "=CONFIRMED")).unwrap();
    let precommit = |by: &str| {
        Command::new(bin)
            .args(["g04-precommit", "--rundir"])
            .arg(&base)
            .arg("--criteria")
            .arg(&criteria)
            .args(["--decided-by", by])
            .output()
            .unwrap()
    };
    let first = precommit("operator");
    assert!(first.status.success(), "{first:?}");
    assert!(!precommit("someone else").status.success());
    let mut child = Command::new(bin)
        .arg("run")
        .arg("--sources")
        .arg(&sources)
        .arg("--rundir")
        .arg(&base)
        .arg("--config")
        .arg(&config_path)
        .spawn()
        .unwrap();
    let status = base.join("bsc-runtime/status.dbrec");
    let started = Instant::now();
    while !status.exists() {
        assert!(started.elapsed() < Duration::from_secs(10), "no status");
        std::thread::sleep(Duration::from_millis(20));
    }
    let kill = Command::new("/bin/kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    let bound = Duration::from_millis(config.stop_timeout_ms);
    let signalled = Instant::now();
    let exit = loop {
        if let Some(exit) = child.try_wait().unwrap() {
            break exit;
        }
        assert!(signalled.elapsed() < bound, "no stop within the stop bound");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(exit.success(), "{exit:?}");
    // The handler wrote the runtime's own stop request (its stop record
    // ends the window below), and the clean stop consumed it (Paket 8
    // review L7: a stop file left behind stopped the next start at once).
    assert!(!stop_file(&base).exists());
    let c = census(
        &base.join("bsc-runtime/journal"),
        journal_max_payload(&config),
        None,
    )
    .unwrap();
    assert_eq!(c.windows.len(), 1);
    assert_eq!(c.windows[0].state, "COMPLETE");
    let committed = darkbot_bsc_execution::g04_criteria::Precommit::read(&base)
        .unwrap()
        .unwrap();
    assert_eq!(committed.decided_by, "operator");
    assert_eq!(c.windows[0].g04_precommit_sha256, Some(committed.sha256));
    // The verdict names it (and rates nothing: the campaign is running).
    let verdict = Command::new(bin)
        .args(["g04-verdict", "--rundir"])
        .arg(&base)
        .arg("--config")
        .arg(&config_path)
        .arg("--criteria")
        .arg(&criteria)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&verdict.stdout);
    assert!(!verdict.status.success(), "{text}");
    assert!(text.contains("precommitted by operator"), "{text}");
    assert!(!text.contains("PRECOMMIT_NOT_BOUND_AT_START"), "{text}");
    assert!(base.join("bsc-runtime/g04-census.dbrec").exists());
    // Phase F §9: the run replays to the same counts as the process
    // reported (its last status), sealed by its manifest.
    let proof = Command::new(bin)
        .args(["replay-proof", "--rundir"])
        .arg(&base)
        .arg("--config")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(proof.status.success(), "{proof:?}");
    assert!(String::from_utf8_lossy(&proof.stdout).contains(": PASS"));
    // The capacity preflight measures from sealed runs only: this run's
    // records span no time, so a capture stays UNMEASURED and is refused.
    let pre = Command::new(bin)
        .args(["capture-preflight", "--rundir"])
        .arg(&base)
        .arg("--config")
        .arg(&config_path)
        .output()
        .unwrap();
    assert_eq!(pre.status.code(), Some(2), "{pre:?}");
    assert!(String::from_utf8_lossy(&pre.stdout).contains("BLOCKED by UNMEASURED"));
    assert!(base.join("bsc-runtime/capture-preflight.dbrec").exists());
    // The Phase F quality verdict of the same run: its records span no
    // time, so it observed nothing and is no clean capture (Paket 8
    // review L7: before, CLEAN); no native source, so no acceptance.
    let quality = Command::new(bin)
        .args(["capture-quality", "--rundir"])
        .arg(&base)
        .arg("--config")
        .arg(&config_path)
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&quality.stdout);
    assert!(!quality.status.success(), "{quality:?}");
    assert!(out.contains(": NOT_CLEAN (Phase F acceptance NOT_MET"), "{out}");
    assert!(out.contains("empty capture window (0 s)"), "{out}");
    fs::remove_dir_all(&base).unwrap();
}
