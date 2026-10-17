//! G0.8 read-only BSC Mainnet acquisition, ordering, economics and canonical
//! 15-second evidence reconciliation for a preceding G0.7 attempt.
//!
//! This binary never submits a transaction. Builder acknowledgements are not
//! acquisition authority; canonical chain execution is.
//!
//! It reads the binary G0.7 evidence record and writes a binary result
//! record (DBREC001) and a binary chain capture journal. JSON exists only on
//! the JSON-RPC wire, inside [`wire`].

use alloy_primitives::{Address, B256, U256, address};
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
    time::Duration,
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

// ---- JSON-RPC wire boundary --------------------------------------------
//
// The only place JSON exists in this binary. Request bodies are fixed
// templates filled with hex quantities and data; every response is decoded
// into the typed values below before it leaves this module, and no JSON
// type crosses its border (tests/g08_binary_records.rs checks the source).
mod wire {
    use super::req_env;
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
        let url = req_env("BSC_MAINNET_RPC_HTTPS");
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
                "curl {method}: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        response(method, &out.stdout)
    }

    fn response(method: &str, bytes: &[u8]) -> Result<Json, String> {
        let v: Json = serde_json::from_slice(bytes)
            .map_err(|e| format!("invalid JSON-RPC response for {method}: {e}"))?;
        if let Some(err) = v.get("error") {
            let code = err.get("code").and_then(Json::as_i64).unwrap_or(0);
            let message = err.get("message").and_then(Json::as_str).unwrap_or("");
            return Err(format!("JSON-RPC {method}: error {code}: {message}"));
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
                "JSON-RPC eth_call: error -32000: execution reverted"
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
            let receipt: Json = serde_json::from_str(&format!(
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
            let r = receipt_of(&receipt).unwrap();
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

fn wait_receipt(hash: B256) -> Option<Receipt> {
    for _ in 0..RECEIPT_TIMEOUT_POLLS {
        match wire::receipt(hash) {
            Ok(Some(receipt)) => return Some(receipt),
            Ok(None) | Err(_) => thread::sleep(RECEIPT_POLL),
        }
    }
    None
}

fn balance_of_at(token: Address, owner: Address, block: u64) -> Result<U256, String> {
    let data = balanceOfCall { owner }.abi_encode();
    let value = wire::eth_call(token, &data, block)?;
    U256::try_from_be_slice(&value).ok_or_else(|| "balanceOf result exceeds 32 bytes".to_string())
}

fn transaction_at_index(block: &Block, index: u64) -> Option<&Tx> {
    block.transactions.get(index as usize)
}

fn receipt_gas_cost(receipt: &Receipt, tx: &Tx) -> U256 {
    let gas_price = receipt
        .effective_gas_price
        .or(tx.gas_price)
        .expect("neither effectiveGasPrice nor gasPrice reported");
    U256::from(receipt.gas_used) * gas_price
}

fn successful_competing_buys_between(
    launch_block: u64,
    launch_index: u64,
    buy_block: u64,
    buy_index: u64,
    token: Address,
) -> Option<Vec<B256>> {
    if buy_block < launch_block || buy_block.saturating_sub(launch_block) > 32 {
        return None;
    }
    let mut hashes = Vec::new();
    for block_number in launch_block..=buy_block {
        let block = wire::block(block_number).ok()??;
        for tx in &block.transactions {
            let idx = tx.transaction_index.unwrap_or(0);
            let after_launch = block_number > launch_block || idx > launch_index;
            let before_buy = block_number < buy_block || idx < buy_index;
            if !after_launch || !before_buy {
                continue;
            }
            if tx.to != Some(PORTAL) {
                continue;
            }
            let Ok(call) = swapExactInputCall::abi_decode_validate(&tx.input) else {
                continue;
            };
            if call.params.outputToken != token {
                continue;
            }
            let receipt = wire::receipt(tx.hash).ok()?;
            if receipt.is_some_and(|r| r.status == 1) {
                hashes.push(tx.hash);
            }
        }
    }
    Some(hashes)
}

fn block_receipts(block: &Block) -> Result<Vec<Receipt>, String> {
    if let Ok(Some(receipts)) = wire::block_receipts(block.number) {
        return Ok(receipts);
    }

    let mut receipts = Vec::with_capacity(block.transactions.len());
    for tx in &block.transactions {
        let receipt = wire::receipt(tx.hash)?
            .ok_or_else(|| format!("receipt unresolved during canonical backfill: {}", tx.hash))?;
        receipts.push(receipt);
    }
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
        .write(true)
        .open(path)
        .map_err(|e| format!("create-new chain capture {path}: {e}"))?;
    let mut writer = JournalWriter::new(file);
    let mut sequence = 0u64;

    let launch = wire::block(launch_block)?.ok_or("launch block unavailable for chain capture")?;
    let launch_timestamp = launch.timestamp;
    let horizon_timestamp = launch_timestamp.saturating_add(MARKET_WINDOW_SECS);

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
    let mut last_block = first_block;
    loop {
        let mut waits = 0u16;
        let block = loop {
            if let Some(block) = wire::block(number)? {
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

        let receipts = block_receipts(&block)?;
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
        last_block = number;

        if crossed_horizon {
            break;
        }
        if timestamp >= horizon_timestamp {
            crossed_horizon = true;
        }
        number = number.checked_add(1).ok_or("block number overflow")?;
    }

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

    let chain_id = wire::chain_id().expect("chain-id read failed");
    assert_eq!(chain_id, CHAIN_ID, "wrong RPC chain");

    let launch_receipt = wait_receipt(launch_hash);
    let buy_receipt = wait_receipt(buy_hash);
    if launch_receipt.is_none() || buy_receipt.is_none() {
        let mut unresolved = Draft::new(&unresolved::SCHEMA);
        unresolved
            .set(unresolved::GATE, "G0.8")
            .set(unresolved::VERDICT, "STOP")
            .set(unresolved::REASON, "SENT_TRANSACTION_UNRESOLVED")
            .set(unresolved::ATTEMPT_ID, attempt_id.as_str())
            .set(unresolved::LAUNCH_TX, word(launch_hash))
            .set(unresolved::BUY_TX, word(buy_hash))
            .set(
                unresolved::LAUNCH_RECEIPT_RESOLVED,
                launch_receipt.is_some(),
            )
            .set(unresolved::BUY_RECEIPT_RESOLVED, buy_receipt.is_some())
            .set(unresolved::AUTOMATIC_RETRY, false)
            .set(unresolved::T0_TO_BUY, "UNKNOWN");
        let bytes = unresolved.encode().expect("encode G0.8 unresolved record");
        let record = decode(&unresolved::SCHEMA, &bytes).expect("decode G0.8 unresolved record");
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

    let launch_tx = wire::transaction(launch_hash).expect("launch transaction lookup failed");
    let buy_tx = wire::transaction(buy_hash).expect("BUY transaction lookup failed");
    assert!(launch_tx.is_some() && buy_tx.is_some());
    let launch_tx = launch_tx.unwrap();
    let buy_tx = buy_tx.unwrap();

    let launch_scope_ok =
        launch_tx.from == launcher && launch_tx.to == Some(PORTAL) && launch_tx.value == U256::ZERO;
    let buy_scope_ok =
        buy_tx.from == buyer && buy_tx.to == Some(PORTAL) && buy_tx.value == expected_buy_value;

    let buy_call = swapExactInputCall::abi_decode_validate(&buy_tx.input)
        .expect("BUY calldata is not the expected swapExactInput call");
    let buy_calldata_ok = buy_call.params.inputToken == Address::ZERO
        && buy_call.params.outputToken == token
        && buy_call.params.inputAmount == expected_buy_value
        && buy_call.params.minOutputAmount == expected_min_output;

    let same_block = launch_block == buy_block;
    let ordered = buy_block > launch_block || (same_block && buy_index > launch_index);
    let adjacent = same_block && buy_index == launch_index.saturating_add(1);

    let launch_canonical_block = wire::block(launch_block)
        .expect("canonical launch block lookup failed")
        .expect("canonical launch block hash missing");
    let buy_canonical_block = if same_block {
        launch_canonical_block.clone()
    } else {
        wire::block(buy_block)
            .expect("canonical BUY block lookup failed")
            .expect("canonical BUY block hash missing")
    };
    let launch_canonical_hash = launch_canonical_block.hash;
    let buy_canonical_hash = buy_canonical_block.hash;
    let canonical =
        launch_canonical_hash == launch_block_hash && buy_canonical_hash == buy_block_hash;
    let launch_at_index = transaction_at_index(&launch_canonical_block, launch_index)
        .is_some_and(|tx| tx.hash == launch_hash);
    let buy_at_index =
        transaction_at_index(&buy_canonical_block, buy_index).is_some_and(|tx| tx.hash == buy_hash);

    let prior_block = launch_block.saturating_sub(1);
    let token_absent_before = wire::code(token, prior_block)
        .ok()
        .map(|code| code.is_empty());
    let token_exists_after = wire::code(token, buy_block)
        .ok()
        .map(|code| !code.is_empty());
    let buyer_balance_after = match token_exists_after {
        Some(true) => balance_of_at(token, buyer, buy_block).ok(),
        Some(false) => Some(U256::ZERO),
        None => None,
    };
    let acquisition_evidence_complete = token_absent_before.is_some()
        && token_exists_after.is_some()
        && buyer_balance_after.is_some();
    let acquisition_positive = token_absent_before == Some(true)
        && token_exists_after == Some(true)
        && buyer_balance_after
            .as_ref()
            .is_some_and(|balance| *balance > U256::ZERO);

    let competitor_hashes =
        successful_competing_buys_between(launch_block, launch_index, buy_block, buy_index, token);
    let competitor_count = competitor_hashes.as_ref().map(Vec::len);

    let launch_gas_cost = receipt_gas_cost(&launch_receipt, &launch_tx);
    let buy_gas_cost = receipt_gas_cost(&buy_receipt, &buy_tx);
    let buyer_total_debit = expected_buy_value + buy_gas_cost;
    let experiment_total_cost = launch_gas_cost + buyer_total_debit;

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
    let adapt = !acquisition_evidence_complete
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

    let head = wire::block_number().expect("head lookup failed");
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
            "Direct Portal swapExactInput is parsed online; full block/receipt/log material is retained for complete offline derivation. N/N+1 itself proves zero intervening transactions for PASS.",
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
            r.set(result::CHAIN_JOURNAL_ERROR, e.as_str());
        }
    }

    let bytes = r.encode().expect("encode G0.8 result record");
    let record = decode(&result::SCHEMA, &bytes).expect("decode G0.8 result record");
    // A result that could not be written is no result: the view goes to
    // stderr only, stdout carries the STOP line and no PASS.
    if let Ok(path) = env::var("DARKBOT_G08_EVIDENCE_PATH")
        && let Err(e) = write_atomic(&path, &bytes)
    {
        eprint!("{}", record.render());
        eprintln!("G0.8 evidence write failed: {e}");
        println!("G0_8_STOP_EVIDENCE_NOT_WRITTEN");
        std::process::exit(2);
    }
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
        assert_eq!(
            successful_competing_buys_between(10, 0, 9, 0, Address::ZERO),
            None
        );
        assert_eq!(
            successful_competing_buys_between(0, 0, 33, 0, Address::ZERO),
            None
        );
    }

    #[test]
    fn receipt_gas_cost_uses_effective_price_then_signed_price() {
        let signed = tx(1, Some(3));
        assert_eq!(
            receipt_gas_cost(&receipt(2, Some(5)), &signed),
            U256::from(10u64)
        );
        assert_eq!(
            receipt_gas_cost(&receipt(2, None), &signed),
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
