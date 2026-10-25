//! The WebSocket session and the reconnect loop, over any byte stream (the
//! production stream is [`super::endpoint::TlsStream`]).

use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::error::{CapacityError, ProtocolError};
use tungstenite::handshake::HandshakeError;
use tungstenite::protocol::{CloseFrame, Message, WebSocket, WebSocketConfig};

use super::endpoint::{classify_io, is_tick};
use super::jsonrpc::{
    Inbound, Item, SUBSCRIBE_ID, SUBSCRIBE_REQUEST, is_rate_limit_code, parse_inbound,
};
use super::verify::verify;
use super::{
    BSC_MAINNET_CHAIN_ID, DisconnectReason, ProtocolFault, RateLimitSignal, WssEvent, WssLimits,
    stopped,
};

/// HTTP status of a rate-limited upgrade.
const HTTP_TOO_MANY_REQUESTS: u16 = 429;
/// WebSocket close code "Try Again Later" (RFC 6455 registry).
const CLOSE_TRY_AGAIN_LATER: u16 = 1013;
/// Largest WebSocket frame header (RFC 6455 §5.2: 2 + 8 length + 4 mask).
const MAX_FRAME_HEADER: usize = 14;

/// The smallest `max_frame_bytes` that still carries the subscribe request.
pub(super) fn min_frame_bytes() -> usize {
    SUBSCRIBE_REQUEST.len() + MAX_FRAME_HEADER
}

fn ws_config(limits: &WssLimits) -> WebSocketConfig {
    WebSocketConfig {
        // Every outbound frame (subscribe, pong, close) is written at once.
        write_buffer_size: 0,
        max_write_buffer_size: limits.max_frame_bytes,
        max_message_size: Some(limits.max_message_bytes),
        max_frame_size: Some(limits.max_frame_bytes),
        accept_unmasked_frames: false,
        ..WebSocketConfig::default()
    }
}

/// The client upgrade over `stream`, resumed after each read tick until it
/// completes, `stop` is set or the attempt's `handshake_timeout` passes.
pub(super) fn ws_handshake<S: Read + Write>(
    stream: S,
    url: &str,
    limits: &WssLimits,
    stop: &AtomicBool,
    started: Instant,
) -> Result<WebSocket<S>, DisconnectReason> {
    let mut attempt = tungstenite::client::client_with_config(url, stream, Some(ws_config(limits)));
    loop {
        match attempt {
            Ok((ws, _response)) => return Ok(ws),
            Err(HandshakeError::Failure(e)) => return Err(classify_ws(e)),
            Err(HandshakeError::Interrupted(mid)) => {
                if stopped(stop) {
                    return Err(DisconnectReason::Stopped);
                }
                if started.elapsed() >= limits.handshake_timeout {
                    return Err(DisconnectReason::Timeout);
                }
                attempt = mid.handshake();
            }
        }
    }
}

/// Connects (via `connect`, given the attempt's start), runs the session,
/// reports the end, waits the backoff; until `stop`.
pub(super) fn run_loop<S, C>(
    limits: &WssLimits,
    stop: &AtomicBool,
    sink: &mut dyn FnMut(WssEvent),
    mut connect: C,
) where
    S: Read + Write,
    C: FnMut(Instant) -> Result<WebSocket<S>, DisconnectReason>,
{
    let mut next_wait = limits.backoff_initial;
    while !stopped(stop) {
        let started = Instant::now();
        let (reason, subscribed_for) = match connect(started) {
            Ok(mut ws) => session(&mut ws, limits, stop, started, sink),
            Err(reason) => (reason, None),
        };
        sink(WssEvent::Disconnected(reason));
        if reason == DisconnectReason::Stopped {
            return;
        }
        let wait = match reason {
            DisconnectReason::RateLimited(_) => limits.backoff_max,
            _ if subscribed_for.is_some_and(|d| d >= limits.stable_after) => limits.backoff_initial,
            _ => next_wait,
        };
        next_wait = wait.saturating_mul(2).min(limits.backoff_max);
        if !wait_unless_stopped(wait, limits.read_tick, stop) {
            return;
        }
    }
}

/// Sleeps `wait` in slices of at most `tick`; `false` when `stop` was set.
fn wait_unless_stopped(wait: Duration, tick: Duration, stop: &AtomicBool) -> bool {
    let start = Instant::now();
    loop {
        if stopped(stop) {
            return false;
        }
        let remaining = wait.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return true;
        }
        thread::sleep(remaining.min(tick));
    }
}

/// Subscribes and reads until the connection ends. Returns why, and how
/// long the subscription was held (`None` if it was never confirmed).
fn session<S: Read + Write>(
    ws: &mut WebSocket<S>,
    limits: &WssLimits,
    stop: &AtomicBool,
    started: Instant,
    sink: &mut dyn FnMut(WssEvent),
) -> (DisconnectReason, Option<Duration>) {
    let mut subscription: Option<(String, Instant)> = None;
    let held = |s: &Option<(String, Instant)>| s.as_ref().map(|(_, at)| at.elapsed());
    if let Err(e) = ws.send(Message::text(SUBSCRIBE_REQUEST)) {
        return (classify_ws(e), None);
    }
    let mut last_notification = Instant::now();
    loop {
        if stopped(stop) {
            close_quietly(ws);
            return (DisconnectReason::Stopped, held(&subscription));
        }
        match subscription {
            None if started.elapsed() >= limits.handshake_timeout => {
                close_quietly(ws);
                return (DisconnectReason::Timeout, None);
            }
            Some(_) if last_notification.elapsed() >= limits.idle_timeout => {
                close_quietly(ws);
                return (DisconnectReason::Idle, held(&subscription));
            }
            _ => {}
        }
        let message = match ws.read() {
            Ok(message) => message,
            Err(tungstenite::Error::Io(e)) if is_tick(&e) => continue,
            Err(e) => return (classify_ws(e), held(&subscription)),
        };
        let inbound = match &message {
            Message::Text(text) => parse_inbound(text.as_bytes()),
            Message::Binary(bytes) => parse_inbound(bytes),
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Close(frame) => {
                // tungstenite queued the close reply; send it if possible.
                let _ = ws.flush();
                return (close_reason(frame.as_ref()), held(&subscription));
            }
            Message::Frame(_) => {
                return (
                    DisconnectReason::Protocol(ProtocolFault::WebSocket),
                    held(&subscription),
                );
            }
        };
        let fault = match (inbound, &subscription) {
            (Err(_), _) => DisconnectReason::Protocol(ProtocolFault::MalformedJson),
            (
                Ok(Inbound::Subscribed {
                    id,
                    subscription: sub,
                }),
                None,
            ) if id == SUBSCRIBE_ID => {
                subscription = Some((sub, Instant::now()));
                last_notification = Instant::now();
                sink(WssEvent::Connected);
                continue;
            }
            (
                Ok(Inbound::Notification {
                    subscription: sub,
                    item,
                }),
                Some((ours, _)),
            ) => {
                if sub != *ours {
                    DisconnectReason::Protocol(ProtocolFault::UnknownSubscription)
                } else {
                    last_notification = Instant::now();
                    sink(event_of(item));
                    continue;
                }
            }
            (Ok(Inbound::RpcError { code }), held_sub) => {
                if is_rate_limit_code(code) {
                    DisconnectReason::RateLimited(RateLimitSignal::RpcError(code))
                } else if held_sub.is_none() {
                    DisconnectReason::Protocol(ProtocolFault::SubscribeRefused(code))
                } else {
                    DisconnectReason::Protocol(ProtocolFault::RpcError(code))
                }
            }
            (Ok(Inbound::Notification { .. }), None) => {
                DisconnectReason::Protocol(ProtocolFault::UnknownSubscription)
            }
            (Ok(_), _) => DisconnectReason::Protocol(ProtocolFault::UnexpectedMessage),
        };
        close_quietly(ws);
        return (fault, held(&subscription));
    }
}

/// One notification's event: a hash as sent, a full object only verified.
fn event_of(item: Item) -> WssEvent {
    match item {
        Item::Hash(hash) => WssEvent::HashOnly(hash),
        Item::Tx(tx) => match verify(&tx, BSC_MAINNET_CHAIN_ID) {
            Ok(verified) => WssEvent::Full(verified),
            Err(reason) => WssEvent::Rejected(reason),
        },
        Item::Rejected(reason) => WssEvent::Rejected(reason),
    }
}

/// Starts the closing handshake and writes it within `write_timeout`; the
/// connection is dropped either way, so a failure changes nothing.
fn close_quietly<S: Read + Write>(ws: &mut WebSocket<S>) {
    let _ = ws.close(None);
    let _ = ws.flush();
}

fn close_reason(frame: Option<&CloseFrame<'_>>) -> DisconnectReason {
    match frame.map(|f| u16::from(f.code)) {
        Some(CLOSE_TRY_AGAIN_LATER) => {
            DisconnectReason::RateLimited(RateLimitSignal::CloseCode(CLOSE_TRY_AGAIN_LATER))
        }
        code => DisconnectReason::Closed(code),
    }
}

fn classify_ws(e: tungstenite::Error) -> DisconnectReason {
    use tungstenite::Error as E;
    match e {
        E::ConnectionClosed
        | E::AlreadyClosed
        | E::Protocol(ProtocolError::ResetWithoutClosingHandshake) => {
            DisconnectReason::Closed(None)
        }
        E::Io(e) => classify_io(&e),
        E::Capacity(CapacityError::MessageTooLong { .. }) => {
            DisconnectReason::Protocol(ProtocolFault::FrameTooLarge)
        }
        E::Http(response) => match response.status().as_u16() {
            HTTP_TOO_MANY_REQUESTS => DisconnectReason::RateLimited(RateLimitSignal::Http429),
            status => DisconnectReason::Handshake(status),
        },
        _ => DisconnectReason::Protocol(ProtocolFault::WebSocket),
    }
}
