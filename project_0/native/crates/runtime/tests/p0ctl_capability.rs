//! DEC-13: `p0ctl capability` over the real binary. This test process stands in for the runtime:
//! it holds the owner lock, has the doorbell installed and runs the control thread with the
//! capability log, so `p0ctl capability set` writes the typed request (CAS-versioned), finds the
//! lock holder and rings it, and the owner queue receives exactly that request. Offline only.
use darkbot_common::channel::bounded;
use darkbot_common::lifecycle::Shutdown;
use darkbot_runner::orchestration::control;
use darkbot_runner::orchestration::{capability_control, source_control};
use source_control::ControlMessage;
use std::process::Command;
use std::time::{Duration, Instant};

fn p0ctl(project: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_p0ctl"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .arg("--project")
        .arg(project)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn dec13_p0ctl_capability_set_reaches_the_runtime_owner_queue() {
    let d = std::env::temp_dir().join(format!("p0ctl_capability_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("data")).unwrap();
    let dir = capability_control::request_dir(&d);
    // No runtime: refused, nothing recorded; an unknown capability or value never.
    let (code, _, err) = p0ctl(&d, &["capability", "set", "LIVE", "LIVE"]);
    assert_eq!(code, 2);
    assert!(err.contains("CAPABILITY_NO_RUNTIME"), "{err}");
    assert_eq!(capability_control::latest_request(&dir).unwrap(), None);
    for (capability, value, code_text) in [
        ("JITO", "LIVE", "CAPABILITY_UNKNOWN"),
        ("LIVE", "ON", "CAPABILITY_VALUE_INVALID"),
    ] {
        let (code, _, err) = p0ctl(&d, &["capability", "set", capability, value]);
        assert_eq!(code, 2);
        assert!(err.contains(code_text), "{err}");
    }

    // The stand-in runtime: lock, doorbell, control thread with both logs.
    let _lock =
        control::acquire_owner(&control::default_lock_path(&d), "cap_test", "shadow").unwrap();
    let (tx, rx) = bounded::<ControlMessage>("source-control", 4);
    let shutdown = Shutdown::new();
    let sources = source_control::request_dir(&d);
    let thread = source_control::spawn(
        sources.clone(),
        tx,
        shutdown.clone(),
        source_control::head(&sources).unwrap(),
        Some((dir.clone(), capability_control::head(&dir).unwrap())),
    )
    .unwrap();
    let next = || {
        let t = Instant::now();
        loop {
            if let Some(m) = rx.try_pop() {
                return m;
            }
            assert!(t.elapsed() < Duration::from_secs(5), "no control message");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let (code, out, err) = p0ctl(
        &d,
        &["capability", "set", "LIVE", "LIVE", "--expect-version", "0"],
    );
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("seq=1") && out.contains("BLOCKED until FE-06"),
        "{out}"
    );
    match next() {
        ControlMessage::Capability { seq: 1, request } => {
            let r = request.unwrap();
            assert_eq!(
                (r.capability.as_str(), r.requested.as_str()),
                ("LIVE", "LIVE")
            );
        }
        other => panic!("{other:?}"),
    }
    // CTRL-03 CAS: a stale version is refused and writes nothing.
    let (code, _, err) = p0ctl(
        &d,
        &["capability", "set", "LIVE", "OFF", "--expect-version", "0"],
    );
    assert_eq!(code, 2);
    assert!(err.contains("CAPABILITY_VERSION_CONFLICT"), "{err}");
    assert_eq!(capability_control::head(&dir).unwrap(), 1);
    let (code, out, _) = p0ctl(&d, &["capability", "status"]);
    assert_eq!(code, 0);
    // No owner decided it here: undecided, the runtime running, every run starting OFF.
    assert!(
        out.contains("seq=1 LIVE LIVE")
            && out.contains("undecided: requests after seq=0 up to seq=1")
            && out.contains("runtime: running")
            && out.contains("starts with LIVE requested OFF"),
        "{out}"
    );
    shutdown.request();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
}
