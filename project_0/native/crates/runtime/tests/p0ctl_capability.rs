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
        ("HOLD_B", "LIVE", "CAPABILITY_VALUE_INVALID"),
        ("HOLD_B", "ACTIVE", "CAPABILITY_NO_RUNTIME"),
        ("THRESHOLD_TRIGGER", "SWEEP", "CAPABILITY_VALUE_INVALID"),
        ("THRESHOLD_TRIGGER", "OBSERVE", "CAPABILITY_NO_RUNTIME"),
    ] {
        let (code, _, err) = p0ctl(&d, &["capability", "set", capability, value]);
        assert_eq!(code, 2);
        assert!(err.contains(code_text), "{err}");
    }

    // The stand-in runtime: lock, doorbell, control thread with both logs.
    let _lock =
        control::acquire_owner(&control::default_lock_path(&d), "cap_test", "shadow").unwrap();
    // P8-CTRL.6: the runtime's epoch (every owner start opens one).
    let (code, _, err) = p0ctl(&d, &["capability", "set", "LIVE", "LIVE"]);
    assert_eq!(code, 2);
    assert!(err.contains("CAPABILITY_NO_OWNER_EPOCH"), "{err}");
    assert_eq!(
        capability_control::open_epoch(&dir, std::process::id(), None).unwrap(),
        1
    );
    let (tx, rx) = bounded::<ControlMessage>("source-control", 8);
    let shutdown = Shutdown::new();
    let sources = source_control::request_dir(&d);
    let thread = source_control::spawn(
        sources.clone(),
        tx,
        shutdown.clone(),
        source_control::head(&sources).unwrap(),
        Some((
            dir.clone(),
            capability_control::head(&dir).unwrap(),
            Default::default(),
        )),
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
        out.contains("seq=1") && out.contains("epoch=1") && out.contains("FE-06"),
        "{out}"
    );
    match next() {
        ControlMessage::Capability { seq: 1, request } => {
            let r = request.unwrap();
            assert_eq!(
                (r.capability.as_str(), r.requested.as_str(), r.owner_epoch),
                ("LIVE", "LIVE", 1)
            );
            assert_eq!(r.request_id.len(), 32, "a fresh id");
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
    // DEC-06: the threshold mode on the same CAS version.
    let (code, _, err) = p0ctl(
        &d,
        &[
            "capability",
            "set",
            "THRESHOLD_TRIGGER",
            "OBSERVE",
            "--expect-version",
            "0",
        ],
    );
    assert_eq!(code, 2);
    assert!(err.contains("CAPABILITY_VERSION_CONFLICT"), "{err}");
    assert_eq!(capability_control::head(&dir).unwrap(), 1);
    // P8-CTRL.1/.6: an economic switch under the operator's request id; the same action again
    // is the same request (nothing new written), the id with another value a conflict; a stale
    // epoch or an invalid wallet is refused before anything is written.
    let set = |value: &str, extra: &[&str]| {
        let mut args = vec![
            "capability",
            "set",
            "ECON_SIGN",
            value,
            "--request-id",
            "op-1",
        ];
        args.extend_from_slice(extra);
        p0ctl(&d, &args)
    };
    // P8-CTRL.6 (review M2): an economic switch states version, epoch and id.
    for extra in [
        &[][..],
        &["--expect-epoch", "1"][..],
        &["--expect-version", "1"][..],
    ] {
        let (code, _, err) = set("ENABLED", extra);
        assert_eq!(code, 2);
        assert!(err.contains("CAPABILITY_BINDING_REQUIRED"), "{err}");
    }
    let bound = ["--expect-epoch", "1", "--expect-version", "1"];
    let (code, out, err) = set("ENABLED", &bound);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("seq=2 ECON_SIGN ENABLED request_id=op-1 epoch=1 sent"),
        "{out}"
    );
    match next() {
        ControlMessage::Capability { seq: 2, request } => {
            let r = request.unwrap();
            assert_eq!(
                (
                    r.capability.as_str(),
                    r.requested.as_str(),
                    r.request_id.as_str()
                ),
                ("ECON_SIGN", "ENABLED", "op-1")
            );
        }
        other => panic!("{other:?}"),
    }
    // The retry of the same action (its version is stale by now): the same request.
    let (code, out, _) = set("ENABLED", &bound);
    assert_eq!(code, 0);
    assert!(
        out.contains("seq=2") && out.contains("the same request again"),
        "{out}"
    );
    for (value, extra, refusal) in [
        (
            "DISABLED",
            &["--expect-epoch", "1", "--expect-version", "2"][..],
            "CAPABILITY_REQUEST_ID_CONFLICT",
        ),
        (
            "ENABLED",
            &["--expect-epoch", "9", "--expect-version", "2"][..],
            "CAPABILITY_EPOCH_STALE",
        ),
        ("ON", &bound[..], "CAPABILITY_VALUE_INVALID"),
    ] {
        let (code, _, err) = set(value, extra);
        assert_eq!(code, 2);
        assert!(err.contains(refusal), "{err}");
    }
    let (code, _, err) = p0ctl(&d, &["capability", "set", "WALLET:nope", "ENABLED"]);
    assert_eq!(code, 2);
    assert!(err.contains("CAPABILITY_WALLET_INVALID"), "{err}");
    assert_eq!(capability_control::head(&dir).unwrap(), 2);
    let (code, out, _) = p0ctl(&d, &["capability", "status"]);
    assert_eq!(code, 0);
    // No owner decided it here: undecided, the runtime running, every run starting OFF.
    assert!(
        out.contains("seq=2 ECON_SIGN ENABLED request_id=op-1 epoch=1")
            && out.contains("undecided: requests after seq=0 up to seq=2")
            && out.contains("owner epoch: 1")
            && out.contains("runtime: running")
            && out.contains("every switch DISABLED and LIVE requested OFF"),
        "{out}"
    );
    // P8-CTRL.6: the epoch must be the running owner's own — one another process opened is
    // refused before anything is written.
    assert_eq!(capability_control::open_epoch(&dir, 1, None).unwrap(), 2);
    let (code, _, err) = set("ENABLED", &["--expect-epoch", "2", "--expect-version", "2"]);
    assert_eq!(code, 2);
    assert!(err.contains("CAPABILITY_EPOCH_FOREIGN"), "{err}");
    assert_eq!(capability_control::head(&dir).unwrap(), 2);
    shutdown.request();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
}
