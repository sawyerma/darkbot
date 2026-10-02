//! G0.6a current-native Mainnet target-path T_signal -> T_send benchmark.
//!
//! Safety: locally builds valid-looking launch/BUY transactions with fixed non-funded
//! benchmark keys, then corrupts BOTH wire RLP payloads before any network write.
//! No valid/value-bearing transaction can leave this process.

use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use alloy_sol_types::{sol, SolCall};
use darkbot_gate0::native_bnb_quote::quote_native_buy;
use k256::ecdsa::SigningKey;
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use serde_json::{json, Value};
use std::{
    env,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    str::FromStr,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const CHAIN_ID: u64 = 56;
const PORTAL: &str = "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0";
const TAX_V3_IMPL: &str = "0x024f18294970b5c76c0691b87f138a0317156422";
const BUY_VALUE_WEI: u64 = 1_000_000_000_000_000;
const GAS_PRICE_WEI: u64 = 50_000_000;
const BUY_GAS_LIMIT: u64 = 3_000_000;
const LAUNCH_GAS_LIMIT: u64 = 6_603_270;
const SLIPPAGE_BPS: u64 = 100;
const WARMUP: usize = 10;
const SAMPLES: usize = 100;
const SALT_HEX: &str = "0x09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb";

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

#[derive(Clone, Copy)]
enum Transport {
    FortyEightClub,
    BlockRazor,
}

impl Transport {
    fn from_args() -> Self {
        match env::args().nth(1).as_deref() {
            Some("--48club") => Self::FortyEightClub,
            Some("--blockrazor") => Self::BlockRazor,
            _ => panic!("usage: g06a_target_handoff --48club|--blockrazor"),
        }
    }

    fn host(self) -> &'static str {
        match self {
            Self::FortyEightClub => "puissant-builder.48.club",
            Self::BlockRazor => "bsc.blockrazor.xyz",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FortyEightClub => "48club_puissant_builder",
            Self::BlockRazor => "blockrazor_bsc_rpc_bundle",
        }
    }
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

fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len: usize = items.iter().map(Vec::len).sum();
    let mut payload = Vec::with_capacity(payload_len);
    for item in items {
        payload.extend_from_slice(item);
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

fn signer_address(key: &SigningKey) -> Address {
    let point = key.verifying_key().to_encoded_point(false);
    let h = keccak256(&point.as_bytes()[1..]);
    Address::from_slice(&h.as_slice()[12..])
}

fn sign_legacy(
    signer: &SigningKey,
    nonce: u64,
    gas_price: u64,
    gas_limit: u64,
    to: Address,
    value: U256,
    calldata: &[u8],
) -> Vec<u8> {
    let unsigned = rlp_list(&[
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(to.as_slice()),
        rlp_u256(value),
        rlp_bytes(calldata),
        rlp_u64(CHAIN_ID),
        rlp_bytes(&[]),
        rlp_bytes(&[]),
    ]);
    let sighash = keccak256(unsigned);
    let (sig, rid) = signer
        .sign_prehash_recoverable(sighash.as_slice())
        .expect("signing failed");
    let sb = sig.to_bytes();
    let v = 35u64 + 2 * CHAIN_ID + rid.to_byte() as u64;
    rlp_list(&[
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(to.as_slice()),
        rlp_u256(value),
        rlp_bytes(calldata),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sb[..32])),
        rlp_u256(U256::from_be_slice(&sb[32..])),
    ])
}

fn predicted_token(salt: B256) -> Address {
    let portal = Address::from_str(PORTAL).unwrap();
    let impl_addr = Address::from_str(TAX_V3_IMPL).unwrap();
    let mut init = hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73").unwrap();
    init.extend_from_slice(impl_addr.as_slice());
    init.extend_from_slice(&hex::decode("5af43d82803e903d91602b57fd5bf3").unwrap());
    let mut preimage = Vec::with_capacity(85);
    preimage.push(0xff);
    preimage.extend_from_slice(portal.as_slice());
    preimage.extend_from_slice(salt.as_slice());
    preimage.extend_from_slice(keccak256(init).as_slice());
    let digest = keccak256(preimage);
    Address::from_slice(&digest.as_slice()[12..])
}

fn launch_calldata(launcher: Address, salt: B256) -> Vec<u8> {
    newTokenV6Call {
        params: LaunchParams {
            name: "DarkBot G06A Handoff".into(),
            symbol: "DBG06H".into(),
            uri: "ipfs://darkbot-g06a-handoff".into(),
            dexThresh: 1,
            salt,
            migratorType: 1,
            quoteToken: Address::ZERO,
            quoteAmt: U256::ZERO,
            beneficiary: launcher,
            permit: Bytes::new(),
            extensionId: B256::ZERO,
            extensionData: Bytes::new(),
            dexId: 0,
            lpFeeProfile: 0,
            buyTax: 100,
            sellTax: 100,
            taxDuration: 31_536_000,
            antiFarmerDuration: 86_400,
            mktBps: 10_000,
            deflationBps: 0,
            dividendBps: 0,
            lpBps: 0,
            minimumShareBalance: U256::ZERO,
            dividendToken: Address::ZERO,
            commissionReceiver: Address::ZERO,
            tokenVersion: 6,
        },
    }
    .abi_encode()
}

fn buy_calldata(token: Address, minimum: U256) -> Vec<u8> {
    swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: U256::from(BUY_VALUE_WEI),
            minOutputAmount: minimum,
            permit: Bytes::new(),
        },
    }
    .abi_encode()
}

fn http_request(host: &str, body: &str) -> Vec<u8> {
    format!(
        "POST / HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nAccept: application/json\r\nAccept-Encoding: identity\r\nConnection: keep-alive\r\nContent-Length: {}\r\n\r\n{}",
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
        let line_end_rel = raw[pos..].windows(2).position(|w| w == b"\r\n")?;
        let line_end = pos + line_end_rel;
        let size_line = std::str::from_utf8(&raw[pos..line_end]).ok()?;
        let size_hex = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(size_hex, 16).ok()?;
        let data_start = line_end + 2;
        if size == 0 {
            return Some(out);
        }
        let data_end = data_start.checked_add(size)?;
        if raw.len() < data_end + 2 || &raw[data_end..data_end + 2] != b"\r\n" {
            return None;
        }
        out.extend_from_slice(&raw[data_start..data_end]);
        pos = data_end + 2;
    }
}

fn read_http<W: Read>(stream: &mut W) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];
    loop {
        let n = stream.read(&mut tmp).expect("TLS read failed");
        assert!(n > 0, "server closed connection");
        buf.extend_from_slice(&tmp[..n]);
        let Some(h) = find_header_end(&buf) else { continue; };
        let header = String::from_utf8_lossy(&buf[..h]).to_lowercase();
        if header.contains("transfer-encoding: chunked") {
            let Some(decoded) = decode_chunked_body(&buf[h + 4..]) else { continue; };
            let mut response = Vec::with_capacity(h + 4 + decoded.len());
            response.extend_from_slice(&buf[..h + 4]);
            response.extend_from_slice(&decoded);
            return response;
        }
        let content_length = header
            .lines()
            .find_map(|line| line.strip_prefix("content-length:").map(|x| x.trim().parse::<usize>().unwrap()))
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

fn connect_tls(host: &str) -> StreamOwned<ClientConnection, TcpStream> {
    let addr = (host, 443)
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("DNS returned no address");
    let mut tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).expect("TCP connect failed");
    tcp.set_nodelay(true).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    let server_name = ServerName::try_from(host.to_string()).unwrap();
    let mut conn = ClientConnection::new(Arc::new(config), server_name).unwrap();
    conn.complete_io(&mut tcp).expect("TLS handshake failed");
    StreamOwned::new(conn, tcp)
}

#[derive(Default)]
struct Sample {
    decode_create2_ns: u64,
    quote_ns: u64,
    build_sign_ns: u64,
    framing_ns: u64,
    handoff_ns: u64,
    signal_to_ready_ns: u64,
    signal_to_send_ns: u64,
    response_ns: u64,
}

fn run_once<W: Read + Write>(
    stream: &mut W,
    transport: Transport,
    launch_signal: &[u8],
    launch_wire_valid_local: &[u8],
    buyer_key: &SigningKey,
    nonce: u64,
) -> Sample {
    let t_signal = Instant::now();

    let decoded = newTokenV6Call::abi_decode_validate(launch_signal)
        .expect("current-scope launch decode failed");
    assert_eq!(decoded.params.tokenVersion, 6);
    assert_eq!(decoded.params.quoteToken, Address::ZERO);
    assert_eq!(decoded.params.dexThresh, 1);
    assert_eq!(decoded.params.migratorType, 1);
    assert_eq!(decoded.params.buyTax, 100);
    assert_eq!(decoded.params.sellTax, 100);

    let token = predicted_token(decoded.params.salt);
    let t_decode = Instant::now();

    let quote = quote_native_buy(
        20,
        U256::ZERO,
        U256::ZERO,
        U256::from(BUY_VALUE_WEI),
        100,
        100,
        true,
    );
    let minimum = quote.output_amount * U256::from(10_000u64 - SLIPPAGE_BPS) / U256::from(10_000u64);
    assert_eq!(quote.output_amount, U256::from(176_664_965_031_639_901_123_273u128));
    assert_eq!(minimum, U256::from(174_898_315_381_323_502_112_040u128));
    let t_quote = Instant::now();

    let buy_data = buy_calldata(token, minimum);
    let buy_signed = sign_legacy(
        buyer_key,
        nonce,
        GAS_PRICE_WEI,
        BUY_GAS_LIMIT,
        Address::from_str(PORTAL).unwrap(),
        U256::from(BUY_VALUE_WEI),
        &buy_data,
    );
    let t_ready = Instant::now();

    // Safety boundary: valid local bytes exist only in memory. Corrupt both RLP list prefixes
    // before constructing ANY network request.
    let mut launch_wire = launch_wire_valid_local.to_vec();
    let mut buy_wire = buy_signed;
    assert!(launch_wire.first().copied().unwrap_or_default() >= 0xc0);
    assert!(buy_wire.first().copied().unwrap_or_default() >= 0xc0);
    launch_wire[0] = 0x00;
    buy_wire[0] = 0x00;

    let launch_hex = hex::encode(&launch_wire);
    let buy_hex = hex::encode(&buy_wire);
    let body = match transport {
        Transport::FortyEightClub => format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"eth_sendBundle","params":[{{"txs":["0x{}","0x{}"]}}]}}"#,
            launch_hex, buy_hex
        ),
        Transport::BlockRazor => format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"eth_sendBundle","params":[{{"txs":["0x{}","0x{}"]}}]}}"#,
            launch_hex, buy_hex
        ),
    };
    let request = http_request(transport.host(), &body);
    let t_queue = Instant::now();

    stream.write_all(&request).expect("TLS write failed");
    stream.flush().expect("TLS flush failed");
    let t_send = Instant::now();

    let response = read_http(stream);
    let t_response = Instant::now();
    let response_body = String::from_utf8_lossy(body_of(&response)).to_string();
    assert!(
        response_body.contains("\"error\""),
        "SAFETY FAILURE: invalid-wire bundle unexpectedly did not error: {response_body}"
    );
    assert!(
        !response_body.contains("\"result\":\"0x"),
        "SAFETY FAILURE: provider returned bundle/tx hash for invalid-wire probe"
    );
    let lower = response_body.to_ascii_lowercase();
    assert!(!lower.contains("rate limit") && !lower.contains("too many request"), "provider rate limited probe: {response_body}");
    let parsed: Value = serde_json::from_str(&response_body).expect("provider response was not JSON");
    let code = parsed.pointer("/error/code").and_then(Value::as_i64);
    assert_ne!(code, Some(-32601), "eth_sendBundle method not found: {response_body}");

    Sample {
        decode_create2_ns: (t_decode - t_signal).as_nanos() as u64,
        quote_ns: (t_quote - t_decode).as_nanos() as u64,
        build_sign_ns: (t_ready - t_quote).as_nanos() as u64,
        framing_ns: (t_queue - t_ready).as_nanos() as u64,
        handoff_ns: (t_send - t_queue).as_nanos() as u64,
        signal_to_ready_ns: (t_ready - t_signal).as_nanos() as u64,
        signal_to_send_ns: (t_send - t_signal).as_nanos() as u64,
        response_ns: (t_response - t_signal).as_nanos() as u64,
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
    let transport = Transport::from_args();
    let host = transport.host();

    let launch_key = SigningKey::from_bytes((&[0x22u8; 32]).into()).unwrap();
    let buyer_key = SigningKey::from_bytes((&[0x11u8; 32]).into()).unwrap();
    let launcher = signer_address(&launch_key);
    let salt = B256::from_str(SALT_HEX).unwrap();
    let launch_signal = launch_calldata(launcher, salt);
    let predicted = predicted_token(salt);
    assert_eq!(predicted.to_string().to_ascii_lowercase(), "0xe51c4deb1038afcd95ef12d4a649cdbcf6fa7777");

    let launch_wire_valid_local = sign_legacy(
        &launch_key,
        0,
        GAS_PRICE_WEI,
        LAUNCH_GAS_LIMIT,
        Address::from_str(PORTAL).unwrap(),
        U256::ZERO,
        &launch_signal,
    );

    let mut stream = connect_tls(host);

    for i in 0..WARMUP {
        let _ = run_once(
            &mut stream,
            transport,
            &launch_signal,
            &launch_wire_valid_local,
            &buyer_key,
            1000 + i as u64,
        );
        thread::sleep(Duration::from_millis(200));
    }

    let mut decode = Vec::with_capacity(SAMPLES);
    let mut quote = Vec::with_capacity(SAMPLES);
    let mut build_sign = Vec::with_capacity(SAMPLES);
    let mut framing = Vec::with_capacity(SAMPLES);
    let mut handoff = Vec::with_capacity(SAMPLES);
    let mut ready = Vec::with_capacity(SAMPLES);
    let mut send = Vec::with_capacity(SAMPLES);
    let mut response = Vec::with_capacity(SAMPLES);

    for i in 0..SAMPLES {
        let s = run_once(
            &mut stream,
            transport,
            &launch_signal,
            &launch_wire_valid_local,
            &buyer_key,
            2000 + i as u64,
        );
        decode.push(s.decode_create2_ns);
        quote.push(s.quote_ns);
        build_sign.push(s.build_sign_ns);
        framing.push(s.framing_ns);
        handoff.push(s.handoff_ns);
        ready.push(s.signal_to_ready_ns);
        send.push(s.signal_to_send_ns);
        response.push(s.response_ns);
        thread::sleep(Duration::from_millis(200));
    }

    println!("{}", serde_json::to_string_pretty(&json!({
        "status": "G0.6A_TARGET_MAINNET_HANDOFF_MEASUREMENT",
        "scope": "Current native-BNB Flap V6 target hotpath: full actionable launch signal -> decode/CREATE2 -> exact Curve-20 quote -> BUY build/sign -> two-transaction builder bundle framing -> warm TLS/TCP write+flush",
        "transport_candidate": transport.label(),
        "endpoint_host": host,
        "chain_id": CHAIN_ID,
        "portal": PORTAL,
        "tax_v3_impl": TAX_V3_IMPL,
        "token": predicted.to_string(),
        "buy_value_wei": BUY_VALUE_WEI.to_string(),
        "expected_output": "176664965031639901123273",
        "min_output": "174898315381323502112040",
        "warmup_samples": WARMUP,
        "measured_samples": SAMPLES,
        "stages": {
            "signal_decode_and_create2": stats(decode),
            "native_curve20_quote": stats(quote),
            "buy_build_and_sign": stats(build_sign),
            "safety_invalidation_and_bundle_framing": stats(framing),
            "remote_TLS_TCP_write_flush_handoff": stats(handoff),
            "T_signal_to_T_ready": stats(ready),
            "T_signal_to_T_send": stats(send),
            "T_signal_to_JSON_RPC_response": stats(response)
        },
        "target": {
            "T_signal_to_T_send_ns": 1_000_000
        },
        "wire_safety": "Both locally constructed transaction RLP list prefixes are corrupted to 0x00 before any network request is constructed",
        "valid_mainnet_transaction_broadcast": false,
        "value_bearing_action": false,
        "non_claims": [
            "This measures the current target hotpath and real remote socket handoff, not T_buy",
            "Synthetic actionable launch input is used; it is not a foreign Mainnet first-seen ingress measurement",
            "Invalid-wire bundle response is not builder inclusion acceptance",
            "T0 -> T_buy requires a real controlled Mainnet execution plus execution/ordering evidence"
        ]
    })).unwrap());
}
