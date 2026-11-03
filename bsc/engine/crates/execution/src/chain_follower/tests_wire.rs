//! The HTTP/1.1 framing over an in-memory stream, the strict JSON-RPC
//! shapes, the limits and the configuration refusals.

use std::io::{self, Cursor, Read, Write};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::http::{Bounds, Response, round_trip};
use super::jsonrpc::{
    BlockHead, data_fixed, parse_block, parse_quantity, parse_receipt, parse_receipts,
    quantity_u64, request,
};
use super::*;
use serde_json::Value;

/// A stream that answers with `input` and records what was written.
struct Canned {
    input: Cursor<Vec<u8>>,
    written: Vec<u8>,
}

impl Read for Canned {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

impl Write for Canned {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.written.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn exchange(response: &[u8], max: usize) -> (Result<Response, RequestError>, Vec<u8>) {
    let mut stream = Canned {
        input: Cursor::new(response.to_vec()),
        written: Vec::new(),
    };
    let stop = AtomicBool::new(false);
    let bounds = Bounds {
        started: Instant::now(),
        timeout: Duration::from_secs(5),
        max,
    };
    let result = round_trip(
        &mut stream,
        "node.example:8545",
        "/rpc",
        b"{}",
        bounds,
        &stop,
    )
    .map_err(|f| f.error);
    (result, stream.written)
}

fn ok(body: &[u8], close: bool) -> Result<Response, RequestError> {
    Ok(Response {
        status: 200,
        body: body.to_vec(),
        close,
    })
}

#[test]
fn request_is_one_post_with_host_and_length() {
    let (_, written) = exchange(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n", 1024);
    assert_eq!(
        written,
        b"POST /rpc HTTP/1.1\r\nHost: node.example:8545\r\nContent-Type: application/json\r\n\
          Content-Length: 2\r\n\r\n{}"
    );
}

#[test]
fn content_length_and_chunked_bodies() {
    let plain =
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 5\r\n\r\nhello";
    assert_eq!(exchange(plain, 1024).0, ok(b"hello", false));
    let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                    3;ext=1\r\nhel\r\n2\r\nlo\r\nA\r\n, chunked!\r\n0\r\nX-Trailer: 1\r\n\r\n";
    assert_eq!(exchange(chunked, 1024).0, ok(b"hello, chunked!", false));
    let close = b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 2\r\n\r\nok";
    assert_eq!(exchange(close, 1024).0, ok(b"ok", true));
    let http10 = b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok";
    assert_eq!(exchange(http10, 1024).0, ok(b"ok", true));
}

#[test]
fn non_200_statuses_are_returned_without_their_body() {
    let limited = b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\n\r\n";
    assert_eq!(
        exchange(limited, 1024).0,
        Ok(Response {
            status: 429,
            body: Vec::new(),
            close: true
        })
    );
    let unavailable = b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 3\r\n\r\nbad";
    assert_eq!(exchange(unavailable, 1024).0.map(|r| r.status), Ok(503));
}

#[test]
fn framing_violations_are_typed() {
    let cases: [(&[u8], RequestError); 13] = [
        (
            b"HTTP/2 200 OK\r\nContent-Length: 0\r\n\r\n",
            RequestError::Http(HttpFault::StatusLine),
        ),
        (
            b"HTTP/1.1 2x0 OK\r\nContent-Length: 0\r\n\r\n",
            RequestError::Http(HttpFault::StatusLine),
        ),
        (
            b"HTTP/1.1 200 OK\r\nno colon\r\n\r\n",
            RequestError::Http(HttpFault::Header),
        ),
        (
            b"HTTP/1.1 200 OK\r\n\r\n",
            RequestError::Http(HttpFault::Framing),
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n",
            RequestError::Http(HttpFault::Framing),
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\nabc",
            RequestError::Http(HttpFault::Framing),
        ),
        // Audit E 4.4 (the builders' rule): one length, given once.
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\nab",
            RequestError::Http(HttpFault::Framing),
        ),
        // RFC 9112: a bare LF ends no line.
        (
            b"HTTP/1.1 200 OK\nX: 1\r\nContent-Length: 2\r\n\r\nab",
            RequestError::Http(HttpFault::StatusLine),
        ),
        (
            b"HTTP/1.1 200 OK\r\nX: a\nb\r\nContent-Length: 2\r\n\r\nab",
            RequestError::Http(HttpFault::Header),
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 2\r\n\r\nab",
            RequestError::Http(HttpFault::Coding),
        ),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nabc\r\n0\r\n\r\n",
            RequestError::Http(HttpFault::Chunk),
        ),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nab\r\n0\r\n\r\n",
            RequestError::Http(HttpFault::Chunk),
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nabEXTRA",
            RequestError::Http(HttpFault::Trailing),
        ),
    ];
    for (response, expected) in cases {
        assert_eq!(
            exchange(response, 1024).0,
            Err(expected),
            "{}",
            String::from_utf8_lossy(response)
        );
    }
}

#[test]
fn size_bound_and_early_end() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nshort";
    // The announced body does not fit the bound: refused before reading it.
    assert_eq!(
        exchange(response, 64).0,
        Err(RequestError::Http(HttpFault::TooLarge))
    );
    // It fits, but the peer ends early.
    assert_eq!(
        exchange(response, 1024).0,
        Err(RequestError::Transport(DisconnectReason::Closed(None)))
    );
    // A head that never ends within the bound.
    let endless = [b"HTTP/1.1 200 OK\r\n".as_slice(), &[b'x'; 200]].concat();
    assert_eq!(
        exchange(&endless, 64).0,
        Err(RequestError::Http(HttpFault::TooLarge))
    );
    let chunk = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffff\r\nab";
    assert_eq!(
        exchange(chunk, 128).0,
        Err(RequestError::Http(HttpFault::TooLarge))
    );
}

#[test]
fn request_bodies_are_fixed_templates() {
    assert_eq!(
        request(7, Call::BlockNumber),
        r#"{"jsonrpc":"2.0","id":7,"method":"eth_blockNumber","params":[]}"#
    );
    assert_eq!(
        request(8, Call::Block(129_781_248)),
        r#"{"jsonrpc":"2.0","id":8,"method":"eth_getBlockByNumber","params":["0x7bc4e00",true]}"#
    );
    assert_eq!(
        request(9, Call::Receipts(0)),
        r#"{"jsonrpc":"2.0","id":9,"method":"eth_getBlockReceipts","params":["0x0"]}"#
    );
    // REV.1.0 R3b.
    let mut hash = [0u8; 32];
    hash[0] = 0xab;
    hash[31] = 0x01;
    assert_eq!(
        request(10, Call::Receipt(hash)),
        r#"{"jsonrpc":"2.0","id":10,"method":"eth_getTransactionReceipt","params":["0xab00000000000000000000000000000000000000000000000000000000000001"]}"#
    );
}

#[test]
fn quantities_and_data_are_canonical_lowercase() {
    assert_eq!(quantity_u64("0x0"), Some(0));
    assert_eq!(quantity_u64("0x7bc4e00"), Some(129_781_248));
    assert_eq!(quantity_u64("0xffffffffffffffff"), Some(u64::MAX));
    for bad in [
        "0x",
        "0x00",
        "0x01",
        "7b",
        "0X1",
        "0xA",
        "0x10000000000000000",
        "0x+1",
    ] {
        assert_eq!(quantity_u64(bad), None, "{bad}");
    }
    assert_eq!(data_fixed::<2>("0xab01"), Some([0xab, 0x01]));
    for bad in ["0xab0", "0xab0102", "0xAB01", "ab01"] {
        assert_eq!(data_fixed::<2>(bad), None, "{bad}");
    }
}

#[test]
fn envelope_id_and_errors() {
    assert_eq!(
        parse_quantity(br#"{"jsonrpc":"2.0","id":3,"result":"0x10"}"#, 3),
        Ok(16)
    );
    let cases: [(&[u8], RequestError); 8] = [
        (br#"{"jsonrpc":"2.0","id":3,"#, RequestError::MalformedJson),
        (br#"[1]"#, RequestError::Shape(Field::Envelope)),
        (
            br#"{"id":3,"result":"0x10"}"#,
            RequestError::Shape(Field::Envelope),
        ),
        (
            br#"{"jsonrpc":"2.0","id":4,"result":"0x10"}"#,
            RequestError::Shape(Field::Id),
        ),
        (
            br#"{"jsonrpc":"2.0","id":3}"#,
            RequestError::Shape(Field::Result),
        ),
        (
            br#"{"jsonrpc":"2.0","id":3,"result":"0x010"}"#,
            RequestError::Shape(Field::Result),
        ),
        (
            br#"{"jsonrpc":"2.0","id":null,"error":{"code":-32005,"message":"limit exceeded"}}"#,
            RequestError::RateLimited(RateLimitSignal::RpcError(-32005)),
        ),
        (
            br#"{"jsonrpc":"2.0","id":3,"error":{"code":-32000,"message":"x"}}"#,
            RequestError::Rpc(-32000),
        ),
    ];
    for (body, expected) in cases {
        assert_eq!(
            parse_quantity(body, 3),
            Err(expected),
            "{}",
            String::from_utf8_lossy(body)
        );
    }
}

const HASH_A: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
const HASH_B: &str = "0x2222222222222222222222222222222222222222222222222222222222222222";
/// A `mixHash` of 200 (ms).
const MIX_200: &str = "0x00000000000000000000000000000000000000000000000000000000000000c8";

const SENDER: &str = "0x3333333333333333333333333333333333333333";

/// Block `number` on `HASH_B` with `transactions` (JSON), `mix` its
/// `mixHash`; not yet sealed.
fn header_json(number: &str, transactions: &str, mix: &str) -> Value {
    serde_json::from_str(&format!(
        r#"{{"number":"{number}","hash":"{HASH_A}","parentHash":"{HASH_B}","timestamp":"0x6ade7af4","mixHash":"{mix}","transactions":{transactions}}}"#
    ))
    .unwrap()
}

/// The answer carrying `block` as it is.
fn answer(block: &Value) -> String {
    serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": block}).to_string()
}

/// R7: the answer carrying `block` sealed (consistent in itself).
fn sealed(mut block: Value) -> String {
    super::tests_seal::seal(&mut block, None);
    answer(&block)
}

fn header_body(number: &str, transactions: &str) -> String {
    sealed(header_json(number, transactions, MIX_200))
}

/// One signed transaction object of block 5 at index 0, nonce 7.
fn tx_value() -> Value {
    let mut tx = super::tests_seal::synth_tx(b"wire", 7, 56);
    tx["transactionIndex"] = Value::from("0x0");
    tx["blockHash"] = Value::from(HASH_A);
    tx["blockNumber"] = Value::from("0x5");
    tx
}

/// The typed block of a body (its WSS conversions left out).
fn head(body: &str, number: u64) -> Result<Option<BlockHead>, RequestError> {
    parse_block(body.as_bytes(), 1, number).map(|b| b.map(|b| b.head))
}

#[test]
fn block_shapes() {
    let one = format!("[{}]", tx_value());
    let good = parse_block(header_body("0x5", &one).as_bytes(), 1, 5)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            good.head.header.number,
            good.head.header.timestamp_s,
            good.head.header.block_ms,
        ),
        (5, 0x6ade_7af4, 0x6ade_7af4 * 1000 + 200)
    );
    let tx = good.head.txs[0];
    assert_eq!(
        (tx.hash, tx.from, tx.nonce),
        (
            data_fixed(tx_value()["hash"].as_str().unwrap()).unwrap(),
            super::tests_seal::synth_sender(),
            7
        )
    );
    assert!(matches!(good.objects[..], [Ok(_)]));
    assert_eq!(
        head(r#"{"jsonrpc":"2.0","id":1,"result":null}"#, 5),
        Ok(None)
    );
    assert_eq!(
        head(&header_body("0x6", "[]"), 5),
        Err(RequestError::Shape(Field::Number))
    );
    // The hash-only form (`false`) is refused.
    assert_eq!(
        head(&header_body("0x5", &format!(r#"["{HASH_B}"]"#)), 5),
        Err(RequestError::Shape(Field::Transactions))
    );
    let mut short_hash = header_json("0x5", "[]", MIX_200);
    super::tests_seal::seal(&mut short_hash, None);
    short_hash["hash"] = Value::from("0x11");
    assert_eq!(
        head(&answer(&short_hash), 5),
        Err(RequestError::Shape(Field::Hash))
    );
    // Each object is bound to the block and its position, and strict.
    let cases = [
        ("transactionIndex", "0x1", Field::TxBinding),
        ("blockNumber", "0x6", Field::TxBinding),
        ("blockHash", HASH_B, Field::TxBinding),
        ("from", "0x3", Field::From),
        ("nonce", "0x07", Field::Nonce),
        ("hash", "0x2", Field::TxHash),
    ];
    for (name, value, field) in cases {
        let mut block = header_json("0x5", &one, MIX_200);
        super::tests_seal::seal(&mut block, None);
        block["transactions"][0][name] = Value::from(value);
        assert_eq!(
            head(&answer(&block), 5),
            Err(RequestError::Shape(field)),
            "{name}"
        );
    }
}

/// bsc-geth v1.7.8 `Header.MilliTimestamp()`: a zero `mixHash` adds 0,
/// otherwise its 256-bit value, which must stay below 1000.
#[test]
fn mix_hash_milliseconds() {
    let with_mix = |mix: &str| sealed(header_json("0x5", "[]", mix));
    let ms = |mix: &str| {
        head(&with_mix(mix), 5).map(|b| b.unwrap().header.block_ms - 0x6ade_7af4 * 1000)
    };
    let word = |tail: &str| format!("0x{tail:0>64}");
    assert_eq!(ms(&word("0")), Ok(0));
    assert_eq!(ms(&word("1")), Ok(1));
    assert_eq!(ms(&word("3e7")), Ok(999));
    for bad in [
        word("3e8"),
        word("ffff"),
        format!("0x01{}", "0".repeat(62)),
        "0xc8".to_owned(),
        word("3E7"),
    ] {
        assert_eq!(ms(&bad), Err(RequestError::Shape(Field::MixHash)), "{bad}");
    }
    let mut missing = header_json("0x5", "[]", MIX_200);
    missing.as_object_mut().unwrap().remove("mixHash");
    assert_eq!(
        head(&sealed(missing), 5),
        Err(RequestError::Shape(Field::MixHash))
    );
    let overflow = with_mix(&word("1")).replace("0x6ade7af4", "0xffffffffffffffff");
    assert_eq!(
        head(&overflow, 5),
        Err(RequestError::Shape(Field::Timestamp))
    );
}

fn receipt_body(log: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":[{{"transactionHash":"{HASH_B}","transactionIndex":"0x0","from":"{SENDER}","blockHash":"{HASH_A}","blockNumber":"0x5","status":"0x1","type":"0x2","cumulativeGasUsed":"0xa410","logs":[{log}]}}]}}"#
    )
}

fn log_json(block_number: &str, removed: &str, topics: usize) -> String {
    let topics = vec![format!(r#""{HASH_A}""#); topics].join(",");
    format!(
        r#"{{"address":"0x5c952063c7fc8610ffdb798152d69f0b9550762b","topics":[{topics}],"data":"0x00ff","logIndex":"0x0","blockHash":"{HASH_A}","blockNumber":"{block_number}","transactionHash":"{HASH_B}","transactionIndex":"0x0","removed":{removed}}}"#
    )
}

/// REV.1.0 R3b: one receipt by hash, `null` when the node knows none; any
/// other `result` is refused, a receipt is checked as in a block's list.
#[test]
fn a_receipt_by_hash_or_none() {
    let list = receipt_body(&log_json("0x5", "false", 1));
    // The list's one receipt object as the whole `result`.
    let start = list.find(r#""result":["#).unwrap() + r#""result":["#.len();
    let one = format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{}}}"#,
        &list[start..list.len() - "]}".len()]
    );
    let r = parse_receipt(one.as_bytes(), 1).unwrap().unwrap();
    assert_eq!(r, parse_receipts(list.as_bytes(), 1).unwrap().unwrap()[0]);
    assert_eq!(
        parse_receipt(br#"{"jsonrpc":"2.0","id":1,"result":null}"#, 1),
        Ok(None)
    );
    assert_eq!(
        parse_receipt(list.as_bytes(), 1),
        Err(RequestError::Shape(Field::Result))
    );
    let removed = one.replace(r#""removed":false"#, r#""removed":true"#);
    assert_eq!(
        parse_receipt(removed.as_bytes(), 1),
        Err(RequestError::Shape(Field::LogRemoved))
    );
}

#[test]
fn receipt_and_log_shapes() {
    let receipts = parse_receipts(receipt_body(&log_json("0x5", "false", 4)).as_bytes(), 1)
        .unwrap()
        .unwrap();
    assert_eq!(receipts.len(), 1);
    assert!(receipts[0].succeeded);
    assert_eq!(receipts[0].logs[0].data, vec![0x00, 0xff]);
    assert_eq!(receipts[0].logs[0].topics.len(), 4);
    let cases = [
        (log_json("0x6", "false", 1), Field::LogBinding),
        (log_json("0x5", "true", 1), Field::LogRemoved),
        (log_json("0x5", "null", 1), Field::LogRemoved),
        (log_json("0x5", "false", 5), Field::LogTopics),
    ];
    for (log, field) in cases {
        assert_eq!(
            parse_receipts(receipt_body(&log).as_bytes(), 1),
            Err(RequestError::Shape(field)),
            "{log}"
        );
    }
    // R7: its transaction's type and the block's gas up to it.
    assert_eq!(
        (receipts[0].tx_type, receipts[0].cumulative_gas),
        (2, 42_000)
    );
    for (from, to) in [
        (r#""type":"0x2","#, ""),
        (r#""type":"0x2""#, r#""type":"0x100""#),
        (
            r#""cumulativeGasUsed":"0xa410""#,
            r#""cumulativeGasUsed":"0x0a410""#,
        ),
    ] {
        assert_eq!(
            parse_receipts(receipt_body("").replace(from, to).as_bytes(), 1),
            Err(RequestError::Shape(Field::ReceiptFields)),
            "{to}"
        );
    }
    let failed = receipt_body("").replace(r#""status":"0x1""#, r#""status":"0x0""#);
    assert!(!parse_receipts(failed.as_bytes(), 1).unwrap().unwrap()[0].succeeded);
    let odd = receipt_body("").replace(r#""status":"0x1""#, r#""status":"0x2""#);
    assert_eq!(
        parse_receipts(odd.as_bytes(), 1),
        Err(RequestError::Shape(Field::Status))
    );
    let no_sender = receipt_body("").replace(&format!(r#""from":"{SENDER}","#), "");
    assert_eq!(
        parse_receipts(no_sender.as_bytes(), 1),
        Err(RequestError::Shape(Field::From))
    );
    assert_eq!(
        parse_receipts(receipt_body("").as_bytes(), 1)
            .unwrap()
            .unwrap()[0]
            .from,
        data_fixed::<20>(SENDER).unwrap()
    );
}

/// Plan 28.10.: a receipt's gas (used, effective price) is read when the
/// node states both; a malformed one is refused, never guessed.
#[test]
fn a_receipt_states_its_gas_cost() {
    let none = parse_receipts(receipt_body("").as_bytes(), 1)
        .unwrap()
        .unwrap();
    assert_eq!(none[0].cost, None);
    let with = receipt_body("").replace(
        r#""status":"0x1""#,
        r#""status":"0x1","gasUsed":"0x5208","effectiveGasPrice":"0x2faf080""#,
    );
    let r = parse_receipts(with.as_bytes(), 1).unwrap().unwrap();
    let cost = r[0].cost.unwrap();
    assert_eq!(
        (cost.gas_used, cost.effective_gas_price),
        (21_000, 50_000_000)
    );
    assert_eq!(cost.wei(), 1_050_000_000_000);
    let bad = with.replace("0x5208", "5208");
    assert_eq!(
        parse_receipts(bad.as_bytes(), 1),
        Err(RequestError::Shape(Field::GasCost))
    );
}

fn limits() -> FollowerLimits {
    FollowerLimits {
        poll: Duration::from_millis(500),
        reorg_depth: 8,
        max_catch_up: 64,
        connect_timeout: Duration::from_secs(5),
        handshake_timeout: Duration::from_secs(10),
        read_tick: Duration::from_millis(100),
        write_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(10),
        max_response_bytes: 8 << 20,
        backoff_initial: Duration::from_millis(250),
        backoff_max: Duration::from_secs(5),
        tx_attempts: 3,
        max_launch_calldata: 16_384,
        failover_after: 2,
        stall_after: Duration::from_secs(60),
    }
}

#[test]
fn limits_and_endpoint_are_refused_before_any_connect() {
    assert_eq!(limits().validate(), Ok(()));
    let zero_poll = FollowerLimits {
        poll: Duration::ZERO,
        ..limits()
    };
    assert_eq!(zero_poll.validate(), Err(LimitsError::ZeroDuration("poll")));
    let no_depth = FollowerLimits {
        reorg_depth: 0,
        ..limits()
    };
    assert_eq!(no_depth.validate(), Err(LimitsError::Zero("reorg_depth")));
    let inverted = FollowerLimits {
        backoff_initial: Duration::from_secs(9),
        ..limits()
    };
    assert_eq!(
        inverted.validate(),
        Err(LimitsError::BackoffInitialAboveMax)
    );

    let stop = AtomicBool::new(false);
    let mut sink = |_: CanonicalEvent| panic!("no event before a connect");
    let refused = [
        ("http://node.example", FollowerConfigError::NotHttps),
        ("wss://node.example", FollowerConfigError::NotHttps),
        (
            "https://user@node.example",
            FollowerConfigError::Url(UrlError::Credentials),
        ),
        (
            "https://node.example/?key=1",
            FollowerConfigError::Url(UrlError::QueryOrFragment),
        ),
        (
            "https://node.example:0",
            FollowerConfigError::Url(UrlError::Port),
        ),
        ("https://", FollowerConfigError::Url(UrlError::Host)),
    ];
    for (url, expected) in refused {
        assert_eq!(
            run_follower(
                &[url.to_string()],
                &limits(),
                &[],
                &NonceWatch::default(),
                &stop,
                &mut sink
            ),
            Err(expected),
            "{url}"
        );
    }
    assert_eq!(
        run_follower(
            &["https://node.example".to_string()],
            &zero_poll,
            &[],
            &NonceWatch::default(),
            &stop,
            &mut sink
        ),
        Err(FollowerConfigError::Limits(LimitsError::ZeroDuration(
            "poll"
        )))
    );
}

#[test]
fn host_and_target_come_from_the_normalized_url() {
    assert_eq!(
        https_endpoint("https://node.example:8545/v1/rpc")
            .unwrap()
            .authority_and_path(),
        ("node.example:8545".to_owned(), "/v1/rpc".to_owned())
    );
    assert_eq!(
        https_endpoint("https://node.example")
            .unwrap()
            .authority_and_path(),
        ("node.example".to_owned(), "/".to_owned())
    );
    // Audit E 5.14: an https:// endpoint is one, never rewritten as wss://.
    assert_eq!(
        https_endpoint("https://node.example/v1").unwrap().url(),
        "https://node.example/v1"
    );
    assert!(matches!(
        https_endpoint("wss://node.example/"),
        Err(FollowerConfigError::NotHttps)
    ));
}
