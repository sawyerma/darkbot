//! Controlled BSC Testnet birth->BUY latency/inclusion test.
//! Creates a fresh Flap TAXED_V3 token with quoteAmt=0, observes that exact launch
//! through direct BSC P2P transaction gossip, then builds/signs/sends the BUY.
//! Public WSS remains parallel control/evidence and submission transport only.
//! Testnet only. No Mainnet addresses or value are used.

use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use alloy_sol_types::{sol, SolCall};
use k256::ecdsa::SigningKey;
use darkbot_gate0::p2p_ingress::spawn_testnet_flap_sensor;
use reth_chainspec::Head;
use rustls::{
    pki_types::ServerName,
    ClientConfig,
    ClientConnection,
    RootCertStore,
    StreamOwned,
};
use serde_json::{json, Value};
use std::{
    env,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    process::Command,
    str::FromStr,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
    function balanceOf(address owner) external view returns(uint256);
}

const RPC_DEFAULT: &str = "https://data-seed-prebsc-1-s1.bnbchain.org:8545/";
const WSS_HOST: &str = "bsc-testnet-rpc.publicnode.com";
const WSS_PATH: &str = "/";
const LAUNCH_HTTPS_HOST_DEFAULT: &str = "data-seed-prebsc-1-s1.bnbchain.org";
const LAUNCH_HTTPS_PATH: &str = "/";
const CHAIN_ID: u64 = 97;
const PORTAL: &str = "0x5bEacaF7ABCbB3aB280e80D007FD31fcE26510e9";
const TAX_V3_IMPL: &str = "0xE6Ff967a887084c16D0fD71548CF709542cc1557";
const WALLET: &str = "0x9c8a0Db41914beEE52Dd49790c4F087E252743DE";

const BUY_VALUE_WEI: u64 = 100_000_000_000_000; // 0.0001 tBNB
const BUY_GAS_LIMIT: u64 = 3_000_000; // fresh TAXED_V3 first BUY can be much heavier than a warm existing-token BUY
const MAX_GAS_PRICE_WEI: u64 = 1_000_000_000; // 1 gwei
const MAX_LAUNCH_GAS: u64 = 15_000_000;
const TESTNET_MIN_OUTPUT: u64 = 0; // testnet latency/inclusion only; acquisition >0 is verified after execution

fn rpc(method: &str, params: Value) -> Value {
    let rpc = env::var("BSC_TESTNET_RPC_HTTPS").unwrap_or_else(|_| RPC_DEFAULT.to_string());
    let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();

    let out = Command::new("curl")
        .args([
            "-sS",
            &rpc,
            "-H",
            "Content-Type: application/json",
            "--data",
            &body,
        ])
        .output()
        .expect("failed to execute curl");

    assert!(
        out.status.success(),
        "curl failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v: Value = serde_json::from_slice(&out.stdout).expect("invalid JSON-RPC response");
    if let Some(err) = v.get("error") {
        panic!("JSON-RPC error for {method}: {err}");
    }
    v.get("result").cloned().expect("JSON-RPC missing result")
}

fn hex_u64(v: &Value) -> u64 {
    u64::from_str_radix(
        v.as_str().expect("expected hex string").trim_start_matches("0x"),
        16,
    )
    .unwrap()
}

fn hex_u256(v: &Value) -> U256 {
    U256::from_str_radix(
        v.as_str().expect("expected hex string").trim_start_matches("0x"),
        16,
    )
    .unwrap()
}

fn latest_p2p_head() -> Head {
    let b = rpc("eth_getBlockByNumber", json!(["latest", false]));
    Head {
        number: hex_u64(&b["number"]),
        hash: B256::from_str(b["hash"].as_str().expect("latest block hash missing")).unwrap(),
        difficulty: hex_u256(&b["difficulty"]),
        total_difficulty: b
            .get("totalDifficulty")
            .filter(|v| !v.is_null())
            .map(hex_u256)
            .unwrap_or(U256::ZERO),
        timestamp: hex_u64(&b["timestamp"]),
    }
}

fn signer_address(key: &SigningKey) -> Address {
    let point = key.verifying_key().to_encoded_point(false);
    let h = keccak256(&point.as_bytes()[1..]);
    Address::from_slice(&h.as_slice()[12..])
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

fn sign_legacy(
    signer: &SigningKey,
    nonce: u64,
    gas_price: u64,
    gas_limit: u64,
    to: Address,
    value: U256,
    calldata: &[u8],
) -> Vec<u8> {
    let unsigned = rlp_list(vec![
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

    let sighash = keccak256(&unsigned);
    let (sig, recovery_id) = signer
        .sign_prehash_recoverable(sighash.as_slice())
        .expect("signing failed");
    let sig_bytes = sig.to_bytes();
    let v = 35u64 + 2 * CHAIN_ID + recovery_id.to_byte() as u64;

    rlp_list(vec![
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(to.as_slice()),
        rlp_u256(value),
        rlp_bytes(calldata),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sig_bytes[..32])),
        rlp_u256(U256::from_be_slice(&sig_bytes[32..])),
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

fn find_vanity_salt(sender: Address, nonce: u64) -> (B256, Address, u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_be_bytes();

    let mut seed = Vec::new();
    seed.extend_from_slice(sender.as_slice());
    seed.extend_from_slice(&nonce.to_be_bytes());
    seed.extend_from_slice(&now);
    let mut salt: B256 = keccak256(seed);

    for i in 0u64..2_000_000 {
        let token = predicted_token(salt);
        let a = token.as_slice();
        if a[18] == 0x77 && a[19] == 0x77 {
            return (salt, token, i + 1);
        }
        salt = keccak256(salt.as_slice());
    }

    panic!("vanity salt not found within bound");
}

fn launch_calldata(sender: Address, salt: B256) -> Vec<u8> {
    newTokenV6Call {
        params: LaunchParams {
            name: "DarkBot G07 Birth Test".to_string(),
            symbol: "DBG07".to_string(),
            uri: "ipfs://darkbot-g07-controlled-test".to_string(),
            dexThresh: 1,
            salt,
            migratorType: 1,
            quoteToken: Address::ZERO,
            quoteAmt: U256::ZERO,
            beneficiary: sender,
            permit: Bytes::new(),
            extensionId: B256::ZERO,
            extensionData: Bytes::new(),
            dexId: 0,
            lpFeeProfile: 0,
            buyTax: 100,
            sellTax: 100,
            taxDuration: 31_536_000,
            antiFarmerDuration: 3_600,
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

fn buy_calldata(token: Address) -> Vec<u8> {
    swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: U256::from(BUY_VALUE_WEI),
            minOutputAmount: U256::from(TESTNET_MIN_OUTPUT),
            permit: Bytes::new(),
        },
    }
    .abi_encode()
}

type Tls = StreamOwned<ClientConnection, TcpStream>;

fn https_connect(host: &str) -> Tls {
    let addr = (host, 443)
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("HTTPS DNS returned no address");

    let tcp =
        TcpStream::connect_timeout(&addr, Duration::from_secs(5)).expect("HTTPS TCP connect failed");
    tcp.set_nodelay(true).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();

    let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = ServerName::try_from(host.to_string()).unwrap();
    let conn = ClientConnection::new(Arc::new(config), server_name).unwrap();
    StreamOwned::new(conn, tcp)
}

fn https_send_json(stream: &mut Tls, host: &str, body: &str) {
    let req = format!(
        "POST {LAUNCH_HTTPS_PATH} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: keep-alive\r\n\
         \r\n{}",
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).expect("HTTPS write failed");
    stream.flush().expect("HTTPS flush failed");
}

fn ws_connect() -> Tls {
    let addr = (WSS_HOST, 443)
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("WSS DNS returned no address");

    let tcp =
        TcpStream::connect_timeout(&addr, Duration::from_secs(5)).expect("WSS TCP connect failed");
    tcp.set_nodelay(true).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();

    let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = ServerName::try_from(WSS_HOST.to_string()).unwrap();
    let conn = ClientConnection::new(Arc::new(config), server_name).unwrap();
    let mut stream = StreamOwned::new(conn, tcp);

    let req = format!(
        "GET {WSS_PATH} HTTP/1.1\r\n\
         Host: {WSS_HOST}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: ZGFya2JvdC1nMDctdGVzdA==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n"
    );
    stream.write_all(req.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut header = Vec::new();
    let mut b = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        stream
            .read_exact(&mut b)
            .expect("WSS upgrade response failed");
        header.push(b[0]);
        assert!(header.len() < 16_384, "WSS upgrade header too large");
    }

    let text = String::from_utf8_lossy(&header);
    assert!(
        text.starts_with("HTTP/1.1 101") || text.starts_with("HTTP/1.0 101"),
        "WSS upgrade failed: {text}"
    );

    stream
}

fn ws_send_frame(stream: &mut Tls, opcode: u8, payload: &[u8], mask_counter: &mut u32) {
    let mut frame = Vec::with_capacity(payload.len() + 14);
    frame.push(0x80 | opcode);

    let n = payload.len();
    if n <= 125 {
        frame.push(0x80 | n as u8);
    } else if n <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(n as u64).to_be_bytes());
    }

    *mask_counter = mask_counter.wrapping_add(0x9e37_79b9);
    let mask = mask_counter.to_be_bytes();
    frame.extend_from_slice(&mask);

    for (i, b) in payload.iter().enumerate() {
        frame.push(*b ^ mask[i & 3]);
    }

    stream.write_all(&frame).expect("WSS write failed");
    stream.flush().expect("WSS flush failed");
}

fn ws_send_text(stream: &mut Tls, text: &str, mask_counter: &mut u32) {
    ws_send_frame(stream, 0x1, text.as_bytes(), mask_counter);
}

fn ws_read_message(stream: &mut Tls, mask_counter: &mut u32) -> String {
    let mut collected = Vec::new();
    let mut started = false;

    loop {
        let mut h = [0u8; 2];
        stream.read_exact(&mut h).expect("WSS frame header failed");
        let fin = h[0] & 0x80 != 0;
        let opcode = h[0] & 0x0f;
        let masked = h[1] & 0x80 != 0;
        let mut len = (h[1] & 0x7f) as u64;

        if len == 126 {
            let mut x = [0u8; 2];
            stream.read_exact(&mut x).unwrap();
            len = u16::from_be_bytes(x) as u64;
        } else if len == 127 {
            let mut x = [0u8; 8];
            stream.read_exact(&mut x).unwrap();
            len = u64::from_be_bytes(x);
        }

        assert!(len <= 16 * 1024 * 1024, "WSS frame unexpectedly large");

        let mut mask = [0u8; 4];
        if masked {
            stream.read_exact(&mut mask).unwrap();
        }

        let mut payload = vec![0u8; len as usize];
        stream.read_exact(&mut payload).unwrap();

        if masked {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= mask[i & 3];
            }
        }

        match opcode {
            0x8 => panic!("WSS server closed connection"),
            0x9 => {
                ws_send_frame(stream, 0xA, &payload, mask_counter);
                continue;
            }
            0xA => continue,
            0x1 => {
                collected.extend_from_slice(&payload);
                started = true;
            }
            0x0 if started => collected.extend_from_slice(&payload),
            _ => continue,
        }

        if fin && started {
            return String::from_utf8(collected).expect("non-UTF8 WSS JSON frame");
        }
    }
}

fn wait_rpc_id(stream: &mut Tls, id: u64, mask_counter: &mut u32) -> Value {
    loop {
        let text = ws_read_message(stream, mask_counter);
        let v: Value = serde_json::from_str(&text).expect("invalid WSS JSON");

        if v.get("id").and_then(Value::as_u64) == Some(id) {
            if let Some(e) = v.get("error") {
                panic!("WSS RPC error id={id}: {e}");
            }
            return v["result"].clone();
        }
    }
}

fn receipt(tx_hash: &str) -> Value {
    for _ in 0..100 {
        let r = rpc("eth_getTransactionReceipt", json!([tx_hash]));
        if !r.is_null() {
            return r;
        }
        thread::sleep(Duration::from_millis(100));
    }

    panic!("receipt timeout: {tx_hash}");
}

fn balance_of(token: Address, owner: Address) -> U256 {
    let data = balanceOfCall { owner }.abi_encode();

    hex_u256(&rpc(
        "eth_call",
        json!([{
            "to": token.to_string(),
            "data": format!("0x{}", hex::encode(data))
        }, "latest"]),
    ))
}

fn successful_competing_buys_between(
    launch_block: u64,
    launch_index: u64,
    buy_block: u64,
    buy_index: u64,
    token: Address,
) -> u64 {
    if buy_block < launch_block || buy_block.saturating_sub(launch_block) > 20 {
        return u64::MAX;
    }

    let portal = PORTAL.to_lowercase();
    let mut count = 0u64;

    for block in launch_block..=buy_block {
        let b = rpc(
            "eth_getBlockByNumber",
            json!([format!("0x{block:x}"), true]),
        );
        let txs = b["transactions"]
            .as_array()
            .expect("block transactions missing");

        for tx in txs {
            let idx = tx["transactionIndex"]
                .as_str()
                .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
                .unwrap_or(0);

            let after_launch = block > launch_block || idx > launch_index;
            let before_buy = block < buy_block || idx < buy_index;
            if !after_launch || !before_buy {
                continue;
            }

            if tx["to"].as_str().unwrap_or("").to_lowercase() != portal {
                continue;
            }

            let input = tx["input"].as_str().unwrap_or("");
            let Ok(raw) = hex::decode(input.trim_start_matches("0x")) else {
                continue;
            };
            let Ok(call) = swapExactInputCall::abi_decode_validate(&raw) else {
                continue;
            };
            if call.params.outputToken != token {
                continue;
            }

            let h = tx["hash"].as_str().expect("tx hash missing");
            let r = receipt(h);
            if hex_u64(&r["status"]) == 1 {
                count += 1;
            }
        }
    }

    count
}

fn main() {
    assert_eq!(
        env::var("DARKBOT_G07_BIRTH_BUY").as_deref(),
        Ok("YES"),
        "set DARKBOT_G07_BIRTH_BUY=YES to arm this controlled testnet launch+BUY"
    );

    let expected_wallet = Address::from_str(WALLET).unwrap();
    let portal = Address::from_str(PORTAL).unwrap();

    let key_hex = env::var("DARKBOT_TESTNET_PRIVATE_KEY")
        .expect("DARKBOT_TESTNET_PRIVATE_KEY is not set");
    let key_bytes = hex::decode(key_hex.trim().trim_start_matches("0x"))
        .expect("private key is not valid hex");
    assert_eq!(key_bytes.len(), 32, "private key must be exactly 32 bytes");
    let signer = SigningKey::from_slice(&key_bytes).expect("invalid secp256k1 private key");

    let sender = signer_address(&signer);
    assert_eq!(
        sender, expected_wallet,
        "private key does not match frozen test wallet"
    );

    let chain_id = hex_u64(&rpc("eth_chainId", json!([])));
    assert_eq!(chain_id, CHAIN_ID, "wrong chain");

    let balance_before = hex_u256(&rpc("eth_getBalance", json!([WALLET, "latest"])));
    let nonce = hex_u64(&rpc(
        "eth_getTransactionCount",
        json!([WALLET, "pending"]),
    ));
    let gas_price = hex_u64(&rpc("eth_gasPrice", json!([])));
    assert!(
        gas_price <= MAX_GAS_PRICE_WEI,
        "gas price exceeds test ceiling"
    );

    let salt_start = Instant::now();
    let (salt, predicted, salt_iterations) = find_vanity_salt(sender, nonce);
    let salt_time = salt_start.elapsed();

    let launch_data = launch_calldata(sender, salt);
    let estimate_obj = json!({
        "from": WALLET,
        "to": PORTAL,
        "value": "0x0",
        "data": format!("0x{}", hex::encode(&launch_data))
    });

    let launch_estimate = hex_u64(&rpc("eth_estimateGas", json!([estimate_obj])));
    let launch_gas = launch_estimate
        .checked_mul(12)
        .and_then(|x| x.checked_div(10))
        .expect("launch gas overflow");
    assert!(launch_gas <= MAX_LAUNCH_GAS, "launch gas exceeds cap");

    let worst_case_debit = launch_gas as u128 * gas_price as u128
        + BUY_VALUE_WEI as u128
        + BUY_GAS_LIMIT as u128 * gas_price as u128;
    assert!(
        U256::from(worst_case_debit) <= balance_before,
        "insufficient tBNB"
    );

    let launch_signed = sign_legacy(
        &signer,
        nonce,
        gas_price,
        launch_gas,
        portal,
        U256::ZERO,
        &launch_data,
    );

    let launch_hash = keccak256(&launch_signed);
    let launch_hash_hex = format!("0x{}", hex::encode(launch_hash));

    println!("G0.7 TESTNET BIRTH->BUY CONTROLLED TEST");
    println!("chain_id:                 {chain_id}");
    println!("wallet:                   {sender}");
    println!("portal:                   {portal}");
    println!("nonce_launch:             {nonce}");
    println!("nonce_buy:                {}", nonce + 1);
    println!("gas_price_wei:            {gas_price}");
    println!("launch_gas_estimate:      {launch_estimate}");
    println!("launch_gas_limit:         {launch_gas}");
    println!("buy_gas_limit:            {BUY_GAS_LIMIT}");
    println!("buy_value_wei:            {BUY_VALUE_WEI}");
    println!("predicted_token:          {predicted}");
    println!("vanity_iterations:        {salt_iterations}");
    println!(
        "vanity_search_us:         {:.3}",
        salt_time.as_nanos() as f64 / 1000.0
    );
    println!("launch_local_tx_hash:     {launch_hash_hex}");
    println!("hotpath_remote_reads:     0");
    println!(
        "testnet_min_output:       {TESTNET_MIN_OUTPUT} (TESTNET LATENCY ONLY)"
    );

    // Race ingress: direct BSC devp2p/eth transaction propagation via standalone Reth.
    // No full-node sync, DB, WSS or synchronous RPC lookup is involved here.
    let p2p_min_peers = env::var("DARKBOT_G07_P2P_MIN_PEERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(8)
        .max(1);
    println!("p2p_min_peers:             {p2p_min_peers}");
    let p2p_head = latest_p2p_head();
    println!("p2p_head_number:           {}", p2p_head.number);
    println!("p2p_head_timestamp:        {}", p2p_head.timestamp);
    let p2p = spawn_testnet_flap_sensor(
        portal,
        newTokenV6Call::SELECTOR,
        launch_hash,
        p2p_min_peers,
        p2p_head,
    );
    let p2p_ready = p2p
        .ready
        .recv_timeout(Duration::from_secs(30))
        .expect("BSC P2P sensor did not reach minimum peer count");
    println!("p2p_ready_peers:           {}", p2p_ready.peers);

    // Parallel WSS control sensor. It never gates BUY construction.
    let mut control_ws = ws_connect();
    let mut control_mask = 0x4b1d_2e37u32;
    ws_send_text(
        &mut control_ws,
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newPendingTransactions",true]}"#,
        &mut control_mask,
    );
    let sub = wait_rpc_id(&mut control_ws, 1, &mut control_mask);
    println!(
        "control_subscription:      {}",
        sub.as_str().unwrap_or("<non-string>")
    );

    let control_hash = launch_hash_hex.clone();
    let (control_tx, control_rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        loop {
            let text = ws_read_message(&mut control_ws, &mut control_mask);
            let frame_arrival = Instant::now();
            let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
            let Some(result) = v.pointer("/params/result") else { continue };

            let matched = if let Some(h) = result.as_str() {
                h.eq_ignore_ascii_case(&control_hash)
            } else if let Some(obj) = result.as_object() {
                obj.get("hash")
                    .and_then(Value::as_str)
                    .map(|h| h.eq_ignore_ascii_case(&control_hash))
                    .unwrap_or(false)
            } else {
                false
            };

            if matched {
                let _ = control_tx.send(frame_arrival);
                break;
            }
        }
    });

    // Independent launch injection. PublicNode WSS is not used to inject the
    // launch, so its subscription remains a genuine control sensor.
    let launch_https_host = env::var("DARKBOT_G07_LAUNCH_HTTPS_HOST")
        .unwrap_or_else(|_| LAUNCH_HTTPS_HOST_DEFAULT.to_string());
    let mut launch_https = https_connect(&launch_https_host);
    let launch_body = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"eth_sendRawTransaction","params":["0x{}"]}}"#,
        hex::encode(&launch_signed)
    );
    https_send_json(&mut launch_https, &launch_https_host, &launch_body);
    let launch_handoff = Instant::now();
    println!("launch_injection_transport: independent HTTPS {launch_https_host}");

    // BUY submission remains on its own warm WSS connection; it is not a signal source.
    let mut ws = ws_connect();
    let mut mask_counter = 0x17a2_93c1u32;

    let p2p_signal = loop {
        let signal = p2p
            .signals
            .recv_timeout(Duration::from_secs(15))
            .expect("timed out waiting for direct BSC P2P launch signal");
        if signal.hash == launch_hash {
            break signal;
        }
    };

    let t_signal = p2p_signal.observed;
    let signal_mode = p2p_signal.source.as_str();
    let signal_launch_data = p2p_signal.input;
    println!("p2p_signal_peers:          {}", p2p_signal.peers);
    println!("p2p_signal_source:         {signal_mode}");
    if let Some(t_hash) = p2p_signal.hash_observed {
        println!(
            "P2P_hash_to_full_ns:       {}",
            (t_signal - t_hash).as_nanos()
        );
    } else {
        println!("P2P_hash_to_full_ns:       DIRECT_FULL_BROADCAST");
    }

    let decoded = newTokenV6Call::abi_decode_validate(&signal_launch_data)
        .expect("pending launch decode failed");
    assert_eq!(decoded.params.tokenVersion, 6);
    assert_eq!(decoded.params.quoteToken, Address::ZERO);
    assert_eq!(decoded.params.salt, salt);

    let derived = predicted_token(decoded.params.salt);
    assert_eq!(derived, predicted);
    let t_decode = Instant::now();

    let buy_data = buy_calldata(derived);
    let unsigned = rlp_list(vec![
        rlp_u64(nonce + 1),
        rlp_u64(gas_price),
        rlp_u64(BUY_GAS_LIMIT),
        rlp_bytes(portal.as_slice()),
        rlp_u256(U256::from(BUY_VALUE_WEI)),
        rlp_bytes(&buy_data),
        rlp_u64(CHAIN_ID),
        rlp_bytes(&[]),
        rlp_bytes(&[]),
    ]);
    let sighash = keccak256(&unsigned);
    let t_build = Instant::now();

    let (sig, recovery_id) = signer
        .sign_prehash_recoverable(sighash.as_slice())
        .expect("BUY signing failed");
    let t_sign = Instant::now();

    let sig_bytes = sig.to_bytes();
    let v = 35u64 + 2 * CHAIN_ID + recovery_id.to_byte() as u64;
    let buy_signed = rlp_list(vec![
        rlp_u64(nonce + 1),
        rlp_u64(gas_price),
        rlp_u64(BUY_GAS_LIMIT),
        rlp_bytes(portal.as_slice()),
        rlp_u256(U256::from(BUY_VALUE_WEI)),
        rlp_bytes(&buy_data),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sig_bytes[..32])),
        rlp_u256(U256::from_be_slice(&sig_bytes[32..])),
    ]);

    let buy_hash = keccak256(&buy_signed);
    let buy_hash_hex = format!("0x{}", hex::encode(buy_hash));
    let t_ready = Instant::now();

    let buy_body = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"eth_sendRawTransaction","params":["0x{}"]}}"#,
        hex::encode(&buy_signed)
    );

    let t_queue = Instant::now();
    ws_send_text(&mut ws, &buy_body, &mut mask_counter);
    let t_send = Instant::now();

    let buy_ack = loop {
        let text = ws_read_message(&mut ws, &mut mask_counter);
        let v: Value = serde_json::from_str(&text).expect("invalid WSS JSON");

        if v.get("id").and_then(Value::as_u64) != Some(3) {
            continue;
        }
        if let Some(e) = v.get("error") {
            panic!("BUY broadcast rejected: {e}");
        }
        break v["result"]
            .as_str()
            .expect("BUY tx hash missing")
            .to_string();
    };
    let t_ack = Instant::now();

    assert_eq!(buy_ack.to_lowercase(), buy_hash_hex.to_lowercase());
    let t_wss_control = control_rx.recv_timeout(Duration::from_secs(5)).ok();
    let t_p2p_hash = p2p.hashes.try_recv().ok();

    let launch_receipt = receipt(&launch_hash_hex);
    let buy_receipt = receipt(&buy_hash_hex);
    let t_observed = Instant::now();

    let launch_status = hex_u64(&launch_receipt["status"]);
    let buy_status = hex_u64(&buy_receipt["status"]);
    let launch_gas_used = hex_u64(&launch_receipt["gasUsed"]);
    let buy_gas_used = hex_u64(&buy_receipt["gasUsed"]);

    println!("launch_receipt_status:    {launch_status}");
    println!("launch_gas_used:          {launch_gas_used}");
    println!("buy_tx:                   {buy_hash_hex}");
    println!("buy_receipt_status:       {buy_status}");
    println!("buy_gas_used:             {buy_gas_used}");
    println!("buy_gas_limit:            {BUY_GAS_LIMIT}");

    assert_eq!(launch_status, 1, "launch reverted");
    assert_eq!(
        buy_status, 1,
        "BUY reverted; compare buy_gas_used with buy_gas_limit above"
    );

    let launch_block = hex_u64(&launch_receipt["blockNumber"]);
    let launch_index = hex_u64(&launch_receipt["transactionIndex"]);
    let buy_block = hex_u64(&buy_receipt["blockNumber"]);
    let buy_index = hex_u64(&buy_receipt["transactionIndex"]);

    let ordered =
        buy_block > launch_block || (buy_block == launch_block && buy_index > launch_index);
    assert!(ordered, "BUY did not execute after launch");

    let acquired_balance = balance_of(derived, sender);
    assert!(
        acquired_balance > U256::ZERO,
        "BUY receipt succeeded but token balance is zero"
    );

    let native_after = hex_u256(&rpc("eth_getBalance", json!([WALLET, "latest"])));
    let native_debit = balance_before - native_after;

    let competitors = successful_competing_buys_between(
        launch_block,
        launch_index,
        buy_block,
        buy_index,
        derived,
    );

    println!();
    println!("=== G0.7 TESTNET BIRTH->BUY RESULT ===");
    println!("status:                    PASS");
    println!("signal_mode:               {signal_mode}");
    println!("launch_ack_seen:           INDEPENDENT_HTTPS_NOT_WAITED");
    if let Some(t_hash) = t_p2p_hash {
        let hash_delta_ns: i128 = if t_hash >= launch_handoff {
            (t_hash - launch_handoff).as_nanos() as i128
        } else {
            -((launch_handoff - t_hash).as_nanos() as i128)
        };
        println!("launch_handoff_to_P2P_hash_ns: {hash_delta_ns}");
    } else {
        println!("launch_handoff_to_P2P_hash_ns: UNKNOWN_OR_FULL_ONLY");
    }

    if let Some(t_wss) = t_wss_control {
        let delta_ns: i128 = if t_wss >= t_signal {
            (t_wss - t_signal).as_nanos() as i128
        } else {
            -((t_signal - t_wss).as_nanos() as i128)
        };
        println!("WSS_minus_P2P_full_ns:     {delta_ns}");

        if let Some(t_hash) = t_p2p_hash {
            let hash_vs_wss: i128 = if t_wss >= t_hash {
                (t_wss - t_hash).as_nanos() as i128
            } else {
                -((t_hash - t_wss).as_nanos() as i128)
            };
            println!("WSS_minus_P2P_hash_ns:     {hash_vs_wss}");
        } else {
            println!("WSS_minus_P2P_hash_ns:     UNKNOWN");
        }
    } else {
        println!("WSS_minus_P2P_full_ns:     UNKNOWN");
        println!("WSS_minus_P2P_hash_ns:     UNKNOWN");
    }
    println!("launch_tx:                 {launch_hash_hex}");
    println!("buy_tx:                    {buy_hash_hex}");
    println!("token:                     {derived}");
    println!("token_balance_after:       {acquired_balance}");
    println!("native_debit_wei:          {native_debit}");
    println!("launch_block:              {launch_block}");
    println!("launch_tx_index:           {launch_index}");
    println!("buy_block:                 {buy_block}");
    println!("buy_tx_index:              {buy_index}");
    println!("same_block:                {}", launch_block == buy_block);
    println!(
        "successful_buys_between:   {}",
        if competitors == u64::MAX {
            "UNKNOWN".to_string()
        } else {
            competitors.to_string()
        }
    );
    println!();
    println!(
        "T_signal_to_decode_ns:     {}",
        (t_decode - t_signal).as_nanos()
    );
    println!(
        "T_decode_to_build_ns:      {}",
        (t_build - t_decode).as_nanos()
    );
    println!(
        "T_build_to_sign_ns:        {}",
        (t_sign - t_build).as_nanos()
    );
    println!(
        "T_sign_to_ready_ns:        {}",
        (t_ready - t_sign).as_nanos()
    );
    println!(
        "T_ready_to_queue_ns:       {}",
        (t_queue - t_ready).as_nanos()
    );
    println!(
        "T_queue_to_send_ns:        {}",
        (t_send - t_queue).as_nanos()
    );
    println!(
        "T_signal_to_ready_ns:      {}",
        (t_ready - t_signal).as_nanos()
    );
    println!(
        "T_signal_to_send_ns:       {}",
        (t_send - t_signal).as_nanos()
    );
    println!(
        "T_signal_to_ack_ns:        {}",
        (t_ack - t_signal).as_nanos()
    );
    let launch_handoff_to_signal_ns: i128 = if t_signal >= launch_handoff {
        (t_signal - launch_handoff).as_nanos() as i128
    } else {
        -((launch_handoff - t_signal).as_nanos() as i128)
    };
    println!(
        "launch_handoff_to_signal_ns: {launch_handoff_to_signal_ns}"
    );
    println!(
        "T_signal_to_receipts_seen_ns: {}",
        (t_observed - t_signal).as_nanos()
    );
    println!("race_ingress_boundary:     raw BSC ETH P2P transaction channel before txpool");
    println!("transport_boundary:        BUY live WSS write_all+flush; launch independent HTTPS write_all+flush");
    println!("T_buy_minus_T0:            UNKNOWN_FROM_STANDARD_BSC_BLOCK_DATA");
    println!("G0.7_TESTNET_BIRTH_BUY_PASS: true");
}

