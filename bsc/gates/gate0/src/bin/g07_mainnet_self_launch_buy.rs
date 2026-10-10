//! G0.7 controlled value-bearing BSC Mainnet self-launch + BUY qualification.
//!
//! This binary is deliberately fail-closed and is NOT a generic live trading runner.
//! It sends exactly one approved economic attempt: the byte-identical ordered
//! [launch_A, buy_B] pair to 48Club and BlockRazor in parallel. There is no public
//! mempool launch and no automatic retry. The compile-time v2 hash remains the
//! execution authority.

use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use alloy_sol_types::{sol, SolCall};
use darkbot_gate0::{
    acceptance_guard::{sha256_hex, verify_live_authorization, LiveGuardContext, V2_RELATIVE_PATH},
    qualification_capture::{CaptureClock, CaptureSession},
};
use k256::ecdsa::SigningKey;
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    process::Command,
    str::FromStr,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

type Tls = StreamOwned<ClientConnection, TcpStream>;

const CHAIN_ID: u64 = 56;
const PORTAL: &str = "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0";
const CLUB48_HOST: &str = "puissant-builder.48.club";
const BLOCKRAZOR_HOST: &str = "virginia.builder.blockrazor.io";
const HTTPS_PORT: u16 = 443;
const CAPTURE_WINDOW_NS: u64 = 15_000_000_000;
const PRODUCER_WITNESS_SCHEMA_PATH: &str =
    "bsc/gates/gate0/spec/producer_execution_witness_v1.schema.json";

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

#[derive(Clone)]
struct SignedLegacy {
    raw: Vec<u8>,
    hash: B256,
}

#[derive(Debug)]
struct ProviderResult {
    provider: &'static str,
    write_started_ns: Option<u64>,
    write_flushed_ns: Option<u64>,
    bytes_may_have_left: bool,
    http_status: Option<String>,
    response_body: Option<String>,
    error: Option<String>,
}

fn req_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn env_u64(name: &str) -> u64 {
    req_env(name)
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{name} must be decimal u64"))
}

fn env_u128(name: &str) -> u128 {
    req_env(name)
        .parse::<u128>()
        .unwrap_or_else(|_| panic!("{name} must be decimal u128"))
}

fn norm(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn key_from_env(name: &str) -> SigningKey {
    let raw = req_env(name);
    let bytes = hex::decode(raw.trim().trim_start_matches("0x"))
        .unwrap_or_else(|_| panic!("{name} is not valid hex"));
    assert_eq!(bytes.len(), 32, "{name} must be exactly 32 bytes");
    SigningKey::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("{name} is not a valid secp256k1 private key"))
}

fn signer_address(key: &SigningKey) -> Address {
    let point = key.verifying_key().to_encoded_point(false);
    let h = keccak256(&point.as_bytes()[1..]);
    Address::from_slice(&h.as_slice()[12..])
}

fn load_json(path: &str) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}")))
        .unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

fn req_str<'a>(v: &'a Value, ptr: &str) -> &'a str {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing/bad string at {ptr}"))
}

fn req_bool(v: &Value, ptr: &str) -> bool {
    v.pointer(ptr)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("missing/bad bool at {ptr}"))
}

fn req_u64(v: &Value, ptr: &str) -> u64 {
    v.pointer(ptr)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing/bad u64 at {ptr}"))
}

fn req_u64_string(v: &Value, ptr: &str) -> u64 {
    req_str(v, ptr)
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("bad decimal u64 at {ptr}"))
}

fn req_u128_string(v: &Value, ptr: &str) -> u128 {
    req_str(v, ptr)
        .parse::<u128>()
        .unwrap_or_else(|_| panic!("bad decimal u128 at {ptr}"))
}

fn validate_producer_witness_source(path: &str, attempt_id: &str) -> (Value, String) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read producer witness source contract {path}: {e}"));
    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse producer witness source contract {path}: {e}"));
    assert_eq!(
        req_str(&value, "/schema"),
        "darkbot.producer-witness-source.v1",
        "wrong producer witness source contract schema"
    );
    assert_eq!(
        req_str(&value, "/attempt_id"),
        attempt_id,
        "producer witness source contract belongs to another attempt"
    );
    assert!(req_bool(&value, "/source_ready"));
    assert!(req_bool(
        &value,
        "/original_successful_producer_execution_only"
    ));
    assert_eq!(
        req_str(&value, "/required_witness_schema"),
        PRODUCER_WITNESS_SCHEMA_PATH
    );
    assert!(req_bool(&value, "/delivery_may_be_asynchronous"));
    assert!(req_bool(
        &value,
        "/canonical_identity_binding_required"
    ));
    assert!(req_bool(
        &value,
        "/clock_and_uncertainty_metadata_required"
    ));
    assert!(
        !req_str(&value, "/source_description").trim().is_empty(),
        "producer witness source description is empty"
    );
    (value, sha256_hex(&bytes))
}

fn validate_builder_policy(path: &str, signed_gas_price: u64) -> (Value, String) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read builder policy evidence {path}: {e}"));
    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse builder policy evidence {path}: {e}"));
    assert_eq!(
        req_str(&value, "/schema"),
        "darkbot.g06a.builder-floor-evidence-addendum.v1"
    );
    assert!(req_bool(&value, "/dry3_ready"));
    assert!(req_bool(&value, "/blockrazor/verified"));
    assert!(req_bool(&value, "/fortyeightclub/verified"));
    let strict_floor = req_u64_string(&value, "/strictest_explicit_verified_floor_wei");
    let stage1_policy = req_u64_string(&value, "/stage1_signed_gas_price_policy_wei");
    assert!(
        signed_gas_price >= strict_floor,
        "signed gas price is below the frozen builder floor evidence"
    );
    assert_eq!(
        signed_gas_price, stage1_policy,
        "signed gas price differs from the frozen Stage-1 builder policy"
    );
    (value, sha256_hex(&bytes))
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

fn sign_legacy(
    signer: &SigningKey,
    nonce: u64,
    gas_price: u64,
    gas_limit: u64,
    to: Address,
    value: U256,
    calldata: &[u8],
) -> SignedLegacy {
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
    let raw = rlp_list(&[
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(to.as_slice()),
        rlp_u256(value),
        rlp_bytes(calldata),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sb[..32])),
        rlp_u256(U256::from_be_slice(&sb[32..])),
    ]);
    SignedLegacy {
        hash: keccak256(&raw),
        raw,
    }
}

fn predicted_token(portal: Address, impl_addr: Address, salt: B256) -> Address {
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
            name: "DarkBot G06A Self Launch".into(),
            symbol: "DBG06A".into(),
            uri: "ipfs://darkbot-g06a-dry".into(),
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

fn buy_calldata(token: Address, input: U256, minimum: U256) -> Vec<u8> {
    swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: input,
            minOutputAmount: minimum,
            permit: Bytes::new(),
        },
    }
    .abi_encode()
}

fn rpc(method: &str, params: Value) -> Result<Value, String> {
    let rpc = req_env("BSC_MAINNET_RPC_HTTPS");
    let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
    let out = Command::new("curl")
        .args(["-sS", &rpc, "-H", "Content-Type: application/json", "--data", &body])
        .output()
        .map_err(|e| format!("execute curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("curl {method}: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let v: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("invalid JSON-RPC response for {method}: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("JSON-RPC {method}: {err}"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| format!("JSON-RPC {method}: missing result"))
}

fn hex_u64(v: &Value) -> u64 {
    u64::from_str_radix(
        v.as_str().expect("expected hex string").trim_start_matches("0x"),
        16,
    )
    .expect("bad hex u64")
}

fn run_current_preflight(launcher: Address, buyer: Address) -> Value {
    let path = format!("/tmp/darkbot-g07-preflight-{}.json", std::process::id());
    let out = Command::new("node")
        .arg("bsc/gates/gate0/tools/g06a_preflight_readonly.cjs")
        .arg(&path)
        .env("DARKBOT_G06A_LAUNCHER_ADDRESS", launcher.to_string())
        .env("DARKBOT_G06A_BUYER_ADDRESS", buyer.to_string())
        .output()
        .expect("failed to execute G0.6a readonly Mainnet preflight");
    let _ = fs::remove_file(&path);
    assert!(
        out.status.success(),
        "Mainnet readonly preflight failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("readonly preflight emitted invalid JSON")
}

fn assert_same_preflight_field(baseline: &Value, current: &Value, ptr: &str) {
    let a = baseline
        .pointer(ptr)
        .unwrap_or_else(|| panic!("baseline preflight missing {ptr}"));
    let b = current
        .pointer(ptr)
        .unwrap_or_else(|| panic!("current preflight missing {ptr}"));
    assert_eq!(a, b, "preflight deployment/economics drift at {ptr}");
}

fn validate_current_preflight(
    baseline: &Value,
    current: &Value,
    launcher: Address,
    buyer: Address,
    signed_gas_price: u64,
) -> (u64, u64, u64) {
    assert_eq!(req_str(current, "/schema"), "darkbot.g06a.preflight-readonly.v2");
    assert_eq!(req_u64_string(current, "/chain_id"), CHAIN_ID);
    assert_eq!(norm(req_str(current, "/portal")), norm(PORTAL));
    assert_eq!(
        norm(req_str(current, "/launcher_address")),
        norm(&launcher.to_string())
    );
    assert_eq!(
        norm(req_str(current, "/buyer_address")),
        norm(&buyer.to_string())
    );
    assert!(req_bool(current, "/wallet_qualification/addresses_bound"));
    assert!(req_bool(current, "/wallet_qualification/distinct"));
    assert!(req_bool(current, "/wallet_qualification/funding_pass"));

    for ptr in [
        "/chain_id",
        "/portal",
        "/portal_code_sha256",
        "/portal_version",
        "/portal_implementation",
        "/portal_implementation_code_sha256",
        "/tax_v3_impl",
        "/tax_v3_impl_code_sha256",
        "/native_quote_configuration",
        "/protocol_fee_bps",
        "/tax_on_bonding_curve_enabled",
    ] {
        assert_same_preflight_field(baseline, current, ptr);
    }

    let launcher_nonce = req_u64_string(current, "/launcher_pending_nonce");
    let buyer_nonce = req_u64_string(current, "/buyer_pending_nonce");

    let network_gas_price = req_u64_string(current, "/gas_price_wei");
    assert!(
        network_gas_price <= signed_gas_price,
        "current network gas price {network_gas_price} exceeds signed gas price {signed_gas_price}"
    );

    (
        launcher_nonce,
        buyer_nonce,
        req_u64_string(current, "/latest_block_number"),
    )
}

fn connect_tls(host: &str) -> Result<Tls, String> {
    let addrs: Vec<_> = (host, HTTPS_PORT)
        .to_socket_addrs()
        .map_err(|e| format!("DNS {host}: {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err(format!("DNS returned no addresses for {host}"));
    }
    let mut failures = Vec::new();
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
            Ok(mut tcp) => {
                tcp.set_nodelay(true).map_err(|e| e.to_string())?;
                tcp.set_read_timeout(Some(Duration::from_secs(15)))
                    .map_err(|e| e.to_string())?;
                tcp.set_write_timeout(Some(Duration::from_secs(5)))
                    .map_err(|e| e.to_string())?;
                let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                let mut config = ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth();
                config.alpn_protocols = vec![b"http/1.1".to_vec()];
                let server_name = ServerName::try_from(host.to_string())
                    .map_err(|e| format!("invalid TLS server name {host}: {e}"))?;
                let mut conn = ClientConnection::new(Arc::new(config), server_name)
                    .map_err(|e| format!("TLS init {host}: {e}"))?;
                match conn.complete_io(&mut tcp) {
                    Ok(_) => return Ok(StreamOwned::new(conn, tcp)),
                    Err(e) => failures.push(format!("{addr}: TLS {e}")),
                }
            }
            Err(e) => failures.push(format!("{addr}: TCP {e}")),
        }
    }
    Err(format!("cannot connect to {host}: {}", failures.join(" | ")))
}

fn http_request(host: &str, body: &str, authorization: Option<&str>) -> Vec<u8> {
    let auth = authorization.map_or_else(String::new, |token| {
        assert!(
            !token.is_empty()
                && token
                    .bytes()
                    .all(|b| (0x21..=0x7e).contains(&b) || b == b' '),
            "invalid BlockRazor Authorization value"
        );
        format!("Authorization: {token}\r\n")
    });
    format!(
        "POST / HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{auth}\r\n{}",
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

fn read_http(stream: &mut Tls) -> Result<(String, String), String> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];
    loop {
        let n = stream.read(&mut tmp).map_err(|e| format!("TLS read: {e}"))?;
        if n == 0 {
            return Err("server closed connection before complete response".into());
        }
        buf.extend_from_slice(&tmp[..n]);
        let Some(h) = find_header_end(&buf) else {
            continue;
        };
        let header = String::from_utf8_lossy(&buf[..h]).to_string();
        let lower = header.to_ascii_lowercase();
        if lower.contains("transfer-encoding: chunked") {
            let Some(decoded) = decode_chunked_body(&buf[h + 4..]) else {
                continue;
            };
            return Ok((
                header.lines().next().unwrap_or("").to_string(),
                String::from_utf8_lossy(&decoded).to_string(),
            ));
        }
        let content_length = lower.lines().find_map(|line| {
            line.strip_prefix("content-length:")
                .and_then(|x| x.trim().parse::<usize>().ok())
        });
        let Some(content_length) = content_length else {
            continue;
        };
        let full = h + 4 + content_length;
        if buf.len() >= full {
            return Ok((
                header.lines().next().unwrap_or("").to_string(),
                String::from_utf8_lossy(&buf[h + 4..full]).to_string(),
            ));
        }
    }
}

fn submit_provider(
    provider: &'static str,
    mut stream: Tls,
    request: Vec<u8>,
    barrier: Arc<Barrier>,
    clock: CaptureClock,
) -> ProviderResult {
    barrier.wait();
    let write_started_ns = clock.now_ns();
    match stream.write_all(&request).and_then(|_| stream.flush()) {
        Err(e) => ProviderResult {
            provider,
            write_started_ns: Some(write_started_ns),
            write_flushed_ns: None,
            bytes_may_have_left: true,
            http_status: None,
            response_body: None,
            error: Some(format!("write/flush failed: {e}")),
        },
        Ok(()) => {
            let write_flushed_ns = clock.now_ns();
            match read_http(&mut stream) {
                Ok((status, body)) => ProviderResult {
                    provider,
                    write_started_ns: Some(write_started_ns),
                    write_flushed_ns: Some(write_flushed_ns),
                    bytes_may_have_left: true,
                    http_status: Some(status),
                    response_body: Some(body),
                    error: None,
                },
                Err(e) => ProviderResult {
                    provider,
                    write_started_ns: Some(write_started_ns),
                    write_flushed_ns: Some(write_flushed_ns),
                    bytes_may_have_left: true,
                    http_status: None,
                    response_body: None,
                    error: Some(format!("request sent; response unavailable: {e}")),
                },
            }
        }
    }
}

fn provider_json(r: &ProviderResult) -> Value {
    let response_json = r
        .response_body
        .as_deref()
        .and_then(|body| serde_json::from_str::<Value>(body).ok());
    json!({
        "provider": r.provider,
        "write_started_ns": r.write_started_ns,
        "write_flushed_ns": r.write_flushed_ns,
        "bytes_may_have_left": r.bytes_may_have_left,
        "http_status": r.http_status.as_deref(),
        "response_json": response_json,
        "response_raw_body": r.response_body.as_deref(),
        "error": r.error.as_deref()
    })
}

fn main() {
    assert_eq!(
        env::var("DARKBOT_G07_MAINNET_SEND").as_deref(),
        Ok("YES"),
        "set DARKBOT_G07_MAINNET_SEND=YES only for the explicitly authorized G0.7 attempt"
    );

    let dry_path = req_env("DARKBOT_G07_DRY_EVIDENCE");
    let baseline_path = req_env("DARKBOT_G07_PREFLIGHT_EVIDENCE");
    let capture_path = req_env("DARKBOT_G07_CAPTURE_JOURNAL");
    let attempt_id = req_env("DARKBOT_G07_ATTEMPT_ID");
    let clock_id = req_env("DARKBOT_G07_CLOCK_ID");
    let boot_id = req_env("DARKBOT_G07_BOOT_ID");
    let producer_witness_contract_path =
        req_env("DARKBOT_G07_PRODUCER_WITNESS_CONTRACT");
    let builder_policy_path = req_env("DARKBOT_G07_BUILDER_POLICY_EVIDENCE");
    let (producer_witness_contract, producer_witness_contract_sha256) =
        validate_producer_witness_source(&producer_witness_contract_path, &attempt_id);
    let producer_witness_source =
        req_str(&producer_witness_contract, "/source_description").to_string();

    let dry = load_json(&dry_path);
    let baseline = load_json(&baseline_path);
    assert_eq!(req_str(&dry, "/status"), "G0.6A_SELF_LAUNCH_BUNDLE_DRY");
    assert!(req_bool(&dry, "/dry_pass/all"), "DRY-1..DRY-4 are not all PASS");

    let launcher_key = key_from_env("DARKBOT_G07_LAUNCHER_PRIVATE_KEY");
    let buyer_key = key_from_env("DARKBOT_G07_BUYER_PRIVATE_KEY");
    let launcher = signer_address(&launcher_key);
    let buyer = signer_address(&buyer_key);
    assert_ne!(launcher, buyer, "G0.7 requires distinct launcher and buyer wallets");
    assert_eq!(norm(req_str(&dry, "/wallets/launcher")), norm(&launcher.to_string()));
    assert_eq!(norm(req_str(&dry, "/wallets/buyer")), norm(&buyer.to_string()));
    assert!(req_bool(&dry, "/wallets/distinct"));

    assert_eq!(req_u64(&dry, "/transactions/chain_id"), CHAIN_ID);
    assert_eq!(norm(req_str(&dry, "/transactions/portal")), norm(PORTAL));

    let gas_price = req_u64(&dry, "/transactions/gas_price_wei");
    let (_builder_policy, builder_policy_sha256) =
        validate_builder_policy(&builder_policy_path, gas_price);
    let launch_gas_limit = req_u64(&dry, "/transactions/launch_gas_limit");
    let buy_gas_limit = req_u64(&dry, "/transactions/buy_gas_limit");
    let launch_value = req_u128_string(&dry, "/transactions/launch_value_wei");
    let buy_value = req_u64_string(&dry, "/transactions/buy_value_wei");
    // G0.6a's frozen DRY harness uses the v2 maximum 100-bps policy.
    let slippage_bps = 100u64;
    let salt = B256::from_str(&req_env("DARKBOT_G07_SALT"))
        .expect("DARKBOT_G07_SALT must be the exact 32-byte DRY salt");
    let local_expected_output = U256::from_str(req_str(
        &dry,
        "/transactions/local_expected_output",
    ))
    .expect("invalid DRY local_expected_output");
    assert!(local_expected_output > U256::ZERO);

    let current = run_current_preflight(launcher, buyer);
    let (launcher_nonce, buyer_nonce, send_head) = validate_current_preflight(
        &baseline,
        &current,
        launcher,
        buyer,
        gas_price,
    );
    let impl_addr = Address::from_str(req_str(&current, "/tax_v3_impl"))
        .expect("invalid current TAX_V3 implementation");
    let portal = Address::from_str(PORTAL).unwrap();
    let token = predicted_token(portal, impl_addr, salt);
    assert_eq!(norm(req_str(&dry, "/transactions/token")), norm(&token.to_string()));

    let min_output = local_expected_output * U256::from(10_000u64 - slippage_bps)
        / U256::from(10_000u64);
    assert!(min_output > U256::ZERO);
    assert_eq!(req_str(&dry, "/transactions/min_output"), min_output.to_string());

    let launch_data = launch_calldata(launcher, salt);
    let buy_data = buy_calldata(token, U256::from(buy_value), min_output);
    let launch = sign_legacy(
        &launcher_key,
        launcher_nonce,
        gas_price,
        launch_gas_limit,
        portal,
        U256::from(launch_value),
        &launch_data,
    );
    let buy = sign_legacy(
        &buyer_key,
        buyer_nonce,
        gas_price,
        buy_gas_limit,
        portal,
        U256::from(buy_value),
        &buy_data,
    );
    let launch_hash = launch.hash.to_string();
    let buy_hash = buy.hash.to_string();
    assert_eq!(
        norm(&launch_hash),
        norm(req_str(&dry, "/transactions/launch_tx_hash")),
        "fresh Mainnet launch bytes do not equal the DRY-qualified launch bytes"
    );
    assert_eq!(
        norm(&buy_hash),
        norm(req_str(&dry, "/transactions/buy_tx_hash")),
        "fresh Mainnet BUY bytes do not equal the DRY-qualified BUY bytes"
    );

    let token_code_before = rpc("eth_getCode", json!([token.to_string(), "latest"]))
        .expect("token-code precheck failed");
    assert_eq!(token_code_before.as_str(), Some("0x"), "predicted token already exists");

    let launch_estimate = hex_u64(
        &rpc(
            "eth_estimateGas",
            json!([{
                "from": launcher.to_string(),
                "to": PORTAL,
                "value": format!("0x{launch_value:x}"),
                "data": format!("0x{}", hex::encode(&launch_data))
            }]),
        )
        .expect("current launch gas estimate failed"),
    );
    assert!(
        launch_estimate <= launch_gas_limit,
        "current launch gas estimate exceeds signed DRY limit"
    );

    let reserved_attempt_wei = launch_value
        .checked_add(buy_value as u128)
        .and_then(|x| {
            x.checked_add(
                (launch_gas_limit as u128 + buy_gas_limit as u128) * gas_price as u128,
            )
        })
        .expect("attempt reserve overflow");

    let attempt_number = env_u64("DARKBOT_G07_ATTEMPT_NUMBER");
    let spent_budget_wei = env_u128("DARKBOT_G07_SPENT_BUDGET_WEI");

    let dry_bytes = fs::read(&dry_path).expect("re-read DRY evidence");
    let baseline_bytes = fs::read(&baseline_path).expect("re-read preflight baseline");
    let dry_evidence_sha256 = sha256_hex(&dry_bytes);
    let preflight_evidence_sha256 = sha256_hex(&baseline_bytes);

    let mut capture = CaptureSession::prearm(
        Path::new(&capture_path),
        &attempt_id,
        &clock_id,
        &boot_id,
        &producer_witness_source,
    )
    .expect("G0.7 capture pre-arm failed");
    let capture_clock = capture.clock();

    let contract = match verify_live_authorization(
        Path::new(V2_RELATIVE_PATH),
        &LiveGuardContext {
            launcher_address: launcher.to_string(),
            buyer_address: buyer.to_string(),
            chain_id: CHAIN_ID,
            portal_address: PORTAL.to_string(),
            launch_value_wei: launch_value,
            buy_value_wei: buy_value as u128,
            gas_price_wei: gas_price as u128,
            launch_gas_limit,
            buy_gas_limit,
            direct_builder_tip_wei: 0,
            spent_budget_wei,
            reserved_attempt_wei,
            attempt_number,
            dry_pass_all: true,
            capture_prearmed: true,
            producer_witness_source_bound: true,
            dry_evidence_sha256: dry_evidence_sha256.clone(),
            preflight_evidence_sha256: preflight_evidence_sha256.clone(),
            builder_policy_evidence_sha256: builder_policy_sha256.clone(),
            producer_witness_source_contract_sha256:
                producer_witness_contract_sha256.clone(),
        },
    ) {
        Ok(contract) => contract,
        Err(e) => {
            let now = capture_clock.now_ns();
            let _ = capture.record_text("AUTHORIZATION_REFUSED", now, &format!("reason={e}"));
            let _ = capture.finish();
            panic!("G0.7 live authorization refused: {e}");
        }
    };

    let evidence_binding = format!(
        "dry_sha256={}\npreflight_sha256={}\nbuilder_policy_sha256={}\nproducer_witness_contract_sha256={}\nlaunch_tx_hash={}\nbuy_tx_hash={}\ntoken={}",
        dry_evidence_sha256,
        preflight_evidence_sha256,
        builder_policy_sha256,
        producer_witness_contract_sha256,
        launch_hash,
        buy_hash,
        token
    );
    capture
        .record_text("ATTEMPT_BOUND", capture_clock.now_ns(), &evidence_binding)
        .expect("capture ATTEMPT_BOUND failed before egress");
    capture
        .record_raw_tx("launch_A", capture_clock.now_ns(), &launch_hash, &launch.raw)
        .expect("capture raw launch failed before egress");
    capture
        .record_raw_tx("buy_B", capture_clock.now_ns(), &buy_hash, &buy.raw)
        .expect("capture raw BUY failed before egress");
    assert!(capture.healthy(), "capture became unhealthy before egress");

    let max_block = send_head.checked_add(2).expect("max block overflow");
    let launch_hex = format!("0x{}", hex::encode(&launch.raw));
    let buy_hex = format!("0x{}", hex::encode(&buy.raw));
    let request_id = buy_hash.clone();

    let club48_body = json!({
        "jsonrpc":"2.0",
        "id":request_id.clone(),
        "method":"eth_sendBundle",
        "params":[{
            "txs":[launch_hex.clone(), buy_hex.clone()],
            "maxBlockNumber":max_block,
            "noMerge":true,
            "positionFirst":true
        }]
    })
    .to_string();
    let blockrazor_body = json!({
        "jsonrpc":"2.0",
        "id":buy_hash.clone(),
        "method":"eth_sendBundle",
        "params":[{
            "txs":[launch_hex, buy_hex],
            "maxBlockNumber":max_block
        }]
    })
    .to_string();

    let blockrazor_auth = req_env("DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION");
    let club48_request = http_request(CLUB48_HOST, &club48_body, None);
    let blockrazor_request =
        http_request(BLOCKRAZOR_HOST, &blockrazor_body, Some(&blockrazor_auth));

    for (provider, body, request_len) in [
        ("48Club/Puissant", club48_body.as_str(), club48_request.len()),
        ("BlockRazor", blockrazor_body.as_str(), blockrazor_request.len()),
    ] {
        capture
            .record_text(
                "SUBMISSION_PREPARED",
                capture_clock.now_ns(),
                &format!(
                    "provider={provider}\nbody_sha256={}\nrequest_len={}\nauthorization_persisted=false\nbody={body}",
                    sha256_hex(body.as_bytes()),
                    request_len
                ),
            )
            .expect("capture selected submission failed before egress");
    }
    assert!(capture.healthy(), "capture unhealthy after submission capture");
    drop(blockrazor_auth);

    // TLS/DNS/handshake all happen before the economic write boundary.
    let club48_tls = match connect_tls(CLUB48_HOST) {
        Ok(stream) => stream,
        Err(e) => {
            let _ = capture.record_text(
                "PRE_EGRESS_ABORT",
                capture_clock.now_ns(),
                &format!("provider=48Club/Puissant\nreason={e}"),
            );
            let _ = capture.finish();
            panic!("48Club TLS prewarm failed; no valid bundle write attempted: {e}");
        }
    };
    let blockrazor_tls = match connect_tls(BLOCKRAZOR_HOST) {
        Ok(stream) => stream,
        Err(e) => {
            let _ = capture.record_text(
                "PRE_EGRESS_ABORT",
                capture_clock.now_ns(),
                &format!("provider=BlockRazor\nreason={e}"),
            );
            let _ = capture.finish();
            panic!("BlockRazor TLS prewarm failed; no valid bundle write attempted: {e}");
        }
    };

    capture
        .record_text(
            "TRANSPORTS_PREWARMED",
            capture_clock.now_ns(),
            &format!("send_head={send_head}\nmax_block={max_block}"),
        )
        .expect("capture transport-prewarm event failed before egress");
    assert!(capture.healthy(), "capture unhealthy immediately before egress");

    let barrier = Arc::new(Barrier::new(3));
    let mut release_before_ns = 0u64;
    let mut release_after_ns = 0u64;
    let (club48_result, blockrazor_result) = thread::scope(|scope| {
        let b48 = Arc::clone(&barrier);
        let bbr = Arc::clone(&barrier);
        let c48_clock = capture_clock.clone();
        let br_clock = capture_clock.clone();
        let h48 = scope.spawn(move || {
            submit_provider(
                "48Club/Puissant",
                club48_tls,
                club48_request,
                b48,
                c48_clock,
            )
        });
        let hbr = scope.spawn(move || {
            submit_provider(
                "BlockRazor",
                blockrazor_tls,
                blockrazor_request,
                bbr,
                br_clock,
            )
        });
        release_before_ns = capture_clock.now_ns();
        barrier.wait();
        release_after_ns = capture_clock.now_ns();
        (
            h48.join().expect("48Club send thread panicked"),
            hbr.join().expect("BlockRazor send thread panicked"),
        )
    });

    for result in [&club48_result, &blockrazor_result] {
        let body = format!(
            "provider={}\nwrite_started_ns={:?}\nwrite_flushed_ns={:?}\nbytes_may_have_left={}\nhttp_status={:?}\nresponse_body={}\nerror={:?}",
            result.provider,
            result.write_started_ns,
            result.write_flushed_ns,
            result.bytes_may_have_left,
            result.http_status.as_deref(),
            result.response_body.as_deref().unwrap_or(""),
            result.error.as_deref()
        );
        let _ = capture.record_text("PROVIDER_RESULT", capture_clock.now_ns(), &body);
    }

    let _ = capture.record_text(
        "BUNDLE_RELEASE_BRACKET",
        capture_clock.now_ns(),
        &format!(
            "release_before_ns={release_before_ns}\nrelease_after_ns={release_after_ns}"
        ),
    );

    // Keep the local journal alive conservatively. This is NOT T0 timing authority.
    // If both receipts become observable, retain at least another 15 seconds after
    // that observation. Canonical [T0,T0+15s] backfill is done by G0.8 from the
    // launch block, and strict sub-ms timing still requires the original producer.
    let receipt_poll_deadline = release_after_ns.saturating_add(30_000_000_000);
    let mut launch_receipt_observer = None;
    let mut buy_receipt_observer = None;
    let mut both_receipts_observed_ns = None;
    while capture_clock.now_ns() < receipt_poll_deadline {
        if launch_receipt_observer.as_ref().is_none_or(Value::is_null) {
            launch_receipt_observer =
                rpc("eth_getTransactionReceipt", json!([launch_hash.clone()])).ok();
        }
        if buy_receipt_observer.as_ref().is_none_or(Value::is_null) {
            buy_receipt_observer =
                rpc("eth_getTransactionReceipt", json!([buy_hash.clone()])).ok();
        }
        let launch_seen = launch_receipt_observer.as_ref().is_some_and(|v| !v.is_null());
        let buy_seen = buy_receipt_observer.as_ref().is_some_and(|v| !v.is_null());
        if launch_seen && buy_seen {
            both_receipts_observed_ns = Some(capture_clock.now_ns());
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    let coverage_end_ns = both_receipts_observed_ns
        .map(|seen| seen.saturating_add(CAPTURE_WINDOW_NS))
        .unwrap_or(receipt_poll_deadline)
        .max(release_after_ns.saturating_add(CAPTURE_WINDOW_NS));
    while capture_clock.now_ns() < coverage_end_ns {
        thread::sleep(Duration::from_millis(20));
    }

    let post_window_head = rpc("eth_blockNumber", json!([])).ok().map(|v| hex_u64(&v));
    let _ = capture.record_text(
        "LOCAL_CAPTURE_WINDOW_CLOSED",
        capture_clock.now_ns(),
        &format!(
            "release_before_ns={release_before_ns}\nrelease_after_ns={release_after_ns}\nminimum_post_observation_window_ns={CAPTURE_WINDOW_NS}\nboth_receipts_observed_ns={both_receipts_observed_ns:?}\ncoverage_end_ns={coverage_end_ns}\npost_window_head={post_window_head:?}\nlaunch_receipt_observer={}\nbuy_receipt_observer={}",
            launch_receipt_observer
                .as_ref()
                .map(Value::to_string)
                .unwrap_or_else(|| "UNAVAILABLE".into()),
            buy_receipt_observer
                .as_ref()
                .map(Value::to_string)
                .unwrap_or_else(|| "UNAVAILABLE".into())
        ),
    );

    let capture_summary = capture.finish();
    let capture_complete = capture_summary
        .as_ref()
        .map(|s| s.complete_local_journal)
        .unwrap_or(false);
    let capture_json = match &capture_summary {
        Ok(s) => json!({
            "journal_path": s.path.display().to_string(),
            "records": s.records,
            "first_sequence": s.first_sequence,
            "last_sequence": s.last_sequence,
            "bytes": s.bytes,
            "sha256": s.sha256_hex,
            "next_sequence": s.next_sequence,
            "enqueued": s.enqueued,
            "dropped_full": s.dropped_full,
            "rejected_oversize": s.rejected_oversize,
            "disconnected": s.disconnected,
            "complete_local_journal": s.complete_local_journal,
            "producer_witness_source": producer_witness_source.clone(),
            "producer_witness_contract_path": producer_witness_contract_path.clone(),
            "producer_witness_contract_sha256": producer_witness_contract_sha256.clone(),
            "builder_policy_path": builder_policy_path.clone(),
            "builder_policy_sha256": builder_policy_sha256.clone(),
            "producer_witness_required_for_t0_to_buy": true
        }),
        Err(e) => json!({
            "complete_local_journal": false,
            "error": e,
            "producer_witness_source": producer_witness_source.clone(),
            "producer_witness_contract_path": producer_witness_contract_path.clone(),
            "producer_witness_contract_sha256": producer_witness_contract_sha256.clone(),
            "builder_policy_path": builder_policy_path.clone(),
            "builder_policy_sha256": builder_policy_sha256.clone(),
            "producer_witness_required_for_t0_to_buy": true
        }),
    };

    let both_flushed = club48_result.write_flushed_ns.is_some()
        && blockrazor_result.write_flushed_ns.is_some();
    let any_possible_egress = club48_result.bytes_may_have_left || blockrazor_result.bytes_may_have_left;
    let flush_spread_ns = match (
        club48_result.write_flushed_ns,
        blockrazor_result.write_flushed_ns,
    ) {
        (Some(a), Some(b)) => Some(a.abs_diff(b)),
        _ => None,
    };

    let evidence = json!({
        "schema":"darkbot.g07.mainnet-self-launch-buy.v2",
        "gate":"G0.7",
        "attempt_id":attempt_id,
        "classification": if capture_complete {
            "SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY"
        } else {
            "STOP_CAPTURE_INCOMPLETE__RECONCILE_G08__NO_AUTOMATIC_RETRY"
        },
        "chain_id":CHAIN_ID,
        "portal":PORTAL,
        "acceptance_contract":{
            "path":V2_RELATIVE_PATH,
            "sha256":contract.sha256
        },
        "evidence_inputs":{
            "dry_path":dry_path,
            "dry_sha256":dry_evidence_sha256,
            "preflight_path":baseline_path,
            "preflight_sha256":preflight_evidence_sha256,
            "builder_policy_path":builder_policy_path,
            "builder_policy_sha256":builder_policy_sha256,
            "producer_witness_contract_path":producer_witness_contract_path,
            "producer_witness_contract_sha256":producer_witness_contract_sha256
        },
        "attempt":{
            "number":attempt_number,
            "spent_before_wei":spent_budget_wei.to_string(),
            "reserved_worst_case_wei":reserved_attempt_wei.to_string()
        },
        "wallets":{
            "launcher":launcher.to_string(),
            "buyer":buyer.to_string(),
            "distinct":true,
            "launcher_nonce":launcher_nonce,
            "buyer_nonce":buyer_nonce
        },
        "preflight":{
            "latest_block_number":send_head,
            "latest_block_hash":current["latest_block_hash"],
            "portal_version":current["portal_version"],
            "portal_implementation":current["portal_implementation"],
            "tax_v3_impl":current["tax_v3_impl"],
            "network_gas_price_wei":current["gas_price_wei"]
        },
        "transactions":{
            "launch_tx_hash":launch_hash,
            "buy_tx_hash":buy_hash,
            "token":token.to_string(),
            "token_code_before_empty":true,
            "salt":salt.to_string(),
            "launch_value_wei":launch_value.to_string(),
            "buy_value_wei":buy_value.to_string(),
            "gas_price_wei":gas_price,
            "launch_gas_estimate":launch_estimate,
            "launch_gas_limit":launch_gas_limit,
            "buy_gas_limit":buy_gas_limit,
            "local_expected_output":local_expected_output.to_string(),
            "min_output":min_output.to_string(),
            "slippage_bps":slippage_bps
        },
        "bundle":{
            "tx_order":["launch_A","buy_B"],
            "identical_raw_transactions_across_builders":true,
            "send_head":send_head,
            "max_block":max_block,
            "public_mempool_launch":false,
            "automatic_retry":false,
            "fortyeightclub_noMerge":true,
            "fortyeightclub_positionFirst":true,
            "blockrazor_noMerge":false,
            "blockrazor_positionFirst":false
        },
        "local_send_boundaries":{
            "clock_id":clock_id,
            "boot_id":boot_id,
            "release_before_ns":release_before_ns,
            "release_after_ns":release_after_ns,
            "fortyeightclub":provider_json(&club48_result),
            "blockrazor":provider_json(&blockrazor_result),
            "both_flush_completed":both_flushed,
            "write_flush_boundary_spread_ns":flush_spread_ns,
            "any_valid_bytes_may_have_left":any_possible_egress,
            "not_t0_to_buy":true
        },
        "capture":capture_json,
        "capture_window":{
            "both_receipts_observed_ns":both_receipts_observed_ns,
            "coverage_end_ns":coverage_end_ns,
            "canonical_t0_plus_15s_reconciled_in_g08":true
        },
        "timing":{
            "T0_to_T_buy":"UNKNOWN_PENDING_ORIGINAL_PRODUCER_WITNESS",
            "producer_witness_source":producer_witness_source,
            "rule":"Local send/receipt/block timestamps are not producer execution timestamps. G0.8 producer-timing evaluation requires the original producing execution attempt."
        },
        "next_required_gate":"G0.8",
        "critical_rule":"DO NOT RETRY THIS ATTEMPT AUTOMATICALLY; reconcile canonical execution first."
    });

    let pretty = serde_json::to_string_pretty(&evidence).unwrap();
    println!("{pretty}");
    if let Ok(path) = env::var("DARKBOT_G07_EVIDENCE_PATH") {
        if let Err(e) = fs::write(&path, format!("{pretty}\n")) {
            eprintln!("WARNING: G0.7 evidence write failed: {path}: {e}");
        }
    }
    if !capture_complete {
        std::process::exit(2);
    }
}
