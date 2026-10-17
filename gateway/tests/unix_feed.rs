#![cfg(unix)]
// Project_0 IPC boundary: fake source, never a trading engine.
use darkbot_gateway::{
    engine::{spawn_supervisor, SourceRegistry, SourceState},
    feeds::UnixSocketFeed,
    config::{GatewayConfig, Limits},
    hub::StateHub,
    proto::{engine_frame, EngineFrame, EngineHello, ProjectId, StateFull, StateQuality, TopicId},
    SCHEMA_ID,
};
use prost::Message;
use std::{path::PathBuf, sync::Arc, time::{Duration, SystemTime, UNIX_EPOCH}};
use tokio::{io::AsyncWriteExt, net::{UnixListener, UnixStream}};

fn socket(tag: &str) -> PathBuf {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("darkbot-p0-{}-{tag}-{now}.sock", std::process::id()))
}
fn config() -> GatewayConfig {
    GatewayConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        static_dir: None,
        allowed_origins: vec![],
        allowed_hosts: vec![],
        session_cookie: "test".into(),
        limits: Limits::default(),
        source_stale_after_ms: 500,
        source_detach_after_ms: 1000,
        source_reattach_backoff_ms: 25,
        overview_interval_ms: 60_000,
        feeds: vec![],
    }
}
fn hello(schema: &str) -> EngineFrame {
    EngineFrame { msg: Some(engine_frame::Msg::Hello(EngineHello {
        project: ProjectId::Project0 as i32,
        schema_id: schema.into(),
        source_id: "test-boot-1".into(),
        generation: 42,
        topics: vec![TopicId::Project0Status as i32],
    })) }
}
fn state(seq: u64) -> EngineFrame {
    EngineFrame { msg: Some(engine_frame::Msg::State(StateFull {
        topic: TopicId::Project0Status as i32,
        generation: 42,
        seq,
        source_timestamp_unix_ns: 123,
        quality: StateQuality::Ok as i32,
        source_id: "test-boot-1".into(),
        payload: bytes::Bytes::from_static(b"\x12\x03xyz"),
    })) }
}
async fn send(socket: &mut UnixStream, frame: EngineFrame) {
    let data = frame.encode_to_vec();
    socket.write_all(&(data.len() as u32).to_be_bytes()).await.unwrap();
    socket.write_all(&data).await.unwrap();
}
async fn wait_until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(4), async {
        while !predicate() { tokio::time::sleep(Duration::from_millis(10)).await; }
    }).await.expect("state transition timed out");
}

#[tokio::test]
async fn real_socket_publishes_project0_and_detaches_after_close() {
    let path = socket("happy");
    let listener = UnixListener::bind(&path).unwrap();
    let hub = Arc::new(StateHub::new(1024));
    let registry = SourceRegistry::new();
    let supervisor = spawn_supervisor(
        UnixSocketFeed::new(ProjectId::Project0, "Project 0", path.clone(), 1024),
        hub.clone(), registry.clone(), &config()
    );
    let (mut stream, _) = listener.accept().await.unwrap();
    send(&mut stream, hello(SCHEMA_ID)).await;
    send(&mut stream, state(1)).await;
    wait_until(|| hub.current(TopicId::Project0Status).is_some()).await;
    let cached = hub.current(TopicId::Project0Status).unwrap();
    assert_eq!((cached.generation, cached.seq), (42, 1));
    assert_eq!(cached.payload.as_ref(), b"\x12\x03xyz");
    wait_until(|| registry.status(ProjectId::Project0).unwrap().state == SourceState::Connected).await;
    drop(stream);
    wait_until(|| hub.current(TopicId::Project0Status).unwrap().quality == StateQuality::SourceDetached).await;
    assert_eq!(registry.status(ProjectId::Project0).unwrap().state, SourceState::Detached);
    supervisor.abort();
    drop(listener);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn mismatched_schema_never_publishes_project0_data() {
    let path = socket("reject");
    let listener = UnixListener::bind(&path).unwrap();
    let hub = Arc::new(StateHub::new(1024));
    let feed = UnixSocketFeed::new(ProjectId::Project0, "Project 0", path.clone(), 1024);
    let handle = tokio::spawn(async move { 
        use darkbot_gateway::engine::EngineFeed;
        feed.attach().await
    });
    let (mut stream, _) = listener.accept().await.unwrap();
    send(&mut stream, hello("wrong-schema")).await;
    assert!(handle.await.unwrap().is_err());
    assert!(hub.current(TopicId::Project0Status).is_none());
    drop(stream);
    drop(listener);
    std::fs::remove_file(path).unwrap();
}
