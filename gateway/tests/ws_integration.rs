//! F3 integration tests: real TCP/WebSocket, no engine data fabrication.
use bytes::Bytes;
use darkbot_gateway::{
    config::{GatewayConfig, Limits},
    engine::SourceRegistry,
    http::{self, AppState},
    hub::{StateHub, StateUpdate},
    proto::{
        client_message, server_message, AckStatus, ClientMessage, Hello, ServerMessage,
        StateQuality, Subscribe, TopicId, WelcomeStatus,
    },
    PROTOCOL_MAJOR, PROTOCOL_MINOR, SCHEMA_ID,
};
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, Error as WsError, Message},
    MaybeTlsStream, WebSocketStream,
};
type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
struct TestServer {
    addr: SocketAddr,
    state: Arc<AppState>,
    hub: Arc<StateHub>,
}
impl TestServer {
    fn host(&self) -> String {
        format!("127.0.0.1:{}", self.addr.port())
    }
    fn origin(&self) -> String {
        format!("http://{}", self.host())
    }
    fn url(&self) -> String {
        format!("ws://{}/ws/v1/gateway", self.host())
    }
}
async fn spawn(mut limits: Limits) -> TestServer {
    limits.handshake_timeout_ms = 350;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cfg = GatewayConfig {
        bind: addr,
        static_dir: None,
        allowed_origins: vec![format!("http://{addr}")],
        allowed_hosts: vec![addr.to_string()],
        session_cookie: "darkbot_session".into(),
        limits,
        source_stale_after_ms: 1000,
        source_detach_after_ms: 2000,
        source_reattach_backoff_ms: 50,
        overview_interval_ms: 60000,
        feeds: vec![],
    };
    let hub = Arc::new(StateHub::new(1024));
    let registry = SourceRegistry::new();
    let state = AppState::new(cfg, hub.clone(), registry);
    let server_state = Arc::clone(&state);
    tokio::spawn(async move {
        axum::serve(listener, http::router(server_state))
            .await
            .expect("test server");
    });
    TestServer { addr, state, hub }
}
async fn try_connect(
    s: &TestServer,
    cookie: bool,
    origin: Option<&str>,
    host: Option<&str>,
) -> Result<Ws, Box<WsError>> {
    let mut req = s.url().into_client_request().unwrap();
    req.headers_mut()
        .insert("origin", origin.unwrap_or(&s.origin()).parse().unwrap());
    if let Some(host) = host {
        req.headers_mut().insert("host", host.parse().unwrap());
    }
    if cookie {
        let token = s.state.sessions.issue();
        req.headers_mut().insert(
            "cookie",
            format!("darkbot_session={token}").parse().unwrap(),
        );
    }
    // Boxed: the handshake error is large (clippy::result_large_err on current stable).
    tokio_tungstenite::connect_async(req)
        .await
        .map(|(ws, _)| ws)
        .map_err(Box::new)
}
async fn connect(s: &TestServer) -> Ws {
    try_connect(s, true, None, None).await.unwrap()
}
async fn send(ws: &mut Ws, m: client_message::Msg) {
    ws.send(Message::Binary(
        ClientMessage { msg: Some(m) }.encode_to_vec(),
    ))
    .await
    .unwrap();
}
async fn recv(ws: &mut Ws) -> Option<server_message::Msg> {
    loop {
        let item = tokio::time::timeout(Duration::from_secs(3), ws.next())
            .await
            .expect("receive timeout")?;
        match item {
            Ok(Message::Binary(b)) => return ServerMessage::decode(b.as_slice()).unwrap().msg,
            Ok(Message::Close(_)) | Err(_) => return None,
            _ => {}
        }
    }
}
async fn handshake(ws: &mut Ws) -> darkbot_gateway::proto::Welcome {
    send(
        ws,
        client_message::Msg::Hello(Hello {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            schema_id: SCHEMA_ID.into(),
            capabilities: None,
        }),
    )
    .await;
    match recv(ws).await {
        Some(server_message::Msg::Welcome(w)) => w,
        other => panic!("expected Welcome: {other:?}"),
    }
}
async fn subscribe(ws: &mut Ws) -> darkbot_gateway::proto::ControlAck {
    send(
        ws,
        client_message::Msg::Subscribe(Subscribe {
            request_id: 1,
            topic: TopicId::OverviewAggregates as i32,
            cursor: None,
            start_policy: 0,
        }),
    )
    .await;
    match recv(ws).await {
        Some(server_message::Msg::ControlAck(a)) => a,
        other => panic!("expected Ack: {other:?}"),
    }
}
async fn state(ws: &mut Ws) -> darkbot_gateway::proto::StateFull {
    loop {
        match recv(ws).await {
            Some(server_message::Msg::StateFull(s)) => return s,
            Some(server_message::Msg::Heartbeat(h)) => {
                send(
                    ws,
                    client_message::Msg::Pong(darkbot_gateway::proto::Pong {
                        heartbeat_id: h.heartbeat_id,
                    }),
                )
                .await
            }
            other => panic!("expected state: {other:?}"),
        }
    }
}
fn update(g: u64, s: u64) -> StateUpdate {
    StateUpdate {
        topic: TopicId::OverviewAggregates,
        generation: g,
        seq: s,
        source_timestamp_unix_ns: 1,
        source_id: Arc::from("TEST_ONLY"),
        payload: Bytes::from_static(b"\x08\x01"),
    }
}
#[tokio::test]
async fn accepted_handshake_and_topics() {
    let srv = spawn(Limits::default()).await;
    let mut ws = connect(&srv).await;
    let w = handshake(&mut ws).await;
    assert_eq!(w.status, WelcomeStatus::Accepted as i32);
    assert!(w
        .topics
        .iter()
        .any(|x| x.topic == TopicId::OverviewAggregates as i32 && x.available));
    // BSC status is a read-only feed since 9a43fb05; Solana has no payload yet.
    assert!(w
        .topics
        .iter()
        .any(|x| x.topic == TopicId::BscStatus as i32 && x.available));
    assert!(w
        .topics
        .iter()
        .any(|x| x.topic == TopicId::SolanaStatus as i32 && !x.available));
}
#[tokio::test]
async fn incompatible_major_rejected_and_compatible_minor_schema_accepted() {
    let s = spawn(Limits::default()).await;
    // Same major, other schema fingerprint (newer/older minor): diagnostic only, never blocks.
    for (major, schema, expected) in [
        (PROTOCOL_MAJOR + 1, SCHEMA_ID, WelcomeStatus::RejectedMajor),
        (
            PROTOCOL_MAJOR,
            "fingerprint-of-another-minor",
            WelcomeStatus::Accepted,
        ),
    ] {
        let mut ws = connect(&s).await;
        send(
            &mut ws,
            client_message::Msg::Hello(Hello {
                protocol_major: major,
                protocol_minor: PROTOCOL_MINOR,
                schema_id: schema.into(),
                capabilities: None,
            }),
        )
        .await;
        match recv(&mut ws).await {
            Some(server_message::Msg::Welcome(w)) => assert_eq!(w.status, expected as i32),
            other => panic!("{other:?}"),
        }
    }
}
#[tokio::test]
async fn cookie_origin_host_guards() {
    let s = spawn(Limits::default()).await;
    for (cookie, origin, host, expected) in [
        (false, None, None, 401),
        (true, Some("http://evil.example"), None, 403),
        (true, None, Some("evil.example"), 403),
    ] {
        let err = try_connect(&s, cookie, origin, host)
            .await
            .expect_err("must reject");
        match *err {
            WsError::Http(res) => assert_eq!(res.status().as_u16(), expected),
            other => panic!("{other:?}"),
        }
    }
}
#[tokio::test]
async fn session_bootstrap_and_cross_site_rejection() {
    let s = spawn(Limits::default()).await;
    for (site, status) in [("same-origin", 204), ("cross-site", 403)] {
        let mut conn = TcpStream::connect(s.addr).await.unwrap();
        conn.write_all(format!("GET /api/v1/session HTTP/1.1\r\nHost: {}\r\nSec-Fetch-Site: {site}\r\nConnection: close\r\n\r\n",s.host()).as_bytes()).await.unwrap();
        let mut b = Vec::new();
        conn.read_to_end(&mut b).await.unwrap();
        let r = String::from_utf8_lossy(&b);
        assert!(r.starts_with(&format!("HTTP/1.1 {status}")), "{r}");
        if status == 204 {
            assert!(r.to_ascii_lowercase().contains("httponly"));
            assert!(r.contains("SameSite=Strict"));
        }
    }
}
#[tokio::test]
async fn subscription_and_latest_generation() {
    let s = spawn(Limits::default()).await;
    s.hub.publish(update(1, 1)).unwrap();
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    assert_eq!(subscribe(&mut ws).await.status, AckStatus::Accepted as i32);
    assert_eq!((state(&mut ws).await.generation, state_seq(&s)), (1, 1));
    s.hub.publish(update(1, 2)).unwrap();
    assert_eq!(state(&mut ws).await.seq, 2);
    s.hub.publish(update(0, 99)).unwrap();
    s.hub.publish(update(2, 1)).unwrap();
    let v = state(&mut ws).await;
    assert_eq!((v.generation, v.seq), (2, 1));
}
fn state_seq(s: &TestServer) -> u64 {
    s.hub.current(TopicId::OverviewAggregates).unwrap().seq
}
#[tokio::test]
async fn unavailable_topic_rejected() {
    let s = spawn(Limits::default()).await;
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    send(
        &mut ws,
        client_message::Msg::Subscribe(Subscribe {
            request_id: 9,
            topic: TopicId::SolanaStatus as i32,
            cursor: None,
            start_policy: 0,
        }),
    )
    .await;
    match recv(&mut ws).await {
        Some(server_message::Msg::ControlAck(a)) => {
            assert_eq!(a.status, AckStatus::Rejected as i32)
        }
        other => panic!("{other:?}"),
    }
}
#[tokio::test]
async fn coalescing_at_watch_boundary() {
    let (tx, rx) = tokio::sync::watch::channel(0u64);
    let mut stream = tokio_stream::wrappers::WatchStream::new(rx);
    assert_eq!(stream.next().await, Some(0));
    for i in 1..=1000 {
        tx.send(i).unwrap();
    }
    assert_eq!(stream.next().await, Some(1000));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), stream.next())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn detached_overlay_persists_for_new_subscriber() {
    let s = spawn(Limits::default()).await;
    s.hub.publish(update(5, 7)).unwrap();
    s.hub
        .mark_detached(darkbot_gateway::proto::ProjectId::Unspecified);
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    subscribe(&mut ws).await;
    let frame = state(&mut ws).await;
    assert_eq!(frame.quality, StateQuality::SourceDetached as i32);
}
#[tokio::test]
async fn reconnect_gets_latest() {
    let s = spawn(Limits::default()).await;
    s.hub.publish(update(1, 2)).unwrap();
    let mut a = connect(&s).await;
    handshake(&mut a).await;
    subscribe(&mut a).await;
    assert_eq!(state(&mut a).await.seq, 2);
    a.close(None).await.unwrap();
    s.hub.publish(update(1, 3)).unwrap();
    let mut b = connect(&s).await;
    handshake(&mut b).await;
    subscribe(&mut b).await;
    assert_eq!(state(&mut b).await.seq, 3);
}
#[tokio::test]
async fn detached_is_pushed_to_an_existing_subscriber_with_the_same_generation_and_seq() {
    let s = spawn(Limits::default()).await;
    s.hub.publish(update(4, 9)).unwrap();
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    subscribe(&mut ws).await;
    assert_eq!(state(&mut ws).await.quality, StateQuality::Ok as i32);
    s.hub
        .mark_detached(darkbot_gateway::proto::ProjectId::Unspecified);
    let pushed = state(&mut ws).await;
    assert_eq!(
        (pushed.quality, pushed.generation, pushed.seq),
        (StateQuality::SourceDetached as i32, 4, 9)
    );
}
#[tokio::test]
async fn an_oversized_frame_from_the_client_closes_the_connection() {
    let s = spawn(Limits::default()).await;
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    let _ = ws.send(Message::Binary(vec![0u8; 5000])).await;
    assert!(recv(&mut ws).await.is_none(), "connection must close");
}
#[tokio::test]
async fn a_client_that_never_answers_heartbeats_is_closed() {
    let s = spawn(Limits {
        heartbeat_interval_ms: 100,
        stale_after_missed_heartbeats: 2,
        ..Limits::default()
    })
    .await;
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    let closed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Close(frame))) => return frame.map(|f| u16::from(f.code)),
                None | Some(Err(_)) => return None,
                _ => {}
            }
        }
    })
    .await
    .expect("pong timeout must close the connection");
    assert_eq!(closed, Some(1001));
}
#[tokio::test]
async fn event_topics_and_state_cursors_are_refused() {
    let s = spawn(Limits::default()).await;
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    for (topic, cursor, reason) in [
        (
            TopicId::BscExecutionEvents,
            None,
            darkbot_gateway::proto::AckReason::TopicUnavailable,
        ),
        (
            TopicId::OverviewAggregates,
            Some(darkbot_gateway::proto::EventCursor {
                journal_epoch: "e".into(),
                durable_cursor: 1,
            }),
            darkbot_gateway::proto::AckReason::InvalidCursorForState,
        ),
    ] {
        send(
            &mut ws,
            client_message::Msg::Subscribe(Subscribe {
                request_id: 7,
                topic: topic as i32,
                cursor,
                start_policy: 0,
            }),
        )
        .await;
        match recv(&mut ws).await {
            Some(server_message::Msg::ControlAck(a)) => assert_eq!(
                (a.status, a.reason),
                (AckStatus::Rejected as i32, reason as i32)
            ),
            other => panic!("{other:?}"),
        }
    }
}
#[tokio::test]
async fn an_oversized_state_is_frame_too_large_until_the_next_valid_state_on_the_same_subscription()
{
    let s = spawn(Limits::default()).await;
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    subscribe(&mut ws).await;
    let mut big = update(1, 1);
    big.payload = Bytes::from(vec![8u8; 2048]);
    assert!(s.hub.publish(big).is_err());
    loop {
        match recv(&mut ws).await {
            Some(server_message::Msg::Error(e)) => {
                assert_eq!(
                    e.code,
                    darkbot_gateway::proto::ErrorCode::FrameTooLarge as i32
                );
                assert_eq!(e.topic, TopicId::OverviewAggregates as i32);
                break;
            }
            Some(server_message::Msg::Heartbeat(_)) => {}
            other => panic!("expected FRAME_TOO_LARGE: {other:?}"),
        }
    }
    // Still too large at the next seq: no repeated error. The next valid state arrives on the
    // same subscription, without a new Subscribe (Wire V1.3 §2 recovery).
    let mut still = update(1, 2);
    still.payload = Bytes::from(vec![8u8; 2048]);
    assert!(s.hub.publish(still).is_ok());
    s.hub.publish(update(1, 3)).unwrap();
    loop {
        match recv(&mut ws).await {
            Some(server_message::Msg::StateFull(f)) => {
                assert_eq!(
                    (f.topic, f.generation, f.seq),
                    (TopicId::OverviewAggregates as i32, 1, 3)
                );
                assert_eq!(f.quality, darkbot_gateway::proto::StateQuality::Ok as i32);
                break;
            }
            Some(server_message::Msg::Heartbeat(_)) => {}
            other => panic!("expected the recovered state, not {other:?}"),
        }
    }
}
#[tokio::test]
async fn every_response_carries_the_content_security_policy() {
    let s = spawn(Limits::default()).await;
    let mut conn = TcpStream::connect(s.addr).await.unwrap();
    conn.write_all(
        format!(
            "GET /healthz HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            s.host()
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let mut b = Vec::new();
    conn.read_to_end(&mut b).await.unwrap();
    let r = String::from_utf8_lossy(&b).to_ascii_lowercase();
    assert!(r.contains("content-security-policy:"), "{r}");
    assert!(r.contains("connect-src 'self'"), "{r}");
    assert!(r.contains("frame-ancestors 'none'"), "{r}");
}
#[tokio::test]
async fn a_client_that_stops_reading_is_evicted_and_never_blocks_publishing() {
    let s = spawn(Limits {
        heartbeat_interval_ms: 1500,
        stale_after_missed_heartbeats: 2,
        ..Limits::default()
    })
    .await;
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    subscribe(&mut ws).await;
    // The client never reads again while the server keeps publishing until the kernel buffers
    // are full: only a bounded send can then free the slot (the heartbeat branch is starved).
    let hub = s.hub.clone();
    let state = s.state.clone();
    let publisher = tokio::spawn(async move {
        let mut seq = 0u64;
        while state.clients.load(std::sync::atomic::Ordering::Acquire) != 0 {
            seq += 1;
            let mut u = update(1, seq);
            u.payload = Bytes::from(vec![(seq % 251) as u8; 1000]);
            let _ = hub.publish(u);
            if seq % 16 == 0 {
                tokio::task::yield_now().await;
            }
        }
    });
    let evicted = tokio::time::timeout(Duration::from_secs(20), async {
        while s.state.clients.load(std::sync::atomic::Ordering::Acquire) != 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    publisher.abort();
    assert!(evicted.is_ok(), "stalled client still holds its slot");
    drop(ws);
}
#[tokio::test]
async fn a_reconnecting_browser_gets_frame_too_large_never_the_older_value_then_the_recovery() {
    let s = spawn(Limits::default()).await;
    s.hub.publish(update(1, 1)).unwrap();
    let mut big = update(1, 2);
    big.payload = Bytes::from(vec![8u8; 2048]);
    assert!(s.hub.publish(big).is_err());
    let mut ws = connect(&s).await;
    handshake(&mut ws).await;
    subscribe(&mut ws).await;
    match recv(&mut ws).await {
        Some(server_message::Msg::Error(e)) => assert_eq!(
            e.code,
            darkbot_gateway::proto::ErrorCode::FrameTooLarge as i32
        ),
        other => panic!("expected FRAME_TOO_LARGE, never the older value: {other:?}"),
    }
    s.hub.publish(update(1, 3)).unwrap();
    let f = state(&mut ws).await;
    assert_eq!(
        (f.seq, f.quality),
        (3, darkbot_gateway::proto::StateQuality::Ok as i32)
    );
}
#[tokio::test]
async fn a_stalled_client_on_a_flapping_too_large_topic_is_evicted_while_a_reader_recovers() {
    use std::sync::atomic::Ordering::Acquire;
    let s = spawn(Limits {
        heartbeat_interval_ms: 1500,
        stale_after_missed_heartbeats: 2,
        ..Limits::default()
    })
    .await;
    let mut stalled = connect(&s).await;
    handshake(&mut stalled).await;
    subscribe(&mut stalled).await;
    let mut reader = connect(&s).await;
    handshake(&mut reader).await;
    subscribe(&mut reader).await;
    // Oversized and valid states alternate until the stalled client is gone.
    let hub = s.hub.clone();
    let state_ = s.state.clone();
    let publisher = tokio::spawn(async move {
        let mut seq = 0u64;
        while state_.clients.load(Acquire) > 1 {
            seq += 1;
            let mut u = update(1, seq);
            u.payload = Bytes::from(vec![
                (seq % 251) as u8;
                if seq % 2 == 0 { 2048 } else { 1000 }
            ]);
            let _ = hub.publish(u);
            // Yield after a marker and after a valid state alike: on this single-threaded
            // runtime a reader only ever sees the value published right before a yield.
            if seq % 16 == 0 || seq % 16 == 9 {
                tokio::task::yield_now().await;
            }
        }
    });
    let (mut errors, mut states) = (0, 0);
    let evicted = tokio::time::timeout(Duration::from_secs(20), async {
        while s.state.clients.load(Acquire) > 1 {
            match recv(&mut reader).await {
                Some(server_message::Msg::Error(e)) => {
                    assert_eq!(
                        e.code,
                        darkbot_gateway::proto::ErrorCode::FrameTooLarge as i32
                    );
                    errors += 1;
                }
                Some(server_message::Msg::StateFull(f)) => {
                    assert!(f.payload.len() <= 1024, "never an oversized frame");
                    states += 1;
                }
                Some(server_message::Msg::Heartbeat(h)) => {
                    send(
                        &mut reader,
                        client_message::Msg::Pong(darkbot_gateway::proto::Pong {
                            heartbeat_id: h.heartbeat_id,
                        }),
                    )
                    .await
                }
                other => panic!("reader must stay connected: {other:?}"),
            }
        }
    })
    .await;
    publisher.abort();
    assert!(evicted.is_ok(), "stalled client still holds its slot");
    assert!(errors > 0 && states > 0, "errors {errors}, states {states}");
    // The reader keeps its subscription: a new publisher generation still arrives.
    s.hub.publish(update(2, 1)).unwrap();
    loop {
        let f = state(&mut reader).await;
        if f.generation == 2 {
            break;
        }
    }
    drop(stalled);
}
