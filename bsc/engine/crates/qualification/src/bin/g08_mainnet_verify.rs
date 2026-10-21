//! G0.8 read-only BSC Mainnet acquisition, ordering, economics and canonical
//! 15-second evidence reconciliation for a preceding G0.7 attempt.
//!
//! This binary never submits a transaction. Builder acknowledgements are not
//! acquisition authority; canonical chain execution is.
//!
//! It reads the binary G0.7 evidence record and writes a binary result
//! record (DBREC001) and a binary chain capture journal. JSON exists only on
//! the JSON-RPC wire, inside [`wire`].

use alloy_primitives::{Address, B256, U256, address, keccak256};
use alloy_sol_types::{SolCall, sol};
use darkbot_bsc_journal::{CaptureRecord, JournalWriter, RecordKind};
use darkbot_bsc_record::schema::g07::evidence;
use darkbot_bsc_record::schema::g08::{
    chain_block, chain_receipt, chain_session, chain_transaction, result, unresolved,
};
use darkbot_bsc_record::{Draft, RecordError, Value, decode};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    thread,
    time::{Duration, Instant},
};
use wire::{Block, Receipt, Tx};

const CHAIN_ID: u64 = 56;
const PORTAL: Address = address!("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0");
const MARKET_WINDOW_SECS: u64 = 15;
const RECEIPT_TIMEOUT_POLLS: usize = 240;
const RECEIPT_POLL: Duration = Duration::from_millis(500);
const BLOCK_TIMESTAMP_SOURCE: &str = "BSC_BLOCK_HEADER_SECONDS";
const BLOCK_TIMESTAMP_RESOLUTION_NS: u64 = 1_000_000_000;

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
    sha256: [u8; 32],
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

/// A field of the decoded G0.7 evidence; decoding already checked that
/// every required field is present with its kind.
fn g07_field<T>(value: Result<T, RecordError>) -> T {
    value.unwrap_or_else(|e| panic!("G0.7 evidence: {e}"))
}

fn addr(value: Address) -> Value {
    Value::Address(value.into_array())
}

fn word(value: B256) -> Value {
    Value::B256(value.0)
}

fn wei(value: U256) -> Value {
    Value::U256(value.to_be_bytes())
}

/// Writes `bytes` to a staging file beside `path`, syncs it and renames it
/// over `path`: a reader sees the previous file or the complete new one.
fn write_atomic(path: &str, bytes: &[u8]) -> Result<(), String> {
    let target = Path::new(path);
    let name = target
        .file_name()
        .ok_or_else(|| format!("{path} names no file"))?;
    let tmp = target.with_file_name(format!(
        ".{}.tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let staged = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .and_then(|()| fs::rename(&tmp, target));
    if let Err(e) = staged {
        let _ = fs::remove_file(&tmp);
        return Err(format!("{path}: {e}"));
    }
    let dir = match target.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("{path} replaced but not yet durable: {e}"))
}

fn persist_result(path: &str, draft: &Draft) -> Result<darkbot_bsc_record::Record, String> {
    let bytes = draft.encode().map_err(|e| e.to_string())?;
    let record = decode(draft.schema(), &bytes).map_err(|e| e.to_string())?;
    write_atomic(path, &bytes)?;
    Ok(record)
}

fn unresolved_draft(
    attempt: &str,
    launch: B256,
    buy: B256,
    lr: bool,
    br: bool,
    reason: &str,
) -> Draft {
    let mut d = Draft::new(&unresolved::SCHEMA);
    d.set(unresolved::GATE, "G0.8")
        .set(unresolved::VERDICT, "UNRESOLVED")
        .set(unresolved::REASON, bounded_error(reason))
        .set(unresolved::ATTEMPT_ID, attempt)
        .set(unresolved::LAUNCH_TX, word(launch))
        .set(unresolved::BUY_TX, word(buy))
        .set(unresolved::LAUNCH_RECEIPT_RESOLVED, lr)
        .set(unresolved::BUY_RECEIPT_RESOLVED, br)
        .set(unresolved::AUTOMATIC_RETRY, false)
        .set(unresolved::T0_TO_BUY, "UNKNOWN");
    d
}

// ---- JSON-RPC wire boundary --------------------------------------------
//
// The only place JSON exists in this binary. Request bodies are fixed
// templates filled with hex quantities and data; every response is decoded
// into the typed values below before it leaves this module, and no JSON
// type crosses its border (tests/g08_binary_records.rs checks the source).
mod wire {
    use alloy_primitives::{Address, B256, U256};
    use serde_json::Value as Json;
    use std::{env, process::Command};

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Tx {
        pub hash: B256,
        pub from: Address,
        pub to: Option<Address>,
        pub input: Vec<u8>,
        pub value: U256,
        pub gas: u64,
        pub gas_price: Option<U256>,
        pub nonce: u64,
        pub transaction_index: Option<u64>,
        pub tx_type: Option<u64>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Block {
        pub number: u64,
        pub hash: B256,
        pub parent_hash: B256,
        pub timestamp: u64,
        pub milli_timestamp: Option<u64>,
        pub mix_hash: Option<B256>,
        pub miner: Address,
        pub gas_limit: u64,
        pub gas_used: u64,
        pub base_fee_per_gas: Option<U256>,
        pub transactions: Vec<Tx>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Log {
        pub log_index: u64,
        pub address: Address,
        pub topics: Vec<B256>,
        pub data: Vec<u8>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Receipt {
        pub block_number: u64,
        pub block_hash: B256,
        pub transaction_index: u64,
        pub transaction_hash: B256,
        pub from: Address,
        pub to: Option<Address>,
        pub contract_address: Option<Address>,
        pub status: u64,
        pub gas_used: u64,
        pub cumulative_gas_used: u64,
        pub effective_gas_price: Option<U256>,
        pub logs: Vec<Log>,
    }

    fn quantity(n: u64) -> String {
        format!("\"0x{n:x}\"")
    }

    fn data(bytes: &[u8]) -> String {
        format!("\"0x{}\"", hex::encode(bytes))
    }

    fn body(method: &str, params: &str) -> String {
        format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"{method}\",\"params\":{params}}}")
    }

    /// One JSON-RPC call over curl. The child sees only PATH; it never hangs
    /// (connect 2 s, whole call 5 s) and reads no .curlrc (-q first).
    fn call(method: &str, params: &str) -> Result<Json, String> {
        let url = env::var("BSC_MAINNET_RPC_HTTPS").map_err(|_| "BSC_MAINNET_RPC_HTTPS is required".to_string())?;
        let body = body(method, params);
        let mut curl = Command::new("curl");
        curl.env_clear();
        if let Some(path) = env::var_os("PATH") {
            curl.env("PATH", path);
        }
        let out = curl
            .args([
                "-q",
                "--connect-timeout",
                "2",
                "--max-time",
                "5",
                "-sS",
                &url,
                "-H",
                "Content-Type: application/json",
                "--data",
                &body,
            ])
            .output()
            .map_err(|e| format!("execute curl: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "curl {method}: transport failure (exit {:?})",
                out.status.code()
            ));
        }
        response(method, &out.stdout)
    }

    fn response(method: &str, bytes: &[u8]) -> Result<Json, String> {
        let v: Json = serde_json::from_slice(bytes)
            .map_err(|e| format!("invalid JSON-RPC response for {method}: {e}"))?;
        if let Some(err) = v.get("error") {
            let code = err.get("code").and_then(Json::as_i64).unwrap_or(0);
            return Err(format!("JSON-RPC {method}: error {code} (provider text redacted)"));
        }
        v.get("result")
            .cloned()
            .ok_or_else(|| format!("JSON-RPC {method}: missing result"))
    }

    fn present<'a>(v: &'a Json, name: &str) -> Option<&'a Json> {
        v.get(name).filter(|f| !f.is_null())
    }

    fn field<'a>(v: &'a Json, name: &str) -> Result<&'a Json, String> {
        present(v, name).ok_or_else(|| format!("{name} missing"))
    }

    fn digits<'a>(v: &'a Json, what: &str) -> Result<&'a str, String> {
        v.as_str()
            .and_then(|s| s.strip_prefix("0x"))
            .ok_or_else(|| format!("{what} is not a 0x-prefixed hex string"))
    }

    fn u64_of(v: &Json, what: &str) -> Result<u64, String> {
        u64::from_str_radix(digits(v, what)?, 16)
            .map_err(|_| format!("{what} is not a u64 quantity"))
    }

    fn u256_of(v: &Json, what: &str) -> Result<U256, String> {
        let digits = digits(v, what)?;
        if digits.is_empty() {
            return Err(format!("{what} is an empty quantity"));
        }
        U256::from_str_radix(digits, 16).map_err(|_| format!("{what} is not a U256 quantity"))
    }

    fn bytes_of(v: &Json, what: &str) -> Result<Vec<u8>, String> {
        hex::decode(digits(v, what)?).map_err(|_| format!("{what} is not hex data"))
    }

    fn b256_of(v: &Json, what: &str) -> Result<B256, String> {
        B256::try_from(bytes_of(v, what)?.as_slice()).map_err(|_| format!("{what} is not 32 bytes"))
    }

    fn address_of(v: &Json, what: &str) -> Result<Address, String> {
        Address::try_from(bytes_of(v, what)?.as_slice())
            .map_err(|_| format!("{what} is not 20 bytes"))
    }

    fn opt<T>(
        v: &Json,
        name: &str,
        decode: fn(&Json, &str) -> Result<T, String>,
    ) -> Result<Option<T>, String> {
        present(v, name).map(|f| decode(f, name)).transpose()
    }

    fn req<T>(
        v: &Json,
        name: &str,
        decode: fn(&Json, &str) -> Result<T, String>,
    ) -> Result<T, String> {
        decode(field(v, name)?, name)
    }

    fn tx_of(v: &Json) -> Result<Tx, String> {
        Ok(Tx {
            hash: req(v, "hash", b256_of)?,
            from: req(v, "from", address_of)?,
            to: opt(v, "to", address_of)?,
            input: req(v, "input", bytes_of)?,
            value: req(v, "value", u256_of)?,
            gas: req(v, "gas", u64_of)?,
            gas_price: opt(v, "gasPrice", u256_of)?,
            nonce: req(v, "nonce", u64_of)?,
            transaction_index: opt(v, "transactionIndex", u64_of)?,
            tx_type: opt(v, "type", u64_of)?,
        })
    }

    fn block_of(v: &Json) -> Result<Block, String> {
        let transactions = field(v, "transactions")?
            .as_array()
            .ok_or("full block transactions missing")?
            .iter()
            .map(tx_of)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Block {
            number: req(v, "number", u64_of)?,
            hash: req(v, "hash", b256_of)?,
            parent_hash: req(v, "parentHash", b256_of)?,
            timestamp: req(v, "timestamp", u64_of)?,
            milli_timestamp: opt(v, "milliTimestamp", u64_of)?,
            mix_hash: opt(v, "mixHash", b256_of)?,
            miner: req(v, "miner", address_of)?,
            gas_limit: req(v, "gasLimit", u64_of)?,
            gas_used: req(v, "gasUsed", u64_of)?,
            base_fee_per_gas: opt(v, "baseFeePerGas", u256_of)?,
            transactions,
        })
    }

    fn log_of(v: &Json) -> Result<Log, String> {
        let topics = field(v, "topics")?
            .as_array()
            .ok_or("log topics are not an array")?
            .iter()
            .map(|t| b256_of(t, "topic"))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Log {
            log_index: req(v, "logIndex", u64_of)?,
            address: req(v, "address", address_of)?,
            topics,
            data: req(v, "data", bytes_of)?,
        })
    }

    fn receipt_of(v: &Json) -> Result<Receipt, String> {
        let mut indices = std::collections::BTreeSet::new();
        for log in field(v, "logs")?
            .as_array()
            .ok_or("logs are not an array")?
        {
            for key in [
                "blockHash",
                "blockNumber",
                "transactionHash",
                "transactionIndex",
            ] {
                if field(log, key)? != field(v, key)? {
                    return Err(format!("log {key} mismatch"));
                }
            }
            if log.get("removed").and_then(Json::as_bool) != Some(false) {
                return Err("removed or unqualified log".into());
            }
            if !indices.insert(req(log, "logIndex", u64_of)?) {
                return Err("duplicate log index".into());
            }
        }
        let logs = field(v, "logs")?
            .as_array()
            .ok_or("receipt logs are not an array")?
            .iter()
            .map(log_of)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Receipt {
            block_number: req(v, "blockNumber", u64_of)?,
            block_hash: req(v, "blockHash", b256_of)?,
            transaction_index: req(v, "transactionIndex", u64_of)?,
            transaction_hash: req(v, "transactionHash", b256_of)?,
            from: req(v, "from", address_of)?,
            to: opt(v, "to", address_of)?,
            contract_address: opt(v, "contractAddress", address_of)?,
            status: req(v, "status", u64_of)?,
            gas_used: req(v, "gasUsed", u64_of)?,
            cumulative_gas_used: req(v, "cumulativeGasUsed", u64_of)?,
            effective_gas_price: opt(v, "effectiveGasPrice", u256_of)?,
            logs,
        })
    }

    /// `None` for a JSON null result (unknown hash or block).
    fn nullable<T>(v: &Json, decode: fn(&Json) -> Result<T, String>) -> Result<Option<T>, String> {
        if v.is_null() {
            Ok(None)
        } else {
            decode(v).map(Some)
        }
    }

    pub fn chain_id() -> Result<u64, String> {
        u64_of(&call("eth_chainId", "[]")?, "eth_chainId")
    }

    pub fn block_number() -> Result<u64, String> {
        u64_of(&call("eth_blockNumber", "[]")?, "eth_blockNumber")
    }

    pub fn receipt(hash: B256) -> Result<Option<Receipt>, String> {
        let v = call(
            "eth_getTransactionReceipt",
            &format!("[{}]", data(hash.as_slice())),
        )?;
        nullable(&v, receipt_of).map_err(|e| format!("receipt {hash}: {e}"))
    }

    pub fn transaction(hash: B256) -> Result<Option<Tx>, String> {
        let v = call(
            "eth_getTransactionByHash",
            &format!("[{}]", data(hash.as_slice())),
        )?;
        nullable(&v, tx_of).map_err(|e| format!("transaction {hash}: {e}"))
    }

    /// The block with its full transactions.
    pub fn block(number: u64) -> Result<Option<Block>, String> {
        let v = call(
            "eth_getBlockByNumber",
            &format!("[{},true]", quantity(number)),
        )?;
        nullable(&v, block_of).map_err(|e| format!("block {number}: {e}"))
    }

    /// `None` when the node answers with anything but a receipt list.
    pub fn block_receipts(number: u64) -> Result<Option<Vec<Receipt>>, String> {
        let v = call("eth_getBlockReceipts", &format!("[{}]", quantity(number)))?;
        let Some(list) = v.as_array() else {
            return Ok(None);
        };
        list.iter()
            .map(receipt_of)
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
            .map_err(|e| format!("receipts of block {number}: {e}"))
    }

    pub fn code(address: Address, block: u64) -> Result<Vec<u8>, String> {
        let params = format!("[{},{}]", data(address.as_slice()), quantity(block));
        bytes_of(&call("eth_getCode", &params)?, "eth_getCode")
    }

    pub fn eth_call(to: Address, input: &[u8], block: u64) -> Result<Vec<u8>, String> {
        let params = format!(
            "[{{\"to\":{},\"data\":{}}},{}]",
            data(to.as_slice()),
            data(input),
            quantity(block)
        );
        bytes_of(&call("eth_call", &params)?, "eth_call")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn request_bodies_are_fixed_templates() {
            let params = format!("[{},true]", quantity(0x2a));
            let body = body("eth_getBlockByNumber", &params);
            assert_eq!(
                body,
                r#"{"jsonrpc":"2.0","id":1,"method":"eth_getBlockByNumber","params":["0x2a",true]}"#
            );
            // A template body is valid JSON-RPC.
            let parsed: Json = serde_json::from_str(&body).unwrap();
            assert_eq!(parsed["params"][0], "0x2a");
        }

        #[test]
        fn responses_decode_into_typed_values_or_fail() {
            let ok = response(
                "eth_chainId",
                br#"{"jsonrpc":"2.0","id":1,"result":"0x38"}"#,
            );
            assert_eq!(u64_of(&ok.unwrap(), "chain"), Ok(56));
            let err = response(
                "eth_call",
                br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"execution reverted"}}"#,
            );
            assert_eq!(
                err.unwrap_err(),
                "JSON-RPC eth_call: error -32000 (provider text redacted)"
            );
            assert!(response("eth_chainId", b"not json").is_err());
            assert!(u64_of(&Json::from("38"), "q").is_err());
            assert!(u256_of(&Json::from("0x"), "q").is_err());
            assert!(b256_of(&Json::from("0x01"), "h").is_err());
            assert!(address_of(&Json::from(format!("0x{}", "11".repeat(32))), "a").is_err());
        }

        #[test]
        fn receipts_and_blocks_decode_with_optional_fields() {
            let h = |b: &str| format!("0x{}", b.repeat(32));
            let a = |b: &str| format!("0x{}", b.repeat(20));
            let mut receipt: Json = serde_json::from_str(&format!(
                r#"{{"blockNumber":"0x10","blockHash":"{}","transactionIndex":"0x1",
                "transactionHash":"{}","from":"{}","to":null,"contractAddress":"{}",
                "status":"0x1","gasUsed":"0x5208","cumulativeGasUsed":"0x5208",
                "effectiveGasPrice":null,"logs":[{{"logIndex":"0x0","address":"{}",
                "topics":["{}"],"data":"0x"}}]}}"#,
                h("aa"),
                h("bb"),
                a("11"),
                a("22"),
                a("33"),
                h("cc")
            ))
            .unwrap();
            for key in [
                "blockNumber",
                "blockHash",
                "transactionIndex",
                "transactionHash",
            ] {
                receipt["logs"][0][key] = receipt[key].clone();
            }
            receipt["logs"][0]["removed"] = Json::Bool(false);
            let r = receipt_of(&receipt).unwrap();
            let mut wrong = receipt.clone();
            wrong["logs"][0]["removed"] = Json::Bool(true);
            assert!(receipt_of(&wrong).is_err());
            wrong = receipt.clone();
            wrong["logs"][0]["blockHash"] = Json::String(h("dd"));
            assert!(receipt_of(&wrong).is_err());
            assert_eq!((r.block_number, r.transaction_index, r.status), (16, 1, 1));
            assert_eq!(r.to, None);
            assert_eq!(r.effective_gas_price, None);
            assert_eq!(r.logs[0].topics, vec![B256::repeat_byte(0xcc)]);
            let block: Json = serde_json::from_str(&format!(
                r#"{{"number":"0x10","hash":"{}","parentHash":"{}","timestamp":"0x64",
                "miner":"{}","gasLimit":"0x1","gasUsed":"0x0","transactions":[{{
                "hash":"{}","from":"{}","to":"{}","input":"0xabcd","value":"0x0",
                "gas":"0x5208","gasPrice":"0x3","nonce":"0x7","transactionIndex":"0x0"}}]}}"#,
                h("aa"),
                h("99"),
                a("11"),
                h("bb"),
                a("22"),
                a("33")
            ))
            .unwrap();
            let b = block_of(&block).unwrap();
            assert_eq!((b.number, b.timestamp, b.mix_hash), (16, 100, None));
            assert_eq!(b.transactions[0].input, vec![0xab, 0xcd]);
            assert_eq!(b.transactions[0].gas_price, Some(U256::from(3u64)));
            // A hashes-only block is not a full block.
            let hashes_only: Json = serde_json::from_str(&format!(
                r#"{{"number":"0x10","hash":"{}","parentHash":"{}","timestamp":"0x64",
                "miner":"{}","gasLimit":"0x1","gasUsed":"0x0","transactions":["{}"]}}"#,
                h("aa"),
                h("99"),
                a("11"),
                h("bb")
            ))
            .unwrap();
            assert!(block_of(&hashes_only).is_err());
        }
    }
}
// ---- end of the JSON-RPC wire boundary ---------------------------------

fn bounded_error(text: &str) -> &str {
    let mut end = text.len().min(4000);
    while !text.is_char_boundary(end) { end -= 1; }
    &text[..end]
}

fn permanent_read_error(error: &str) -> bool {
    ["invalid", "decode", "missing result", "removed", "wrong chain", "required", "error -32600", "error -32601", "error -32602"].iter().any(|needle| error.contains(needle))
}

/// At most three attempts; each wire call has a 5s curl limit, plus 2*100ms.
fn recover_read<T>(mut read: impl FnMut() -> Result<T, String>, mut pause: impl FnMut()) -> Result<T, String> {
    for attempt in 1..=3 {
        match read() {
            Ok(value) => return Ok(value),
            Err(error) if attempt == 3 || permanent_read_error(&error) => return Err(format!("read_errors={attempt}; last={}", bounded_error(&error))),
            Err(_) => pause(),
        }
    }
    unreachable!()
}
fn read_rpc<T>(read: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    recover_read(read, || thread::sleep(Duration::from_millis(100)))
}

/// Includes call time in the 120s bound (reserve 5s for the last call).
/// A missing receipt is distinct from unreadable evidence; neither permits retry/send.
fn poll_receipt<T>(mut read: impl FnMut() -> Result<Option<T>, String>, mut pause: impl FnMut(), deadline: Instant, polls: usize) -> Result<Option<T>, String> {
    let mut errors = 0;
    let mut last = String::new();
    for _ in 0..polls {
        if Instant::now() >= deadline { break; }
        match read() {
            Ok(Some(receipt)) => return Ok(Some(receipt)),
            Ok(None) => {},
            Err(error) => {
                errors += 1;
                let permanent = permanent_read_error(&error);
                last = bounded_error(&error).to_owned();
                if permanent || errors >= 3 { return Err(format!("receipt_errors={errors}; last={last}")); }
            }
        }
        pause();
    }
    if errors == 0 { Ok(None) } else { Err(format!("receipt_errors={errors}; unresolved after polling; last={last}")) }
}
fn wait_receipt(hash: B256) -> Result<Option<Receipt>, String> {
    poll_receipt(|| wire::receipt(hash), || thread::sleep(RECEIPT_POLL), Instant::now() + Duration::from_secs(114), RECEIPT_TIMEOUT_POLLS)
}

fn balance_of_at(token: Address, owner: Address, block: u64) -> Result<U256, String> {
    let data = balanceOfCall { owner }.abi_encode();
    let value = wire::eth_call(token, &data, block)?;
    U256::try_from_be_slice(&value).ok_or_else(|| "balanceOf result exceeds 32 bytes".to_string())
}

fn transaction_at_index(block: &Block, index: u64) -> Option<&Tx> {
    block.transactions.get(index as usize)
}

fn receipt_gas_cost(receipt: &Receipt, tx: &Tx) -> Result<U256, String> {
    let gas_price = receipt
        .effective_gas_price
        .or(tx.gas_price)
        .ok_or("neither effectiveGasPrice nor gasPrice reported")?;
    U256::from(receipt.gas_used).checked_mul(gas_price).ok_or_else(|| "receipt gas cost overflow".into())
}

#[derive(Debug)]
struct BuyEvent {
    token: Address,
    buyer: Address,
    amount: U256,
    value: U256,
    fee: U256,
}

// Frozen official IPortal ABI: seven non-indexed words. Applies to direct
// and router-internal Portal calls alike; top-level tx.to is irrelevant.
fn portal_buys(receipt: &Receipt) -> Result<Vec<BuyEvent>, String> {
    let topic = keccak256("TokenBought(uint256,address,address,uint256,uint256,uint256,uint256)");
    let mut events = Vec::new();
    for log in &receipt.logs {
        if log.address != PORTAL || log.topics.first() != Some(&topic) {
            continue;
        }
        if log.topics.len() != 1 || log.data.len() != 224 {
            return Err("malformed TokenBought".into());
        }
        if log.data[32..44]
            .iter()
            .chain(&log.data[64..76])
            .any(|b| *b != 0)
        {
            return Err("noncanonical TokenBought address".into());
        }
        let amount = |n: usize| U256::from_be_slice(&log.data[n * 32..(n + 1) * 32]);
        events.push(BuyEvent {
            token: Address::from_slice(&log.data[44..64]),
            buyer: Address::from_slice(&log.data[76..96]),
            amount: amount(3),
            value: amount(4),
            fee: amount(5),
        });
    }
    Ok(events)
}

// Receipt identity/status are independently checkable. Event/Transfer economics
// remain unqualified for v5.25 tax tokens until an original canonical receipt
// proves their relation. A mismatch is incomplete evidence, not proof of failure.
#[derive(Debug, PartialEq, Eq)]
enum AcquisitionError {
    Identity,
    Unverified(String),
}
impl From<String> for AcquisitionError {
    fn from(reason: String) -> Self { Self::Unverified(reason) }
}
impl From<&str> for AcquisitionError {
    fn from(reason: &str) -> Self { Self::Unverified(reason.into()) }
}
impl std::fmt::Display for AcquisitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Identity => f.write_str("BUY receipt execution/sender/recipient mismatch"),
            Self::Unverified(reason) => write!(f, "UNVERIFIED_TAX_ATTRIBUTION: {reason}"),
        }
    }
}

fn attributable_acquisition(
    receipt: &Receipt,
    token: Address,
    buyer: Address,
    value: U256,
    minimum: U256,
) -> Result<(U256, U256), AcquisitionError> {
    if receipt.status != 1 || receipt.from != buyer || receipt.to != Some(PORTAL) {
        return Err(AcquisitionError::Identity);
    }
    let buys = portal_buys(receipt)?;
    let matching: Vec<_> = buys
        .iter()
        .filter(|b| b.token == token && b.buyer == buyer)
        .collect();
    if matching.len() != 1 {
        return Err("BUY needs exactly one attributable TokenBought".into());
    }
    let event = matching[0];
    if event.value != value || event.fee > value {
        return Err("BUY event economics mismatch".into());
    }
    let topic = keccak256("Transfer(address,address,uint256)");
    let mut incoming = U256::ZERO;
    let mut outgoing = U256::ZERO;
    for log in &receipt.logs {
        if log.address != token || log.topics.first() != Some(&topic) {
            continue;
        }
        if log.topics.len() != 3
            || log.data.len() != 32
            || log.topics[1][..12]
                .iter()
                .chain(&log.topics[2][..12])
                .any(|b| *b != 0)
        {
            return Err("malformed token Transfer".into());
        }
        let amount = U256::from_be_slice(&log.data);
        if Address::from_slice(&log.topics[2][12..]) == buyer {
            incoming = incoming.checked_add(amount).ok_or("transfer overflow")?;
        }
        if Address::from_slice(&log.topics[1][12..]) == buyer {
            outgoing = outgoing.checked_add(amount).ok_or("transfer overflow")?;
        }
    }
    let net = incoming
        .checked_sub(outgoing)
        .ok_or("negative BUY acquisition")?;
    if net == U256::ZERO || net < minimum || net != event.amount {
        return Err("BUY transfer net does not match event/minOutput".into());
    }
    Ok((net, event.fee))
}

fn validate_receipt_set(block: &Block, receipts: &[Receipt]) -> Result<(), String> {
    if receipts.len() != block.transactions.len() {
        return Err("incomplete receipt set".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut logs = std::collections::BTreeSet::new();
    for r in receipts {
        let tx =
            transaction_at_index(block, r.transaction_index).ok_or("receipt index out of range")?;
        if r.block_number != block.number
            || r.block_hash != block.hash
            || r.transaction_hash != tx.hash
            || tx.transaction_index != Some(r.transaction_index)
            || r.from != tx.from
            || r.to != tx.to
            || r.status > 1
            || !seen.insert(r.transaction_hash)
        {
            return Err("receipt/transaction/canonical block identity mismatch".into());
        }
        for log in &r.logs {
            if !logs.insert(log.log_index) {
                return Err("duplicate block log index".into());
            }
        }
    }
    Ok(())
}

fn successful_competing_buys_between(
    launch_block: u64,
    launch_index: u64,
    buy_block: u64,
    buy_index: u64,
    token: Address,
) -> Result<Vec<B256>, String> {
    if buy_block < launch_block || buy_block.saturating_sub(launch_block) > 32 {
        return Err("incomplete competitor chain window".into());
    }
    let mut hashes = Vec::new();
    let mut previous = None;
    for number in launch_block..=buy_block {
        let block = read_rpc(|| wire::block(number))?.ok_or("competitor block unavailable")?;
        if previous.is_some_and(|hash| block.parent_hash != hash) {
            return Err("incomplete competitor chain window".into());
        }
        previous = Some(block.hash);
        let receipts = read_rpc(|| block_receipts(&block))?;
        for r in receipts {
            let after_launch = number > launch_block || r.transaction_index > launch_index;
            let before_buy = number < buy_block || r.transaction_index < buy_index;
            if after_launch
                && before_buy
                && r.status == 1
                && portal_buys(&r)?
                    .iter()
                    .any(|b| b.token == token && b.amount > U256::ZERO)
            {
                hashes.push(r.transaction_hash);
            }
        }
    }
    Ok(hashes)
}

fn block_receipts(block: &Block) -> Result<Vec<Receipt>, String> {
    let receipts = if let Ok(Some(receipts)) = wire::block_receipts(block.number) {
        receipts
    } else {
        let mut receipts = Vec::with_capacity(block.transactions.len());
        for tx in &block.transactions {
            receipts.push(wire::receipt(tx.hash)?.ok_or("receipt unresolved during backfill")?);
        }
        receipts
    };
    validate_receipt_set(block, &receipts)?;
    Ok(receipts)
}

fn block_draft(block: &Block) -> Draft {
    let mut d = Draft::new(&chain_block::SCHEMA);
    d.set(chain_block::NUMBER, block.number)
        .set(chain_block::HASH, word(block.hash))
        .set(chain_block::PARENT_HASH, word(block.parent_hash))
        .set(chain_block::TIMESTAMP, block.timestamp)
        .set_opt(chain_block::MILLI_TIMESTAMP, block.milli_timestamp)
        .set_opt(chain_block::MIX_HASH, block.mix_hash.map(word))
        .set(chain_block::MINER, addr(block.miner))
        .set(chain_block::GAS_LIMIT, block.gas_limit)
        .set(chain_block::GAS_USED, block.gas_used)
        .set_opt(
            chain_block::BASE_FEE_PER_GAS,
            block.base_fee_per_gas.map(wei),
        )
        .set(
            chain_block::TRANSACTION_COUNT,
            block.transactions.len() as u64,
        );
    d
}

fn transaction_draft(block: &Block, tx: &Tx) -> Draft {
    use chain_transaction as t;
    let mut d = Draft::new(&t::SCHEMA);
    d.set(t::BLOCK_NUMBER, block.number)
        .set(t::BLOCK_HASH, word(block.hash))
        .set_opt(t::TRANSACTION_INDEX, tx.transaction_index)
        .set(t::HASH, word(tx.hash))
        .set(t::FROM, addr(tx.from))
        .set_opt(t::TO, tx.to.map(addr))
        .set(t::NONCE, tx.nonce)
        .set(t::VALUE, wei(tx.value))
        .set(t::GAS, tx.gas)
        .set_opt(t::GAS_PRICE, tx.gas_price.map(wei))
        .set_opt(t::TX_TYPE, tx.tx_type)
        .set(t::INPUT, Value::Bytes(tx.input.clone()));
    d
}

fn receipt_draft(receipt: &Receipt) -> Draft {
    use chain_receipt as r;
    let mut d = Draft::new(&r::SCHEMA);
    d.set(r::BLOCK_NUMBER, receipt.block_number)
        .set(r::BLOCK_HASH, word(receipt.block_hash))
        .set(r::TRANSACTION_INDEX, receipt.transaction_index)
        .set(r::TRANSACTION_HASH, word(receipt.transaction_hash))
        .set(r::FROM, addr(receipt.from))
        .set_opt(r::TO, receipt.to.map(addr))
        .set_opt(r::CONTRACT_ADDRESS, receipt.contract_address.map(addr))
        .set(r::STATUS, receipt.status)
        .set(r::GAS_USED, receipt.gas_used)
        .set(r::CUMULATIVE_GAS_USED, receipt.cumulative_gas_used)
        .set_opt(r::EFFECTIVE_GAS_PRICE, receipt.effective_gas_price.map(wei));
    for log in &receipt.logs {
        let mut l = Draft::new(&r::log::SCHEMA);
        l.set(r::log::LOG_INDEX, log.log_index)
            .set(r::log::ADDRESS, addr(log.address))
            .set(r::log::DATA, Value::Bytes(log.data.clone()));
        for topic in &log.topics {
            l.push(r::log::TOPICS, word(*topic));
        }
        d.push(r::LOGS, l.into_nested());
    }
    d
}

/// Appends one record as the next journal payload.
fn append_record(
    writer: &mut JournalWriter<File>,
    sequence: &mut u64,
    kind: RecordKind,
    timestamp_ns: u64,
    record: &Draft,
) -> Result<(), String> {
    let payload = record
        .encode()
        .map_err(|e| format!("encode chain capture record: {e}"))?;
    writer
        .append(&CaptureRecord {
            sequence: *sequence,
            kind,
            timestamp_ns,
            payload: payload.into_boxed_slice(),
        })
        .map_err(|e| format!("append {} {sequence}: {e}", record.schema().name))?;
    *sequence = sequence
        .checked_add(1)
        .ok_or("chain capture sequence overflow")?;
    Ok(())
}

fn capture_chain_window(
    path: &str,
    launch_block: u64,
    attempt_id: &str,
    launch_hash: B256,
    buy_hash: B256,
) -> Result<ChainCaptureSummary, String> {
    let file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("create-new chain capture {path}: {e}"))?;
    let mut writer = JournalWriter::new(file);
    let mut sequence = 0u64;

    let launch = read_rpc(|| wire::block(launch_block))?.ok_or("launch block unavailable for chain capture")?;
    let launch_timestamp = launch.timestamp;
    // Whole-second headers cannot locate T0 within that second. Include
    // its upper edge plus 15 seconds, then one further boundary block.
    let horizon_timestamp = launch_timestamp
        .checked_add(MARKET_WINDOW_SECS + 1)
        .ok_or("capture horizon overflow")?;

    let mut session = Draft::new(&chain_session::SCHEMA);
    session
        .set(chain_session::ATTEMPT_ID, attempt_id)
        .set(chain_session::LAUNCH_TX_HASH, word(launch_hash))
        .set(chain_session::BUY_TX_HASH, word(buy_hash))
        .set(chain_session::LAUNCH_BLOCK, launch_block)
        .set(
            chain_session::LAUNCH_BLOCK_TIMESTAMP_SECONDS,
            launch_timestamp,
        )
        .set(chain_session::WINDOW_SECONDS, MARKET_WINDOW_SECS)
        .set(chain_session::TIMESTAMP_SOURCE, BLOCK_TIMESTAMP_SOURCE)
        .set(
            chain_session::TIMESTAMP_RESOLUTION_NS,
            BLOCK_TIMESTAMP_RESOLUTION_NS,
        )
        .set(chain_session::PRODUCER_EXECUTION_TIMING_AUTHORITY, false);
    append_record(
        &mut writer,
        &mut sequence,
        RecordKind::Runtime,
        launch_timestamp.saturating_mul(1_000_000_000),
        &session,
    )
    .map_err(|e| format!("chain capture session metadata: {e}"))?;

    let first_block = launch_block.saturating_sub(1);
    let mut number = first_block;
    let mut crossed_horizon = false;
    let mut captured = Vec::new();
    let last_block;
    let capture_deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let mut waits = 0u16;
        let block = loop {
            if Instant::now() >= capture_deadline { return Err("chain capture time budget exhausted; INCOMPLETE".into()); }
            if let Some(block) = read_rpc(|| wire::block(number))? {
                break block;
            }
            waits = waits.saturating_add(1);
            if waits >= 600 {
                return Err(format!(
                    "timed out waiting for canonical block {number} during 15-second backfill"
                ));
            }
            thread::sleep(Duration::from_millis(200));
        };
        if block.number != number
            || captured
                .last()
                .is_some_and(|(_, hash)| *hash != block.parent_hash)
        {
            return Err("canonical chain discontinuity during capture".into());
        }
        if number == launch_block && block.hash != launch.hash {
            return Err("launch reorg during capture".into());
        }
        captured.push((number, block.hash));
        let timestamp = block.timestamp;
        let timestamp_ns = timestamp.saturating_mul(1_000_000_000);
        // Header, then the transactions in index order, then the receipts.
        append_record(
            &mut writer,
            &mut sequence,
            RecordKind::Canonical,
            timestamp_ns,
            &block_draft(&block),
        )
        .map_err(|e| format!("block {number}: {e}"))?;
        for tx in &block.transactions {
            append_record(
                &mut writer,
                &mut sequence,
                RecordKind::Canonical,
                timestamp_ns,
                &transaction_draft(&block, tx),
            )
            .map_err(|e| format!("transaction {} of block {number}: {e}", tx.hash))?;
        }

        let receipts = read_rpc(|| block_receipts(&block))?;
        for receipt in &receipts {
            append_record(
                &mut writer,
                &mut sequence,
                RecordKind::Canonical,
                timestamp_ns,
                &receipt_draft(receipt),
            )
            .map_err(|e| format!("receipts {number}: {e}"))?;
        }
        if crossed_horizon {
            last_block = number;
            break;
        }
        if timestamp >= horizon_timestamp {
            crossed_horizon = true;
        }
        number = number.checked_add(1).ok_or("block number overflow")?;
    }

    let manifest = writer.commit_file().map_err(|e| format!("chain capture commit: {e}"))?;

    let parent = Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    for (number, hash) in captured {
        if wire::block(number)?.is_none_or(|b| b.hash != hash) {
            return Err("reorg during canonical reconciliation".into());
        }
    }
    Ok(ChainCaptureSummary {
        path: path.to_string(),
        records: manifest.records,
        bytes: manifest.bytes,
        sha256: manifest.sha256,
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
    let g07_bytes = fs::read(&g07_path).unwrap_or_else(|e| panic!("read {g07_path}: {e}"));
    // The record type and version replace the former schema-name check.
    let g07 = decode(&evidence::SCHEMA, &g07_bytes)
        .unwrap_or_else(|e| panic!("G0.7 evidence {g07_path}: {e}"));

    // Every G0.7 field G0.8 reads, read once, before the first RPC.
    let attempt_id = g07_field(g07.text(evidence::ATTEMPT_ID)).to_string();
    let g07_chain_id = g07_field(g07.u64(evidence::CHAIN_ID));
    let g07_portal = Address::from(g07_field(g07.address(evidence::PORTAL)));
    let local_capture_complete = g07_field(g07.bool(evidence::CAPTURE_COMPLETE));
    let launch_hash = B256::from(g07_field(g07.b256(evidence::LAUNCH_TX_HASH)));
    let buy_hash = B256::from(g07_field(g07.b256(evidence::BUY_TX_HASH)));
    let token = Address::from(g07_field(g07.address(evidence::TOKEN)));
    let launcher = Address::from(g07_field(g07.address(evidence::LAUNCHER)));
    let buyer = Address::from(g07_field(g07.address(evidence::BUYER)));
    let expected_min_output = U256::from_be_bytes(g07_field(g07.u256(evidence::MIN_OUTPUT)));
    let expected_buy_value = U256::from_be_bytes(g07_field(g07.u256(evidence::BUY_VALUE_WEI)));

    assert_eq!(g07_chain_id, CHAIN_ID);
    assert_eq!(g07_portal, PORTAL);

    let evidence_path = req_env("DARKBOT_G08_EVIDENCE_PATH");
    let pending = unresolved_draft(
        &attempt_id,
        launch_hash,
        buy_hash,
        false,
        false,
        "RECONCILIATION_PENDING_OR_INTERRUPTED",
    );
    persist_result(&evidence_path, &pending).expect("durable UNRESOLVED evidence before RPC");

    let abort = |reason: &str| -> ! {
        let pending = unresolved_draft(&attempt_id, launch_hash, buy_hash, false, false, bounded_error(reason));
        match persist_result(&evidence_path, &pending) {
            Ok(record) => print!("{}", record.render()),
            Err(_) => eprintln!("UNRESOLVED terminal evidence write failed; prior pending evidence remains authoritative"),
        }
        std::process::exit(2)
    };
    macro_rules! resolved { ($value:expr) => { match $value { Ok(value) => value, Err(error) => abort(&error) } }; }
    let chain_id = resolved!(read_rpc(wire::chain_id));
    if chain_id != CHAIN_ID { abort("wrong RPC chain"); }

    let launch_receipt = resolved!(wait_receipt(launch_hash));
    let buy_receipt = resolved!(wait_receipt(buy_hash));
    if launch_receipt.is_none() || buy_receipt.is_none() {
        let pending = unresolved_draft(
            &attempt_id,
            launch_hash,
            buy_hash,
            launch_receipt.is_some(),
            buy_receipt.is_some(),
            "SENT_TRANSACTION_UNRESOLVED",
        );
        let record =
            persist_result(&evidence_path, &pending).expect("persist missing-receipt result");
        print!("{}", record.render());
        std::process::exit(2);
    }
    let launch_receipt = launch_receipt.unwrap();
    let buy_receipt = buy_receipt.unwrap();

    let launch_status = launch_receipt.status;
    let buy_status = buy_receipt.status;
    let launch_block = launch_receipt.block_number;
    let buy_block = buy_receipt.block_number;
    let launch_index = launch_receipt.transaction_index;
    let buy_index = buy_receipt.transaction_index;
    let launch_block_hash = launch_receipt.block_hash;
    let buy_block_hash = buy_receipt.block_hash;

    let launch_tx = resolved!(read_rpc(|| wire::transaction(launch_hash))).unwrap_or_else(|| abort("launch transaction unavailable"));
    let buy_tx = resolved!(read_rpc(|| wire::transaction(buy_hash))).unwrap_or_else(|| abort("BUY transaction unavailable"));

    let launch_scope_ok =
        launch_tx.from == launcher && launch_tx.to == Some(PORTAL) && launch_tx.value == U256::ZERO;
    let buy_scope_ok =
        buy_tx.from == buyer && buy_tx.to == Some(PORTAL) && buy_tx.value == expected_buy_value;

    let buy_call = swapExactInputCall::abi_decode_validate(&buy_tx.input)
        .unwrap_or_else(|_| abort("BUY calldata decode invalid"));
    let buy_calldata_ok = buy_call.params.inputToken == Address::ZERO
        && buy_call.params.outputToken == token
        && buy_call.params.inputAmount == expected_buy_value
        && buy_call.params.minOutputAmount == expected_min_output;

    let same_block = launch_block == buy_block;
    let ordered = buy_block > launch_block || (same_block && buy_index > launch_index);
    let adjacent = same_block && buy_index == launch_index.saturating_add(1);

    let launch_canonical_block = resolved!(read_rpc(|| wire::block(launch_block))).unwrap_or_else(|| abort("canonical launch block unavailable"));
    let buy_canonical_block = if same_block {
        launch_canonical_block.clone()
    } else {
        resolved!(read_rpc(|| wire::block(buy_block))).unwrap_or_else(|| abort("canonical BUY block unavailable"))
    };
    let launch_canonical_hash = launch_canonical_block.hash;
    let buy_canonical_hash = buy_canonical_block.hash;
    let mut canonical =
        launch_canonical_hash == launch_block_hash && buy_canonical_hash == buy_block_hash;
    let launch_at_index = transaction_at_index(&launch_canonical_block, launch_index)
        .is_some_and(|tx| tx.hash == launch_hash && tx == &launch_tx);
    let buy_at_index = transaction_at_index(&buy_canonical_block, buy_index)
        .is_some_and(|tx| tx.hash == buy_hash && tx == &buy_tx);

    let prior_block = launch_block.saturating_sub(1);
    let token_absent_before = Some(resolved!(read_rpc(|| wire::code(token, prior_block))))
        .map(|code| code.is_empty());
    let token_exists_after = Some(resolved!(read_rpc(|| wire::code(token, buy_block))))
        .map(|code| !code.is_empty());
    let buyer_balance_after = match token_exists_after {
        Some(true) => Some(resolved!(read_rpc(|| balance_of_at(token, buyer, buy_block)))),
        Some(false) => Some(U256::ZERO),
        None => None,
    };
    let acquisition = attributable_acquisition(
        &buy_receipt,
        token,
        buyer,
        expected_buy_value,
        expected_min_output,
    );
    // Persist the exact bounded diagnostic using the existing versioned
    // UNRESOLVED record. Do not reinterpret an unqualified tax ABI mismatch
    // as STOP or ADAPT, even when archive state is otherwise available.
    if let Err(error @ AcquisitionError::Unverified(_)) = &acquisition {
        abort(&error.to_string());
    }
    let acquisition_evidence_complete = token_absent_before.is_some()
        && token_exists_after.is_some()
        && buyer_balance_after.is_some();
    let acquisition_positive = acquisition.is_ok()
        && token_absent_before == Some(true)
        && token_exists_after == Some(true)
        && buyer_balance_after
            .as_ref()
            .is_some_and(|balance| *balance > U256::ZERO);

    let competitor_hashes =
        Some(resolved!(successful_competing_buys_between(launch_block, launch_index, buy_block, buy_index, token)));
    let competitor_count = competitor_hashes.as_ref().map(Vec::len);

    let launch_gas_cost = resolved!(receipt_gas_cost(&launch_receipt, &launch_tx));
    let buy_gas_cost = resolved!(receipt_gas_cost(&buy_receipt, &buy_tx));
    let buyer_total_debit = buy_tx.value.checked_add(buy_gas_cost).unwrap_or_else(|| abort("buyer cost overflow"));
    let experiment_total_cost = launch_gas_cost.checked_add(buyer_total_debit).unwrap_or_else(|| abort("experiment cost overflow"));

    let chain_capture = capture_chain_window(
        &chain_capture_path,
        launch_block,
        &attempt_id,
        launch_hash,
        buy_hash,
    );
    let chain_capture_complete = chain_capture
        .as_ref()
        .is_ok_and(|capture| capture.leading_boundary_included && capture.boundary_block_included);

    // Revalidate receipts and blocks after the entire observation window.
    canonical = canonical
        && resolved!(read_rpc(|| wire::block(launch_block)))
            .is_some_and(|b| b.hash == launch_block_hash)
        && resolved!(read_rpc(|| wire::block(buy_block)))
            .is_some_and(|b| b.hash == buy_block_hash)
        && resolved!(read_rpc(|| wire::receipt(launch_hash))).as_ref() == Some(&launch_receipt)
        && resolved!(read_rpc(|| wire::receipt(buy_hash))).as_ref() == Some(&buy_receipt);
    let receipt_identity = launch_receipt.transaction_hash == launch_hash
        && buy_receipt.transaction_hash == buy_hash
        && launch_tx.hash == launch_hash
        && buy_tx.hash == buy_hash
        && launch_receipt.from == launcher
        && launch_receipt.to == Some(PORTAL)
        && buy_receipt.from == buyer
        && buy_receipt.to == Some(PORTAL);
    let hard_stop = !receipt_identity
        || launch_status != 1
        || buy_status != 1
        || !launch_scope_ok
        || !buy_scope_ok
        || !buy_calldata_ok
        || !canonical
        || !launch_at_index
        || !buy_at_index
        || (acquisition_evidence_complete && !acquisition_positive)
        || !ordered;
    let adapt = !acquisition_evidence_complete
        || !same_block
        || !adjacent
        || competitor_count != Some(0)
        || !local_capture_complete
        || !chain_capture_complete;
    let verdict = if !canonical || !acquisition_evidence_complete || !chain_capture_complete || !local_capture_complete || competitor_count.is_none() {
        "UNRESOLVED"
    } else if hard_stop {
        "STOP"
    } else if adapt {
        "ADAPT"
    } else {
        "PASS"
    };

    let head = resolved!(read_rpc(wire::block_number));
    let mut r = Draft::new(&result::SCHEMA);
    r.set(result::GATE, "G0.8")
        .set(result::ATTEMPT_ID, attempt_id.as_str())
        .set(result::VERDICT, verdict)
        .set(
            result::G07_EVIDENCE_SHA256,
            Value::B256(darkbot_bsc_record::sha256(&g07_bytes)),
        )
        .set(result::LAUNCH_TX, word(launch_hash))
        .set(result::BUY_TX, word(buy_hash))
        .set(result::LAUNCH_STATUS, launch_status)
        .set(result::BUY_STATUS, buy_status)
        .set(result::LAUNCH_SCOPE_OK, launch_scope_ok)
        .set(result::BUY_SCOPE_OK, buy_scope_ok)
        .set(result::BUY_CALLDATA_OK, buy_calldata_ok)
        .set(result::TOKEN, addr(token))
        .set(result::BUYER, addr(buyer))
        .set(result::ACQUISITION_EVIDENCE_COMPLETE, acquisition_evidence_complete)
        .set_opt(result::TOKEN_ABSENT_BEFORE_LAUNCH_BLOCK, token_absent_before)
        .set_opt(result::TOKEN_EXISTS_AT_BUY_BLOCK, token_exists_after)
        .set_opt(
            result::BUYER_TOKEN_BALANCE_AT_BUY_BLOCK,
            buyer_balance_after.map(wei),
        )
        .set(result::POSITIVE_ATTRIBUTABLE_ACQUISITION, acquisition_positive)
        .set(
            result::BLOCK_HASH,
            word(if same_block {
                launch_canonical_hash
            } else {
                buy_canonical_hash
            }),
        )
        .set(result::LAUNCH_BLOCK_HASH, word(launch_canonical_hash))
        .set(result::BUY_BLOCK_HASH, word(buy_canonical_hash))
        .set(result::LAUNCH_BLOCK, launch_block)
        .set(result::LAUNCH_INDEX, launch_index)
        .set(result::BUY_BLOCK, buy_block)
        .set(result::BUY_INDEX, buy_index)
        .set(result::ORDERED, ordered)
        .set(result::SAME_BLOCK, same_block)
        .set(result::ADJACENT, adjacent)
        .set(result::CANONICAL, canonical)
        .set(result::LAUNCH_HASH_AT_INDEX, launch_at_index)
        .set(result::BUY_HASH_AT_INDEX, buy_at_index)
        .set_opt(
            result::EXTERNAL_BUYS_BETWEEN_COUNT,
            competitor_count.map(|n| n as u64),
        )
        .set(result::LAUNCH_GAS_COST_WEI, wei(launch_gas_cost))
        .set(result::BUY_GAS_COST_WEI, wei(buy_gas_cost))
        .set(result::BUY_VALUE_WEI, wei(expected_buy_value))
        .set(result::BUYER_TOTAL_DEBIT_WEI, wei(buyer_total_debit))
        .set(result::EXPERIMENT_TOTAL_COST_WEI, wei(experiment_total_cost))
        .set(result::DIRECT_BUILDER_TIP_WEI, wei(U256::ZERO))
        .set(result::G07_LOCAL_JOURNAL_COMPLETE, local_capture_complete)
        .set(result::CHAIN_WINDOW_COMPLETE, chain_capture_complete)
        .set(result::MARKET_WINDOW_SECONDS, MARKET_WINDOW_SECS)
        .set(result::CHAIN_RECORD_TIMESTAMP_SOURCE, BLOCK_TIMESTAMP_SOURCE)
        .set(
            result::CHAIN_RECORD_TIMESTAMP_RESOLUTION_NS,
            BLOCK_TIMESTAMP_RESOLUTION_NS,
        )
        .set(result::CHAIN_RECORD_TIMESTAMP_IS_PRODUCER_TIME, false)
        .set(
            result::COMPETITION_SCAN,
            "Complete canonical receipt sets; TokenBought events include router-internal Portal buys. Positive acquisition requires matching observed Transfer net and event amount in the exact BUY receipt; v5.25 tax semantics are not independently qualified. A mismatch is UNRESOLVED. Event fees are reported without adding them a second time to the transaction value.",
        )
        .set(result::CURRENT_HEAD, head)
        .set(
            result::CONFIRMATIONS_AFTER_LAUNCH_BLOCK,
            head.saturating_sub(launch_block),
        )
        .set(result::T0_TO_BUY, "UNKNOWN_PENDING_ORIGINAL_PRODUCER_WITNESS")
        .set(result::CHAIN_DATA_IS_NOT_EXECUTION_TIMESTAMP, true)
        .set(
            result::TIMING_NEXT,
            "run g08_producer_timing with the original producing execution witness",
        );
    for hash in competitor_hashes.iter().flatten() {
        r.push(result::EXTERNAL_BUYS_BETWEEN_HASHES, word(*hash));
    }
    if let Ok((amount, fee)) = acquisition {
        r.set(result::BUY_RECEIPT_TOKEN_NET, wei(amount))
            .set(result::BUY_EVENT_FEE_WEI, wei(fee));
    }
    match &chain_capture {
        Ok(c) => {
            use result::chain_journal as j;
            let mut journal = Draft::new(&j::SCHEMA);
            journal
                .set(j::PATH, c.path.as_str())
                .set(j::RECORDS, c.records)
                .set(j::BYTES, c.bytes)
                .set(j::SHA256, Value::B256(c.sha256))
                .set(j::FIRST_BLOCK, c.first_block)
                .set(j::LAST_BLOCK, c.last_block)
                .set(j::LEADING_BOUNDARY_INCLUDED, c.leading_boundary_included)
                .set(j::LAUNCH_TIMESTAMP, c.launch_timestamp)
                .set(j::HORIZON_TIMESTAMP, c.horizon_timestamp)
                .set(j::BOUNDARY_BLOCK_INCLUDED, c.boundary_block_included)
                .set(j::WINDOW_SECONDS, MARKET_WINDOW_SECS)
                .set(
                    j::COMPLETE,
                    c.leading_boundary_included && c.boundary_block_included,
                );
            r.set(result::CHAIN_JOURNAL, journal.into_nested());
        }
        Err(e) => {
            r.set(result::CHAIN_JOURNAL_ERROR, bounded_error(e));
        }
    }

    let record = match persist_result(&evidence_path, &r) {
        Ok(record) => record,
        Err(e) => {
            eprintln!("G0.8 evidence write failed: {e}");
            println!("G0_8_STOP_EVIDENCE_NOT_WRITTEN");
            std::process::exit(2);
        }
    };
    print!("{}", record.render());

    match verdict {
        "PASS" => println!("G0_8_MAINNET_ACQUISITION_ORDERING_PASS"),
        "ADAPT" => std::process::exit(3),
        _ => std::process::exit(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_rpc_recovery_and_receipt_errors_are_distinct_from_null() {
        let mut calls = 0;
        assert_eq!(recover_read(|| { calls += 1; if calls == 1 { Err("transport failure".into()) } else { Ok(7) } }, || {}).unwrap(),7);
        assert_eq!(calls,2);
        calls=0;
        let error = recover_read::<()>(|| { calls+=1; Err("timeout".into()) }, || {}).unwrap_err();
        assert_eq!(calls,3); assert!(error.contains("read_errors=3"));
        calls=0;
        assert!(recover_read::<()>(|| { calls+=1; Err("invalid JSON".into()) }, || {}).is_err()); assert_eq!(calls,1);
        let deadline = Instant::now()+Duration::from_secs(1);
        assert_eq!(poll_receipt::<u8>(||Ok(None),||{},deadline,2).unwrap(),None);
        assert_eq!(poll_receipt(||Ok(Some(1)),||{},deadline,2).unwrap(),Some(1));
        assert!(poll_receipt::<u8>(||Err("timeout".into()),||{},deadline,3).unwrap_err().contains("receipt_errors=3"));
        let reason="🦀".repeat(4097);
        let record=unresolved_draft("offline-test", B256::ZERO,B256::ZERO,false,false,&reason);
        assert!(record.encode().is_ok()); assert_eq!(bounded_error(&reason).len(),4000);
        assert!(receipt_gas_cost(&receipt(2,None), &tx(1,None)).is_err());
    }


    fn buy_log(token: Address, buyer: Address, amount: u64, value: u64, fee: u64) -> wire::Log {
        let mut data = Vec::new();
        for word in [
            U256::ZERO,
            U256::from_be_slice(token.as_slice()),
            U256::from_be_slice(buyer.as_slice()),
            U256::from(amount),
            U256::from(value),
            U256::from(fee),
            U256::ZERO,
        ] {
            data.extend_from_slice(&word.to_be_bytes::<32>());
        }
        wire::Log {
            log_index: 0,
            address: PORTAL,
            topics: vec![keccak256(
                "TokenBought(uint256,address,address,uint256,uint256,uint256,uint256)",
            )],
            data,
        }
    }

    fn transfer_log(token: Address, buyer: Address, amount: u64) -> wire::Log {
        wire::Log {
            log_index: 1,
            address: token,
            topics: vec![
                keccak256("Transfer(address,address,uint256)"),
                B256::ZERO,
                B256::from(U256::from_be_slice(buyer.as_slice()).to_be_bytes::<32>()),
            ],
            data: U256::from(amount).to_be_bytes::<32>().to_vec(),
        }
    }

    #[test]
    fn acquisition_requires_buy_event_and_net_transfer_to_exact_buyer() {
        let token = Address::repeat_byte(3);
        let buyer = Address::repeat_byte(4);
        let mut r = receipt(2, Some(5));
        r.from = buyer;
        r.to = Some(PORTAL);
        r.logs = vec![
            buy_log(token, buyer, 20, 100, 1),
            transfer_log(token, buyer, 20),
        ];
        assert_eq!(
            attributable_acquisition(&r, token, buyer, U256::from(100), U256::from(19)).unwrap(),
            (U256::from(20), U256::from(1))
        );
        for change in 0..6 {
            let mut bad = r.clone();
            match change {
                0 => bad.status = 0,
                1 => {
                    bad.logs.pop();
                }
                2 => bad.logs[1].address = Address::ZERO,
                3 => bad.logs[1].topics[2] = B256::ZERO,
                4 => bad.logs.push(bad.logs[0].clone()),
                _ => bad.logs[0].data.pop().map(|_| ()).unwrap(),
            }
            assert!(
                attributable_acquisition(&bad, token, buyer, U256::from(100), U256::from(19))
                    .is_err()
            );
        }
        assert!(
            attributable_acquisition(&r, token, buyer, U256::from(101), U256::from(19)).is_err()
        );
    }

    #[test]
    fn unqualified_tax_mismatches_are_distinct_from_proven_identity_failure() {
        let token = Address::repeat_byte(3);
        let buyer = Address::repeat_byte(4);
        let mut receipt = receipt(2, Some(5));
        receipt.from = buyer;
        receipt.to = Some(PORTAL);
        receipt.logs = vec![buy_log(token, buyer, 20, 100, 1), transfer_log(token, buyer, 20)];
        let assess = |r: &Receipt| attributable_acquisition(r, token, buyer, U256::from(100), U256::from(19));
        assert!(assess(&receipt).is_ok()); // Synthetic structure only, no tax qualification.
        for variant in 0..6 {
            let mut changed = receipt.clone();
            match variant {
                0 => changed.logs.clear(),
                1 => changed.logs[0] = buy_log(token, buyer, 21, 100, 1),
                2 => changed.logs[0] = buy_log(token, buyer, 20, 99, 1),
                3 => changed.logs[0] = buy_log(token, buyer, 20, 100, 101),
                4 => changed.logs[0] = buy_log(token, Address::ZERO, 20, 100, 1),
                _ => changed.logs[0] = buy_log(Address::ZERO, buyer, 20, 100, 1),
            }
            let error = assess(&changed).unwrap_err();
            assert!(matches!(error, AcquisitionError::Unverified(_)));
            assert!(error.to_string().starts_with("UNVERIFIED_TAX_ATTRIBUTION:"));
        }
        receipt.from = Address::ZERO;
        assert_eq!(assess(&receipt), Err(AcquisitionError::Identity));
        receipt.from = buyer;
        receipt.status = 0;
        assert_eq!(assess(&receipt), Err(AcquisitionError::Identity));
    }

    #[test]
    fn routed_portal_events_are_detected_without_top_level_portal_destination() {
        let token = Address::repeat_byte(3);
        let mut r = receipt(2, Some(5));
        r.to = Some(Address::repeat_byte(9));
        r.logs = vec![buy_log(token, Address::repeat_byte(4), 20, 100, 1)];
        assert_eq!(portal_buys(&r).unwrap()[0].token, token);
        r.logs[0].data.pop();
        assert!(portal_buys(&r).is_err());
    }

    #[test]
    fn receipt_set_requires_exact_canonical_membership_and_unique_logs() {
        let mut t = tx(1, Some(3));
        t.transaction_index = Some(0);
        let block = Block {
            number: 1,
            hash: B256::ZERO,
            parent_hash: B256::ZERO,
            timestamp: 0,
            milli_timestamp: None,
            mix_hash: None,
            miner: Address::ZERO,
            gas_limit: 100,
            gas_used: 2,
            base_fee_per_gas: None,
            transactions: vec![t.clone()],
        };
        let mut r = receipt(2, Some(3));
        r.transaction_hash = t.hash;
        r.to = t.to;
        validate_receipt_set(&block, &[r.clone()]).unwrap();
        assert!(validate_receipt_set(&block, &[]).is_err());
        let mut wrong = r.clone();
        wrong.block_hash = B256::repeat_byte(8);
        assert!(validate_receipt_set(&block, &[wrong]).is_err());
        let mut duplicate = block.clone();
        duplicate.transactions.push(t);
        assert!(validate_receipt_set(&duplicate, &[r.clone(), r]).is_err());
    }

    fn filled(schema: &'static darkbot_bsc_record::Schema) -> Draft {
        use darkbot_bsc_record::{Kind, Presence};
        let mut d = Draft::new(schema);
        for f in schema
            .fields
            .iter()
            .filter(|f| f.presence == Presence::Required)
        {
            d.set(
                f.tag,
                match f.kind {
                    Kind::Bool => Value::Bool(false),
                    Kind::U64 => Value::U64(0),
                    Kind::I64 => Value::I64(0),
                    Kind::U256 => Value::U256([0; 32]),
                    Kind::B256 => Value::B256([0; 32]),
                    Kind::Address => Value::Address([0; 20]),
                    Kind::Text => Value::Text("offline".into()),
                    Kind::Bytes => Value::Bytes(vec![]),
                    Kind::Nested => filled(f.nested.unwrap()).into_nested(),
                },
            );
        }
        d
    }

    #[test]
    fn every_terminal_verdict_is_durable_and_write_errors_fail_closed() {
        let dir = env::temp_dir().join(format!("g08-verdicts-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("result.dbrec");
        for verdict in ["PASS", "ADAPT", "STOP", "UNRESOLVED"] {
            let mut d = filled(&result::SCHEMA);
            d.set(result::VERDICT, verdict);
            persist_result(path.to_str().unwrap(), &d).unwrap();
            assert_eq!(
                decode(&result::SCHEMA, &fs::read(&path).unwrap())
                    .unwrap()
                    .text(result::VERDICT)
                    .unwrap(),
                verdict
            );
            assert!(persist_result(dir.join("absent/result.dbrec").to_str().unwrap(), &d).is_err());
            assert!(persist_result(dir.to_str().unwrap(), &d).is_err());
        }
        let d = unresolved_draft(
            "test",
            B256::ZERO,
            B256::repeat_byte(1),
            true,
            false,
            "SENT_TRANSACTION_UNRESOLVED",
        );
        persist_result(path.to_str().unwrap(), &d).unwrap();
        assert_eq!(
            decode(&unresolved::SCHEMA, &fs::read(&path).unwrap())
                .unwrap()
                .text(unresolved::VERDICT)
                .unwrap(),
            "UNRESOLVED"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    fn tx(hash: u8, gas_price: Option<u64>) -> Tx {
        Tx {
            hash: B256::repeat_byte(hash),
            from: Address::ZERO,
            to: Some(PORTAL),
            input: Vec::new(),
            value: U256::ZERO,
            gas: 21_000,
            gas_price: gas_price.map(U256::from),
            nonce: 0,
            transaction_index: None,
            tx_type: None,
        }
    }

    fn receipt(gas_used: u64, effective_gas_price: Option<u64>) -> Receipt {
        Receipt {
            block_number: 1,
            block_hash: B256::ZERO,
            transaction_index: 0,
            transaction_hash: B256::ZERO,
            from: Address::ZERO,
            to: None,
            contract_address: None,
            status: 1,
            gas_used,
            cumulative_gas_used: gas_used,
            effective_gas_price: effective_gas_price.map(U256::from),
            logs: Vec::new(),
        }
    }

    #[test]
    fn invalid_ordering_window_is_unknown_without_rpc() {
        // BUY before launch, or a window wider than 32 blocks, yields no
        // verdict (None = UNKNOWN) before any network read.
        assert!(successful_competing_buys_between(10, 0, 9, 0, Address::ZERO).is_err());
        assert!(successful_competing_buys_between(0, 0, 33, 0, Address::ZERO).is_err());
    }

    #[test]
    fn receipt_gas_cost_uses_effective_price_then_signed_price() {
        let signed = tx(1, Some(3));
        assert_eq!(
            receipt_gas_cost(&receipt(2, Some(5)), &signed).unwrap(),
            U256::from(10u64)
        );
        assert_eq!(
            receipt_gas_cost(&receipt(2, None), &signed).unwrap(),
            U256::from(6u64)
        );
    }

    #[test]
    fn transaction_index_lookup_is_bounded() {
        let block = Block {
            number: 1,
            hash: B256::ZERO,
            parent_hash: B256::ZERO,
            timestamp: 0,
            milli_timestamp: None,
            mix_hash: None,
            miner: Address::ZERO,
            gas_limit: 0,
            gas_used: 0,
            base_fee_per_gas: None,
            transactions: vec![tx(0xa, None), tx(0xb, None)],
        };
        assert_eq!(
            transaction_at_index(&block, 1).unwrap().hash,
            B256::repeat_byte(0xb)
        );
        assert!(transaction_at_index(&block, 2).is_none());
    }

    #[test]
    fn chain_capture_payloads_are_typed_records() {
        let block = Block {
            number: 7,
            hash: B256::repeat_byte(1),
            parent_hash: B256::repeat_byte(2),
            timestamp: 100,
            milli_timestamp: Some(100_250),
            mix_hash: None,
            miner: Address::repeat_byte(3),
            gas_limit: 1,
            gas_used: 0,
            base_fee_per_gas: None,
            transactions: vec![tx(0xa, Some(1))],
        };
        let mut r = receipt(21_000, None);
        r.logs.push(wire::Log {
            log_index: 0,
            address: PORTAL,
            topics: vec![B256::repeat_byte(4), B256::repeat_byte(5)],
            data: vec![1, 2, 3],
        });
        let header = decode(&chain_block::SCHEMA, &block_draft(&block).encode().unwrap()).unwrap();
        assert_eq!(header.u64(chain_block::TRANSACTION_COUNT).unwrap(), 1);
        assert_eq!(header.opt_b256(chain_block::MIX_HASH).unwrap(), None);
        let t = decode(
            &chain_transaction::SCHEMA,
            &transaction_draft(&block, &block.transactions[0])
                .encode()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            t.address(chain_transaction::TO).unwrap(),
            PORTAL.into_array()
        );
        assert_eq!(
            t.opt_u64(chain_transaction::TRANSACTION_INDEX).unwrap(),
            None
        );
        let receipt = decode(&chain_receipt::SCHEMA, &receipt_draft(&r).encode().unwrap()).unwrap();
        let logs = receipt.nested_all(chain_receipt::LOGS).unwrap();
        assert_eq!(logs[0].bytes(chain_receipt::log::DATA).unwrap(), [1, 2, 3]);
        assert_eq!(logs[0].all(chain_receipt::log::TOPICS).count(), 2);
    }

    #[test]
    fn atomic_write_replaces_the_whole_file() {
        let dir = env::temp_dir().join(format!("darkbot-g08-atomic-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("result.dbrec");
        let path = path.to_str().unwrap();
        fs::write(path, b"old").unwrap();
        write_atomic(path, b"new record").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"new record");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
