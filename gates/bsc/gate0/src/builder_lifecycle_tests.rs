use super::*;
use darkbot_gate0::intent_lane::Error as LaneError;
use std::io::{Cursor, Error, ErrorKind};

fn context() -> Ctx {
    let fi: Value = serde_json::from_str(include_str!("../evidence/g03_rust_inputs.json")).unwrap();
    let fo: Value = serde_json::from_str(include_str!("../evidence/g03_rust_output.json")).unwrap();
    let launch = hex::decode(
        fi["launch_calldata"]
            .as_str()
            .unwrap()
            .trim_start_matches("0x"),
    )
    .unwrap();

    assert_eq!(
        keccak256(&launch).to_string(),
        fi["launch_calldata_keccak"].as_str().unwrap()
    );

    let init = hex::decode(
        "3d602d80600a3d3981f3363d3d373d3d3d363d73024f18294970b5c76c0691b87f138a03171564225af43d82803e903d91602b57fd5bf3"
    ).unwrap();

    let init_hash = keccak256(init);

    // Fixed NON-FUNDED test key used only to benchmark secp256k1 signing cost.
    let test_key = [0x11u8; 32];
    let signer = SigningKey::from_bytes((&test_key).into()).unwrap();

    let ctx = Ctx {
        launch,
        portal: Address::from_str("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0").unwrap(),
        init_hash: init_hash.into(),
        sqrt: n(fi["sqrt_price_x96"].as_str().unwrap()),
        liquidity: n(fi["liquidity"].as_str().unwrap()),
        input: n(fi["native_input"].as_str().unwrap()),
        timestamp: n(fi["timestamp"].as_str().unwrap()),
        slippage_bps: fi["test_slippage_bps"]
            .as_u64()
            .unwrap()
            .try_into()
            .unwrap(),
        signer,
        expected_token: Address::from_str(fi["expected_token"].as_str().unwrap()).unwrap(),
        expected_output: n(fo["expected_output"].as_str().unwrap()),
        expected_min: n(fo["min_output"].as_str().unwrap()),
        expected_calldata: hex::decode(fo["calldata"].as_str().unwrap().trim_start_matches("0x"))
            .unwrap(),
    };

    ctx
}

struct MemoryTransport {
    response: Cursor<Vec<u8>>,
    written: Vec<u8>,
    fail_after: Option<usize>,
    flush_error: bool,
}
impl MemoryTransport {
    fn new(body: &str) -> Self {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .into_bytes();
        Self {
            response: Cursor::new(response),
            written: vec![],
            fail_after: None,
            flush_error: false,
        }
    }
}
impl Read for MemoryTransport {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.response.read(b)
    }
}
impl Write for MemoryTransport {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let n = if let Some(limit) = self.fail_after {
            if self.written.len() >= limit {
                return Err(Error::new(
                    ErrorKind::ConnectionReset,
                    "injected partial write",
                ));
            }
            b.len().min(limit - self.written.len())
        } else {
            b.len()
        };
        self.written.extend_from_slice(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if self.flush_error {
            Err(Error::other("injected flush failure"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn both_existing_builder_paths_consume_permit_without_confirming_http_or_ack() {
    let ctx = context();
    for transport in [
        TransportKind::FortyEightClubBundle,
        TransportKind::BlockRazorRpcBundle,
    ] {
        for ack in [false, true] {
            let body = if ack {
                r#"{"jsonrpc":"2.0","id":2,"result":"0x1234"}"#
            } else {
                r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"transaction type not supported"}}"#
            };
            let mut sink = MemoryTransport::new(body);
            let mut lane = IntentLane::new(56, sender(&ctx), 814, 2);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_once(&ctx, &mut sink, 814, true, transport, &mut lane)
            }));
            assert_eq!(
                result.is_err(),
                ack,
                "invalid-wire probe must reject even a successful ACK"
            );
            let raw = String::from_utf8(sink.written.clone()).unwrap();
            let body = raw.split_once("\r\n\r\n").unwrap().1;
            let request: Value = serde_json::from_str(body).unwrap();
            assert_eq!(request["method"], "eth_sendBundle");
            assert_eq!(
                request["params"][0]["noMerge"],
                if transport == TransportKind::FortyEightClubBundle {
                    json!(true)
                } else {
                    Value::Null
                }
            );
            assert!(
                request["params"][0]["txs"][0]
                    .as_str()
                    .unwrap()
                    .starts_with("0x00")
            );
            let next = lane.reserve(Address::repeat_byte(0x99)).unwrap();
            lane.prepare(next, keccak256(b"next")).unwrap();
            // If HTTP/ACK had confirmed the head, this would return the next permit.
            assert_eq!(lane.begin_send(), Err(LaneError::NotReady));
        }
    }
}

#[test]
fn consumed_permit_cannot_retry_partial_write_or_flush_failure() {
    for flush in [false, true] {
        let ctx = context();
        let mut lane = IntentLane::new(56, sender(&ctx), 814, 1);
        let ticket = lane.reserve(ctx.expected_token).unwrap();
        let hash = keccak256(b"signed fixture");
        lane.prepare(ticket, hash).unwrap();
        let mut sink = MemoryTransport::new("{}");
        sink.fail_after = if flush { None } else { Some(5) };
        sink.flush_error = flush;
        assert!(
            guarded_transport_write(&mut lane, ticket, hash, b"request-payload", &mut sink)
                .is_err()
        );
        let bytes = sink.written.len();
        assert!(bytes > 0);
        assert_eq!(lane.begin_send(), Err(LaneError::Halted));
        assert!(guarded_transport_write(&mut lane, ticket, hash, b"retry", &mut sink).is_err());
        assert_eq!(sink.written.len(), bytes);
    }
}

#[test]
fn mismatched_request_permit_never_reaches_writer() {
    let ctx = context();
    let mut lane = IntentLane::new(56, sender(&ctx), 814, 1);
    let ticket = lane.reserve(ctx.expected_token).unwrap();
    lane.prepare(ticket, keccak256(b"right")).unwrap();
    let mut sink = MemoryTransport::new("{}");
    assert!(
        guarded_transport_write(
            &mut lane,
            ticket,
            keccak256(b"wrong"),
            b"request",
            &mut sink
        )
        .is_err()
    );
    assert!(sink.written.is_empty());
    assert_eq!(lane.begin_send(), Err(LaneError::Halted));
}

#[test]
fn wrong_sender_lane_is_rejected_before_any_transport_write() {
    let ctx = context();
    let mut lane = IntentLane::new(56, Address::repeat_byte(0x77), 814, 1);
    let mut sink = MemoryTransport::new("{}");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_once(
            &ctx,
            &mut sink,
            814,
            true,
            TransportKind::BlockRazorRpcBundle,
            &mut lane,
        )
    }));
    assert!(result.is_err());
    assert!(sink.written.is_empty());
}

#[test]
fn persistent_fence_covers_existing_builder_write_and_restart() {
    use darkbot_gate0::restart_fence::{FencedLane, OpenError};
    let ctx = context();
    let root = std::env::temp_dir().join(format!(
        "darkbot-builder-fence-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut armed = FencedLane::arm(
        &root,
        56,
        sender(&ctx),
        814,
        1,
        alloy_primitives::B256::repeat_byte(1),
    )
    .unwrap();
    let mut sink = MemoryTransport::new(
        r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"transaction type not supported"}}"#,
    );
    run_once(
        &ctx,
        &mut sink,
        814,
        true,
        TransportKind::BlockRazorRpcBundle,
        armed.for_transport(),
    );
    assert!(!sink.written.is_empty());
    drop(armed);
    assert!(matches!(
        FencedLane::arm(
            &root,
            56,
            sender(&ctx),
            814,
            1,
            alloy_primitives::B256::repeat_byte(1)
        ),
        Err(OpenError::Quarantined(_))
    ));
    std::fs::remove_dir_all(root).unwrap();
}
