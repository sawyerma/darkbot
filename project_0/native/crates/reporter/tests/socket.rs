#![cfg(unix)]
use darkbot_p0_reporter::proto::control::{engine_frame, EngineFrame, ProjectId, TopicId};
use darkbot_p0_reporter::proto::project0::CaptureState;
use darkbot_p0_reporter::{start, ReporterConfig};
use darkbot_runner::status_tap::{MarketSnapshot, MarketStats, StatusTap};
use prost::Message;
use std::io::Read;
use std::os::unix::{fs::PermissionsExt, net::UnixStream};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn directory(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "darkbot-reporter-{}-{name}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
fn sock(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    PathBuf::from(format!(
        "/tmp/p0r-{}-{tag}-{:x}.sock",
        std::process::id(),
        stamp & 0xffff_ffff
    ))
}
fn next(stream: &mut UnixStream) -> EngineFrame {
    let mut prefix = [0; 4];
    stream.read_exact(&mut prefix).unwrap();
    let n = u32::from_be_bytes(prefix) as usize;
    assert!((1..=262_144).contains(&n));
    let mut bytes = vec![0; n];
    stream.read_exact(&mut bytes).unwrap();
    EngineFrame::decode(bytes.as_slice()).unwrap()
}
fn connect(cfg: &ReporterConfig) -> UnixStream {
    let stream = UnixStream::connect(&cfg.socket_path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
}
fn expect_state(stream: &mut UnixStream) -> darkbot_p0_reporter::proto::control::StateFull {
    match next(stream).msg.unwrap() {
        engine_frame::Msg::State(s) => s,
        unexpected => panic!("expected state, got {unexpected:?}"),
    }
}
#[test]
fn handshake_state_heartbeat_permissions_and_reconnect_seq() {
    let dir = directory("wire");
    let tap = StatusTap::new();
    let mut cfg = ReporterConfig::new(sock("wire"), dir.join("generation"), "run-TEST_ONLY".into());
    cfg.heartbeat = Duration::from_millis(150);
    let handle = start(cfg.clone(), tap.clone()).unwrap();
    assert_eq!(
        std::fs::metadata(&cfg.socket_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let mut first = connect(&cfg);
    let hello = match next(&mut first).msg.unwrap() {
        engine_frame::Msg::Hello(h) => h,
        other => panic!("expected hello, got {other:?}"),
    };
    assert_eq!(hello.project, ProjectId::Project0 as i32);
    assert_eq!(
        hello.topics,
        vec![
            TopicId::Project0Status as i32,
            TopicId::Project0CaptureState as i32
        ]
    );
    assert_eq!(hello.generation, handle.generation);
    assert_eq!(hello.schema_id, darkbot_p0_reporter::schema_id());
    tap.publish_market(MarketSnapshot {
        stats: MarketStats {
            gap_count: 11,
            last_observed_slot: Some(123),
            ..Default::default()
        },
        ..Default::default()
    });
    let one = expect_state(&mut first);
    assert_eq!(one.generation, hello.generation);
    assert_eq!(one.source_id, hello.source_id);
    assert_eq!(one.seq, 1);
    assert_eq!(one.topic, TopicId::Project0CaptureState as i32);
    let proj = CaptureState::decode(one.payload.as_ref()).unwrap();
    let counters = proj.market.unwrap().counters;
    assert!(counters
        .iter()
        .any(|c| c.name == "gap_count" && c.value == 11));
    assert!(counters
        .iter()
        .any(|c| c.name == "last_observed_slot" && c.value == 123));
    match next(&mut first).msg.unwrap() {
        engine_frame::Msg::Heartbeat(_) => (),
        other => panic!("expected heartbeat, got {other:?}"),
    }
    drop(first);
    // The next connection must deliver the last snapshot with a strictly higher seq.
    let mut second = connect(&cfg);
    match next(&mut second).msg.unwrap() {
        engine_frame::Msg::Hello(h) => assert_eq!(h.generation, hello.generation),
        other => panic!("expected reconnect hello, got {other:?}"),
    }
    let two = expect_state(&mut second);
    assert!(two.seq > one.seq, "sequence reset after reconnect");
    assert_eq!(two.source_id, hello.source_id);
    drop(second);
    handle.stop();
    assert!(!cfg.socket_path.exists());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn slow_and_disconnected_client_do_not_block_capture_publisher() {
    let dir = directory("nonblocking");
    let tap = StatusTap::new();
    let mut cfg = ReporterConfig::new(sock("slow"), dir.join("generation"), "run-TEST_ONLY".into());
    cfg.heartbeat = Duration::from_millis(100);
    cfg.write_timeout = Duration::from_millis(200);
    let handle = start(cfg.clone(), tap.clone()).unwrap();
    let slow = connect(&cfg);
    let started = Instant::now();
    for n in 0..20_000 {
        tap.publish_market(MarketSnapshot {
            stats: MarketStats {
                gap_count: n,
                ..Default::default()
            },
            ..Default::default()
        });
    }
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "capture publisher stalled by slow socket client"
    );
    drop(slow);
    // No publisher needs to wait for the socket worker to observe disconnection.
    for n in 0..1000 {
        tap.publish_market(MarketSnapshot {
            stats: MarketStats {
                gap_count: n,
                ..Default::default()
            },
            ..Default::default()
        });
    }
    // connect() completes before the nonblocking server thread necessarily accepts.
    // Give the accept loop a bounded chance to report the connection.
    let deadline = Instant::now() + Duration::from_secs(2);
    while handle.counters.connections.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        handle.counters.connections.load(Ordering::Relaxed) >= 1,
        "reporter did not accept the connection"
    );
    handle.stop();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn coalesces_bursts_keeps_heartbeats_and_hands_over_terminal_state() {
    use darkbot_runner::status_tap::{operational_snapshot, CapturePhase};
    let dir = directory("cadence");
    let tap = StatusTap::new();
    tap.publish_operational(operational_snapshot());
    tap.publish_market(MarketSnapshot::default());
    let mut cfg = ReporterConfig::new(sock("cadence"), dir.join("generation"), "TEST_ONLY".into());
    cfg.heartbeat = Duration::from_millis(200);
    let handle = start(cfg.clone(), tap.clone()).unwrap();
    let mut client = connect(&cfg);
    assert!(matches!(
        next(&mut client).msg,
        Some(engine_frame::Msg::Hello(_))
    ));
    assert_eq!(
        expect_state(&mut client).topic,
        TopicId::Project0Status as i32
    );
    assert_eq!(
        expect_state(&mut client).topic,
        TopicId::Project0CaptureState as i32
    );
    let producer = std::thread::spawn(move || {
        for n in 1..=80 {
            tap.publish_market(MarketSnapshot {
                stats: MarketStats {
                    gap_count: n,
                    ..Default::default()
                },
                ..Default::default()
            });
            std::thread::sleep(Duration::from_millis(5));
        }
        tap.publish_phase(|p| {
            p.phase = CapturePhase::Ended;
            p.end_result = "CAPTURE_COMPLETE".into();
        });
        assert!(tap.flush(Duration::from_secs(2)));
    });
    let started = Instant::now();
    let mut states = 0;
    let mut heartbeats = 0;
    loop {
        assert!(started.elapsed() < Duration::from_secs(3));
        match next(&mut client).msg.unwrap() {
            engine_frame::Msg::State(s) => {
                assert_eq!(
                    s.topic,
                    TopicId::Project0CaptureState as i32,
                    "unchanged system must not be resent"
                );
                states += 1;
                let c = CaptureState::decode(s.payload.as_ref()).unwrap();
                if c.phase
                    .as_ref()
                    .is_some_and(|p| p.end_result == "CAPTURE_COMPLETE")
                {
                    assert!(c
                        .market
                        .unwrap()
                        .counters
                        .iter()
                        .any(|v| v.name == "gap_count" && v.value == 80));
                    break;
                }
            }
            engine_frame::Msg::Heartbeat(_) => heartbeats += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
    producer.join().unwrap();
    assert!(
        states <= 1 + started.elapsed().as_millis() / 100,
        "capture exceeded 10 Hz"
    );
    assert!(heartbeats >= 1, "busy stream starved heartbeats");
    handle.stop();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn idle_heartbeat_obeys_its_configured_interval() {
    let dir = directory("heartbeat");
    let mut cfg = ReporterConfig::new(sock("hb"), dir.join("generation"), "TEST_ONLY".into());
    cfg.heartbeat = Duration::from_millis(700);
    let handle = start(cfg.clone(), StatusTap::new()).unwrap();
    let mut client = connect(&cfg);
    assert!(matches!(
        next(&mut client).msg,
        Some(engine_frame::Msg::Hello(_))
    ));
    let started = Instant::now();
    assert!(matches!(
        next(&mut client).msg,
        Some(engine_frame::Msg::Heartbeat(_))
    ));
    assert!(started.elapsed() >= Duration::from_millis(650));
    handle.stop();
    std::fs::remove_dir_all(dir).unwrap();
}

/// R21: the heartbeat follows its configured period, not the 500-ms stop poll (700 ms used to
/// arrive every ~1000 ms).
#[test]
fn r21_heartbeat_period_is_the_configured_interval() {
    let dir = directory("r21");
    let mut cfg = ReporterConfig::new(sock("r21"), dir.join("generation"), "TEST_ONLY".into());
    cfg.heartbeat = Duration::from_millis(700);
    let handle = start(cfg.clone(), StatusTap::new()).unwrap();
    let mut client = connect(&cfg);
    assert!(matches!(
        next(&mut client).msg,
        Some(engine_frame::Msg::Hello(_))
    ));
    let mut last = Instant::now();
    for _ in 0..3 {
        assert!(matches!(
            next(&mut client).msg,
            Some(engine_frame::Msg::Heartbeat(_))
        ));
        let gap = last.elapsed();
        last = Instant::now();
        assert!(
            gap >= Duration::from_millis(650) && gap < Duration::from_millis(900),
            "heartbeat gap {gap:?}"
        );
    }
    handle.stop();
    std::fs::remove_dir_all(dir).unwrap();
}

/// R22: leftovers of a crashed owner (lock file, generation temp file, unserved socket) never
/// block a restart; a live owner, a live foreign server, a foreign file and a symlink refuse
/// the start and stay untouched.
#[test]
fn r22_crash_leftovers_restart_but_live_owners_and_foreign_files_refuse() {
    let dir = directory("r22");
    let cfg = ReporterConfig::new(sock("r22"), dir.join("generation"), "TEST_ONLY".into());
    std::fs::write(dir.join("generation.generation.lock"), b"").unwrap();
    std::fs::write(dir.join("generation.next"), b"7\n").unwrap();
    drop(std::os::unix::net::UnixListener::bind(&cfg.socket_path).unwrap());
    assert!(cfg.socket_path.exists(), "stale socket file left behind");
    let handle = start(cfg.clone(), StatusTap::new()).expect("restart after crash");
    assert_eq!(handle.generation, 1);
    let mut client = connect(&cfg);
    assert!(matches!(
        next(&mut client).msg,
        Some(engine_frame::Msg::Hello(_))
    ));
    // Same owner lock while alive.
    let second = start(cfg.clone(), StatusTap::new()).err().unwrap();
    assert!(second.contains("already owned"), "{second}");
    // Another generation file but the live socket.
    let other = ReporterConfig::new(
        cfg.socket_path.clone(),
        dir.join("other"),
        "TEST_ONLY".into(),
    );
    let live = start(other, StatusTap::new()).err().unwrap();
    assert!(live.contains("live process"), "{live}");
    assert!(cfg.socket_path.exists());
    // A regular file and a symlink at the socket path are never removed.
    let file = sock("r22f");
    std::fs::write(&file, b"keep").unwrap();
    let foreign = start(
        ReporterConfig::new(file.clone(), dir.join("f"), "TEST_ONLY".into()),
        StatusTap::new(),
    )
    .err()
    .unwrap();
    assert!(foreign.contains("foreign file"), "{foreign}");
    assert_eq!(std::fs::read(&file).unwrap(), b"keep");
    let link = sock("r22l");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    let linked = start(
        ReporterConfig::new(link.clone(), dir.join("l"), "TEST_ONLY".into()),
        StatusTap::new(),
    )
    .err()
    .unwrap();
    assert!(linked.contains("foreign file"), "{linked}");
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    std::fs::remove_file(&link).unwrap();
    std::fs::remove_file(&file).unwrap();
    handle.stop();
    assert!(!cfg.socket_path.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

/// R23: a heartbeat names, per topic sent in the session, the generation and the highest seq
/// of the state frames already delivered.
#[test]
fn r23_heartbeat_watermarks_follow_the_sent_state_frames() {
    let dir = directory("r23");
    let mut cfg = ReporterConfig::new(sock("r23"), dir.join("generation"), "TEST_ONLY".into());
    cfg.heartbeat = Duration::from_millis(600);
    let tap = StatusTap::new();
    tap.publish_market(MarketSnapshot::default());
    let handle = start(cfg.clone(), tap.clone()).unwrap();
    let mut client = connect(&cfg);
    let mut last_capture_seq = 0;
    let generation = loop {
        match next(&mut client).msg.unwrap() {
            engine_frame::Msg::Hello(h) => assert_eq!(h.generation, handle.generation),
            engine_frame::Msg::State(s) => {
                assert_eq!(s.topic, TopicId::Project0CaptureState as i32);
                last_capture_seq = s.seq;
                tap.publish_market(MarketSnapshot::default());
            }
            engine_frame::Msg::Heartbeat(hb) => {
                assert!(last_capture_seq > 0);
                let w = hb
                    .state_watermarks
                    .iter()
                    .find(|w| w.topic == TopicId::Project0CaptureState as i32)
                    .expect("capture watermark");
                assert!(w.highest_seq_sent >= last_capture_seq, "{w:?}");
                break w.generation;
            }
            // Protocol 0495fd04: a P0 capture state never exceeds the frame bound here.
            engine_frame::Msg::StateTooLarge(t) => panic!("unexpected StateTooLarge {t:?}"),
        }
    };
    assert_eq!(generation, handle.generation);
    handle.stop();
    std::fs::remove_dir_all(dir).unwrap();
}

/// B03/U-9: a reporter with a project directory declares project0.analytics.jobs and publishes
/// the job store: a requested job arrives QUEUED with its run, its outcome follows; stopping the
/// reporter ends the publisher without waiting out its interval.
#[test]
fn b03_analytics_jobs_reach_the_wire_from_the_job_store() {
    use darkbot_p0_reporter::proto::project0::AnalyticsJobs;
    use darkbot_runner::analytics::backtest::Params;
    use darkbot_runner::analytics::jobs;
    let dir = directory("analytics");
    let project = dir.join("project");
    std::fs::create_dir_all(project.join("data/econ4h/run-a/capture")).unwrap();
    let request = jobs::JobRequest {
        job_id: "j-1".into(),
        run: "run-a".into(),
        verification_result: None,
        max_seconds: 5,
        max_ram_bytes: 1 << 20,
        spec: jobs::JobSpec::Backtest {
            params: Params {
                min_profit_usd_micro: 0,
                capital_usd_micro: vec![1_000_000_000],
                funding: vec![],
            },
        },
        requested_unix_ms: Some(1),
    };
    jobs::submit(&project, &request).unwrap();
    let tap = StatusTap::new();
    let mut cfg = ReporterConfig::new(sock("analytics"), dir.join("generation"), "run".into());
    cfg.project_dir = Some(project.clone());
    cfg.capture = false;
    let handle = start(cfg.clone(), tap).unwrap();
    let mut stream = connect(&cfg);
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    match next(&mut stream).msg.unwrap() {
        // The analytics publisher declares its one topic (the gateway's auxiliary feed).
        engine_frame::Msg::Hello(h) => {
            assert_eq!(h.topics, vec![TopicId::Project0AnalyticsJobs as i32])
        }
        other => panic!("{other:?}"),
    }
    let mut analytics = || loop {
        if let Some(engine_frame::Msg::State(s)) = next(&mut stream).msg {
            if s.topic == TopicId::Project0AnalyticsJobs as i32 {
                return AnalyticsJobs::decode(s.payload).unwrap();
            }
        }
    };
    let first = analytics();
    assert_eq!(first.runs.len(), 1);
    assert_eq!(
        (first.runs[0].run_id.as_str(), first.runs[0].layout.as_str()),
        ("run-a", "HISTORICAL")
    );
    assert_eq!(
        (first.jobs[0].job_id.as_str(), first.jobs[0].state.as_str()),
        ("j-1", "QUEUED")
    );
    let (outcome, _) = jobs::run(&project, "j-1", None).unwrap();
    let second = analytics();
    assert_eq!(second.jobs[0].state, "FAILED");
    assert_eq!(second.jobs[0].code, outcome.code);
    let stopped = Instant::now();
    handle.stop();
    assert!(
        stopped.elapsed() < Duration::from_millis(1_000),
        "the publisher is unparked, never waited out"
    );
    let _ = std::fs::remove_dir_all(dir);
}
