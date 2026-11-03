//! The two `bsc/tools` tests that need the status reporter, in the cargo
//! regression with their fixed test counts (the other six run in
//! execution's `bsc_tools`). Both get exactly this build's reporter; the
//! readiness binary is execution's: its `bsc_tools` checks in the same
//! workspace run that this path is that build's `CARGO_BIN_EXE_*` (cargo
//! test writes no dep-info to judge it by here); this test needs it present.

#[path = "../../execution/tests/support/bsc_tools.rs"]
mod bsc_tools;

use bsc_tools::{repo, run, same_binary};

const REPORTER: &str = env!("CARGO_BIN_EXE_bsc-status-reporter");

#[test]
fn test_readiness_job_py() {
    same_binary("bsc-status-reporter", REPORTER);
    let readiness = repo().join("bsc/engine/target/debug/g07_pre_live_readiness");
    assert!(
        readiness.is_file(),
        "{} missing: build execution with g07-mainnet (the workspace regression does)",
        readiness.display()
    );
    run(
        "test_readiness_job.py",
        5,
        &[("DARKBOT_TEST_REPORTER", REPORTER)],
    );
}

#[test]
fn test_status_sync_py() {
    same_binary("bsc-status-reporter", REPORTER);
    run(
        "test_status_sync.py",
        5,
        &[("DARKBOT_TEST_REPORTER", REPORTER)],
    );
}

/// Audit E 3.5: the reporter takes its socket and the readiness label only from the plist its
/// controller (`ops/bsc-control.sh`) writes; without either it stops at once, binding nothing,
/// instead of serving at a second, built-in default.
#[test]
fn the_reporter_takes_its_socket_and_label_only_from_its_controller() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let dir = std::env::temp_dir().join(format!("bsc-reporter-env-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("run")).unwrap();
    let socket = dir.join("r.sock");
    for (missing, env) in [
        (
            "DARKBOT_BSC_READINESS_LABEL",
            vec![("DARKBOT_BSC_SOCKET", socket.as_os_str())],
        ),
        (
            "DARKBOT_BSC_SOCKET",
            vec![(
                "DARKBOT_BSC_READINESS_LABEL",
                "com.darkbot.test.label".as_ref(),
            )],
        ),
    ] {
        let mut child = Command::new(REPORTER)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("DARKBOT_BSC_ROOT", &dir)
            .env("DARKBOT_RUNDIR", dir.join("run"))
            .envs(env)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(10) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the reporter served without {missing}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let mut stderr = String::new();
        std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
        assert!(!status.success(), "{missing}");
        assert!(stderr.contains(&format!("{missing} missing")), "{stderr}");
        assert!(!socket.exists(), "{missing}: nothing bound");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
