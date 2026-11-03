//! The `bsc/tools` test that needs the status reporter, in the cargo
//! regression with its fixed test count (the other six run in execution's
//! `bsc_tools`); it gets exactly this build's reporter. (Audit E 4.13: the
//! readiness job is the reporter's own now, `tests/readiness_job.rs`.)

#[path = "../../execution/tests/support/bsc_tools.rs"]
mod bsc_tools;

use bsc_tools::{run, same_binary};

const REPORTER: &str = env!("CARGO_BIN_EXE_bsc-status-reporter");

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
