//! Qualification-only loopback proxy. Closes the first market WebSocket once,
//! after a real slot notification. Does not alter production ingress defaults.
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, connect_async};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let upstream = std::env::var("P0_GAP_UPSTREAM_WS_URL")?;
    let ready_file = std::env::var("P0_GAP_PROXY_READY_FILE")?;
    let after: u64 = std::env::var("P0_GAP_INTERRUPT_AFTER_SECS")
        .unwrap_or_else(|_| "20".into())
        .parse()?;
    if !(1..=120).contains(&after) {
        return Err("invalid interruption delay".into());
    }
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    std::fs::write(ready_file, format!("ws://{}", listener.local_addr()?))?;
    let mut interrupted = false;
    loop {
        let (socket, _) = listener.accept().await?;
        let Ok(mut downstream) = accept_async(socket).await else {
            continue;
        };
        let mut remote =
            match tokio::time::timeout(Duration::from_secs(10), connect_async(&upstream)).await {
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
                        let Ok(v) = serde_json::from_str::<serde_json::Value>(message.to_text()?) else { break; };
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
                        let v = serde_json::from_str::<serde_json::Value>(message.to_text()?).unwrap_or_default();
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
