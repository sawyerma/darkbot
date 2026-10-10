//! Qualification-only loopback proxy. Closes the first market WebSocket once,
//! after a real slot notification. Does not alter production ingress defaults.
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, connect_async};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        // Only our fixed diagnostic codes may reach stderr, never a library error URL.
        eprintln!("GAP_PROXY_FAILED {error}");
        std::process::exit(2);
    }
}

fn ws_error(error: &tokio_tungstenite::tungstenite::Error) -> String {
    use tokio_tungstenite::tungstenite::Error;
    match error {
        Error::Io(e) => format!("IO kind={:?} os_code={:?}", e.kind(), e.raw_os_error()),
        Error::Http(r) => format!("HTTP status={}", r.status().as_u16()),
        Error::Tls(_) => "TLS".into(),
        _ => "WEBSOCKET_PROTOCOL".into(),
    }
}

async fn probe_session(url: &str, wait_for_drop: bool) -> Result<(), String> {
    use tokio_tungstenite::tungstenite::Message;
    tokio::time::timeout(Duration::from_secs(if wait_for_drop { 150 } else { 20 }), async {
        let started = std::time::Instant::now();
        let (mut ws, _) = connect_async(url).await.map_err(|e| ws_error(&e))?;
        eprintln!("GAP_PROBE_CONNECTED elapsed_ms={}", started.elapsed().as_millis());
        for request in [
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"slotSubscribe"}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"logsSubscribe","params":[{"mentions":["MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA" ]},{"commitment":"finalized"}]}),
        ] {
            ws.send(Message::Text(request.to_string())).await.map_err(|e|ws_error(&e))?;
        }
        let mut ack = [false;2];
        let mut slot = None;
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(t))) => {
                    let v: serde_json::Value=serde_json::from_str(&t).map_err(|_|"INVALID_JSON".to_string())?;
                    if v.get("error").is_some() { return Err("SUBSCRIPTION_REJECTED".into()); }
                    if let Some(id @ 1..=2)=v["id"].as_u64() {
                        if v["result"].as_u64().is_some() { ack[(id-1) as usize]=true; }
                    }
                    if v["method"]=="slotNotification" { slot=v["params"]["result"]["slot"].as_u64(); }
                    if let Some(observed_slot) = slot.filter(|_| ack.iter().all(|a|*a) && !wait_for_drop) {
                        eprintln!("GAP_PROBE_ACKS_AND_SLOT_PASS slot={observed_slot}");
                        let _=ws.close(None).await;
                        return Ok(());
                    }
                },
                Some(Ok(Message::Ping(p))) => { ws.send(Message::Pong(p)).await.map_err(|e|ws_error(&e))?; },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => {
                    if wait_for_drop && ack.iter().all(|a|*a) && slot.is_some() {
                        eprintln!("GAP_PROBE_DISCONNECT_AFTER_ACKS_AND_SLOT");
                        return Ok(());
                    }
                    return Err("DISCONNECTED_BEFORE_QUALIFICATION".into());
                },
                _ => {},
            }
        }
    }).await.map_err(|_|"PROBE_TIMEOUT".to_string())?
}

async fn run() -> Result<(), String> {
    let upstream = std::env::var("P0_GAP_UPSTREAM_WS_URL").map_err(|_| "MISSING_UPSTREAM")?;
    match std::env::args().nth(1).as_deref() {
        Some("--probe-direct") => return probe_session(&upstream, false).await,
        Some("--probe-proxy") => {
            probe_session(&upstream, true).await?;
            probe_session(&upstream, false).await?;
            eprintln!("GAP_PROBE_RECONNECT_PASS");
            return Ok(());
        }
        Some(_) => return Err("INVALID_MODE".into()),
        None => {}
    }
    let ready_file = std::env::var("P0_GAP_PROXY_READY_FILE").map_err(|_| "MISSING_READY_FILE")?;
    let after: u64 = std::env::var("P0_GAP_INTERRUPT_AFTER_SECS")
        .unwrap_or_else(|_| "20".into())
        .parse()
        .map_err(|_| "INVALID_DELAY")?;
    if !(1..=120).contains(&after) {
        return Err("invalid interruption delay".into());
    }
    // Local bind alone is not readiness. Prove the exact upstream's two subscription
    // ACKs and a real slot before allowing the expensive combined harness to start.
    probe_session(&upstream, false).await?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "LOCAL_BIND_FAILED")?;
    std::fs::write(
        ready_file,
        format!(
            "ws://{}",
            listener.local_addr().map_err(|_| "LOCAL_ADDRESS_FAILED")?
        ),
    )
    .map_err(|_| "READY_FILE_WRITE_FAILED")?;
    let mut interrupted = false;
    loop {
        let (socket, _) = listener.accept().await.map_err(|_| "LOCAL_ACCEPT_FAILED")?;
        let Ok(mut downstream) = accept_async(socket).await else {
            continue;
        };
        let mut remote =
            match tokio::time::timeout(Duration::from_secs(20), connect_async(&upstream)).await {
                Ok(Ok((remote, _))) => {
                    eprintln!("GAP_PROXY_UPSTREAM_CONNECTED");
                    remote
                }
                failure => {
                    // Never format the error/URL/headers: these can carry API credentials.
                    let category = match failure {
                        Err(_) => "TIMEOUT",
                        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(ref r)))
                            if r.status().as_u16() == 429 =>
                        {
                            "HTTP_RATE_LIMIT"
                        }
                        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(_))) => "HTTP_REJECTION",
                        Ok(Err(tokio_tungstenite::tungstenite::Error::Tls(_))) => "TLS",
                        Ok(Err(tokio_tungstenite::tungstenite::Error::Io(_))) => "IO",
                        _ => "PROTOCOL",
                    };
                    eprintln!("GAP_PROXY_UPSTREAM_CONNECT_FAILED category={category}");
                    if let Ok(Err(ref e)) = failure {
                        eprintln!("GAP_PROXY_UPSTREAM_DETAIL {}", ws_error(e));
                    }
                    continue;
                }
            };
        let mut deadline = None;
        loop {
            let wake =
                deadline.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(3600));
            tokio::select! {
                _ = tokio::time::sleep_until(wake), if deadline.is_some() => {
                    interrupted = true;
                    eprintln!("GAP_PROXY_CONTROLLED_DISCONNECT after_slot=true");
                    // Drop both transports; the real WsRpcControlSource observes an error
                    // and performs its ordinary Gap -> reconnect -> subscription ACK path.
                    break;
                }
                message = downstream.next() => {
                    let Some(Ok(message)) = message else { break; };
                    if message.is_text() {
                        let Ok(v) = serde_json::from_str::<serde_json::Value>(message.to_text().map_err(|_|"INVALID_TEXT")?) else { break; };
                        if !matches!(v["method"].as_str(), Some("slotSubscribe" | "logsSubscribe" | "transactionSubscribe")) {
                            eprintln!("GAP_PROXY_REJECTED_NON_MARKET_METHOD");
                            break;
                        }
                    }
                    if remote.send(message).await.is_err() { break; }
                }
                message = remote.next() => {
                    let Some(Ok(message)) = message else { break; };
                    if !interrupted && deadline.is_none() && message.is_text() {
                        let v = serde_json::from_str::<serde_json::Value>(message.to_text().map_err(|_|"INVALID_TEXT")?).unwrap_or_default();
                        if v["method"] == "slotNotification" {
                            deadline = Some(tokio::time::Instant::now() + Duration::from_secs(after));
                        }
                    }
                    if downstream.send(message).await.is_err() { break; }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::{Error, Message};

    #[test]
    fn diagnostic_does_not_expose_io_payload() {
        let e = Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "wss://secret.example/?api-key=PRIVATE",
        ));
        let safe = ws_error(&e);
        assert!(safe.contains("ConnectionReset"));
        assert!(!safe.contains("PRIVATE"));
        assert!(!safe.contains("secret.example"));
    }

    async fn fixture(ack_both: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(s).await.unwrap();
            for id in 1..=2 {
                let _ = ws.next().await;
                if id == 1 || ack_both {
                    ws.send(Message::Text(
                        serde_json::json!({"jsonrpc":"2.0","id":id,"result":id+10}).to_string(),
                    ))
                    .await
                    .unwrap();
                }
            }
            ws.send(Message::Text(
                serde_json::json!({"method":"slotNotification","params":{"result":{"slot":123}}})
                    .to_string(),
            ))
            .await
            .unwrap();
            let _ = ws.close(None).await;
        });
        format!("ws://{address}")
    }
    #[tokio::test]
    async fn preflight_requires_both_acks_and_real_slot() {
        assert!(probe_session(&fixture(true).await, false).await.is_ok());
        assert_eq!(
            probe_session(&fixture(false).await, false)
                .await
                .unwrap_err(),
            "DISCONNECTED_BEFORE_QUALIFICATION"
        );
    }
}
