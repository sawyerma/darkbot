use alloy_primitives::{keccak256, Address, Bytes, U256};
use alloy_sol_types::{sol, SolCall};
use k256::ecdsa::SigningKey;
use serde_json::{json, Value};
use std::{
    env,
    process::Command,
    str::FromStr,
    thread,
    time::Duration,
};

sol! {
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

const RPC: &str = "https://data-seed-prebsc-1-s1.bnbchain.org:8545/";
const CHAIN_ID: u64 = 97;
const PORTAL: &str = "0x5bEacaF7ABCbB3aB280e80D007FD31fcE26510e9";
const WALLET: &str = "0x9c8a0Db41914beEE52Dd49790c4F087E252743DE";
const TOKEN: &str = "0xe39b945a917a3a7E17E006896FEc1f5e4B3D7777";
const VALUE_WEI: u64 = 100_000_000_000_000; // 0.0001 tBNB
const MIN_OUTPUT: &str = "17134827040464882203780"; // 1% below frozen preview
const MAX_GAS_LIMIT: u64 = 300_000;
const MAX_GAS_PRICE_WEI: u64 = 1_000_000_000; // 1 gwei hard ceiling
const MAX_TOTAL_DEBIT_WEI: u128 = 500_000_000_000_000; // 0.0005 tBNB hard ceiling

fn rpc(method: &str, params: Value) -> Value {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    }).to_string();

    let out = Command::new("curl")
        .args([
            "-sS",
            RPC,
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

    let v: Value = serde_json::from_slice(&out.stdout)
        .expect("invalid JSON-RPC response");

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

fn signer_address(key: &SigningKey) -> Address {
    let point = key.verifying_key().to_encoded_point(false);
    let raw = point.as_bytes();
    assert_eq!(raw[0], 0x04);
    let h = keccak256(&raw[1..]);
    Address::from_slice(&h.as_slice()[12..])
}

fn main() {
    let expected_wallet = Address::from_str(WALLET).unwrap();
    let portal = Address::from_str(PORTAL).unwrap();
    let token = Address::from_str(TOKEN).unwrap();

    let key_hex = env::var("DARKBOT_TESTNET_PRIVATE_KEY")
        .expect("DARKBOT_TESTNET_PRIVATE_KEY is not set");
    let key_bytes = hex::decode(key_hex.trim().trim_start_matches("0x"))
        .expect("private key is not valid hex");
    assert_eq!(key_bytes.len(), 32, "private key must be exactly 32 bytes");
    let signer = SigningKey::from_slice(&key_bytes)
        .expect("invalid secp256k1 private key");

    let derived_wallet = signer_address(&signer);
    assert_eq!(
        derived_wallet,
        expected_wallet,
        "PRIVATE KEY DOES NOT MATCH THE FROZEN TEST WALLET"
    );

    let chain_id = hex_u64(&rpc("eth_chainId", json!([])));
    assert_eq!(chain_id, CHAIN_ID, "wrong chain");

    let balance_before_native = hex_u256(&rpc(
        "eth_getBalance",
        json!([WALLET, "latest"]),
    ));
    assert!(
        balance_before_native >= U256::from(VALUE_WEI),
        "insufficient tBNB"
    );

    let balance_call = balanceOfCall { owner: expected_wallet }.abi_encode();
    let token_balance_before = hex_u256(&rpc(
        "eth_call",
        json!([{
            "to": TOKEN,
            "data": format!("0x{}", hex::encode(&balance_call))
        }, "latest"]),
    ));

    let nonce = hex_u64(&rpc(
        "eth_getTransactionCount",
        json!([WALLET, "pending"]),
    ));
    let gas_price = hex_u64(&rpc("eth_gasPrice", json!([])));
    assert!(
        gas_price <= MAX_GAS_PRICE_WEI,
        "gas price exceeds hard ceiling: {gas_price}"
    );

    let min_output = U256::from_str(MIN_OUTPUT).unwrap();
    let calldata = swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: U256::from(VALUE_WEI),
            minOutputAmount: min_output,
            permit: Bytes::new(),
        },
    }
    .abi_encode();

    let tx_obj = json!({
        "from": WALLET,
        "to": PORTAL,
        "value": format!("0x{:x}", VALUE_WEI),
        "data": format!("0x{}", hex::encode(&calldata))
    });

    let preview = hex_u256(&rpc("eth_call", json!([tx_obj.clone(), "latest"])));
    assert!(
        preview >= min_output,
        "live preview fell below minOutput"
    );

    let gas_estimate = hex_u64(&rpc("eth_estimateGas", json!([tx_obj])));
    let gas_limit = gas_estimate
        .checked_mul(12)
        .and_then(|x| x.checked_div(10))
        .expect("gas limit overflow");

    assert!(
        gas_limit <= MAX_GAS_LIMIT,
        "gas limit exceeds hard ceiling: {gas_limit}"
    );

    let max_total_debit =
        VALUE_WEI as u128 + gas_limit as u128 * gas_price as u128;
    assert!(
        max_total_debit <= MAX_TOTAL_DEBIT_WEI,
        "maximum possible debit exceeds hard ceiling"
    );

    let unsigned = rlp_list(vec![
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(portal.as_slice()),
        rlp_u256(U256::from(VALUE_WEI)),
        rlp_bytes(&calldata),
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

    let signed = rlp_list(vec![
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(portal.as_slice()),
        rlp_u256(U256::from(VALUE_WEI)),
        rlp_bytes(&calldata),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sig_bytes[..32])),
        rlp_u256(U256::from_be_slice(&sig_bytes[32..])),
    ]);
    let local_tx_hash = keccak256(&signed);

    println!("G0.7 TESTNET VALID-BUY PREFLIGHT");
    println!("chain_id:              {chain_id}");
    println!("wallet:                {derived_wallet}");
    println!("portal:                {portal}");
    println!("token:                 {token}");
    println!("native_balance_before: {balance_before_native}");
    println!("token_balance_before:  {token_balance_before}");
    println!("nonce:                 {nonce}");
    println!("gas_price_wei:         {gas_price}");
    println!("gas_estimate:          {gas_estimate}");
    println!("gas_limit_120pct:      {gas_limit}");
    println!("buy_value_wei:         {VALUE_WEI}");
    println!("min_output:            {min_output}");
    println!("preview_output:        {preview}");
    println!("max_total_debit_wei:   {max_total_debit}");
    println!("local_tx_hash:         0x{}", hex::encode(local_tx_hash));

    let send_enabled = env::var("DARKBOT_TESTNET_SEND")
        .map(|v| v == "YES")
        .unwrap_or(false);

    if !send_enabled {
        println!("SEND_DISABLED: true");
        println!("PREFLIGHT_PASS: true");
        return;
    }

    let raw = format!("0x{}", hex::encode(&signed));
    let remote_hash = rpc("eth_sendRawTransaction", json!([raw]));
    let remote_hash_str = remote_hash.as_str().expect("tx hash not string");

    assert_eq!(
        remote_hash_str.to_lowercase(),
        format!("0x{}", hex::encode(local_tx_hash)),
        "RPC returned tx hash does not match locally signed tx"
    );

    println!("broadcast_tx_hash:     {remote_hash_str}");

    let mut receipt: Option<Value> = None;
    for _ in 0..60 {
        let r = rpc("eth_getTransactionReceipt", json!([remote_hash_str]));
        if !r.is_null() {
            receipt = Some(r);
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }

    let receipt = receipt.expect("receipt timeout");
    let status = hex_u64(&receipt["status"]);
    let block_number = hex_u64(&receipt["blockNumber"]);
    let tx_index = hex_u64(&receipt["transactionIndex"]);
    let gas_used = hex_u64(&receipt["gasUsed"]);

    assert_eq!(status, 1, "transaction reverted");

    let token_balance_after = hex_u256(&rpc(
        "eth_call",
        json!([{
            "to": TOKEN,
            "data": format!("0x{}", hex::encode(&balance_call))
        }, "latest"]),
    ));

    assert!(
        token_balance_after > token_balance_before,
        "receipt succeeded but token balance did not increase"
    );

    let acquired = token_balance_after - token_balance_before;
    let native_balance_after = hex_u256(&rpc(
        "eth_getBalance",
        json!([WALLET, "latest"]),
    ));

    println!("receipt_status:        {status}");
    println!("block_number:          {block_number}");
    println!("transaction_index:     {tx_index}");
    println!("gas_used:              {gas_used}");
    println!("token_balance_after:   {token_balance_after}");
    println!("token_acquired:        {acquired}");
    println!("native_balance_after:  {native_balance_after}");
    println!("G0.7_TESTNET_VALID_BUY_PASS: true");
}
