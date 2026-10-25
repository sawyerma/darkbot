//! TEST_ONLY reporter fixture. No network or wallet.
use darkbot_p0_reporter::{start, ReporterConfig};
use darkbot_runner::status_tap::{
    operational_snapshot, AssessmentSnapshot, CapturePhase, MarketSnapshot, MarketStats,
    RunSnapshot, StatusTap, TransportSnapshot,
};
use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
fn main() {
    let socket = std::env::var("P0_FIXTURE_SOCKET").expect("fixture socket");
    let gen = std::env::var("P0_FIXTURE_GENERATION_FILE").expect("fixture generation");
    let tap = StatusTap::new();
    let mut cfg = ReporterConfig::new(socket.into(), gen.into(), "run-TEST_ONLY".into());
    cfg.heartbeat = Duration::from_millis(150);
    let handle = start(cfg, tap.clone()).expect("fixture reporter start");
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut buf);
        done.store(true, Ordering::Relaxed);
    });
    tap.publish_operational(operational_snapshot());
    tap.publish_run(RunSnapshot {
        run_id: "run-TEST_ONLY".into(),
        source: "logs",
        commitment: "finalized",
        ..Default::default()
    });
    tap.publish_phase(|p| p.phase = CapturePhase::Capture);
    tap.publish_assessment(AssessmentSnapshot {
        verdict: "MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED",
        coverage_complete: true,
        decode_complete: true,
        ..Default::default()
    });
    let mut i = 0u64;
    while !stop.load(Ordering::Relaxed) {
        i += 1;
        tap.publish_transport(TransportSnapshot {
            connected: true,
            application_frames: i,
            reconnects: 0,
            last_frame_age_ns: Some(1000000 * i),
            last_slot: Some(300000000 + i),
            sampled_at: None,
        });
        tap.publish_market(MarketSnapshot {
            observing: true,
            stream_ready: true,
            stats: MarketStats {
                unique_transactions: i,
                gap_count: i / 50,
                liquidation_events: i / 10,
                last_observed_slot: Some(300000000 + i),
                ..Default::default()
            },
        });
        tap.publish_storage(darkbot_runner::storage::writer::StorageStatus {
            queue_bytes: 4096 * (i % 7),
            lag_frames: i % 3,
            ..Default::default()
        });
        std::thread::sleep(Duration::from_millis(50));
    }
    tap.publish_phase(|p| {
        p.phase = CapturePhase::Ended;
        p.end_result = "CAPTURE_COMPLETE".into();
    });
    let _ = tap.flush(Duration::from_secs(2));
    handle.stop();
}
