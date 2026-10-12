// Loopback sockets only: these tests run the real adapter, never Mainnet.
fn p2_source(url: String, budget: Duration) -> WsRpcControlSource {
    let mut cfg = WsRpcConfig::new(
        url,
        Subscription {
            slots: true,
            ..Default::default()
        },
    );
    cfg.idle_timeout = Duration::from_millis(180);
    cfg.heartbeat_interval = Some(Duration::from_millis(30));
    cfg.reconnect_initial = Duration::from_millis(5);
    cfg.reconnect_max = Duration::from_millis(10);
    cfg.recovery_budget = Some(budget);
    cfg.recovery_attempts = 3;
    let mut src = WsRpcControlSource::new(cfg, Clock::new());
    src.provider_budget = std::sync::Arc::new(crate::provider::Budget::default());
    src
}
fn p2_slot(slot: u64) -> Message {
    Message::Text(json!({"jsonrpc":"2.0","method":"slotNotification","params":{"subscription":1,"result":{"slot":slot,"parent":slot-1,"root":slot-2}}}).to_string())
}
async fn p2_ack(ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) {
    ws.next().await.unwrap().unwrap();
    ws.send(Message::Text(
        json!({"jsonrpc":"2.0","id":1,"result":1}).to_string(),
    ))
    .await
    .unwrap();
}
#[tokio::test]
async fn p2_429_retry_after_exhausts_budget_without_spam_or_secret_leak() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let src = p2_source(
        format!("ws://{}", listener.local_addr().unwrap()),
        Duration::from_millis(500),
    );
    let server = tokio::spawn(async move {
        let (s, _) = listener.accept().await.unwrap();
        let result = tokio_tungstenite::accept_hdr_async(
            s,
            |_req: &tokio_tungstenite::tungstenite::handshake::server::Request, _resp| {
                Err(tokio_tungstenite::tungstenite::http::Response::builder()
                    .status(429)
                    .header("retry-after", "86400")
                    .body(Some("PRIVATE_PROVIDER_BODY".into()))
                    .unwrap())
            },
        )
        .await;
        assert!(result.is_err());
    });
    let (tx, rx) = darkbot_common::channel::bounded("test", 100);
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        src.run_async(tx, Shutdown::new(), IngressHalt::new(), SlotWatch::new()),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    server.await.unwrap();
    let mut events = Vec::new();
    while let Some(e) = rx.try_pop() {
        events.push(e);
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, IngressEvent::Gap { .. }))
            .count(),
        1
    );
    assert!(events.iter().any(|e| matches!(
        e,
        IngressEvent::Transport {
            code: "WSS_HTTP_429",
            ..
        }
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        IngressEvent::Transport {
            state: "INCOMPLETE_ABORT",
            ..
        }
    )));
    assert!(!format!("{events:?}").contains("PRIVATE_PROVIDER_BODY"));
}
#[tokio::test]
async fn p2_acks_and_ping_pong_do_not_start_capture() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let src = p2_source(
        format!("ws://{}", listener.local_addr().unwrap()),
        Duration::from_millis(250),
    );
    let server = tokio::spawn(async move {
        let (s, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
        p2_ack(&mut ws).await;
        loop {
            if ws.send(Message::Ping(vec![1])).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    });
    let (tx, rx) = darkbot_common::channel::bounded("test", 100);
    assert!(tokio::time::timeout(
        Duration::from_secs(2),
        src.run_async(tx, Shutdown::new(), IngressHalt::new(), SlotWatch::new())
    )
    .await
    .unwrap()
    .is_err());
    server.abort();
    let mut events = Vec::new();
    while let Some(e) = rx.try_pop() {
        events.push(e);
    }
    assert!(events
        .iter()
        .any(|e| matches!(e, IngressEvent::Transport { state: "READY", .. })));
    assert!(!events
        .iter()
        .any(|e| matches!(e, IngressEvent::Reconnected { .. })));
}
#[tokio::test]
async fn p2_stale_slot_frames_do_not_keep_capture_alive() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let src = p2_source(
        format!("ws://{}", listener.local_addr().unwrap()),
        Duration::from_millis(300),
    );
    let server = tokio::spawn(async move {
        let (s, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
        p2_ack(&mut ws).await;
        loop {
            if ws.send(p2_slot(10)).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    });
    let (tx, rx) = darkbot_common::channel::bounded("test", 100);
    assert!(tokio::time::timeout(
        Duration::from_secs(2),
        src.run_async(tx, Shutdown::new(), IngressHalt::new(), SlotWatch::new())
    )
    .await
    .unwrap()
    .is_err());
    server.abort();
    let mut events = Vec::new();
    while let Some(e) = rx.try_pop() {
        events.push(e);
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, IngressEvent::Reconnected { .. }))
            .count(),
        1
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, IngressEvent::Fatal { .. })));
}
#[tokio::test]
async fn p2_disconnect_recovers_only_after_new_acked_slot_progress_and_shutdown_is_prompt() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let src = p2_source(
        format!("ws://{}", listener.local_addr().unwrap()),
        Duration::from_secs(4),
    );
    let server = tokio::spawn(async move {
        for slot in [10, 11] {
            let (s, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
            p2_ack(&mut ws).await;
            ws.send(p2_slot(slot)).await.unwrap();
            if slot == 11 {
                while let Some(Ok(_)) = ws.next().await {}
            }
        }
    });
    let (tx, rx) = darkbot_common::channel::bounded("test", 100);
    let shutdown = Shutdown::new();
    let sd = shutdown.clone();
    let source = tokio::spawn(async move {
        src.run_async(tx, sd, IngressHalt::new(), SlotWatch::new())
            .await
    });
    let mut events = Vec::new();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let Some(e) = rx.try_pop() {
                let done = matches!(
                    e,
                    IngressEvent::Transport {
                        state: "RECOVERED",
                        last_slot: Some(11),
                        ..
                    }
                );
                events.push(e);
                if done {
                    break;
                }
            } else {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    })
    .await
    .unwrap();
    shutdown.request();
    assert!(tokio::time::timeout(Duration::from_millis(500), source)
        .await
        .unwrap()
        .unwrap()
        .is_ok());
    server.abort();
    while let Some(e) = rx.try_pop() {
        events.push(e);
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, IngressEvent::Reconnected { .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, IngressEvent::Gap { .. }))
            .count(),
        1
    );
    assert!(events
        .iter()
        .any(|e| matches!(e,IngressEvent::Slot(s) if s.slot==11)));
}

#[tokio::test]
async fn p2_shutdown_cancels_admission_wait_without_background_runtime_hang() {
    let src = p2_source("ws://127.0.0.1:1".into(), Duration::from_secs(120));
    src.provider_budget.rate_limited(Duration::from_secs(60));
    let (tx, _rx) = darkbot_common::channel::bounded("test", 100);
    let shutdown = Shutdown::new();
    let sd = shutdown.clone();
    let source = tokio::spawn(async move {
        src.run_async(tx, sd, IngressHalt::new(), SlotWatch::new())
            .await
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    shutdown.request();
    assert!(tokio::time::timeout(Duration::from_millis(500), source)
        .await
        .unwrap()
        .unwrap()
        .is_ok());
}
