//! S1-02: `p0ctl source` over the real binary. This test process stands in for the runtime: it
//! holds the owner lock, has the source-control doorbell installed and runs the control thread,
//! so `p0ctl source set` writes the typed request, finds the lock holder and rings it, and the
//! owner queue receives exactly the validated request (Geyser ACTIVE refused). Offline only.
use darkbot_common::channel::bounded;
use darkbot_common::lifecycle::Shutdown;
use darkbot_runner::orchestration::control;
use darkbot_runner::orchestration::source_control::{self, ControlMessage};
use darkbot_solana::ingress::sources::OperatingMode;
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
fn p0ctl_source_set_reaches_the_runtime_owner_queue() {
    let d = std::env::temp_dir().join(format!("p0ctl_source_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("data")).unwrap();
    // No runtime: refused, nothing recorded (a request nobody applies is no success; review
    // 72b74fc1: the GUI receipt then says FAILED, never COMPLETED).
    let (code, out, err) = p0ctl(&d, &["source", "set", "geyser:publicnode", "observe"]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(err.contains("SOURCE_NO_RUNTIME"), "{err}");
    assert_eq!(
        source_control::latest_request(&source_control::request_dir(&d)).unwrap(),
        None
    );
    let (code, _, err) = p0ctl(&d, &["source", "set", "geyser:publicnode", "on"]);
    assert_eq!(code, 2);
    assert!(err.contains("SOURCE_MODE_INVALID"), "{err}");

    // The stand-in runtime: lock, doorbell, control thread (requests before its start ignored).
    let _lock =
        control::acquire_owner(&control::default_lock_path(&d), "src_test", "shadow").unwrap();
    let (tx, rx) = bounded::<ControlMessage>("source-control", 4);
    let shutdown = Shutdown::new();
    let dir = source_control::request_dir(&d);
    let thread = source_control::spawn(
        dir.clone(),
        tx,
        shutdown.clone(),
        source_control::head(&dir).unwrap(),
        None,
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
    let (code, out, err) = p0ctl(&d, &["source", "set", "geyser:publicnode", "off"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("seq=1") && out.contains("sent to runtime pid"),
        "{out}"
    );
    assert!(matches!(
        next(),
        ControlMessage::Apply { seq: 1, control } if control.mode == OperatingMode::Off
    ));
    let (code, out, _) = p0ctl(&d, &["source", "set", "geyser:publicnode", "active"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        next(),
        ControlMessage::Refused {
            seq: 2,
            request: "geyser:publicnode active".into(),
            reason: "SOURCE_ACTIVE_REQUIRES_QUALIFICATION".into()
        }
    );
    let (code, out, _) = p0ctl(&d, &["source", "status"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("seq=2 unit=geyser:publicnode mode=active")
            && out.contains("runtime: running"),
        "{out}"
    );
    // CTRL-03 CAS: a change based on a stale configuration version is refused and writes
    // nothing; the current version (the request log's head) admits the next request.
    let set = |v: &str| {
        p0ctl(
            &d,
            &[
                "source",
                "set",
                "geyser:publicnode",
                "observe",
                "--expect-version",
                v,
            ],
        )
    };
    let (code, out, err) = set("1");
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        err.contains("SOURCE_VERSION_CONFLICT") && err.contains("current 2"),
        "{err}"
    );
    assert_eq!(source_control::head(&dir).unwrap(), 2);
    let (code, out, err) = set("2");
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("seq=3"), "{out}");
    assert!(matches!(next(), ControlMessage::Apply { seq: 3, .. }));
    let (code, _, err) = set("x");
    assert_eq!(code, 2);
    assert!(err.contains("--expect-version"), "{err}");
    shutdown.request();
    thread.join().unwrap();
    std::fs::remove_dir_all(d).unwrap();
}
