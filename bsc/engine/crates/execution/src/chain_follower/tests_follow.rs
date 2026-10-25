//! The follower against a local HTTP/1.1 JSON-RPC server (plain TCP: the
//! client runs the production HTTP, JSON-RPC and follower code, only
//! without the TLS layer). The server serves scripted canonical chains
//! with known forks, faults and chunked bodies; one test serves a real
//! fixture block.

use super::follow::{Rpc, run};
use super::http::Client;
use super::launches::{FLAP_PORTAL, FOUR_TOKEN_MANAGER2, LAUNCH_EVENTS};
use super::*;
use alloy_primitives::{hex, keccak256};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// The `result` of `eth_getBlockByNumber(n, false)` and of
/// `eth_getBlockReceipts(n)` for one block.
#[derive(Clone)]
struct Served {
    header: Value,
    receipts: Value,
}

/// The canonical chain as the server reports it during one phase.
struct Phase {
    head: u64,
    blocks: BTreeMap<u64, Served>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Req {
    BlockNumber,
    Header(u64),
    Receipts(u64),
}

#[derive(Clone, Copy, Debug)]
enum Reply {
    /// The normal answer with `Connection: close`, then the server closes.
    Close,
    Http429,
    /// Valid HTTP framing, invalid JSON.
    Malformed,
    RpcError(i64),
    /// Reads the request and never answers (until the client closes).
    Hang,
}

struct Script {
    phases: Vec<Phase>,
    /// One-shot replies: the request, how many equal requests pass first,
    /// and the reply.
    inject: Vec<(Req, usize, Reply)>,
    /// Receipts go out chunked in pieces of this size (everything else
    /// with `Content-Length`).
    chunk: usize,
}

/// The server's view of the run.
struct State {
    script: Script,
    /// The current phase. A phase ends once the receipts of its head were
    /// served; the next `eth_blockNumber` then starts the next one, and
    /// after the last one it sets the follower's `stop`.
    phase: Option<usize>,
    head_done: bool,
    counts: Vec<(Req, usize)>,
    follower_stop: Arc<AtomicBool>,
    stop_set_at: Option<Instant>,
    connections: usize,
}

impl State {
    fn current(&self) -> &Phase {
        &self.script.phases[self.phase.unwrap()]
    }

    fn injected(&mut self, req: Req) -> Option<Reply> {
        let nth = match self.counts.iter_mut().find(|(r, _)| *r == req) {
            Some((_, n)) => {
                *n += 1;
                *n - 1
            }
            None => {
                self.counts.push((req, 1));
                0
            }
        };
        let at = self
            .script
            .inject
            .iter()
            .position(|(r, n, _)| *r == req && *n == nth)?;
        Some(self.script.inject.remove(at).2)
    }

    fn answer(&mut self, req: Req) -> Value {
        match req {
            Req::BlockNumber => {
                let last = self.script.phases.len() - 1;
                match self.phase {
                    None => self.phase = Some(0),
                    Some(p) if self.head_done && p < last => {
                        self.phase = Some(p + 1);
                        self.head_done = false;
                    }
                    Some(_) if self.head_done => {
                        self.stop_set_at = Some(Instant::now());
                        self.follower_stop.store(true, Ordering::Release);
                    }
                    Some(_) => {}
                }
                json!(format!("0x{:x}", self.current().head))
            }
            Req::Header(n) => self
                .current()
                .blocks
                .get(&n)
                .map_or(Value::Null, |b| b.header.clone()),
            Req::Receipts(n) => {
                let receipts = self
                    .current()
                    .blocks
                    .get(&n)
                    .map_or(Value::Null, |b| b.receipts.clone());
                if n == self.current().head && !receipts.is_null() {
                    self.head_done = true;
                }
                receipts
            }
        }
    }

    /// Serves one connection until the client leaves or the test ends.
    fn connection(&mut self, tcp: TcpStream, done: &AtomicBool) {
        let mut reader = BufReader::new(tcp.try_clone().unwrap());
        let mut writer = tcp;
        while let Some(body) = read_request(&mut reader, done) {
            let request: Value = serde_json::from_slice(&body).unwrap();
            let id = request["id"].as_u64().unwrap();
            let number = |v: &Value| u64::from_str_radix(&v.as_str().unwrap()[2..], 16).unwrap();
            let req = match request["method"].as_str().unwrap() {
                "eth_blockNumber" => Req::BlockNumber,
                "eth_getBlockByNumber" => {
                    assert_eq!(request["params"][1], false);
                    Req::Header(number(&request["params"][0]))
                }
                "eth_getBlockReceipts" => Req::Receipts(number(&request["params"][0])),
                other => panic!("unexpected method {other}"),
            };
            let chunk = matches!(req, Req::Receipts(_)).then_some(self.script.chunk);
            let ok = |result: Value| json!({"jsonrpc": "2.0", "id": id, "result": result});
            match self.injected(req) {
                None => respond(&mut writer, &ok(self.answer(req)), chunk, false),
                Some(Reply::Close) => {
                    respond(&mut writer, &ok(self.answer(req)), chunk, true);
                    return;
                }
                Some(Reply::Http429) => {
                    let _ = writer
                        .write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n");
                    return;
                }
                Some(Reply::Malformed) => {
                    let body = br#"{"jsonrpc":"2.0","id":"#;
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                    let _ = writer.write_all(&[head.as_bytes(), body].concat());
                }
                Some(Reply::RpcError(code)) => {
                    let error = json!({"jsonrpc": "2.0", "id": id,
                        "error": {"code": code, "message": "scripted"}});
                    respond(&mut writer, &error, None, false);
                }
                Some(Reply::Hang) => {
                    // Until the client gives up (closes) or the test ends.
                    let mut byte = [0u8; 1];
                    loop {
                        match reader.read(&mut byte) {
                            Ok(0) => return,
                            Ok(_) => {}
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                                ) =>
                            {
                                if done.load(Ordering::Acquire) {
                                    return;
                                }
                            }
                            Err(_) => return,
                        }
                    }
                }
            }
        }
    }
}

/// One request's body; `None` when the client left or the test ended.
fn read_request(reader: &mut BufReader<TcpStream>, done: &AtomicBool) -> Option<Vec<u8>> {
    let mut length = None;
    let mut first = true;
    loop {
        let mut line = Vec::new();
        loop {
            match reader.read_until(b'\n', &mut line) {
                Ok(_) => break,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    if done.load(Ordering::Acquire) {
                        return None;
                    }
                }
                Err(_) => return None,
            }
        }
        if !line.ends_with(b"\n") {
            return None;
        }
        if first {
            assert!(line.starts_with(b"POST /rpc HTTP/1.1\r\n"));
            first = false;
            continue;
        }
        if line == b"\r\n" {
            break;
        }
        let text = String::from_utf8(line).unwrap().to_ascii_lowercase();
        if let Some(value) = text.strip_prefix("content-length:") {
            length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let mut body = vec![0; length.unwrap()];
    let mut got = 0;
    while got < body.len() {
        match reader.read(&mut body[got..]) {
            Ok(0) => return None,
            Ok(n) => got += n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if done.load(Ordering::Acquire) {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    Some(body)
}

/// A 200 answer, chunked in pieces of `chunk` (with an extension on the
/// first chunk, upper-case sizes after it, and a trailer) or with
/// `Content-Length`.
fn respond(writer: &mut TcpStream, message: &Value, chunk: Option<usize>, close: bool) {
    let body = message.to_string().into_bytes();
    let mut out = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n".to_vec();
    if close {
        out.extend_from_slice(b"Connection: close\r\n");
    }
    match chunk {
        None => {
            out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
            out.extend_from_slice(&body);
        }
        Some(size) => {
            out.extend_from_slice(b"Transfer-Encoding: chunked\r\n\r\n");
            for (i, piece) in body.chunks(size).enumerate() {
                let line = if i == 0 {
                    format!("{:x};piece=first\r\n", piece.len())
                } else {
                    format!("{:X}\r\n", piece.len())
                };
                out.extend_from_slice(line.as_bytes());
                out.extend_from_slice(piece);
                out.extend_from_slice(b"\r\n");
            }
            out.extend_from_slice(b"0\r\nX-Served-By: test\r\n\r\n");
        }
    }
    let _ = writer.write_all(&out);
}

struct Server {
    addr: SocketAddr,
    done: Arc<AtomicBool>,
    handle: JoinHandle<State>,
}

fn serve(script: Script, follower_stop: Arc<AtomicBool>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let handle = {
        let done = Arc::clone(&done);
        thread::spawn(move || {
            let mut state = State {
                script,
                phase: None,
                head_done: false,
                counts: Vec::new(),
                follower_stop,
                stop_set_at: None,
                connections: 0,
            };
            while !done.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((tcp, _)) => {
                        state.connections += 1;
                        tcp.set_nonblocking(false).unwrap();
                        tcp.set_read_timeout(Some(Duration::from_millis(20)))
                            .unwrap();
                        state.connection(tcp, &done);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(e) => panic!("accept: {e}"),
                }
            }
            state
        })
    };
    Server { addr, done, handle }
}

fn limits() -> FollowerLimits {
    FollowerLimits {
        poll: Duration::from_millis(20),
        reorg_depth: 3,
        max_catch_up: 5,
        connect_timeout: Duration::from_secs(1),
        handshake_timeout: Duration::from_secs(1),
        read_tick: Duration::from_millis(5),
        write_timeout: Duration::from_secs(1),
        request_timeout: Duration::from_secs(5),
        max_response_bytes: 1 << 20,
        backoff_initial: Duration::from_millis(10),
        backoff_max: Duration::from_millis(40),
    }
}

/// The plain TCP connect of the tests, with the production socket bounds.
fn connector(
    addr: SocketAddr,
    limits: FollowerLimits,
) -> impl FnMut(&AtomicBool) -> Result<TcpStream, DisconnectReason> {
    move |_stop: &AtomicBool| {
        let io = |e: io::Error| DisconnectReason::Io(e.kind());
        let tcp = TcpStream::connect_timeout(&addr, limits.connect_timeout).map_err(io)?;
        tcp.set_nodelay(true).map_err(io)?;
        tcp.set_read_timeout(Some(limits.read_tick)).map_err(io)?;
        tcp.set_write_timeout(Some(limits.write_timeout))
            .map_err(io)?;
        Ok(tcp)
    }
}

struct Outcome {
    /// Every event with the time the sink received it.
    events: Vec<(CanonicalEvent, Instant)>,
    returned: Instant,
    stop_set_at: Option<Instant>,
    connections: usize,
}

/// Runs the production follower loop against `script` until the script or
/// `stop_after` sets `stop`.
fn follow_script(script: Script, limits: FollowerLimits, stop_after: Option<Duration>) -> Outcome {
    let stop = Arc::new(AtomicBool::new(false));
    let server = serve(script, Arc::clone(&stop));
    let stopper = stop_after.map(|after| {
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            thread::sleep(after);
            let at = Instant::now();
            stop.store(true, Ordering::Release);
            at
        })
    });
    let client = Client::new(
        connector(server.addr, limits),
        server.addr.to_string(),
        "/rpc".to_owned(),
        &limits,
    );
    let mut events = Vec::new();
    let mut sink = |event: CanonicalEvent| events.push((event, Instant::now()));
    run(&mut Rpc::new(client), &limits, &stop, &mut sink);
    let returned = Instant::now();
    server.done.store(true, Ordering::Release);
    let state = server.handle.join().unwrap();
    let stopper_at = stopper.map(|s| s.join().unwrap());
    Outcome {
        events,
        returned,
        stop_set_at: state.stop_set_at.or(stopper_at),
        connections: state.connections,
    }
}

// ---- synthetic chains -------------------------------------------------

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn q(n: u64) -> String {
    format!("0x{n:x}")
}

fn named(label: &str) -> [u8; 32] {
    keccak256(label.as_bytes()).0
}

fn word(n: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&n.to_be_bytes());
    w
}

fn address_word(a: [u8; 20]) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(&a);
    w
}

type SynthLog = ([u8; 20], Vec<[u8; 32]>, Vec<u8>);

/// `TokenCreated(ts, creator, nonce, token, name, symbol, meta)` of the
/// Portal, with three one-byte strings.
fn flap_launch(creator: [u8; 20], token: [u8; 20]) -> SynthLog {
    let mut data: Vec<u8> = [
        word(1),
        address_word(creator),
        word(7),
        address_word(token),
        word(0xe0),
        word(0x120),
        word(0x160),
    ]
    .concat();
    for s in [b'N', b'S', b'M'] {
        data.extend_from_slice(&word(1));
        let mut text = [0u8; 32];
        text[0] = s;
        data.extend_from_slice(&text);
    }
    (FLAP_PORTAL, vec![LAUNCH_EVENTS[0].topic0], data)
}

/// `TokenBought(ts, token, buyer, amount, eth, fee, postPrice)`; `words`
/// other than 7 makes it malformed.
fn flap_buy(token: [u8; 20], buyer: [u8; 20], words: usize) -> SynthLog {
    let all = [
        word(1),
        address_word(token),
        address_word(buyer),
        word(1000),
        word(10),
        word(1),
        word(5),
    ];
    let buy = LAUNCH_EVENTS[0].buy.unwrap();
    (FLAP_PORTAL, vec![buy.topic0], all[..words].concat())
}

/// `TokenCreate(creator, token, requestId, name, symbol, totalSupply,
/// launchTime, launchFee)` of TokenManager2.
fn four_launch(creator: [u8; 20], token: [u8; 20]) -> SynthLog {
    let mut data: Vec<u8> = [
        address_word(creator),
        address_word(token),
        word(42),
        word(0x100),
        word(0x140),
        word(1_000_000_000),
        word(0),
        word(0),
    ]
    .concat();
    for s in [b'N', b'S'] {
        data.extend_from_slice(&word(1));
        let mut text = [0u8; 32];
        text[0] = s;
        data.extend_from_slice(&text);
    }
    (FOUR_TOKEN_MANAGER2, vec![LAUNCH_EVENTS[1].topic0], data)
}

struct Tx {
    ok: bool,
    logs: Vec<SynthLog>,
}

fn tx(logs: Vec<SynthLog>) -> Tx {
    Tx { ok: true, logs }
}

struct Synth {
    number: u64,
    served: Served,
}

/// Block `label` (its hash is keccak of the label, its transactions'
/// hashes keccak of `label/txN`) on top of `parent`.
fn block(label: &str, number: u64, parent: &str, txs: Vec<Tx>) -> Synth {
    let hash = named(label);
    let hashes: Vec<[u8; 32]> = (0..txs.len())
        .map(|i| named(&format!("{label}/tx{i}")))
        .collect();
    let header = json!({
        "number": q(number),
        "hash": hx(&hash),
        "parentHash": hx(&named(parent)),
        "timestamp": q(1_760_000_000 + number),
        "miner": hx(&[0u8; 20]),
        "transactions": hashes.iter().map(|h| hx(h)).collect::<Vec<_>>(),
    });
    let mut log_index = 0u64;
    let mut receipts = Vec::new();
    for (i, t) in txs.iter().enumerate() {
        let mut logs = Vec::new();
        for (address, topics, data) in &t.logs {
            logs.push(json!({
                "address": hx(address),
                "topics": topics.iter().map(|t| hx(t)).collect::<Vec<_>>(),
                "data": hx(data),
                "logIndex": q(log_index),
                "blockHash": hx(&hash),
                "blockNumber": q(number),
                "transactionHash": hx(&hashes[i]),
                "transactionIndex": q(i as u64),
                "removed": false,
            }));
            log_index += 1;
        }
        receipts.push(json!({
            "transactionHash": hx(&hashes[i]),
            "transactionIndex": q(i as u64),
            "blockHash": hx(&hash),
            "blockNumber": q(number),
            "status": if t.ok { "0x1" } else { "0x0" },
            "gasUsed": "0x5208",
            "logs": logs,
        }));
    }
    Synth {
        number,
        served: Served {
            header,
            receipts: Value::Array(receipts),
        },
    }
}

fn phase(head: u64, blocks: &[&Synth]) -> Phase {
    Phase {
        head,
        blocks: blocks
            .iter()
            .map(|b| (b.number, b.served.clone()))
            .collect(),
    }
}

/// A readable line per event; hashes by their labels.
fn describe(event: &CanonicalEvent, names: &BTreeMap<[u8; 32], String>) -> String {
    let name = |h: &[u8; 32]| names.get(h).cloned().unwrap_or_else(|| hx(h));
    let list = |blocks: &[(u64, [u8; 32])]| {
        blocks
            .iter()
            .map(|(n, h)| {
                let label = name(h);
                assert!(label.starts_with(&n.to_string()), "{n} {label}");
                label
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    match event {
        CanonicalEvent::Block {
            number,
            hash,
            parent_hash,
            timestamp_s,
            ..
        } => {
            if names.contains_key(hash) {
                assert_eq!(*timestamp_s, 1_760_000_000 + number);
            }
            format!("block {} <- {}", name(hash), name(parent_hash))
        }
        CanonicalEvent::Launch(l) => format!(
            "launch {:?} {} {} token {:?} creator {:?} buys {:?} first {:?}",
            l.venue,
            name(&l.block_hash),
            name(&l.tx_hash),
            l.token.map(|t| t[0]),
            l.creator.map(|c| c[0]),
            l.external_buys_after,
            l.first_external_buy_index
        ),
        CanonicalEvent::Reorg { orphaned, new_head } => {
            format!("reorg {} -> {}", list(orphaned), list(&[*new_head]))
        }
        CanonicalEvent::ReorgTooDeep { orphaned, new_head } => {
            format!("too deep {} -> {}", list(orphaned), list(&[*new_head]))
        }
        CanonicalEvent::Gap { from, to } => format!("gap {from}..={to}"),
        CanonicalEvent::Fault(fault) => format!("fault {fault:?}"),
    }
}

const DEV: [u8; 20] = [0xde; 20];
const BUYER: [u8; 20] = [0xb0; 20];
const T1: [u8; 20] = [0x71; 20];
const T2: [u8; 20] = [0x72; 20];
const T3: [u8; 20] = [0x73; 20];
const OTHER: [u8; 20] = [0x0e; 20];

#[test]
fn follows_reorgs_gaps_and_faults_in_exact_order() {
    // Fork A, then B from 101A (1-block reorg), C from 100A (3-block
    // reorg), D from 101C (4 blocks: deeper than reorg_depth 3).
    let a100 = block("100A", 100, "99", vec![tx(vec![])]);
    let a101 = block(
        "101A",
        101,
        "100A",
        vec![
            // The launch with the creator's own buy (not external).
            tx(vec![flap_launch(DEV, T1), flap_buy(T1, DEV, 7)]),
            tx(vec![flap_buy(T1, BUYER, 7)]),
            Tx {
                ok: false,
                logs: vec![flap_buy(T1, BUYER, 7)],
            },
            tx(vec![flap_buy(OTHER, BUYER, 7), flap_buy(T1, BUYER, 7)]),
        ],
    );
    let a102 = block("102A", 102, "101A", vec![]);
    let b102 = block("102B", 102, "101A", vec![tx(vec![])]);
    let b103 = block("103B", 103, "102B", vec![]);
    let c101 = block("101C", 101, "100A", vec![]);
    let c102 = block("102C", 102, "101C", vec![tx(vec![four_launch(DEV, T2)])]);
    let c103 = block("103C", 103, "102C", vec![]);
    let c104 = block("104C", 104, "103C", vec![]);
    let c105 = block("105C", 105, "104C", vec![]);
    let mut d = vec![block("102D", 102, "101C", vec![])];
    for n in 103..=114u64 {
        let txs = if n == 106 {
            // A later buy event of the token that is malformed.
            vec![
                tx(vec![flap_launch(DEV, T3)]),
                tx(vec![flap_buy(T3, BUYER, 6)]),
            ]
        } else {
            vec![]
        };
        d.push(block(&format!("{n}D"), n, &format!("{}D", n - 1), txs));
    }
    let d_upto = |head: u64| -> Vec<&Synth> {
        let mut chain = vec![&a100, &c101];
        chain.extend(d.iter().filter(|b| b.number <= head));
        chain
    };
    let script = Script {
        phases: vec![
            phase(100, &[&a100]),
            phase(102, &[&a100, &a101, &a102]),
            phase(103, &[&a100, &a101, &b102, &b103]),
            phase(104, &[&a100, &c101, &c102, &c103, &c104]),
            phase(105, &[&a100, &c101, &c102, &c103, &c104, &c105]),
            phase(106, &d_upto(106)),
            phase(112, &d_upto(112)),
            phase(114, &d_upto(114)),
        ],
        inject: vec![
            (Req::Receipts(101), 0, Reply::Close),
            (Req::BlockNumber, 3, Reply::Http429),
            (Req::Header(105), 0, Reply::Malformed),
            (Req::Receipts(113), 0, Reply::RpcError(-32000)),
            (Req::Header(114), 0, Reply::RpcError(-32005)),
        ],
        chunk: 37,
    };
    let mut names = BTreeMap::new();
    let mut labels: Vec<String> = ["99", "100A", "101A", "102A", "102B", "103B"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    labels.extend((101..=105).map(|n| format!("{n}C")));
    labels.extend((102..=114).map(|n| format!("{n}D")));
    for label in labels {
        names.insert(named(&label), label.clone());
        names.insert(named(&format!("{label}/tx0")), format!("{label}/tx0"));
    }

    let limits = limits();
    let outcome = follow_script(script, limits, None);
    let lines: Vec<String> = outcome
        .events
        .iter()
        .map(|(e, _)| describe(e, &names))
        .collect();
    let expected = [
        "block 100A <- 99",
        "block 101A <- 100A",
        "launch Flap 101A 101A/tx0 token Some(113) creator Some(222) buys Some(2) first Some(1)",
        "block 102A <- 101A",
        "reorg 102A -> 103B",
        "block 102B <- 101A",
        "block 103B <- 102B",
        "fault Request { call: BlockNumber, error: RateLimited(Http429) }",
        "reorg 101A,102B,103B -> 104C",
        "block 101C <- 100A",
        "block 102C <- 101C",
        "launch Four 102C 102C/tx0 token Some(114) creator Some(222) buys None first None",
        "block 103C <- 102C",
        "block 104C <- 103C",
        "fault Request { call: Header(105), error: MalformedJson }",
        "block 105C <- 104C",
        "too deep 102C,103C,104C,105C -> 106D",
        "block 106D <- 105D",
        "launch Flap 106D 106D/tx0 token Some(115) creator Some(222) buys None first None",
        "gap 107..=111",
        "block 112D <- 111D",
        "fault Request { call: Receipts(113), error: Rpc(-32000) }",
        "block 113D <- 112D",
        "fault Request { call: Header(114), error: RateLimited(RpcError(-32005)) }",
        "block 114D <- 113D",
    ];
    assert_eq!(lines, expected);

    // Every block was observed before it was reported; 113D keeps the time
    // its header was first held, before the failed receipts.
    let receipts_fault = outcome
        .events
        .iter()
        .find(|(e, _)| {
            matches!(
                e,
                CanonicalEvent::Fault(FollowerFault::Request {
                    call: Call::Receipts(113),
                    ..
                })
            )
        })
        .map(|(_, at)| *at)
        .unwrap();
    for (event, at) in &outcome.events {
        if let CanonicalEvent::Block {
            number, observed, ..
        } = event
        {
            assert!(observed <= at);
            if *number == 113 {
                assert!(*observed < receipts_fault);
            }
        }
    }
    // One kept connection, replaced after `Connection: close` and after
    // the 429; a JSON-level fault keeps it.
    assert_eq!(outcome.connections, 3);
    // The server set `stop` while answering; the loop ended within a tick.
    let stopped_after = outcome.returned - outcome.stop_set_at.unwrap();
    assert!(
        stopped_after < limits.read_tick * 4 + Duration::from_millis(100),
        "{stopped_after:?}"
    );
}

#[test]
fn stop_ends_a_hanging_request_and_the_poll_wait_within_a_bound() {
    let a100 = block("100A", 100, "99", vec![]);
    let bound = |l: &FollowerLimits| l.read_tick * 4 + Duration::from_millis(100);

    // A request the server never answers (deadline far away).
    let script = Script {
        phases: vec![phase(100, &[&a100])],
        inject: vec![(Req::BlockNumber, 1, Reply::Hang)],
        chunk: 64,
    };
    let limits = limits();
    let outcome = follow_script(script, limits, Some(Duration::from_millis(300)));
    assert_eq!(outcome.events.len(), 1, "only block 100A, no fault");
    let after = outcome.returned - outcome.stop_set_at.unwrap();
    assert!(after < bound(&limits), "hanging request: {after:?}");

    // The wait between polls.
    let script = Script {
        phases: vec![phase(100, &[&a100])],
        inject: vec![],
        chunk: 64,
    };
    let long_poll = FollowerLimits {
        poll: Duration::from_secs(60),
        ..limits
    };
    let outcome = follow_script(script, long_poll, Some(Duration::from_millis(300)));
    assert_eq!(outcome.events.len(), 1);
    let after = outcome.returned - outcome.stop_set_at.unwrap();
    assert!(after < bound(&long_poll), "poll wait: {after:?}");

    // Without stop, the hanging request ends at its deadline as a fault.
    let script = Script {
        phases: vec![phase(100, &[&a100])],
        inject: vec![(Req::BlockNumber, 1, Reply::Hang)],
        chunk: 64,
    };
    let short_deadline = FollowerLimits {
        request_timeout: Duration::from_millis(150),
        ..limits
    };
    let outcome = follow_script(script, short_deadline, None);
    let names = BTreeMap::from([
        (named("100A"), "100A".to_owned()),
        (named("99"), "99".to_owned()),
    ]);
    let lines: Vec<String> = outcome
        .events
        .iter()
        .map(|(e, _)| describe(e, &names))
        .collect();
    assert_eq!(
        lines,
        [
            "block 100A <- 99",
            "fault Request { call: BlockNumber, error: Transport(Timeout) }",
        ]
    );
}

#[test]
fn a_real_launch_block_through_the_whole_path() {
    let header: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/chain/block_129774331.header.json"
    ))
    .unwrap();
    let receipts: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/chain/block_129774331.receipts.json"
    ))
    .unwrap();
    let served = Served {
        header: header["result"].clone(),
        receipts: receipts["result"].clone(),
    };
    let script = Script {
        phases: vec![Phase {
            head: 129_774_331,
            blocks: BTreeMap::from([(129_774_331, served)]),
        }],
        inject: vec![],
        chunk: 4096,
    };
    let outcome = follow_script(script, limits(), None);
    let events: Vec<CanonicalEvent> = outcome.events.into_iter().map(|(e, _)| e).collect();
    let [
        CanonicalEvent::Block {
            number,
            hash,
            parent_hash,
            timestamp_s,
            ..
        },
        CanonicalEvent::Launch(first),
        CanonicalEvent::Launch(second),
    ] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!(*number, 129_774_331);
    assert_eq!(
        *hash,
        hex!("b46720d61774811f42474df7f45d98e2967509025fe10a5f67c9eb9f381e12ab")
    );
    assert_eq!(
        hx(parent_hash),
        header["result"]["parentHash"].as_str().unwrap()
    );
    assert_eq!(
        q(*timestamp_s),
        header["result"]["timestamp"].as_str().unwrap()
    );
    assert_eq!(
        (
            first.tx_index,
            first.external_buys_after,
            first.first_external_buy_index
        ),
        (1, Some(1), Some(2))
    );
    assert_eq!(
        first.token,
        Some(hex!("bbe126a97529fc7ace5c8a2167555f41df2d7777"))
    );
    assert_eq!((second.tx_index, second.external_buys_after), (26, Some(0)));
    assert_eq!(
        second.token,
        Some(hex!("e5499b52f4f70ab9efd09880342a53f520dc7777"))
    );
}
