#![cfg(unix)]
//! B03/U-9: the standalone analytics publisher over the real binary — it declares only
//! project0.analytics.jobs, publishes the job store of its project and ends cleanly on SIGTERM
//! (launchd's stop), removing its socket.
use darkbot_p0_reporter::proto::control::{engine_frame, EngineFrame, TopicId};
use darkbot_p0_reporter::proto::project0::AnalyticsJobs;
use darkbot_runner::analytics::backtest::Params;
use darkbot_runner::analytics::jobs;
use prost::Message;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn next(s: &mut UnixStream) -> EngineFrame {
    let mut prefix = [0; 4];
    s.read_exact(&mut prefix).unwrap();
    let mut bytes = vec![0; u32::from_be_bytes(prefix) as usize];
    s.read_exact(&mut bytes).unwrap();
    EngineFrame::decode(bytes.as_slice()).unwrap()
}

#[test]
fn the_analytics_publisher_serves_only_the_job_store_and_stops_on_sigterm() {
    let dir = std::env::temp_dir().join(format!("p0-analytics-bin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = dir.join("project");
    std::fs::create_dir_all(project.join("data/econ4h/run-a/capture")).unwrap();
    jobs::submit(
        &project,
        &jobs::JobRequest {
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
        },
    )
    .unwrap();
    let socket = format!("/tmp/p0a-{}.sock", std::process::id());
    let _ = std::fs::remove_file(&socket);
    let mut child = Command::new(env!("CARGO_BIN_EXE_p0-analytics-reporter"))
        .env_clear()
        .env("P0_REPORT_SOCKET", &socket)
        .env("P0_REPORT_GENERATION_FILE", dir.join("generation"))
        .env("P0_REPORT_ANALYTICS_PROJECT_DIR", &project)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        if let Ok(s) = UnixStream::connect(&socket) {
            break s;
        }
        assert!(Instant::now() < deadline, "publisher never listened");
        std::thread::sleep(Duration::from_millis(20));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    match next(&mut stream).msg {
        Some(engine_frame::Msg::Hello(h)) => {
            assert_eq!(h.topics, vec![TopicId::Project0AnalyticsJobs as i32])
        }
        other => panic!("{other:?}"),
    }
    let state = loop {
        if let Some(engine_frame::Msg::State(s)) = next(&mut stream).msg {
            break s;
        }
    };
    assert_eq!(state.topic, TopicId::Project0AnalyticsJobs as i32);
    let jobs = AnalyticsJobs::decode(state.payload).unwrap();
    assert_eq!(jobs.jobs[0].job_id, "j-1");
    // SAFETY: SIGTERM to the child spawned above.
    assert_eq!(
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) },
        0
    );
    let status = child.wait().unwrap();
    assert!(status.success(), "{status:?}");
    assert!(
        !std::path::Path::new(&socket).exists(),
        "socket removed on stop"
    );
    let _ = std::fs::remove_dir_all(dir);
}
