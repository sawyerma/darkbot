//! DEC-18 §3.3 (C-11, P0-022): the provider pre-measurement's subscription probes — over exactly
//! one WSS connection to the bound endpoint (its credential applied by `ws_client_request`, the
//! ingress's frame limits), one subscription at a time: the subscribe message a session sends,
//! its acknowledgement, its first event, then its unsubscribe — the next probe starts only once
//! the provider confirmed it, so at most one subscription is active (C-11 allows two). A
//! provider's refusal is the method's answer; nothing is retried and nothing reconnects.
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

/// One subscription probe's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WsProbeOutcome {
    /// Acknowledged, and an event of the subscription arrived (both since its subscribe).
    Supported { ack_ms: u64, event_ms: u64 },
    /// Acknowledged; no event within its window.
    AckOnly { ack_ms: u64 },
    /// The provider answered the subscribe with this JSON-RPC error code.
    Refused { code: i64 },
    /// No acknowledgement within its window.
    NoAck,
    /// The connection failed before or during this probe.
    Transport(&'static str),
    /// The measurement's deadline passed before this probe started.
    NotRun,
}

/// The probes of one connection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WsProbeReport {
    /// One outcome per subscribe message, in order.
    pub outcomes: Vec<WsProbeOutcome>,
    /// The connection's refusal before any probe (e.g. `WSS_CONNECT_FAILED`).
    pub connect_error: Option<&'static str>,
    /// A 429 at the handshake: its `Retry-After`, in milliseconds.
    pub retry_after_ms: Option<u64>,
    /// Why no further probe started (`None`: none stopped them).
    pub stopped: Option<Stop>,
    /// Messages and bytes the provider sent.
    pub frames: u64,
    pub bytes: u64,
    /// The bytes the provider sent for each started probe (its acknowledgement, event and
    /// unsubscribe answer).
    pub probe_bytes: Vec<u64>,
}

/// Why the probes stopped before the last: a subscription that was never acknowledged may still
/// become active, an unsubscribe never confirmed may still be — either would let a further probe
/// make more than one active (C-11 allows two at most); a failed connection ends them all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Unacknowledged,
    UnconfirmedUnsubscribe,
    Transport,
}

/// The JSON-RPC method that ends a subscription of `subscribe` (`slotSubscribe` → `slotUnsubscribe`).
pub fn unsubscribe_method(subscribe: &str) -> Option<String> {
    subscribe
        .strip_suffix("Subscribe")
        .map(|kind| format!("{kind}Unsubscribe"))
}

/// Probes `messages` (each a subscribe message as a session sends it) in order over one
/// connection to `endpoint`: each waits at most `window` for its acknowledgement and first
/// event; none starts after `deadline`.
pub fn probe(
    endpoint: &str,
    messages: &[Value],
    window: Duration,
    deadline: Instant,
) -> WsProbeReport {
    let mut report = WsProbeReport::default();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(_) => {
            report.connect_error = Some("WSS_RUNTIME");
            report.outcomes = vec![WsProbeOutcome::NotRun; messages.len()];
            return report;
        }
    };
    runtime.block_on(session(endpoint, messages, window, deadline, &mut report));
    report
}

async fn session(
    endpoint: &str,
    messages: &[Value],
    window: Duration,
    deadline: Instant,
    report: &mut WsProbeReport,
) {
    let not_run = |report: &mut WsProbeReport| {
        while report.outcomes.len() < messages.len() {
            report.outcomes.push(WsProbeOutcome::NotRun);
        }
    };
    let limits = &crate::runtime_config::active().limits;
    let mut wire = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
    let message_bytes = (limits.wss_message_mib * 1024 * 1024) as usize;
    wire.max_message_size = Some(message_bytes);
    wire.max_frame_size = wire.max_message_size;
    let request = match crate::providers::ws_client_request(endpoint) {
        Ok(r) => r,
        Err(code) => {
            report.connect_error = Some(code);
            return not_run(report);
        }
    };
    let connect = deadline
        .saturating_duration_since(Instant::now())
        .min(window);
    let ws = match tokio::time::timeout(
        connect,
        tokio_tungstenite::connect_async_with_config(request, Some(wire), false),
    )
    .await
    {
        Ok(Ok((ws, _))) => ws,
        Ok(Err(e)) => {
            if let tokio_tungstenite::tungstenite::Error::Http(r) = &e {
                if r.status().as_u16() == 429 {
                    let header = crate::provider::retry_after_header(
                        r.headers().get("retry-after").and_then(|h| h.to_str().ok()),
                        std::time::SystemTime::now(),
                    );
                    report.retry_after_ms = Some(header.delay().as_millis() as u64);
                }
                if matches!(r.status().as_u16(), 401 | 403) {
                    report.connect_error = Some("WSS_UNAUTHORIZED");
                    return not_run(report);
                }
            }
            report.connect_error = Some("WSS_CONNECT_FAILED");
            return not_run(report);
        }
        Err(_) => {
            report.connect_error = Some("WSS_CONNECT_TIMEOUT");
            return not_run(report);
        }
    };
    let (mut tx, mut rx) = ws.split();
    for (i, message) in messages.iter().enumerate() {
        if Instant::now() >= deadline {
            break;
        }
        let id = i as u64 + 1;
        let method = message["method"].as_str().unwrap_or_default().to_string();
        let mut message = message.clone();
        message["id"] = json!(id);
        let started = Instant::now();
        let until = (started + window).min(deadline);
        report.probe_bytes.push(0);
        if tx.send(Message::Text(message.to_string())).await.is_err() {
            report.outcomes.push(WsProbeOutcome::Transport("WSS_SEND"));
            report.stopped = Some(Stop::Transport);
            break;
        }
        let ms = |since: Instant| since.elapsed().as_millis() as u64;
        let mut ack: Option<(u64, u64)> = None;
        let outcome = loop {
            let remaining = until.saturating_duration_since(Instant::now());
            let frame = match tokio::time::timeout(remaining, rx.next()).await {
                Err(_) => {
                    break match ack {
                        Some((_, ack_ms)) => WsProbeOutcome::AckOnly { ack_ms },
                        None => WsProbeOutcome::NoAck,
                    }
                }
                Ok(None) | Ok(Some(Err(_))) => break WsProbeOutcome::Transport("WSS_CLOSED"),
                Ok(Some(Ok(frame))) => frame,
            };
            let text = match &frame {
                Message::Text(t) => t.as_str(),
                Message::Close(_) => break WsProbeOutcome::Transport("WSS_CLOSED"),
                _ => continue,
            };
            report.frames += 1;
            count(report, text.len());
            let Ok(v) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            if v["id"].as_u64() == Some(id) {
                if let Some(code) = v["error"]["code"].as_i64() {
                    break WsProbeOutcome::Refused { code };
                }
                match v["result"].as_u64() {
                    Some(sub) => ack = Some((sub, ms(started))),
                    None => break WsProbeOutcome::Transport("WSS_ACK_INVALID"),
                }
            } else if let (Some((sub, ack_ms)), Some(of)) =
                (ack, v["params"]["subscription"].as_u64())
            {
                if of == sub {
                    break WsProbeOutcome::Supported {
                        ack_ms,
                        event_ms: ms(started),
                    };
                }
            }
        };
        let stop = match outcome {
            WsProbeOutcome::Transport(_) => Some(Stop::Transport),
            WsProbeOutcome::NoAck => Some(Stop::Unacknowledged),
            _ => None,
        };
        report.outcomes.push(outcome);
        if stop.is_some() {
            report.stopped = stop;
            break;
        }
        // Its unsubscribe, confirmed before the next probe starts.
        if let (Some((sub, _)), Some(unsubscribe)) = (ack, unsubscribe_method(&method)) {
            let end_id = id + messages.len() as u64;
            let end = json!({"jsonrpc":"2.0","id": end_id,"method": unsubscribe,"params":[sub]});
            if tx.send(Message::Text(end.to_string())).await.is_err() {
                report.stopped = Some(Stop::Transport);
                break;
            }
            let until = (Instant::now() + window).min(deadline);
            let confirmed = loop {
                let remaining = until.saturating_duration_since(Instant::now());
                match tokio::time::timeout(remaining, rx.next()).await {
                    Ok(Some(Ok(Message::Text(t)))) => {
                        report.frames += 1;
                        count(report, t.len());
                        if let Ok(v) = serde_json::from_str::<Value>(&t) {
                            if v["id"].as_u64() == Some(end_id) {
                                break v["result"].as_bool() == Some(true);
                            }
                        }
                    }
                    Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => {
                        break false
                    }
                    Ok(Some(Ok(_))) => {}
                }
            };
            if !confirmed {
                report.stopped = Some(Stop::UnconfirmedUnsubscribe);
                break;
            }
        }
    }
    let _ = tx.send(Message::Close(None)).await;
    not_run(report);
}

/// `n` bytes the provider sent, to the connection's and the current probe's count.
fn count(report: &mut WsProbeReport, n: usize) {
    report.bytes += n as u64;
    if let Some(b) = report.probe_bytes.last_mut() {
        *b += n as u64;
    }
}
