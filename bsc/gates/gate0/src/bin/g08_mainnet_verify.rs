//! G0.8 read-only BSC Mainnet acquisition, ordering, economics and canonical
//! 15-second evidence reconciliation for a preceding G0.7 attempt.
//!
//! This binary never submits a transaction. Builder acknowledgements are not
//! acquisition authority; canonical chain execution is.

use alloy_primitives::{keccak256, Address, U256};
use alloy_sol_types::{sol, SolCall};
use darkbot_gate0::qualification_capture::{CaptureRecord, JournalWriter, RecordKind};
use serde_json::{json, Value};
use std::{
    env,
    fs::{self, OpenOptions},
    process::Command,
    str::FromStr,
    thread,
    time::Duration,
};

const CHAIN_ID: u64 = 56;
const PORTAL: &str = "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0";
const MARKET_WINDOW_SECS: u64 = 15;
const RECEIPT_TIMEOUT_POLLS: usize = 240;
const RECEIPT_POLL: Duration = Duration::from_millis(500);

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

#[derive(Debug)]
struct ChainCaptureSummary {
    path: String,
    records: u64,
    bytes: u64,
    sha256: String,
    first_block: u64,
    last_block: u64,
    leading_boundary_included: bool,
    launch_timestamp: u64,
    horizon_timestamp: u64,
    boundary_block_included: bool,
}

fn req_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn norm(s: &str) -> String {
    s.trim().to_ascii_lowercase()
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

fn req_u256_string(v: &Value, ptr: &str) -> U256 {
    U256::from_str(req_str(v, ptr)).unwrap_or_else(|_| panic!("bad decimal U256 at {ptr}"))
}

fn rpc(method: &str, params: Value) -> Result<Value, String> {
    let url = req_env("BSC_MAINNET_RPC_HTTPS");
    let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
    let out = Command::new("curl")
        .args(["-sS", &url, "-H", "Content-Type: application/json", "--data", &body])
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

fn hex_u256(v: &Value) -> U256 {
    U256::from_str_radix(
        v.as_str().expect("expected hex string").trim_start_matches("0x"),
        16,
    )
    .expect("bad hex U256")
}

fn wait_receipt(hash: &str) -> Option<Value> {
    for _ in 0..RECEIPT_TIMEOUT_POLLS {
        match rpc("eth_getTransactionReceipt", json!([hash])) {
            Ok(v) if !v.is_null() => return Some(v),
            Ok(_) | Err(_) => thread::sleep(RECEIPT_POLL),
        }
    }
    None
}

fn balance_of_at(token: Address, owner: Address, block: u64) -> Result<U256, String> {
    let data = balanceOfCall { owner }.abi_encode();
    let value = rpc(
        "eth_call",
        json!([{
            "to":token.to_string(),
            "data":format!("0x{}", hex::encode(data))
        }, format!("0x{block:x}")]),
    )?;
    Ok(hex_u256(&value))
}

fn transaction_at_index<'a>(block: &'a Value, index: u64) -> Option<&'a Value> {
    block["transactions"].as_array()?.get(index as usize)
}

fn receipt_gas_cost(receipt: &Value, tx: &Value) -> U256 {
    let gas_used = hex_u256(&receipt["gasUsed"]);
    let gas_price = receipt
        .get("effectiveGasPrice")
        .filter(|v| !v.is_null())
        .map(hex_u256)
        .unwrap_or_else(|| hex_u256(&tx["gasPrice"]));
    gas_used * gas_price
}

/// Receipt-scoped ERC-20 Transfer deltas are attributable to this BUY,
/// unlike an end-of-block balance which may include unrelated transactions.
/// Missing or malformed transfer evidence remains UNKNOWN.
fn buy_receipt_transfer_delta(receipt: &Value, token: Address, buyer: Address) -> Option<U256> {
    let logs = receipt.get("logs")?.as_array()?;
    let selector = keccak256(b"Transfer(address,address,uint256)").to_string();
    let mut credited = U256::ZERO;
    let mut debited = U256::ZERO;
    let mut matched = false;
    for log in logs {
        if norm(log.get("address")?.as_str()?) != norm(&token.to_string()) {
            continue;
        }
        let topics = log.get("topics")?.as_array()?;
        if topics.first()?.as_str().map(norm).as_deref() != Some(norm(&selector).as_str()) {
            continue;
        }
        if topics.len() != 3 { return None; }
        let topic_address = |topic: &Value| -> Option<Address> {
            let raw = hex::decode(topic.as_str()?.trim_start_matches("0x")).ok()?;
            if raw.len() != 32 || raw[..12].iter().any(|b| *b != 0) { return None; }
            Some(Address::from_slice(&raw[12..]))
        };
        let sender = topic_address(&topics[1])?;
        let recipient = topic_address(&topics[2])?;
        let data = hex::decode(log.get("data")?.as_str()?.trim_start_matches("0x")).ok()?;
        if data.len() != 32 { return None; }
        let amount = U256::from_be_slice(&data);
        if recipient == buyer {
            credited = credited.checked_add(amount)?;
            matched = true;
        }
        if sender == buyer {
            debited = debited.checked_add(amount)?;
            matched = true;
        }
    }
    if !matched { return None; }
    credited.checked_sub(debited)
}

fn successful_competing_buys_between(
    launch_block: u64,
    launch_index: u64,
    buy_block: u64,
    buy_index: u64,
    token: Address,
) -> Option<Vec<String>> {
    if buy_block < launch_block || buy_block.saturating_sub(launch_block) > 32 {
        return None;
    }
    let mut hashes = Vec::new();
    for block_number in launch_block..=buy_block {
        let block = rpc(
            "eth_getBlockByNumber",
            json!([format!("0x{block_number:x}"), true]),
        )
        .ok()?;
        let txs = block["transactions"].as_array()?;
        for tx in txs {
            let idx = tx
                .get("transactionIndex")
                .and_then(Value::as_str)
                .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .flatten()
                .unwrap_or(0);
            let after_launch = block_number > launch_block || idx > launch_index;
            let before_buy = block_number < buy_block || idx < buy_index;
            if !after_launch || !before_buy {
                continue;
            }
            if norm(tx.get("to").and_then(Value::as_str).unwrap_or("")) != norm(PORTAL) {
                continue;
            }
            let input = tx.get("input").and_then(Value::as_str).unwrap_or("");
            let Ok(raw) = hex::decode(input.trim_start_matches("0x")) else {
                continue;
            };
            let Ok(call) = swapExactInputCall::abi_decode_validate(&raw) else {
                continue;
            };
            if call.params.outputToken != token {
                continue;
            }
            let hash = tx.get("hash").and_then(Value::as_str)?;
            let receipt = rpc("eth_getTransactionReceipt", json!([hash])).ok()?;
            if !receipt.is_null() && hex_u64(&receipt["status"]) == 1 {
                hashes.push(hash.to_string());
            }
        }
    }
    Some(hashes)
}

fn tagged_payload(kind: &str, block: u64, value: &Value) -> Vec<u8> {
    let json = serde_json::to_vec(value).expect("serialize chain capture");
    let mut out = Vec::with_capacity(32 + kind.len() + json.len());
    out.extend_from_slice(b"DBCHAIN1");
    out.extend_from_slice(&(kind.len() as u32).to_le_bytes());
    out.extend_from_slice(kind.as_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    out
}

fn block_receipts(block_number: u64, block: &Value) -> Result<Value, String> {
    if let Ok(receipts) = rpc(
        "eth_getBlockReceipts",
        json!([format!("0x{block_number:x}")]),
    ) {
        if receipts.is_array() {
            return Ok(receipts);
        }
    }

    let txs = block["transactions"]
        .as_array()
        .ok_or("full block transactions missing")?;
    let mut receipts = Vec::with_capacity(txs.len());
    for tx in txs {
        let hash = tx
            .get("hash")
            .and_then(Value::as_str)
            .ok_or("transaction hash missing during receipt backfill")?;
        let receipt = rpc("eth_getTransactionReceipt", json!([hash]))?;
        if receipt.is_null() {
            return Err(format!("receipt unresolved during canonical backfill: {hash}"));
        }
        receipts.push(receipt);
    }
    Ok(Value::Array(receipts))
}

fn capture_chain_window(
    path: &str,
    launch_block: u64,
    attempt_id: &str,
    launch_hash: &str,
    buy_hash: &str,
) -> Result<ChainCaptureSummary, String> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("create-new chain capture {path}: {e}"))?;
    let mut writer = JournalWriter::new(file);
    let mut sequence = 0u64;

    let launch = rpc(
        "eth_getBlockByNumber",
        json!([format!("0x{launch_block:x}"), true]),
    )?;
    if launch.is_null() {
        return Err("launch block unavailable for chain capture".into());
    }
    let launch_timestamp = hex_u64(&launch["timestamp"]);
    let horizon_timestamp = launch_timestamp.saturating_add(MARKET_WINDOW_SECS);

    let session_meta = json!({
        "schema":"darkbot.g08.chain-capture-session.v1",
        "attempt_id":attempt_id,
        "launch_tx_hash":launch_hash,
        "buy_tx_hash":buy_hash,
        "launch_block":launch_block,
        "launch_block_timestamp_seconds":launch_timestamp,
        "window_seconds":MARKET_WINDOW_SECS,
        "timestamp_source":"BSC_BLOCK_HEADER_SECONDS",
        "timestamp_resolution_ns":"1000000000",
        "producer_execution_timing_authority":false
    });
    writer
        .append(&CaptureRecord {
            sequence,
            kind: RecordKind::Runtime,
            timestamp_ns: launch_timestamp.saturating_mul(1_000_000_000),
            payload: tagged_payload("SESSION", launch_block, &session_meta).into_boxed_slice(),
        })
        .map_err(|e| format!("append chain capture session metadata: {e}"))?;
    sequence = sequence.checked_add(1).ok_or("chain capture sequence overflow")?;

    let first_block = launch_block.saturating_sub(1);
    let mut number = first_block;
    let mut crossed_horizon = false;
    let last_block = loop {
        let mut waits = 0u16;
        let block = loop {
            let value = rpc(
                "eth_getBlockByNumber",
                json!([format!("0x{number:x}"), true]),
            )?;
            if !value.is_null() {
                break value;
            }
            waits = waits.saturating_add(1);
            if waits >= 600 {
                return Err(format!(
                    "timed out waiting for canonical block {number} during 15-second backfill"
                ));
            }
            thread::sleep(Duration::from_millis(200));
        };
        let timestamp = hex_u64(&block["timestamp"]);
        let block_record = CaptureRecord {
            sequence,
            kind: RecordKind::Canonical,
            timestamp_ns: timestamp.saturating_mul(1_000_000_000),
            payload: tagged_payload("FULL_BLOCK", number, &block).into_boxed_slice(),
        };
        writer
            .append(&block_record)
            .map_err(|e| format!("append block {number}: {e}"))?;
        sequence = sequence.checked_add(1).ok_or("chain capture sequence overflow")?;

        let receipts = block_receipts(number, &block)?;
        let receipt_record = CaptureRecord {
            sequence,
            kind: RecordKind::Canonical,
            timestamp_ns: timestamp.saturating_mul(1_000_000_000),
            payload: tagged_payload("FULL_RECEIPTS", number, &receipts).into_boxed_slice(),
        };
        writer
            .append(&receipt_record)
            .map_err(|e| format!("append receipts {number}: {e}"))?;
        sequence = sequence.checked_add(1).ok_or("chain capture sequence overflow")?;
        if crossed_horizon {
            break number;
        }
        if timestamp >= horizon_timestamp {
            crossed_horizon = true;
        }
        number = number.checked_add(1).ok_or("block number overflow")?;
    };

    writer
        .flush()
        .map_err(|e| format!("chain capture flush: {e}"))?;
    let manifest = writer.manifest();
    let file = writer.into_inner();
    file.sync_all()
        .map_err(|e| format!("chain capture sync: {e}"))?;

    Ok(ChainCaptureSummary {
        path: path.to_string(),
        records: manifest.records,
        bytes: manifest.bytes,
        sha256: hex::encode(manifest.sha256),
        first_block,
        last_block,
        leading_boundary_included: first_block < launch_block,
        launch_timestamp,
        horizon_timestamp,
        boundary_block_included: last_block > launch_block,
    })
}

fn main() {
    let g07_path = req_env("DARKBOT_G08_G07_EVIDENCE");
    let chain_capture_path = req_env("DARKBOT_G08_CHAIN_JOURNAL");
    let g07 = load_json(&g07_path);

    assert_eq!(
        req_str(&g07, "/schema"),
        "darkbot.g07.mainnet-self-launch-buy.v2"
    );
    assert_eq!(req_u64(&g07, "/chain_id"), CHAIN_ID);
    assert_eq!(norm(req_str(&g07, "/portal")), norm(PORTAL));
    let local_capture_complete = req_bool(&g07, "/capture/complete_local_journal");

    let chain_id = hex_u64(&rpc("eth_chainId", json!([])).expect("chain-id read failed"));
    assert_eq!(chain_id, CHAIN_ID, "wrong RPC chain");

    let launch_hash = req_str(&g07, "/transactions/launch_tx_hash").to_string();
    let buy_hash = req_str(&g07, "/transactions/buy_tx_hash").to_string();
    let token = Address::from_str(req_str(&g07, "/transactions/token"))
        .expect("invalid G0.7 token address");
    let launcher = Address::from_str(req_str(&g07, "/wallets/launcher"))
        .expect("invalid launcher address");
    let buyer = Address::from_str(req_str(&g07, "/wallets/buyer"))
        .expect("invalid buyer address");
    let expected_min_output = req_u256_string(&g07, "/transactions/min_output");
    let expected_buy_value = req_u256_string(&g07, "/transactions/buy_value_wei");

    let launch_receipt = wait_receipt(&launch_hash);
    let buy_receipt = wait_receipt(&buy_hash);
    if launch_receipt.is_none() || buy_receipt.is_none() {
        let unresolved = json!({
            "schema":"darkbot.g08.mainnet-acquisition-ordering.v2",
            "gate":"G0.8",
            "verdict":"STOP",
            "reason":"SENT_TRANSACTION_UNRESOLVED",
            "attempt_id":req_str(&g07, "/attempt_id"),
            "launch_tx":launch_hash,
            "buy_tx":buy_hash,
            "launch_receipt_resolved":launch_receipt.is_some(),
            "buy_receipt_resolved":buy_receipt.is_some(),
            "automatic_retry":false,
            "T0_TO_BUY":"UNKNOWN"
        });
        println!("{}", serde_json::to_string_pretty(&unresolved).unwrap());
        std::process::exit(2);
    }
    let launch_receipt = launch_receipt.unwrap();
    let buy_receipt = buy_receipt.unwrap();

    let launch_status = hex_u64(&launch_receipt["status"]);
    let buy_status = hex_u64(&buy_receipt["status"]);
    let launch_block = hex_u64(&launch_receipt["blockNumber"]);
    let buy_block = hex_u64(&buy_receipt["blockNumber"]);
    let launch_index = hex_u64(&launch_receipt["transactionIndex"]);
    let buy_index = hex_u64(&buy_receipt["transactionIndex"]);
    let launch_block_hash = req_str(&launch_receipt, "/blockHash").to_string();
    let buy_block_hash = req_str(&buy_receipt, "/blockHash").to_string();

    let launch_tx = rpc("eth_getTransactionByHash", json!([launch_hash.clone()]))
        .expect("launch transaction lookup failed");
    let buy_tx = rpc("eth_getTransactionByHash", json!([buy_hash.clone()]))
        .expect("BUY transaction lookup failed");
    assert!(!launch_tx.is_null() && !buy_tx.is_null());

    let launch_scope_ok = norm(launch_tx["from"].as_str().unwrap_or(""))
        == norm(&launcher.to_string())
        && norm(launch_tx["to"].as_str().unwrap_or("")) == norm(PORTAL)
        && hex_u256(&launch_tx["value"]) == U256::ZERO;
    let buy_scope_ok = norm(buy_tx["from"].as_str().unwrap_or("")) == norm(&buyer.to_string())
        && norm(buy_tx["to"].as_str().unwrap_or("")) == norm(PORTAL)
        && hex_u256(&buy_tx["value"]) == expected_buy_value;

    let buy_input = hex::decode(
        buy_tx["input"]
            .as_str()
            .expect("BUY input missing")
            .trim_start_matches("0x"),
    )
    .expect("BUY input invalid hex");
    let buy_call = swapExactInputCall::abi_decode_validate(&buy_input)
        .expect("BUY calldata is not the expected swapExactInput call");
    let buy_calldata_ok = buy_call.params.inputToken == Address::ZERO
        && buy_call.params.outputToken == token
        && buy_call.params.inputAmount == expected_buy_value
        && buy_call.params.minOutputAmount == expected_min_output;

    let same_block = launch_block == buy_block;
    let ordered = buy_block > launch_block || (same_block && buy_index > launch_index);
    let adjacent = same_block && buy_index == launch_index.saturating_add(1);

    let launch_canonical_block = rpc(
        "eth_getBlockByNumber",
        json!([format!("0x{launch_block:x}"), true]),
    )
    .expect("canonical launch block lookup failed");
    let buy_canonical_block = if same_block {
        launch_canonical_block.clone()
    } else {
        rpc(
            "eth_getBlockByNumber",
            json!([format!("0x{buy_block:x}"), true]),
        )
        .expect("canonical BUY block lookup failed")
    };
    let launch_canonical_hash = launch_canonical_block["hash"]
        .as_str()
        .expect("canonical launch block hash missing")
        .to_string();
    let buy_canonical_hash = buy_canonical_block["hash"]
        .as_str()
        .expect("canonical BUY block hash missing")
        .to_string();
    let canonical = norm(&launch_canonical_hash) == norm(&launch_block_hash)
        && norm(&buy_canonical_hash) == norm(&buy_block_hash);
    let launch_at_index = transaction_at_index(&launch_canonical_block, launch_index)
        .and_then(|tx| tx["hash"].as_str())
        .is_some_and(|h| norm(h) == norm(&launch_hash));
    let buy_at_index = transaction_at_index(&buy_canonical_block, buy_index)
        .and_then(|tx| tx["hash"].as_str())
        .is_some_and(|h| norm(h) == norm(&buy_hash));

    let prior_block = launch_block.saturating_sub(1);
    let token_code_before = rpc(
        "eth_getCode",
        json!([token.to_string(), format!("0x{prior_block:x}")]),
    )
    .ok();
    let token_absent_before = token_code_before
        .as_ref()
        .and_then(Value::as_str)
        .map(|code| code == "0x");
    let token_code_after = rpc(
        "eth_getCode",
        json!([token.to_string(), format!("0x{buy_block:x}")]),
    )
    .ok();
    let token_exists_after = token_code_after
        .as_ref()
        .and_then(Value::as_str)
        .map(|code| code != "0x");
    let buyer_balance_after = match token_exists_after {
        Some(true) => balance_of_at(token, buyer, buy_block).ok(),
        Some(false) => Some(U256::ZERO),
        None => None,
    };
    let buy_receipt_token_delta = buy_receipt_transfer_delta(&buy_receipt, token, buyer);
    let acquisition_evidence_complete = token_absent_before.is_some()
        && token_exists_after.is_some()
        && buyer_balance_after.is_some()
        && buy_receipt_token_delta.is_some();
    let acquisition_positive = token_absent_before == Some(true)
        && token_exists_after == Some(true)
        && buy_receipt_token_delta.as_ref().is_some_and(|delta| *delta >= expected_min_output && *delta > U256::ZERO)
        && buyer_balance_after.as_ref().zip(buy_receipt_token_delta.as_ref())
            .is_some_and(|(balance, delta)| balance >= delta);

    let competitor_hashes = successful_competing_buys_between(
        launch_block,
        launch_index,
        buy_block,
        buy_index,
        token,
    );
    let competitor_count = competitor_hashes.as_ref().map(Vec::len);

    let launch_gas_cost = receipt_gas_cost(&launch_receipt, &launch_tx);
    let buy_gas_cost = receipt_gas_cost(&buy_receipt, &buy_tx);
    let buyer_total_debit = expected_buy_value + buy_gas_cost;
    let experiment_total_cost = launch_gas_cost + buyer_total_debit;

    let chain_capture = capture_chain_window(
        &chain_capture_path,
        launch_block,
        req_str(&g07, "/attempt_id"),
        &launch_hash,
        &buy_hash,
    );
    let chain_capture_complete = chain_capture.as_ref().is_ok_and(|capture| {
        capture.leading_boundary_included && capture.boundary_block_included
    });
    let chain_capture_json = match &chain_capture {
        Ok(c) => json!({
            "path":c.path,
            "records":c.records,
            "bytes":c.bytes,
            "sha256":c.sha256,
            "first_block":c.first_block,
            "last_block":c.last_block,
            "leading_boundary_included":c.leading_boundary_included,
            "launch_timestamp":c.launch_timestamp,
            "horizon_timestamp":c.horizon_timestamp,
            "boundary_block_included":c.boundary_block_included,
            "window_seconds":MARKET_WINDOW_SECS,
            "complete":c.leading_boundary_included && c.boundary_block_included
        }),
        Err(e) => json!({"complete":false,"error":e}),
    };

    // A canonical receipt is not finality. Require the independent finalized
    // BSC block height to cover BOTH economic transactions before PASS.
    let finalized_block = rpc("eth_getBlockByNumber", json!(["finalized", false])).ok();
    let finalized_height = finalized_block.as_ref()
        .and_then(|v| v.get("number"))
        .and_then(Value::as_str)
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok());
    let execution_finalized = finalized_height.is_some_and(|height| height >= launch_block && height >= buy_block);

    let hard_stop = launch_status != 1
        || buy_status != 1
        || !launch_scope_ok
        || !buy_scope_ok
        || !buy_calldata_ok
        || !canonical
        || !launch_at_index
        || !buy_at_index
        || (acquisition_evidence_complete && !acquisition_positive)
        || !ordered;
    let adapt = !execution_finalized
        || !acquisition_evidence_complete
        || !same_block
        || !adjacent
        || competitor_count != Some(0)
        || !local_capture_complete
        || !chain_capture_complete;
    let verdict = if hard_stop {
        "STOP"
    } else if adapt {
        "ADAPT"
    } else {
        "PASS"
    };

    let head = hex_u64(&rpc("eth_blockNumber", json!([])).expect("head lookup failed"));
    let result = json!({
        "schema":"darkbot.g08.mainnet-acquisition-ordering.v2",
        "gate":"G0.8",
        "attempt_id":req_str(&g07, "/attempt_id"),
        "verdict":verdict,
        "transactions":{
            "launch_tx":launch_hash,
            "buy_tx":buy_hash,
            "launch_status":launch_status,
            "buy_status":buy_status,
            "launch_scope_ok":launch_scope_ok,
            "buy_scope_ok":buy_scope_ok,
            "buy_calldata_ok":buy_calldata_ok
        },
        "acquisition":{
            "token":token.to_string(),
            "buyer":buyer.to_string(),
            "evidence_complete":acquisition_evidence_complete,
            "token_absent_before_launch_block":token_absent_before,
            "token_exists_at_buy_block":token_exists_after,
            "buyer_token_balance_at_buy_block":buyer_balance_after.as_ref().map(ToString::to_string),
            "buy_receipt_token_delta_raw":buy_receipt_token_delta.as_ref().map(ToString::to_string),
            "receipt_transfer_attribution_required":true,
            "positive_attributable_acquisition":acquisition_positive
        },
        "ordering":{
            "block_hash": if same_block { launch_canonical_hash.clone() } else { buy_canonical_hash.clone() },
            "launch_block_hash":launch_canonical_hash,
            "buy_block_hash":buy_canonical_hash,
            "launch_block":launch_block,
            "launch_index":launch_index,
            "buy_block":buy_block,
            "buy_index":buy_index,
            "ordered":ordered,
            "same_block":same_block,
            "adjacent":adjacent,
            "canonical":canonical,
            "launch_hash_at_index":launch_at_index,
            "buy_hash_at_index":buy_at_index,
            "successful_external_buys_between_count":competitor_count,
            "successful_external_buys_between_hashes":competitor_hashes
        },
        "costs":{
            "launch_gas_cost_wei":launch_gas_cost.to_string(),
            "buy_gas_cost_wei":buy_gas_cost.to_string(),
            "buy_value_wei":expected_buy_value.to_string(),
            "buyer_total_debit_wei":buyer_total_debit.to_string(),
            "experiment_total_cost_wei":experiment_total_cost.to_string(),
            "direct_builder_tip_wei":"0"
        },
        "capture":{
            "g07_local_journal_complete":local_capture_complete,
            "chain_journal":chain_capture_json,
            "chain_window_complete":chain_capture_complete,
            "market_window_seconds":MARKET_WINDOW_SECS,
            "chain_record_timestamp_source":"BSC_BLOCK_HEADER_SECONDS",
            "chain_record_timestamp_resolution_ns":"1000000000",
            "chain_record_timestamp_is_producer_execution_time":false,
            "competition_scan":"Direct Portal swapExactInput is parsed online; full block/receipt/log material is retained for complete offline derivation. N/N+1 itself proves zero intervening transactions for PASS."
        },
        "confirmation":{
            "current_head":head,
            "confirmations_after_launch_block":head.saturating_sub(launch_block),
            "finalized_height":finalized_height,
            "launch_and_buy_finalized":execution_finalized
        },
        "timing":{
            "T0_TO_BUY":"UNKNOWN_PENDING_ORIGINAL_PRODUCER_WITNESS",
            "standard_chain_data_is_not_execution_timestamp":true,
            "next":"run g08_producer_timing with the original producing execution witness"
        }
    });

    let pretty = serde_json::to_string_pretty(&result).unwrap();
    println!("{pretty}");
    if let Ok(path) = env::var("DARKBOT_G08_EVIDENCE_PATH") {
        if let Err(e) = fs::write(&path, format!("{pretty}\n")) {
            eprintln!("WARNING: G0.8 evidence write failed: {path}: {e}");
        }
    }

    match verdict {
        "PASS" => println!("G0_8_MAINNET_ACQUISITION_ORDERING_PASS"),
        "ADAPT" => std::process::exit(3),
        _ => std::process::exit(2),
    }
}

#[cfg(test)]
mod g08_attribution_tests {
    use super::*;
    fn topic_address(addr: Address) -> String {
        format!("0x{}{}", "00".repeat(12), hex::encode(addr.as_slice()))
    }
    #[test]
    fn buy_transfer_delta_is_bound_to_token_buyer_and_receipt() {
        let token = Address::from([0x11; 20]);
        let buyer = Address::from([0x22; 20]);
        let other = Address::from([0x33; 20]);
        let transfer = keccak256(b"Transfer(address,address,uint256)").to_string();
        let log = json!({
            "address": token.to_string(),
            "topics": [transfer, topic_address(other), topic_address(buyer)],
            "data": format!("0x{:064x}", 42u64)
        });
        let receipt = json!({"logs":[log]});
        assert_eq!(buy_receipt_transfer_delta(&receipt, token, buyer), Some(U256::from(42u64)));
        assert_eq!(buy_receipt_transfer_delta(&receipt, token, other), None);
    }
    #[test]
    fn missing_or_malformed_buy_receipt_does_not_infer_acquisition() {
        let token = Address::from([0x11; 20]);
        let buyer = Address::from([0x22; 20]);
        assert_eq!(buy_receipt_transfer_delta(&json!({"logs":[]}), token, buyer), None);
        assert_eq!(buy_receipt_transfer_delta(&json!({"logs":[{"address":token.to_string(), "topics":["0x0"], "data":"0x"}]}), token, buyer), None);
    }
}
