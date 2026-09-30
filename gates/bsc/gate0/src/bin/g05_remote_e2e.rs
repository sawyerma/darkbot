use darkbot_gate0::intent_lane::{IntentLane, Ticket};
use alloy_primitives::{keccak256, Address, Bytes, U256, U512};
use alloy_sol_types::{sol, SolCall, SolValue};
use k256::ecdsa::SigningKey;
use serde_json::{json, Value};
use rustls::{
    pki_types::ServerName,
    ClientConfig,
    ClientConnection,
    RootCertStore,
    StreamOwned,
};
use std::{
    env,
    fs,
    hint::black_box,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    str::FromStr,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

sol! {
    struct LaunchParams {
        string name;
        string symbol;
        string uri;
        uint8 dexThresh;
        bytes32 salt;
        uint8 migratorType;
        address quoteToken;
        uint256 quoteAmt;
        address beneficiary;
        bytes permit;
        bytes32 extensionId;
        bytes extensionData;
        uint8 dexId;
        uint8 lpFeeProfile;
        uint16 buyTax;
        uint16 sellTax;
        uint64 taxDuration;
        uint64 antiFarmerDuration;
        uint16 mktBps;
        uint16 deflationBps;
        uint16 dividendBps;
        uint16 lpBps;
        uint256 minimumShareBalance;
        address dividendToken;
        address commissionReceiver;
        uint8 tokenVersion;
    }

    function newTokenV6(LaunchParams params) external payable returns(address);

    struct SwapParams {
        address inputToken;
        address outputToken;
        uint256 inputAmount;
        uint256 minOutputAmount;
        bytes permit;
    }

    function swapExactInput(SwapParams params) external payable returns(uint256);
}

fn n(s: &str) -> U256 {
    U256::from_str(s).unwrap()
}

fn wide(x: U256) -> U512 {
    U512::from(x)
}

fn narrow(x: U512) -> U256 {
    assert!(x <= U512::from(U256::MAX));
    U256::from(x)
}

fn mul_div(a: U256, b: U256, d: U256) -> U256 {
    narrow(wide(a) * wide(b) / wide(d))
}

fn ceil_div(a: U256, b: U256) -> U256 {
    let q = a / b;
    if a % b == U256::ZERO {
        q
    } else {
        q + U256::ONE
    }
}

fn min_output(expected: U256, bps: u16) -> U256 {
    mul_div(
        expected,
        U256::from(10000u64 - bps as u64),
        U256::from(10000u64),
    )
}

fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    if b.len() == 1 && b[0] < 0x80 {
        return b.to_vec();
    }

    if b.len() <= 55 {
        let mut out = Vec::with_capacity(1 + b.len());
        out.push(0x80 + b.len() as u8);
        out.extend_from_slice(b);
        return out;
    }

    let len_bytes = b.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];

    let mut out = Vec::with_capacity(1 + lb.len() + b.len());
    out.push(0xb7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(b);
    out
}

fn rlp_u64(v: u64) -> Vec<u8> {
    if v == 0 {
        return rlp_bytes(&[]);
    }

    let b = v.to_be_bytes();
    let first = b.iter().position(|x| *x != 0).unwrap();
    rlp_bytes(&b[first..])
}

fn rlp_u256(v: U256) -> Vec<u8> {
    if v == U256::ZERO {
        return rlp_bytes(&[]);
    }

    let b = v.to_be_bytes::<32>();
    let first = b.iter().position(|x| *x != 0).unwrap();
    rlp_bytes(&b[first..])
}

fn rlp_list(items: Vec<Vec<u8>>) -> Vec<u8> {
    let payload_len: usize = items.iter().map(Vec::len).sum();
    let mut payload = Vec::with_capacity(payload_len);

    for item in items {
        payload.extend_from_slice(&item);
    }

    if payload.len() <= 55 {
        let mut out = Vec::with_capacity(1 + payload.len());
        out.push(0xc0 + payload.len() as u8);
        out.extend_from_slice(&payload);
        return out;
    }

    let len_bytes = payload.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];

    let mut out = Vec::with_capacity(1 + lb.len() + payload.len());
    out.push(0xf7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(&payload);
    out
}


const PUBLIC_RPC_HOST: &str = "bsc-dataseed.bnbchain.org";
const FORTY_EIGHT_CLUB_HOST: &str = "puissant-builder.48.club";
const BLOCKRAZOR_RPC_HOST: &str = "bsc.blockrazor.xyz";
const HTTPS_PORT: u16 = 443;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransportKind {
    PublicRpc,
    FortyEightClubBundle,
    BlockRazorRpcBundle,
}

impl TransportKind {
    fn host(self) -> &'static str {
        match self {
            Self::PublicRpc => PUBLIC_RPC_HOST,
            Self::FortyEightClubBundle => FORTY_EIGHT_CLUB_HOST,
            Self::BlockRazorRpcBundle => BLOCKRAZOR_RPC_HOST,
        }
    }

    fn port(self) -> u16 {
        HTTPS_PORT
    }

    fn label(self) -> &'static str {
        match self {
            Self::PublicRpc => "public_bsc_rpc_eth_sendRawTransaction",
            Self::FortyEightClubBundle => "48club_puissant_builder_eth_sendBundle",
            Self::BlockRazorRpcBundle => "blockrazor_bsc_rpc_eth_sendBundle",
        }
    }
}

fn http_request(host: &str, body: &str) -> Vec<u8> {
    format!(
        "POST / HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Accept: application/json\r\n\
         Accept-Encoding: identity\r\n\
         Connection: keep-alive\r\n\
         Content-Length: {}\r\n\
         \r\n{}",
        body.len(),
        body
    )
    .into_bytes()
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn decode_chunked_body(raw: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0usize;
    let mut out = Vec::new();

    loop {
        let line_end_rel = raw[pos..]
            .windows(2)
            .position(|w| w == b"\r\n")?;
        let line_end = pos + line_end_rel;
        let size_line = std::str::from_utf8(&raw[pos..line_end]).ok()?;
        let size_hex = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(size_hex, 16).ok()?;
        let data_start = line_end + 2;

        if size == 0 {
            if raw.len() < data_start + 2 {
                return None;
            }
            if &raw[data_start..data_start + 2] == b"\r\n" {
                return Some(out);
            }
            let trailer_end = raw[data_start..]
                .windows(4)
                .position(|w| w == b"\r\n\r\n")?;
            let _ = trailer_end;
            return Some(out);
        }

        let data_end = data_start.checked_add(size)?;
        if raw.len() < data_end + 2 {
            return None;
        }
        if &raw[data_end..data_end + 2] != b"\r\n" {
            return None;
        }

        out.extend_from_slice(&raw[data_start..data_end]);
        pos = data_end + 2;
    }
}

fn read_http<W: Read>(
    stream: &mut W,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];

    loop {
        let n = stream.read(&mut tmp).expect("TLS read failed");
        assert!(n > 0, "server closed connection");

        buf.extend_from_slice(&tmp[..n]);

        let Some(h) = find_header_end(&buf) else {
            continue;
        };

        let header = String::from_utf8_lossy(&buf[..h]).to_lowercase();

        if header.contains("transfer-encoding: chunked") {
            let raw_body = &buf[h + 4..];
            let Some(decoded) = decode_chunked_body(raw_body) else {
                continue;
            };

            let mut response = Vec::with_capacity(h + 4 + decoded.len());
            response.extend_from_slice(&buf[..h + 4]);
            response.extend_from_slice(&decoded);
            return response;
        }

        let content_length = header
            .lines()
            .find_map(|line| {
                line.strip_prefix("content-length:")
                    .map(|x| x.trim().parse::<usize>().unwrap())
            })
            .expect("response has neither Content-Length nor chunked encoding");

        let full = h + 4 + content_length;

        if buf.len() >= full {
            buf.truncate(full);
            return buf;
        }
    }
}

fn body_of(response: &[u8]) -> &[u8] {
    let h = find_header_end(response).unwrap();
    &response[h + 4..]
}

#[derive(Default)]
struct Sample {
    decode: u64,
    quote: u64,
    finalize: u64,
    sign: u64,
    serialize: u64,
    hex_json: u64,
    handoff: u64,
    ready: u64,
    total: u64,
    roundtrip: u64,
    response_error_code: Option<i64>,
    response_error_message: Option<String>,
}

struct Ctx {
    launch: Vec<u8>,
    portal: Address,
    init_hash: [u8; 32],
    sqrt: U256,
    liquidity: U256,
    input: U256,
    timestamp: U256,
    slippage_bps: u16,
    signer: SigningKey,
    expected_token: Address,
    expected_output: U256,
    expected_min: U256,
    expected_calldata: Vec<u8>,
}

fn sender(ctx: &Ctx) -> Address {
    let pk = ctx.signer.verifying_key().to_encoded_point(false);
    let hash = keccak256(&pk.as_bytes()[1..]);
    Address::from_slice(&hash.as_slice()[12..])
}

fn guarded_transport_write<W: Write>(
    lane: &mut IntentLane, ticket: Ticket, hash: alloy_primitives::B256,
    request: &[u8], writer: &mut W,
) -> std::io::Result<()> {
    let permit = lane.begin_send().map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    if permit != (ticket, hash) {
        lane.halt();
        return Err(std::io::Error::other("permit/request mismatch"));
    }
    // The permit is already consumed before the FIRST write, including partial writes.
    if let Err(e) = writer.write_all(request).and_then(|_| writer.flush()) {
        lane.halt();
        return Err(e);
    }
    // No confirm here. HTTP/JSON-RPC responses cannot establish canonical execution.
    Ok(())
}

fn run_probe<W: Read + Write>(
    ctx: &Ctx, stream: &mut W, nonce: u64, correctness_check: bool, transport: TransportKind,
) -> Sample {
    // Each legacy measurement is an isolated INVALID-WIRE probe, never a live lane.
    let mut lane = IntentLane::new(56, sender(ctx), nonce, 1);
    run_once(ctx, stream, nonce, correctness_check, transport, &mut lane)
}

fn run_once<W: Read + Write>(
    ctx: &Ctx,
    stream: &mut W,
    nonce: u64,
    correctness_check: bool,
    transport: TransportKind,
    lane: &mut IntentLane,
) -> Sample {
    assert_eq!(lane.identity(), (56, sender(ctx)), "lane/signer mismatch");
    let t0 = Instant::now();

    // 1. Minimal launch decode + scope qualification + CREATE2 token address.
    let lp = newTokenV6Call::abi_decode_validate(&ctx.launch)
        .unwrap()
        .params;

    assert_eq!(lp.tokenVersion, 6);
    assert_eq!(lp.buyTax, 100);
    assert_eq!(lp.sellTax, 100);

    let doge =
        Address::from_str("0xba2ae424d960c26247dd6c32edc70b295c744c43").unwrap();

    assert_eq!(lp.quoteToken, doge);

    let (mode, price, deadline, _signature) =
        <(U256, U256, U256, Bytes)>::abi_decode_params_validate(
            &lp.extensionData,
        )
        .unwrap();

    assert_eq!(mode, U256::ZERO);
    assert!(ctx.timestamp <= deadline);

    let mut create2 = Vec::with_capacity(85);
    create2.push(0xff);
    create2.extend_from_slice(ctx.portal.as_slice());
    create2.extend_from_slice(lp.salt.as_slice());
    create2.extend_from_slice(&ctx.init_hash);

    let digest = keccak256(&create2);
    let token = Address::from_slice(&digest.as_slice()[12..]);
    let ticket = lane.reserve(token).expect("intent admission rejected");
    assert_eq!(ticket.nonce(), nonce, "nonce must match admitted ticket");

    let t1 = Instant::now();

    // 2. Qualified G0.3 local quote/residual calculation.
    let net = mul_div(
        ctx.input,
        U256::from(997500u64),
        U256::from(1_000_000u64),
    );

    let next =
        ctx.sqrt + mul_div(net, U256::ONE << 96, ctx.liquidity);

    let quote = narrow(
        (wide(ctx.liquidity) << 96)
            * wide(next - ctx.sqrt)
            / wide(next)
            / wide(ctx.sqrt),
    );

    let protocol = quote / U256::from(100u64);
    let combined = quote / U256::from(50u64);
    let _tax = combined - protocol;

    let r = mul_div(
        n("3837000000000000000000"),
        n("1000000000000000000"),
        price,
    );

    let h = n("107036751000000000000000000");
    let total = n("1000000000000000000000000000");

    let k = mul_div(
        r,
        total + h,
        n("1000000000000000000"),
    );

    let supply = |q: U256| {
        total + h
            - ceil_div(
                k * n("1000000000000000000"),
                r + q * n("10000000000"),
            )
    };

    let creator_reserve =
        lp.quoteAmt - lp.quoteAmt / U256::from(50u64);

    let creator_supply = supply(creator_reserve);

    let output =
        supply(creator_reserve + quote - combined) - creator_supply;

    let minimum = min_output(output, ctx.slippage_bps);

    let t2 = Instant::now();

    // 3. BUY calldata + unsigned EIP-155 transaction.
    let calldata = swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: ctx.input,
            minOutputAmount: minimum,
            permit: Bytes::new(),
        },
    }
    .abi_encode();

    let unsigned = rlp_list(vec![
        rlp_u64(nonce),
        rlp_u64(310_000_000), // historical fixture gasPrice, pre-resident policy input
        rlp_u64(1_500_000),   // historical fixture gas limit, pre-resident policy input
        rlp_bytes(ctx.portal.as_slice()),
        rlp_u256(ctx.input),
        rlp_bytes(&calldata),
        rlp_u64(56),
        rlp_bytes(&[]),
        rlp_bytes(&[]),
    ]);

    let sighash = keccak256(&unsigned);

    let t3 = Instant::now();

    // 4. Warm in-process secp256k1 signer.
    let (sig, recovery_id) = ctx
        .signer
        .sign_prehash_recoverable(sighash.as_slice())
        .unwrap();

    let t4 = Instant::now();

    // 5. Final signed legacy transaction serialization.
    let sig_bytes = sig.to_bytes();
    let v = 35u64 + (2 * 56) + recovery_id.to_byte() as u64;

    let signed = rlp_list(vec![
        rlp_u64(nonce),
        rlp_u64(310_000_000),
        rlp_u64(1_500_000),
        rlp_bytes(ctx.portal.as_slice()),
        rlp_u256(ctx.input),
        rlp_bytes(&calldata),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sig_bytes[..32])),
        rlp_u256(U256::from_be_slice(&sig_bytes[32..])),
    ]);

    let t5 = Instant::now();

    if correctness_check {
        assert_eq!(token, ctx.expected_token);
        assert_eq!(output, ctx.expected_output);
        assert_eq!(minimum, ctx.expected_min);
        assert_eq!(calldata, ctx.expected_calldata);
    }

    black_box(&signed);

    // 6. SAFETY:
    // Build/sign the real transaction locally, then deliberately corrupt
    // the wire payload before ANY network write.
    let mut wire = signed.clone();

    assert!(
        wire.first().copied().unwrap() >= 0xc0,
        "expected signed legacy RLP list"
    );

    wire[0] = 0x00;

    assert_eq!(wire[0], 0x00);

    let raw_hex = hex::encode(&wire);

    let send_body = match transport {
        TransportKind::PublicRpc => format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"eth_sendRawTransaction","params":["0x{}"]}}"#,
            raw_hex
        ),
        TransportKind::FortyEightClubBundle => format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"eth_sendBundle","params":[{{"txs":["0x{}"],"noMerge":true}}]}}"#,
            raw_hex
        ),
        TransportKind::BlockRazorRpcBundle => format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"eth_sendBundle","params":[{{"txs":["0x{}"]}}]}}"#,
            raw_hex
        ),
    };

    let request = http_request(transport.host(), &send_body);
    let signed_hash = keccak256(&signed);
    lane.prepare(ticket, signed_hash).expect("intent preparation rejected");

    let t6 = Instant::now();

    // 7. REAL remote warm TLS/TCP handoff.
    guarded_transport_write(lane, ticket, signed_hash, &request, stream)
        .expect("guarded transport write failed");

    let t7 = Instant::now();

    // Response is deliberately outside T_send measurement.
    let response = read_http(stream);

    let t8 = Instant::now();

    let body =
        String::from_utf8_lossy(body_of(&response)).to_string();

    assert!(
        body.contains("\"error\""),
        "invalid transaction unexpectedly not rejected: {body}"
    );

    assert!(
        !body.contains("\"result\":\"0x"),
        "SAFETY FAILURE: endpoint returned transaction/bundle hash"
    );

    let lower = body.to_lowercase();

    assert!(
        !lower.contains("rate limit")
            && !lower.contains("too many request"),
        "provider rate limit encountered: {body}"
    );

    let parsed: Value =
        serde_json::from_str(&body).expect("invalid JSON-RPC response body");

    let response_error_code =
        parsed.get("error")
            .and_then(|e| e.get("code"))
            .and_then(Value::as_i64);

    let response_error_message =
        parsed.get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .map(str::to_owned);

    if matches!(
        transport,
        TransportKind::FortyEightClubBundle | TransportKind::BlockRazorRpcBundle
    ) {
        assert_ne!(
            response_error_code,
            Some(-32601),
            "bundle endpoint reports eth_sendBundle method not found: {body}"
        );
        assert!(
            response_error_code.is_some(),
            "invalid-wire bundle probe returned neither error nor bundle hash: {body}"
        );
    }

    black_box(&body);

    Sample {
        decode: (t1 - t0).as_nanos() as u64,
        quote: (t2 - t1).as_nanos() as u64,
        finalize: (t3 - t2).as_nanos() as u64,
        sign: (t4 - t3).as_nanos() as u64,
        serialize: (t5 - t4).as_nanos() as u64,
        hex_json: (t6 - t5).as_nanos() as u64,
        handoff: (t7 - t6).as_nanos() as u64,
        ready: (t5 - t0).as_nanos() as u64,
        total: (t7 - t0).as_nanos() as u64,
        roundtrip: (t8 - t0).as_nanos() as u64,
        response_error_code,
        response_error_message,
    }
}

fn stats(mut v: Vec<u64>) -> Value {
    v.sort_unstable();

    let at = |num: usize, den: usize| -> u64 {
        let idx = ((v.len() - 1) * num) / den;
        v[idx]
    };

    let sum: u128 = v.iter().map(|x| *x as u128).sum();
    let over_1ms = v.iter().filter(|x| **x > 1_000_000).count();

    json!({
        "p50_ns": at(50,100),
        "p95_ns": at(95,100),
        "p99_ns": at(99,100),
        "max_ns": *v.last().unwrap(),
        "mean_ns": (sum / v.len() as u128) as u64,
        "p50_us": at(50,100) as f64 / 1000.0,
        "p95_us": at(95,100) as f64 / 1000.0,
        "p99_us": at(99,100) as f64 / 1000.0,
        "max_us": *v.last().unwrap() as f64 / 1000.0,
        "over_1ms_count": over_1ms,
        "over_1ms_fraction": over_1ms as f64 / v.len() as f64
    })
}

fn main() {
    let args: Vec<String> = env::args().collect();

    assert!(
        args.len() == 3 || args.len() == 4,
        "g05_remote_e2e <g03_rust_inputs.json> <g03_rust_output.json> [--48club-builder|--blockrazor-rpc-bundle]"
    );

    let transport = match args.get(3).map(String::as_str) {
        None => TransportKind::PublicRpc,
        Some("--48club-builder") => TransportKind::FortyEightClubBundle,
        Some("--blockrazor-rpc-bundle") => TransportKind::BlockRazorRpcBundle,
        Some(other) => panic!("unsupported transport mode: {other}"),
    };

    let fi: Value =
        serde_json::from_str(&fs::read_to_string(&args[1]).unwrap()).unwrap();

    let fo: Value =
        serde_json::from_str(&fs::read_to_string(&args[2]).unwrap()).unwrap();

    let launch =
        hex::decode(fi["launch_calldata"].as_str().unwrap().trim_start_matches("0x"))
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
        portal: Address::from_str(
            "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0",
        )
        .unwrap(),
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
        expected_token: Address::from_str(
            fi["expected_token"].as_str().unwrap(),
        )
        .unwrap(),
        expected_output: n(fo["expected_output"].as_str().unwrap()),
        expected_min: n(fo["min_output"].as_str().unwrap()),
        expected_calldata: hex::decode(
            fo["calldata"].as_str().unwrap().trim_start_matches("0x"),
        )
        .unwrap(),
    };

    // DNS/TCP/TLS connection setup is deliberately outside T_pre.
    let addr = (transport.host(), transport.port())
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("DNS returned no address");

    let tcp = TcpStream::connect_timeout(
        &addr,
        Duration::from_secs(5),
    )
    .expect("TCP connect failed");

    tcp.set_nodelay(true).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();

    let roots =
        RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
        );

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let server_name =
        ServerName::try_from(transport.host().to_string()).unwrap();

    let conn =
        ClientConnection::new(Arc::new(config), server_name).unwrap();

    let mut stream = StreamOwned::new(conn, tcp);

    // Public RPC supports eth_chainId directly. The 48Club builder path is
    // qualified by the real eth_sendBundle request itself; the payload is
    // deliberately invalid RLP, so it cannot become a valid/value-bearing tx.
    let chain_id_verified = if matches!(
        transport,
        TransportKind::PublicRpc | TransportKind::BlockRazorRpcBundle
    ) {
        let chain_body =
            r#"{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}"#;

        let chain_req = http_request(transport.host(), chain_body);

        stream.write_all(&chain_req).unwrap();
        stream.flush().unwrap();

        let chain_resp = read_http(&mut stream);
        let chain_text =
            String::from_utf8_lossy(body_of(&chain_resp));

        assert!(
            chain_text.contains("\"0x38\""),
            "endpoint did not report BSC chainId 56: {chain_text}"
        );

        Some(56u64)
    } else {
        None
    };

    // Correctness check against G0.3 frozen output and live transport request.
    run_probe(&ctx, &mut stream, 0x032e, true, transport);

    const WARMUP: usize = 10;
    const SAMPLES: usize = 100;

    for i in 0..WARMUP {
        black_box(run_probe(
            &ctx,
            &mut stream,
            0x032e + (i as u64 & 0x3ff),
            false,
            transport,
        ));
    }

    let mut decode = Vec::with_capacity(SAMPLES);
    let mut quote = Vec::with_capacity(SAMPLES);
    let mut finalize = Vec::with_capacity(SAMPLES);
    let mut sign = Vec::with_capacity(SAMPLES);
    let mut serialize = Vec::with_capacity(SAMPLES);
    let mut hex_json = Vec::with_capacity(SAMPLES);
    let mut handoff = Vec::with_capacity(SAMPLES);
    let mut ready = Vec::with_capacity(SAMPLES);
    let mut total = Vec::with_capacity(SAMPLES);
    let mut roundtrip = Vec::with_capacity(SAMPLES);
    let mut response_errors: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();

    for i in 0..SAMPLES {
        let s = run_probe(
            &ctx,
            &mut stream,
            0x032e + (i as u64 & 0x3ff),
            false,
            transport,
        );

        decode.push(s.decode);
        quote.push(s.quote);
        finalize.push(s.finalize);
        sign.push(s.sign);
        serialize.push(s.serialize);
        hex_json.push(s.hex_json);
        handoff.push(s.handoff);
        ready.push(s.ready);
        total.push(s.total);
        roundtrip.push(s.roundtrip);

        if let Some(code) = s.response_error_code {
            let message = s.response_error_message.as_deref().unwrap_or("");
            *response_errors
                .entry(format!("{code}: {message}"))
                .or_insert(0) += 1;
        }

        // Outside measured interval. Keep the builder probe deliberately
        // paced so transport qualification does not become a burst test.
        let pause_ms = if matches!(
            transport,
            TransportKind::FortyEightClubBundle | TransportKind::BlockRazorRpcBundle
        ) {
            200
        } else {
            50
        };
        thread::sleep(Duration::from_millis(pause_ms));
    }

    let total_stats = stats(total);

    let result = json!({
        "status": match transport {
            TransportKind::PublicRpc => "G0.5_REMOTE_E2E_PREQUALIFICATION",
            TransportKind::FortyEightClubBundle => "G0.5_48CLUB_BUILDER_TRANSPORT_PREQUALIFICATION",
            TransportKind::BlockRazorRpcBundle => "G0.5_BLOCKRAZOR_RPC_BUNDLE_TRANSPORT_PREQUALIFICATION",
        },
        "scope": match transport {
            TransportKind::PublicRpc => "Pinned G0.3 Flap fixture; single measured E2E path from T_pre through real remote warm TLS/TCP handoff; wire transaction deliberately corrupted before send",
            TransportKind::FortyEightClubBundle => "Pinned G0.3 Flap fixture; single measured E2E path from T_pre through live 48Club Puissant Builder eth_sendBundle TLS/TCP handoff; wire transaction deliberately corrupted before send",
            TransportKind::BlockRazorRpcBundle => "Pinned G0.3 Flap fixture; single measured E2E path from T_pre through live BlockRazor BSC RPC eth_sendBundle TLS/TCP handoff; wire transaction deliberately corrupted before send",
        },
        "architecture": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "warmup_samples": WARMUP,
        "measured_samples": SAMPLES,
        "correctness": {
            "g03_fixture_match": true,
            "expected_token": ctx.expected_token.to_string(),
            "expected_output": ctx.expected_output.to_string(),
            "expected_min_output": ctx.expected_min.to_string()
        },
        "stages": {
            "decode_and_create2": stats(decode),
            "quote_and_min_output": stats(quote),
            "tx_finalize_and_sighash": stats(finalize),
            "secp256k1_sign": stats(sign),
            "signed_tx_serialize": stats(serialize),
            "safety_invalidate_hex_json_http_framing": stats(hex_json),
            "remote_TLS_TCP_write_flush_handoff": stats(handoff),
            "T_pre_to_T_ready_local": stats(ready),
            "T_pre_to_remote_TLS_TCP_handoff": total_stats,
            "T_pre_to_JSON_RPC_response": stats(roundtrip)
        },
        "target": {
            "remote_e2e_prequalification_ns": 1_000_000,
            "final_real_T_send_target_ns": 1_000_000
        },
        "endpoint_host": transport.host(),
        "transport_candidate": transport.label(),
        "chain_id_verified": chain_id_verified,
        "transport": "preconnected TLS/TCP HTTP/1.1",
        "wire_safety": "valid local signed BUY is corrupted to invalid RLP before network write",
        "json_rpc_error_histogram": response_errors,
        "non_claims": [
            "No valid transaction was broadcast",
            "No value-bearing BUY was sent",
            "Transport socket handoff is not confirmed builder acceptance, mempool acceptance, propagation or inclusion",
            "JSON-RPC response latency is not T_send and is not T_buy",
            "This alone does not prove inclusion advantage"
        ]
    });

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}

#[cfg(test)]
#[path = "../builder_lifecycle_tests.rs"]
mod lifecycle_tests;
