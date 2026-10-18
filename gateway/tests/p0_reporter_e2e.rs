#![cfg(unix)]
// End-to-end real reporter IPC -> real gateway UnixSocketFeed -> supervisor -> StateHub.
// No Helius, wallet, synthetic production data or external services.
use darkbot_gateway::{
    config::GatewayConfig,
    engine::{spawn_supervisor, SourceRegistry, SourceState},
    feeds::UnixSocketFeed,
    hub::StateHub,
    proto::{ProjectId, StateQuality, TopicId},
};
use darkbot_p0_reporter::{start, ReporterConfig};
use darkbot_p0_reporter::proto::project0::StatusProjection;
use darkbot_runner::status_tap::{MarketSnapshot, StatusTap};
use prost::Message;
use std::{sync::Arc, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use tokio::time::sleep;

async fn await_condition(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(6);
    while Instant::now() < end {
        if f() { return; }
        sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for real reporter -> gateway state transition");
}

#[tokio::test]
async fn reporter_gateway_validated_state_detach_and_new_generation() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("p0gw-{}-{nonce}",std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let socket = std::path::PathBuf::from(format!("/tmp/p0gw-{}-{:x}.sock", std::process::id(),nonce as u64 & 0xffffffff));
    let generation_file = dir.join("gen");
    let tap = StatusTap::new();
    let reporter_cfg = ReporterConfig::new(socket.clone(),generation_file,"TEST_ONLY".into());
    let first = start(reporter_cfg.clone(),tap.clone()).unwrap();

    let mut config = GatewayConfig::from_env().unwrap();
    config.source_reattach_backoff_ms = 50;
    let hub = Arc::new(StateHub::new(262_144));
    let registry = SourceRegistry::new();
    let task = spawn_supervisor(
        UnixSocketFeed::new(ProjectId::Project0,"Project0",socket.clone(),262_144),
        hub.clone(),registry.clone(),&config,
    );

    tap.publish_market(MarketSnapshot {
        gap_count: 11,
        last_observed_slot: Some(123),
        live_verdict: "MARKET_DATA_INCOMPLETE",
        ..Default::default()
    });
    await_condition(|| registry.status(ProjectId::Project0).unwrap().state==SourceState::Connected).await;
    let first_state = hub.current(TopicId::Project0Status).expect("validated state in hub");
    assert_eq!(first_state.quality,StateQuality::Ok);
    assert_eq!(first_state.generation,first.generation);
    let p=StatusProjection::decode(first_state.payload.as_ref()).unwrap();
    assert_eq!(p.gap_count,Some(11));
    assert_eq!(p.last_observed_slot,Some(123));

    first.stop();
    await_condition(|| registry.status(ProjectId::Project0).unwrap().state==SourceState::Detached).await;
    assert_eq!(hub.current(TopicId::Project0Status).unwrap().quality,StateQuality::SourceDetached);

    let second = start(reporter_cfg,tap.clone()).unwrap();
    assert!(second.generation>first_state.generation);
    tap.publish_market(MarketSnapshot {
        gap_count: 12,
        last_observed_slot: Some(456),
        live_verdict: "MARKET_DATA_INCOMPLETE",
        ..Default::default()
    });
    await_condition(|| {
        hub.current(TopicId::Project0Status)
            .is_some_and(|s|s.generation==second.generation && s.quality==StateQuality::Ok)
    }).await;
    let s=hub.current(TopicId::Project0Status).unwrap();
    let p=StatusProjection::decode(s.payload.as_ref()).unwrap();
    assert_eq!(p.last_observed_slot,Some(456));
    assert_eq!(p.gap_count,Some(12));
    second.stop();
    task.abort();
    std::fs::remove_dir_all(dir).unwrap();
}
