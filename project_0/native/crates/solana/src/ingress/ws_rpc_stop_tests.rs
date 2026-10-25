// R10/R11/R1: requested stop vs real terminal failure, through the production `run()` path
// (own current-thread runtime and its drop). Loopback sockets only, never Mainnet.

/// One source on its own thread; the stop is requested after `stop_after` (or before the
/// start). Returns the result, the time from the stop request to the thread's end, and every
/// queued event.
fn r10_run(
    cfg: WsRpcConfig,
    stop_after: Option<Duration>,
) -> (Result<(), IngressError>, Duration, Vec<IngressEvent>) {
    let started = std::time::Instant::now();
    match stop_after {
        None => r10_run_until(cfg, None),
        Some(d) => r10_run_until(cfg, Some(&move || started.elapsed() >= d)),
    }
}

/// As [`r10_run`], with the stop requested once `stop_when` holds (bounded wait).
fn r10_run_until(
    cfg: WsRpcConfig,
    stop_when: Option<&dyn Fn() -> bool>,
) -> (Result<(), IngressError>, Duration, Vec<IngressEvent>) {
    let (tx, rx) = darkbot_common::channel::bounded("r10", 1024);
    let stop = Shutdown::new();
    if stop_when.is_none() {
        stop.request();
    }
    let mut src = WsRpcControlSource::new(cfg, Clock::new());
    src.provider_budget = std::sync::Arc::new(crate::provider::Budget::default());
    let sd = stop.clone();
    let thread = std::thread::spawn(move || src.run(tx, sd, IngressHalt::new(), SlotWatch::new()));
    if let Some(when) = stop_when {
        let waiting = std::time::Instant::now();
        while !when() {
            assert!(
                waiting.elapsed() < Duration::from_secs(30),
                "stop condition not reached within 30 s"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
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

/// R1 rest: a requested stop stays bounded, `Ok` and without `Fatal` when the peer stops reading
/// (the source's Pong replies to a Ping flood are blocked, its reader keeps reading: V2-14 P3-04)
/// and while a TLS handshake stalls (the peer reads the ClientHello of the
/// production rustls connector and never answers). Loopback only, never Mainnet. An established
/// TLS session cannot be built offline (the connector trusts only the webpki roots); no stop path
/// awaits a WebSocket close or TLS shutdown — the session future is dropped.
#[test]
fn requested_stop_is_bounded_when_the_peer_stops_reading_or_tls_stalls() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let mut latencies = Vec::new();

    // V2-14 P3-04: a peer that acknowledges the subscription, then floods Pings and never reads
    // again. The source's Pong writes are blocked (more Pong bytes, 131 each, than its autotuned
    // send buffer of at most 4 MiB and the peer's 4 KiB receive buffer hold), its reader is not:
    // every Ping is taken without a stalled send of the peer.
    const FLOOD_PINGS: usize = 48 * 1024;
    let (flooded, done) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    let release = Shutdown::new();
    let (addr_tx, addr_rx) = std::sync::mpsc::channel();
    let (flag, finished, hold) = (flooded.clone(), done.clone(), release.clone());
    let server = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let socket = tokio::net::TcpSocket::new_v4().unwrap();
            // Small buffers of the accepted socket: the client's writes are blocked quickly.
            socket.set_recv_buffer_size(4096).unwrap();
            socket.set_send_buffer_size(4096).unwrap();
            socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
            let listener = socket.listen(4).unwrap();
            addr_tx.send(listener.local_addr().unwrap()).unwrap();
            let limit = Duration::from_secs(20);
            let (s, _) = tokio::time::timeout(limit, listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
            let request = tokio::time::timeout(limit, ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let id = serde_json::from_str::<Value>(request.to_text().unwrap()).unwrap()["id"].clone();
            ws.send(Message::Text(
                json!({"jsonrpc": "2.0", "result": 7, "id": id}).to_string(),
            ))
            .await
            .unwrap();
            let until = tokio::time::Instant::now() + limit;
            let mut sent = 0usize;
            while sent < FLOOD_PINGS && tokio::time::Instant::now() < until {
                let ping = ws.send(Message::Ping(vec![0x5A; 125]));
                match tokio::time::timeout(Duration::from_millis(500), ping).await {
                    Ok(Ok(())) => sent += 1,
                    // A stalled send: the client stopped reading.
                    _ => break,
                }
            }
            flag.store(sent == FLOOD_PINGS, Ordering::Release);
            finished.store(true, Ordering::Release);
            // Keep the connection open (and unread) until the test is done with it.
            while !hold.is_requested() && tokio::time::Instant::now() < until {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
    });
    let addr = addr_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = done.clone();
    let (r, ended, ev) = r10_run_until(
        r10_cfg(format!("ws://{addr}")),
        Some(&move || ready.load(Ordering::Acquire)),
    );
    release.request();
    server.join().unwrap();
    assert!(
        flooded.load(Ordering::Acquire),
        "the reader kept reading while its Pong writes were blocked"
    );
    assert!(r.is_ok() && r10_fatals(&ev) == 0, "peer stops reading: {r:?}");
    assert!(
        ev.iter().any(|e| matches!(e, IngressEvent::Reconnected { .. })),
        "the stream was ready before the peer stopped reading"
    );
    assert!(
        !ev.iter().any(|e| matches!(e, IngressEvent::Gap { .. })),
        "the stop hit the flooded session, not a reconnect"
    );
    latencies.push(("peer_stops_reading", ended));

    // TLS handshake that stalls after the ClientHello.
    let tls = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("wss://{}", tls.local_addr().unwrap());
    let hello = Arc::new(AtomicBool::new(false));
    let (seen, hold) = (hello.clone(), Shutdown::new());
    let held = hold.clone();
    let peer = std::thread::spawn(move || {
        use std::io::Read;
        tls.set_nonblocking(true).unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(20);
        let mut conn = loop {
            match tls.accept() {
                Ok((c, _)) => break c,
                Err(_) if std::time::Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(e) => panic!("no TLS client: {e}"),
            }
        };
        conn.set_nonblocking(false).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut first = [0u8; 1];
        // 0x16: a TLS handshake record (the ClientHello).
        if conn.read_exact(&mut first).is_ok() && first[0] == 0x16 {
            seen.store(true, Ordering::Release);
        }
        while !held.is_requested() && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(conn);
    });
    let seen = hello.clone();
    let (r, ended, ev) = r10_run_until(
        r10_cfg(url),
        Some(&move || seen.load(Ordering::Acquire)),
    );
    hold.request();
    peer.join().unwrap();
    assert!(hello.load(Ordering::Acquire), "the ClientHello reached the peer");
    assert!(r.is_ok() && r10_fatals(&ev) == 0, "TLS stall: {r:?}");
    latencies.push(("tls_handshake_stall", ended));

    for (phase, ended) in &latencies {
        println!("R1_STOP_LATENCY phase={phase} ms={}", ended.as_millis());
        assert!(*ended < Duration::from_secs(2), "{phase}: {ended:?}");
    }
}

/// V2-14 P3-04 loopback proof: the source answers a Ping with the identical Pong although it
/// writes nothing itself (no heartbeat) — tungstenite queues the Pong and the next read poll
/// flushes it. Loopback only, never Mainnet.
#[test]
fn p3_04_a_ping_is_answered_by_the_read_poll() {
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::Arc;
    // 0: pending, 1: the identical Pong, 2: anything else or nothing within the bound.
    let answer = Arc::new(AtomicU8::new(0));
    let (addr_tx, addr_rx) = std::sync::mpsc::channel();
    let (seen, release) = (answer.clone(), Shutdown::new());
    let hold = release.clone();
    let server = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            addr_tx.send(listener.local_addr().unwrap()).unwrap();
            let limit = Duration::from_secs(10);
            let (s, _) = tokio::time::timeout(limit, listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
            let request = tokio::time::timeout(limit, ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let id = serde_json::from_str::<Value>(request.to_text().unwrap()).unwrap()["id"].clone();
            ws.send(Message::Text(
                json!({"jsonrpc": "2.0", "result": 7, "id": id}).to_string(),
            ))
            .await
            .unwrap();
            ws.send(Message::Ping(b"p3-04".to_vec())).await.unwrap();
            let next = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;
            let identical = matches!(&next, Ok(Some(Ok(Message::Pong(p)))) if p == b"p3-04");
            seen.store(if identical { 1 } else { 2 }, Ordering::Release);
            while !hold.is_requested() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
    });
    let addr = addr_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready = answer.clone();
    let (r, _, ev) = r10_run_until(
        r10_cfg(format!("ws://{addr}")),
        Some(&move || ready.load(Ordering::Acquire) != 0),
    );
    release.request();
    server.join().unwrap();
    assert_eq!(answer.load(Ordering::Acquire), 1, "the identical Pong reached the peer");
    assert!(r.is_ok() && r10_fatals(&ev) == 0, "{r:?}");
}

/// Z19: a silent stream (no frame after the last application frame, idle timeout not reached)
/// keeps emitting the periodic health snapshot: the frame age rises in the event stream while
/// the frame count stays, and the silence ends neither the session nor emits Gap/Fatal.
/// Loopback only, never Mainnet.
#[test]
fn z19_silent_stream_keeps_reporting_a_rising_frame_age() {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std_listener.set_nonblocking(true).unwrap();
    let url = format!("ws://{}", std_listener.local_addr().unwrap());
    let release = Shutdown::new();
    let hold = release.clone();
    let server = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (s, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
            let request = ws.next().await.unwrap().unwrap();
            let id = serde_json::from_str::<Value>(request.to_text().unwrap()).unwrap()["id"].clone();
            ws.send(Message::Text(
                json!({"jsonrpc": "2.0", "result": 7, "id": id}).to_string(),
            ))
            .await
            .unwrap();
            ws.send(Message::Text(
                json!({"jsonrpc": "2.0", "method": "slotNotification", "params": {
                    "subscription": 7, "result": {"slot": 100, "parent": 99, "root": 98}}})
                .to_string(),
            ))
            .await
            .unwrap();
            // Silence: the connection stays open without a frame until the test is done.
            let until = tokio::time::Instant::now() + Duration::from_secs(20);
            while !hold.is_requested() && tokio::time::Instant::now() < until {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
    });
    let mut cfg = r10_cfg(url);
    cfg.health_interval = Duration::from_millis(200);
    let started = std::time::Instant::now();
    let (r, _, ev) = r10_run_until(
        cfg,
        Some(&move || started.elapsed() >= Duration::from_millis(1500)),
    );
    release.request();
    server.join().unwrap();
    assert!(r.is_ok() && r10_fatals(&ev) == 0, "{r:?}");
    assert!(!ev.iter().any(|e| matches!(e, IngressEvent::Gap { .. })));
    assert!(ev.iter().any(|e| matches!(e, IngressEvent::Reconnected { .. })));
    let snapshots: Vec<(u64, u64)> = ev
        .iter()
        .filter_map(|e| match e {
            IngressEvent::Health(h) if h.connected => h.last_frame_age_ns.map(|a| (a, h.frames)),
            _ => None,
        })
        .collect();
    // ~1.4 s of silence after the one frame at a 200 ms interval (250 ms wake-ups).
    assert!(snapshots.len() >= 3, "{snapshots:?}");
    assert!(snapshots.iter().all(|&(_, frames)| frames == 1), "{snapshots:?}");
    assert!(snapshots.windows(2).all(|w| w[1].0 > w[0].0), "{snapshots:?}");
    assert!(snapshots.last().unwrap().0 >= 800_000_000, "{snapshots:?}");
}
