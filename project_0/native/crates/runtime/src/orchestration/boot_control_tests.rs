//! DEC-07: the boot controller offline — one flow per attempt, separate approvals naming their
//! own token, each step once, crash and double start, the Capture tab's view.
use super::*;
use crate::orchestration::boot_audit::network_record_fixture;

fn project(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("p0_boot_{}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A flow whose offline step passed (`OFFLINE_READY_NETWORK_NOT_RUN`) with token `a…`.
fn flow(id: &str) -> BootFlow {
    let mut offline = network_record_fixture(vec![]).audit;
    offline.scope = "offline".into();
    offline.verdict = AuditVerdict::OfflineReadyNetworkNotRun;
    offline.readiness_token = "a".repeat(64);
    BootFlow {
        flow_id: id.into(),
        mode: "econ4h".into(),
        operational: vec!["--boot-token-ttl-seconds".into(), "90".into()],
        offline,
        ttl_seconds: 90,
        ttl_source: "cli".into(),
        created_unix_ms: Some(1),
    }
}

fn never() -> bool {
    false
}

/// The network step of `id`, completed with a READY record (token from the fixture).
fn network_done(project: &Path, id: &str) -> NetworkAudit {
    let record = network_record_fixture(vec![]);
    let path = network_record_path(project, id).unwrap();
    let outcome = run_step(project, id, NETWORK, never, |_, _| {
        journal::write_typed_value(&path, journal::source::BOOT_CONTROL, &record).unwrap();
        Ok(None)
    })
    .unwrap();
    assert_eq!(outcome.state, COMPLETED);
    record
}

#[test]
fn dec07_a_flow_records_its_offline_step_once() {
    let p = project("create");
    create(&p, &flow("f1")).unwrap();
    assert_eq!(create(&p, &flow("f1")).unwrap_err(), "BOOT_FLOW_EXISTS");
    assert_eq!(flow_dir(&p, "../x").unwrap_err(), "BOOT_FLOW_ID_INVALID");
    let s = status(&p, "f1").unwrap();
    assert_eq!((s.network.name(), s.start.name()), ("NOT_RUN", "NOT_RUN"));
    assert_eq!(s.flow, flow("f1"));
    assert_eq!(status(&p, "f2").unwrap_err(), "BOOT_FLOW_UNKNOWN");
}

/// The network approval names the offline token, the start approval the network record's; a
/// blocked offline step, a missing or unfinished network audit and another token are refused.
#[test]
fn dec07_each_approval_names_its_own_token() {
    let p = project("approval");
    create(&p, &flow("f1")).unwrap();
    let s = status(&p, "f1").unwrap();
    assert!(check_approval(&s, NETWORK, &"a".repeat(16)).is_ok());
    for wrong in ["b".repeat(16), "a".repeat(15), "a".repeat(17)] {
        assert!(check_approval(&s, NETWORK, &wrong)
            .unwrap_err()
            .starts_with("BOOT_NETWORK_APPROVAL_MISMATCH"));
    }
    assert!(check_approval(&s, START, &"a".repeat(16))
        .unwrap_err()
        .starts_with("BOOT_NETWORK_NOT_READY"));
    let mut blocked = flow("f2");
    blocked.offline.verdict = AuditVerdict::Blocked;
    create(&p, &blocked).unwrap();
    assert!(
        check_approval(&status(&p, "f2").unwrap(), NETWORK, &"a".repeat(16))
            .unwrap_err()
            .starts_with("BOOT_OFFLINE_BLOCKED")
    );
    let record = network_done(&p, "f1");
    let s = status(&p, "f1").unwrap();
    let token = record.audit.readiness_token[..16].to_string();
    assert!(check_approval(&s, START, &token).is_ok());
    assert!(check_approval(&s, START, &"a".repeat(16))
        .unwrap_err()
        .starts_with("BOOT_START_APPROVAL_MISMATCH"));
}

/// A step runs once: COMPLETED with the run it launched, FAILED with its code, CANCELLED when
/// stopped; a second run of a step with an outcome is refused.
#[test]
fn dec07_a_step_runs_once_and_records_its_outcome() {
    let p = project("once");
    for id in ["ok", "bad", "stop"] {
        create(&p, &flow(id)).unwrap();
    }
    let ok = run_step(&p, "ok", START, never, |f, _| {
        assert_eq!(f.ttl_seconds, 90, "the flow's own TTL");
        Ok(Some("econ4h_1".into()))
    })
    .unwrap();
    assert_eq!(
        (ok.state.as_str(), ok.run_id.as_deref()),
        (COMPLETED, Some("econ4h_1"))
    );
    assert_eq!(
        run_step(&p, "ok", START, never, |_, _| Ok(None)).unwrap_err(),
        "BOOT_STEP_DONE: the step has an outcome"
    );
    let bad = run_step(&p, "bad", NETWORK, never, |_, _| {
        Err("BOOT_NETWORK_EXPIRED".into())
    })
    .unwrap();
    assert_eq!(
        (bad.state.as_str(), bad.code.as_str()),
        (FAILED, "BOOT_NETWORK_EXPIRED")
    );
    let stop = run_step(&p, "stop", START, || true, |_, _| Err("STOPPED".into())).unwrap();
    assert_eq!(stop.state, CANCELLED);
    assert_eq!(status(&p, "ok").unwrap().start.name(), COMPLETED);
}

/// No automatic repetition: a requested step (here its worker never took it and is gone — a
/// crash) is INTERRUPTED and never requested again in the flow; while a step of another flow
/// runs, every request is refused (double start).
#[test]
fn dec07_no_step_is_requested_twice_and_one_boot_step_runs_per_project() {
    let p = project("twice");
    create(&p, &flow("f1")).unwrap();
    let gone = Path::new("/usr/bin/true");
    begin(&p, "f1", NETWORK, &"a".repeat(16), gone, &[]).unwrap();
    // The caller of `p0ctl boot network` exits and init reaps its worker; this test reaps it.
    let (child, _) = worker_record(&flow_dir(&p, "f1").unwrap().join("network.spawn"));
    // SAFETY: waitpid on this process's own child.
    unsafe { libc::waitpid(child as libc::pid_t, std::ptr::null_mut(), 0) };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while status(&p, "f1").unwrap().network.name() != "INTERRUPTED" {
        assert!(std::time::Instant::now() < deadline, "the worker is gone");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(begin(&p, "f1", NETWORK, &"a".repeat(16), gone, &[])
        .unwrap_err()
        .starts_with("BOOT_STEP_ALREADY_REQUESTED"));
    // A worker that cannot be spawned records the step FAILED (never QUEUED forever).
    create(&p, &flow("f2")).unwrap();
    let missing = p.join("no-such-p0ctl");
    assert!(begin(&p, "f2", NETWORK, &"a".repeat(16), &missing, &[])
        .unwrap_err()
        .starts_with("BOOT_SPAWN_FAILED"));
    assert_eq!(status(&p, "f2").unwrap().network.name(), FAILED);
    // A wrong token is refused before any worker exists.
    create(&p, &flow("f4")).unwrap();
    assert!(begin(&p, "f4", NETWORK, &"b".repeat(16), gone, &[])
        .unwrap_err()
        .starts_with("BOOT_NETWORK_APPROVAL_MISMATCH"));
    assert_eq!(status(&p, "f4").unwrap().network.name(), "NOT_RUN");
    // A running step of one flow (its worker holds the lock) refuses every other request.
    create(&p, &flow("f3")).unwrap();
    let dir = flow_dir(&p, "f3").unwrap();
    std::fs::write(dir.join("network.spawn"), "pid=1\n").unwrap();
    let lock = open_lock(&dir.join("network.lock")).unwrap();
    assert!(flock_nb(&lock, libc::LOCK_EX).unwrap());
    assert_eq!(status(&p, "f3").unwrap().network.name(), "RUNNING");
    assert!(begin(&p, "f4", NETWORK, &"a".repeat(16), gone, &[])
        .unwrap_err()
        .starts_with("BOOT_BUSY"));
    assert_eq!(
        run_step(&p, "f3", NETWORK, never, |_, _| Ok(None)).unwrap_err(),
        "BOOT_STEP_RUNNING: another worker holds the step"
    );
    drop(lock);
}

/// The Capture tab's view of the newest flow: verdicts, the 16-character tokens each approval
/// names, the flow's TTL and source, the network record's TTL window and every step's state.
#[test]
fn dec07_the_capture_view_shows_ttl_tokens_window_and_steps() {
    let p = project("view");
    assert_eq!(projection(&p), None);
    create(&p, &flow("old")).unwrap();
    let mut newer = flow("new");
    newer.created_unix_ms = Some(2);
    newer.offline.checks[0].state = CheckState::Blocked;
    newer.offline.checks[0].reason = "BOOT_X".into();
    create(&p, &newer).unwrap();
    let record = network_done(&p, "new");
    let v = projection(&p).unwrap();
    assert_eq!(v.flow_id, "new");
    assert_eq!(
        (v.offline_verdict, v.offline_token.as_str()),
        ("OFFLINE_READY_NETWORK_NOT_RUN", "aaaaaaaaaaaaaaaa")
    );
    assert_eq!(
        v.offline_blocked,
        vec![format!("{}: BOOT_X", newer.offline.checks[0].id)]
    );
    assert_eq!((v.ttl_seconds, v.ttl_source.as_str()), (90, "cli"));
    assert_eq!(v.network.state, COMPLETED);
    assert_eq!(v.network_verdict, Some("READY"));
    assert_eq!(
        v.network_token.as_deref(),
        Some(&record.audit.readiness_token[..16])
    );
    assert_eq!(
        (v.observed_unix_s, v.expires_unix_s),
        (Some(record.observed_at), Some(record.expires_at))
    );
    assert_eq!(v.start.state, "NOT_RUN");
}
