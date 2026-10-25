//! The session and reconnect loop against a local WebSocket server (plain
//! TCP + tungstenite server side; the client runs the production loop,
//! handshake and session code, only without the TLS layer).

use super::endpoint::{Endpoint, configure_socket};
use super::jsonrpc::{SUBSCRIBE_ID, SUBSCRIBE_REQUEST};
use super::session::{run_loop, ws_handshake};
use super::*;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, Message};

const LEGACY: &str = include_str!("../../tests/fixtures/wss/legacy.json");
const TYPE1: &str = include_str!("../../tests/fixtures/wss/type1.json");
const TYPE2: &str = include_str!("../../tests/fixtures/wss/type2.json");

/// One server action on an upgraded connection.
enum Act {
    /// Reads the client's request and checks it is the subscribe request.
    ExpectSubscribe,
    Send(String),
    /// Pings and waits for the matching pong.
    PingPong(&'static [u8]),
    Close(u16),
}

enum Conn {
    /// Answers the upgrade request with HTTP 429.
    Http429,
    /// Upgrades, runs the actions, then reads until the client leaves.
    Ws(Vec<Act>),
}

/// When the server accepted a connection and finished its last action.
struct Served {
    accepted: Instant,
    done: Instant,
}

fn serve(listener: TcpListener, script: Vec<Conn>) -> JoinHandle<Vec<Served>> {
    thread::spawn(move || {
        let mut served = Vec::new();
        for conn in script {
            let (tcp, _) = listener.accept().unwrap();
            let accepted = Instant::now();
            tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            match conn {
                Conn::Http429 => {
                    let mut reader = BufReader::new(tcp.try_clone().unwrap());
                    let mut line = String::new();
                    while line != "\r\n" {
                        line.clear();
                        assert!(reader.read_line(&mut line).unwrap() > 0);
                    }
                    let mut tcp = tcp;
                    tcp.write_all(b"HTTP/1.1 429 Too Many Requests\r\ncontent-length: 0\r\n\r\n")
                        .unwrap();
                    served.push(Served {
                        accepted,
                        done: Instant::now(),
                    });
                }
                Conn::Ws(acts) => {
                    let mut ws = tungstenite::accept(tcp).unwrap();
                    for act in acts {
                        match act {
                            Act::ExpectSubscribe => {
                                let Message::Text(text) = ws.read().unwrap() else {
                                    panic!("subscribe request is not a text frame");
                                };
                                let request: Value = serde_json::from_str(&text).unwrap();
                                assert_eq!(request["method"], "eth_subscribe");
                                assert_eq!(
                                    request["params"],
                                    json!(["newPendingTransactions", true])
                                );
                                assert_eq!(request["id"], json!(SUBSCRIBE_ID));
                                assert_eq!(text, SUBSCRIBE_REQUEST);
                            }
                            Act::Send(text) => ws.send(Message::text(text)).unwrap(),
                            Act::PingPong(payload) => {
                                ws.send(Message::Ping(payload.to_vec())).unwrap();
                                loop {
                                    if let Message::Pong(p) = ws.read().unwrap() {
                                        assert_eq!(p, payload);
                                        break;
                                    }
                                }
                            }
                            Act::Close(code) => ws
                                .close(Some(CloseFrame {
                                    code: CloseCode::from(code),
                                    reason: "".into(),
                                }))
                                .unwrap(),
                        }
                    }
                    let done = Instant::now();
                    while ws.read().is_ok() {}
                    served.push(Served { accepted, done });
                }
            }
        }
        served
    })
}

/// The production loop with a plain-TCP connector.
fn spawn_client(
    addr: SocketAddr,
    limits: WssLimits,
    stop: Arc<AtomicBool>,
) -> (JoinHandle<()>, Receiver<WssEvent>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let url = format!("ws://{addr}/");
        let mut sink = |event: WssEvent| tx.send(event).unwrap();
        run_loop(&limits, &stop, &mut sink, |started| {
            let tcp = TcpStream::connect_timeout(&addr, limits.connect_timeout)
                .map_err(|e| DisconnectReason::Io(e.kind()))?;
            configure_socket(&tcp, &limits).map_err(|e| DisconnectReason::Io(e.kind()))?;
            ws_handshake(tcp, &url, &limits, &stop, started)
        });
    });
    (handle, rx)
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn limits() -> WssLimits {
    WssLimits {
        connect_timeout: ms(1000),
        handshake_timeout: ms(2000),
        read_tick: ms(20),
        write_timeout: ms(1000),
        idle_timeout: ms(2000),
        max_frame_bytes: 64 * 1024,
        max_message_bytes: 256 * 1024,
        backoff_initial: ms(50),
        backoff_max: ms(200),
        stable_after: ms(60_000),
    }
}

fn subscribed(id: &str) -> Act {
    Act::Send(json!({"jsonrpc": "2.0", "id": SUBSCRIBE_ID, "result": id}).to_string())
}

fn note(subscription: &str, result: Value) -> Act {
    Act::Send(
        json!({
            "jsonrpc": "2.0",
            "method": "eth_subscription",
            "params": {"subscription": subscription, "result": result},
        })
        .to_string(),
    )
}

/// A mined fixture as a pending notification carries it: no block fields.
fn pending(fixtures: &str, index: usize) -> Value {
    let mut tx = serde_json::from_str::<Vec<Value>>(fixtures)
        .unwrap()
        .remove(index);
    for key in ["blockHash", "blockNumber", "transactionIndex"] {
        tx[key] = Value::Null;
    }
    tx
}

/// Receives events until `count` of them were `Connected`.
fn recv_until_connected(rx: &Receiver<WssEvent>, count: usize, events: &mut Vec<WssEvent>) {
    let mut seen = 0;
    while seen < count {
        let event = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        seen += usize::from(event == WssEvent::Connected);
        events.push(event);
    }
}

#[test]
fn events_backoff_reconnect_and_stop_follow_the_wire() {
    use DisconnectReason::*;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let hash = [0xab; 32];
    let hash_hex = format!("0x{}", "ab".repeat(32));
    let full = pending(TYPE1, 4);
    let expected = verify_tx_object(&serde_json::to_vec(&full).unwrap(), 56).unwrap();
    let mut tampered = pending(LEGACY, 0);
    tampered["value"] = json!("0x1");
    let server = serve(
        listener,
        vec![
            Conn::Ws(vec![
                Act::ExpectSubscribe,
                subscribed("0xs1"),
                Act::PingPong(b"alive"),
                note("0xs1", json!(hash_hex)),
                note("0xs1", full),
                note("0xs1", tampered),
                Act::Close(1000),
            ]),
            Conn::Ws(vec![Act::ExpectSubscribe, subscribed("0xs2"), note("0xother", json!(hash_hex))]),
            Conn::Ws(vec![
                Act::ExpectSubscribe,
                subscribed("0xs3"),
                Act::Send(
                    r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32005,"message":"limit exceeded"}}"#
                        .to_owned(),
                ),
            ]),
            Conn::Http429,
            Conn::Ws(vec![Act::ExpectSubscribe, subscribed("0xs5")]),
        ],
    );
    let limits = limits();
    let stop = Arc::new(AtomicBool::new(false));
    let (client, rx) = spawn_client(addr, limits, Arc::clone(&stop));

    let mut events = Vec::new();
    recv_until_connected(&rx, 4, &mut events);
    let stop_at = Instant::now();
    stop.store(true, Ordering::Release);
    client.join().unwrap();
    let stop_latency = stop_at.elapsed();
    events.extend(rx.try_iter());
    let served = server.join().unwrap();

    assert_eq!(
        events,
        vec![
            WssEvent::Connected,
            WssEvent::HashOnly(hash),
            WssEvent::Full(expected),
            WssEvent::Rejected(RejectReason::HashMismatch),
            WssEvent::Disconnected(Closed(Some(1000))),
            WssEvent::Connected,
            WssEvent::Disconnected(Protocol(ProtocolFault::UnknownSubscription)),
            WssEvent::Connected,
            WssEvent::Disconnected(RateLimited(RateLimitSignal::RpcError(-32005))),
            WssEvent::Disconnected(RateLimited(RateLimitSignal::Http429)),
            WssEvent::Connected,
            WssEvent::Disconnected(Stopped),
        ]
    );
    // Exponential from backoff_initial (50, 100), then backoff_max after
    // each rate-limit signal (200, 200). Measured from the server's last
    // action to its next accept, so each gap includes the client's wait.
    let want = [ms(50), ms(100), ms(200), ms(200)];
    for (i, (pair, want)) in served.windows(2).zip(want).enumerate() {
        let gap = pair[1].accepted - pair[0].done;
        assert!(gap >= want, "gap {i}: {gap:?} < {want:?}");
        assert!(gap < want + ms(400), "gap {i}: {gap:?} far above {want:?}");
    }
    assert!(
        stop_latency < ms(500),
        "stop took {stop_latency:?} with read_tick {:?}",
        limits.read_tick
    );
}

#[test]
fn provider_faults_are_classified_and_stop_ends_the_backoff() {
    use DisconnectReason::*;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let mut type4 = pending(TYPE2, 0);
    type4["type"] = json!("0x4");
    let server = serve(
        listener,
        vec![
            Conn::Ws(vec![
                Act::ExpectSubscribe,
                Act::Send(
                    r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"no such method"}}"#
                        .to_owned(),
                ),
            ]),
            Conn::Ws(vec![
                Act::ExpectSubscribe,
                subscribed("0xa"),
                Act::Send("x".repeat(4097)),
            ]),
            Conn::Ws(vec![
                Act::ExpectSubscribe,
                subscribed("0xb"),
                Act::Close(1013),
            ]),
            Conn::Ws(vec![Act::ExpectSubscribe, subscribed("0xc")]),
            Conn::Ws(vec![Act::ExpectSubscribe]),
            Conn::Ws(vec![
                Act::ExpectSubscribe,
                Act::Send("{not json".to_owned()),
            ]),
            Conn::Ws(vec![
                Act::ExpectSubscribe,
                subscribed("0xd"),
                note("0xd", json!(42)),
                note("0xd", json!("0x1234")),
                note("0xd", type4),
            ]),
        ],
    );
    let limits = WssLimits {
        handshake_timeout: ms(300),
        idle_timeout: ms(300),
        max_frame_bytes: 4096,
        max_message_bytes: 8192,
        backoff_max: ms(400),
        ..limits()
    };
    let stop = Arc::new(AtomicBool::new(false));
    let (client, rx) = spawn_client(addr, limits, Arc::clone(&stop));

    let mut events = Vec::new();
    while events.len() < 14 {
        events.push(rx.recv_timeout(Duration::from_secs(10)).unwrap());
    }
    // The client is now in its backoff wait of backoff_max (400 ms); stop
    // must end it well before, without another attempt.
    let stop_at = Instant::now();
    stop.store(true, Ordering::Release);
    client.join().unwrap();
    let stop_latency = stop_at.elapsed();
    events.extend(rx.try_iter());
    server.join().unwrap();

    assert_eq!(
        events,
        vec![
            WssEvent::Disconnected(Protocol(ProtocolFault::SubscribeRefused(-32601))),
            WssEvent::Connected,
            WssEvent::Disconnected(Protocol(ProtocolFault::FrameTooLarge)),
            WssEvent::Connected,
            WssEvent::Disconnected(RateLimited(RateLimitSignal::CloseCode(1013))),
            WssEvent::Connected,
            WssEvent::Disconnected(Idle),
            WssEvent::Disconnected(Timeout),
            WssEvent::Disconnected(Protocol(ProtocolFault::MalformedJson)),
            WssEvent::Connected,
            WssEvent::Rejected(RejectReason::NotATransaction),
            WssEvent::Rejected(RejectReason::Malformed(TxField::Hash)),
            WssEvent::Rejected(RejectReason::UnsupportedType(4)),
            WssEvent::Disconnected(Idle),
        ]
    );
    assert!(stop_latency < ms(300), "stop took {stop_latency:?}");
}

#[test]
fn configuration_is_refused_before_any_connect() {
    use UrlError::*;
    let stop = AtomicBool::new(false);
    let mut events = Vec::new();
    let mut run = |url: &str, limits: &WssLimits| {
        let source = WssSource {
            name: "TEST",
            url: url.to_owned(),
        };
        let mut sink = |e| events.push(e);
        let result = run_source(&source, limits, &stop, &mut sink);
        assert!(events.is_empty(), "{url}: {events:?}");
        result
    };
    for (url, want) in [
        ("ws://example.org/", NotWss),
        ("https://example.org/", NotWss),
        ("WSS://example.org/", NotWss),
        ("wss://user:secret@example.org/", Credentials),
        ("wss://example.org/ws?key=1", QueryOrFragment),
        ("wss://example.org/#frag", QueryOrFragment),
        ("wss:///path", Host),
        ("wss://exa_mple.org/", Host),
        ("wss://.example.org/", Host),
        ("wss://[::1]/", Host),
        ("wss://example.org:0/", Port),
        ("wss://example.org:+443/", Port),
        ("wss://example.org:65536/", Port),
        ("wss://example.org:/", Port),
        ("wss://example.org/a b", Path),
    ] {
        assert_eq!(run(url, &limits()), Err(WssConfigError::Url(want)), "{url}");
    }
    let url = "wss://example.org/";
    for (bad, want) in [
        (
            WssLimits {
                read_tick: Duration::ZERO,
                ..limits()
            },
            LimitsError::ZeroDuration("read_tick"),
        ),
        (
            WssLimits {
                backoff_initial: ms(300),
                ..limits()
            },
            LimitsError::BackoffInitialAboveMax,
        ),
        (
            WssLimits {
                max_frame_bytes: SUBSCRIBE_REQUEST.len(),
                ..limits()
            },
            LimitsError::FrameBelowMinimum,
        ),
        (
            WssLimits {
                max_message_bytes: 1024,
                ..limits()
            },
            LimitsError::FrameAboveMessage,
        ),
    ] {
        assert_eq!(run(url, &bad), Err(WssConfigError::Limits(want)));
    }

    // Valid, with stop already set: returns without an attempt or event.
    stop.store(true, Ordering::Release);
    let source = WssSource {
        name: "TEST",
        url: "wss://unresolvable.invalid:8443/ws/v1".to_owned(),
    };
    let mut events = Vec::new();
    assert_eq!(
        run_source(&source, &limits(), &stop, &mut |e| events.push(e)),
        Ok(())
    );
    assert!(events.is_empty());

    let endpoint = Endpoint::parse("wss://bsc-rpc.example").unwrap();
    assert_eq!(endpoint.url(), "wss://bsc-rpc.example/");
    let endpoint = Endpoint::parse("wss://10.0.0.1:8546/a/b").unwrap();
    assert_eq!(endpoint.url(), "wss://10.0.0.1:8546/a/b");
}
