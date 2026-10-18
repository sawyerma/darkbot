// R10/R11/R1: requested stop vs real terminal failure, through the production `run()` path
// (own current-thread runtime and its drop). Loopback sockets only, never Mainnet.

/// One source on its own thread; the stop is requested after `stop_after` (or before the
/// start). Returns the result, the time from the stop request to the thread's end, and every
/// queued event.
fn r10_run(
    cfg: WsRpcConfig,
    stop_after: Option<Duration>,
) -> (Result<(), IngressError>, Duration, Vec<IngressEvent>) {
    let (tx, rx) = darkbot_common::channel::bounded("r10", 1024);
    let stop = Shutdown::new();
    if stop_after.is_none() {
        stop.request();
    }
    let mut src = WsRpcControlSource::new(cfg, Clock::new());
    src.provider_budget = std::sync::Arc::new(crate::provider::Budget::default());
    let sd = stop.clone();
    let thread = std::thread::spawn(move || src.run(tx, sd, IngressHalt::new(), SlotWatch::new()));
    if let Some(d) = stop_after {
        std::thread::sleep(d);
    }
    let requested = std::time::Instant::now();
    stop.request();
    while !thread.is_finished() {
        assert!(
            requested.elapsed() < Duration::from_secs(10),
            "source not quiescent 10 s after the stop request"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let ended = requested.elapsed();
    let result = thread.join().expect("ingress thread panicked");
    let mut events = Vec::new();
    while let Some(e) = rx.try_pop() {
        events.push(e);
    }
    (result, ended, events)
}

fn r10_cfg(url: String) -> WsRpcConfig {
    let mut cfg = WsRpcConfig::new(
        url,
        Subscription {
            slots: true,
            ..Default::default()
        },
    );
    cfg.idle_timeout = Duration::from_secs(30);
    cfg.heartbeat_interval = None;
    cfg
}

/// A loopback port nothing listens on (connection refused).
fn r10_closed_port() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", l.local_addr().unwrap());
    drop(l);
    url
}

fn r10_fatals(events: &[IngressEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, IngressEvent::Fatal { .. }))
        .count()
}

/// R10/R11/R1: a requested stop ends the source with `Ok(())` and never emits `Fatal` in any
/// session phase (before start, reconnect backoff, hanging TCP/WS handshake, waiting for
/// subscription ACKs on a real fake WebSocket server); the stop latency is bounded far below
/// the shutdown budgets (Shadow 180 s default, market drain 120 s). Only a real recovery
/// exhaustion without a stop is `Err` with exactly one `Fatal("WSS_RECOVERY_EXHAUSTED")`.
#[test]
fn requested_stop_is_ok_without_fatal_in_every_session_phase() {
    let mut latencies = Vec::new();

    // Stop before the source starts.
    let (r, ended, ev) = r10_run(r10_cfg(r10_closed_port()), None);
    assert!(r.is_ok() && r10_fatals(&ev) == 0, "before start: {r:?}");
    latencies.push(("before_start", ended));

    // Stop during the reconnect backoff after a refused connect (with a recovery budget, so
    // the exhaustion path is armed but not yet reached).
    let mut cfg = r10_cfg(r10_closed_port());
    cfg.reconnect_initial = Duration::from_secs(20);
    cfg.reconnect_max = Duration::from_secs(20);
    cfg.recovery_budget = Some(Duration::from_secs(120));
    cfg.recovery_attempts = 8;
    let (r, ended, ev) = r10_run(cfg, Some(Duration::from_millis(300)));
    assert!(r.is_ok() && r10_fatals(&ev) == 0, "backoff: {r:?}");
    assert!(ev.iter().any(|e| matches!(e, IngressEvent::Gap { .. })), "a refused connect is a Gap");
    latencies.push(("reconnect_backoff", ended));

    // Stop while the TCP connection is accepted but the WebSocket handshake never answers.
    let hang = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", hang.local_addr().unwrap());
    let held = std::thread::spawn(move || {
        let mut kept = Vec::new();
        if let Ok((s, _)) = hang.accept() {
            kept.push(s);
        }
        std::thread::sleep(Duration::from_secs(3));
        drop(kept);
    });
    let (r, ended, ev) = r10_run(r10_cfg(url), Some(Duration::from_millis(300)));
    assert!(r.is_ok() && r10_fatals(&ev) == 0, "handshake: {r:?}");
    latencies.push(("ws_handshake", ended));
    held.join().unwrap();

    // Stop while the subscription ACKs are awaited on a real (fake) WebSocket server.
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std_listener.set_nonblocking(true).unwrap();
    let url = format!("ws://{}", std_listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (s, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
            // Read the subscribe requests, never acknowledge them.
            let _ = tokio::time::timeout(Duration::from_secs(3), async {
                while let Some(Ok(_)) = ws.next().await {}
            })
            .await;
        });
    });
    let (r, ended, ev) = r10_run(r10_cfg(url), Some(Duration::from_millis(500)));
    assert!(r.is_ok() && r10_fatals(&ev) == 0, "ack wait: {r:?}");
    assert!(
        !ev.iter().any(|e| matches!(e, IngressEvent::Reconnected { .. })),
        "no StreamReady without ACKs"
    );
    latencies.push(("subscription_ack_wait", ended));
    server.join().unwrap();

    for (phase, ended) in &latencies {
        println!("R10_STOP_LATENCY phase={phase} ms={}", ended.as_millis());
        assert!(*ended < Duration::from_secs(2), "{phase}: {ended:?}");
    }

    // A real recovery exhaustion without any stop: Err and exactly one Fatal.
    let mut cfg = r10_cfg(r10_closed_port());
    cfg.reconnect_initial = Duration::from_millis(1);
    cfg.reconnect_max = Duration::from_millis(1);
    cfg.recovery_budget = Some(Duration::from_secs(5));
    cfg.recovery_attempts = 2;
    let (tx, rx) = darkbot_common::channel::bounded("r10-fatal", 1024);
    let mut src = WsRpcControlSource::new(cfg, Clock::new());
    src.provider_budget = std::sync::Arc::new(crate::provider::Budget::default());
    let r = src.run(tx, Shutdown::new(), IngressHalt::new(), SlotWatch::new());
    assert!(
        matches!(&r, Err(IngressError::Connect(c)) if c == "WSS_RECOVERY_EXHAUSTED"),
        "{r:?}"
    );
    let mut ev = Vec::new();
    while let Some(e) = rx.try_pop() {
        ev.push(e);
    }
    assert_eq!(r10_fatals(&ev), 1);
    assert!(ev.iter().any(
        |e| matches!(e, IngressEvent::Fatal { detail, .. } if detail == "WSS_RECOVERY_EXHAUSTED")
    ));
}
