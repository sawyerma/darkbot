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
