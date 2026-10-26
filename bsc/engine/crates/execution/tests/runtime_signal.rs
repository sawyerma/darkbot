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

#[test]
fn sigterm_is_a_clean_stop_with_a_committed_journal() {
    let base = std::env::temp_dir().join(format!("bsc-runtime-sigterm-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    let text: String = CONFIG
        .replace("STATUS_MS=1000", "STATUS_MS=100")
        .lines()
        .filter(|l| !l.starts_with("BSC_CANONICAL_"))
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
    // The handler wrote the runtime's own stop request.
    assert!(stop_file(&base).exists());
    let c = census(
        &base.join("bsc-runtime/journal"),
        journal_max_payload(&config),
    )
    .unwrap();
    assert_eq!(c.windows.len(), 1);
    assert_eq!(c.windows[0].state, "COMPLETE");
    assert!(base.join("bsc-runtime/g04-census.dbrec").exists());
    fs::remove_dir_all(&base).unwrap();
}
