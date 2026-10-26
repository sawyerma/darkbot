//! The only place of the freeze where JSON exists: the external JSON-RPC
//! wire, the Sourcify wire and the archived 2026-09-27 legacy files. Request
//! bodies are fixed templates of hex quantities and data; every answer is
//! decoded into the typed values below before it leaves this module, and no
//! JSON type crosses its border (`tests::json_stays_inside_the_edge`).
//!
//! The RPC transport is the shared `rpc_wire::post` (endpoint and request on
//! curl's stdin, never argv). Every state read names one canonical block by
//! hash (EIP-1898 `{blockHash, requireCanonical}`); nothing reads `latest`
//! state. A chain other than 56, a header for another height, receipts or
//! transactions of another block are refused.

use alloy_primitives::{Address, B256, Bloom, U256};
use darkbot_bsc_execution::rpc_wire::{self, Limits};
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::{env, io::Write, process::Command, process::Stdio, thread, time::Duration};

pub const BSC_CHAIN_ID: u64 = 56;

/// A header of the canonical chain as the endpoint answered it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub number: u64,
    pub hash: B256,
    pub parent_hash: B256,
    pub timestamp: u64,
    pub logs_bloom: Bloom,
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
    pub logs: Vec<Log>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tx {
    pub hash: B256,
    pub block_hash: Option<B256>,
    pub block_number: Option<u64>,
    pub transaction_index: Option<u64>,
    pub from: Address,
    pub to: Option<Address>,
    pub input: Vec<u8>,
    pub value: U256,
}

/// A canonical block with its full transactions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FullBlock {
    pub number: u64,
    pub hash: B256,
    pub transactions: Vec<Tx>,
}

/// One message of an `eth_simulateV1` block (no signature: the node
/// executes it as a call from `from`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimCall {
    pub from: Address,
    pub to: Address,
    pub value: U256,
    pub data: Vec<u8>,
    pub gas: u64,
}

/// A storage word a simulation overrides (`stateDiff`): account, slot,
/// value.
pub type StorageOverride = (Address, B256, B256);

/// A log of a simulated message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimLog {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Vec<u8>,
}

/// The node's result of one simulated message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimResult {
    pub status: u64,
    pub return_data: Vec<u8>,
    pub gas_used: u64,
    pub logs: Vec<SimLog>,
    pub error: Option<String>,
}

/// The block every state read names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin {
    pub number: u64,
    pub hash: B256,
}

/// An `eth_call` that executed: returned data or a revert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallOutcome {
    Returned(Vec<u8>),
    Reverted(String),
}

/// Refuses every chain but BSC Mainnet.
pub fn require_bsc(chain_id: u64) -> Result<(), String> {
    if chain_id == BSC_CHAIN_ID {
        Ok(())
    } else {
        Err(format!(
            "wrong chain: eth_chainId {chain_id}, not {BSC_CHAIN_ID}"
        ))
    }
}

/// The pin of `header`, which must be the answer for `requested`.
pub fn pin_of(requested: u64, header: &Header) -> Result<Pin, String> {
    if header.number != requested {
        return Err(format!(
            "unpinned answer: asked for block {requested}, got {}",
            header.number
        ));
    }
    if header.hash == B256::ZERO {
        return Err(format!("block {requested} has no hash (pending)"));
    }
    Ok(Pin {
        number: header.number,
        hash: header.hash,
    })
}

/// Receipts of exactly the block of `header`, unique transaction indices.
pub fn require_receipts_of(header: &Header, receipts: &[Receipt]) -> Result<(), String> {
    let mut indices = std::collections::BTreeSet::new();
    for r in receipts {
        if r.block_hash != header.hash || r.block_number != header.number {
            return Err(format!(
                "receipt {} belongs to block {} {}, not the pinned {} {}",
                r.transaction_hash, r.block_number, r.block_hash, header.number, header.hash
            ));
        }
        if !indices.insert(r.transaction_index) {
            return Err(format!(
                "duplicate transaction index {}",
                r.transaction_index
            ));
        }
    }
    Ok(())
}

/// A transaction mined exactly where its receipt says.
pub fn require_tx_at(tx: &Tx, receipt: &Receipt) -> Result<(), String> {
    if tx.hash != receipt.transaction_hash
        || tx.block_hash != Some(receipt.block_hash)
        || tx.block_number != Some(receipt.block_number)
        || tx.transaction_index != Some(receipt.transaction_index)
    {
        return Err(format!(
            "transaction {} is not mined at block {} index {}",
            receipt.transaction_hash, receipt.block_number, receipt.transaction_index
        ));
    }
    Ok(())
}

// ---- JSON-RPC -------------------------------------------------------------------

/// A read-only JSON-RPC endpoint and the bounds of every call.
pub struct Endpoint {
    url: String,
    limits: Limits,
    attempts: u32,
    retry_pause: Duration,
}

fn quantity(n: u64) -> String {
    format!("\"0x{n:x}\"")
}

fn data(bytes: &[u8]) -> String {
    format!("\"0x{}\"", hex::encode(bytes))
}

fn body(id: u64, method: &str, params: &str) -> String {
    format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"{method}\",\"params\":{params}}}")
}

/// EIP-1898: exactly this canonical block, never a number or a tag.
fn pinned(pin: &Pin) -> String {
    format!(
        "{{\"blockHash\":{},\"requireCanonical\":true}}",
        data(pin.hash.as_slice())
    )
}

fn code_params(address: Address, pin: &Pin) -> String {
    format!("[{},{}]", data(address.as_slice()), pinned(pin))
}

fn storage_params(address: Address, slot: B256, pin: &Pin) -> String {
    format!(
        "[{},{},{}]",
        data(address.as_slice()),
        data(slot.as_slice()),
        pinned(pin)
    )
}

/// `eth_estimateGas` of a call from `from` at the pin.
fn estimate_params(from: Address, to: Address, value: U256, input: &[u8], pin: &Pin) -> String {
    format!(
        "[{{\"from\":{},\"to\":{},\"value\":\"0x{value:x}\",\"data\":{}}},{}]",
        data(from.as_slice()),
        data(to.as_slice()),
        data(input),
        pinned(pin)
    )
}

/// `eth_simulateV1`: one block of `calls` on top of the pinned block, the
/// given balances overridden, no signature validation.
fn simulate_params(
    pin: &Pin,
    balances: &[(Address, U256)],
    storage: &[StorageOverride],
    calls: &[SimCall],
) -> String {
    // One override object per account: its balance and its storage words.
    let mut accounts: BTreeMap<Address, (Option<U256>, Vec<String>)> = BTreeMap::new();
    for (a, b) in balances {
        accounts.entry(*a).or_default().0 = Some(*b);
    }
    for (a, slot, value) in storage {
        accounts.entry(*a).or_default().1.push(format!(
            "{}:{}",
            data(slot.as_slice()),
            data(value.as_slice())
        ));
    }
    let overrides: Vec<String> = accounts
        .iter()
        .map(|(a, (balance, words))| {
            let mut fields = Vec::new();
            if let Some(b) = balance {
                fields.push(format!("\"balance\":\"0x{b:x}\""));
            }
            if !words.is_empty() {
                fields.push(format!("\"stateDiff\":{{{}}}", words.join(",")));
            }
            format!("{}:{{{}}}", data(a.as_slice()), fields.join(","))
        })
        .collect();
    let calls: Vec<String> = calls
        .iter()
        .map(|c| {
            format!(
                "{{\"from\":{},\"to\":{},\"value\":\"0x{:x}\",\"data\":{},\"gas\":{}}}",
                data(c.from.as_slice()),
                data(c.to.as_slice()),
                c.value,
                data(&c.data),
                quantity(c.gas)
            )
        })
        .collect();
    format!(
        "[{{\"blockStateCalls\":[{{\"stateOverrides\":{{{}}},\"calls\":[{}]}}],\"validation\":false}},{}]",
        overrides.join(","),
        calls.join(","),
        pinned(pin)
    )
}

fn sim_result_of(v: &Json) -> Result<SimResult, String> {
    let logs = field(v, "logs")?
        .as_array()
        .ok_or("simulated logs are not a list")?
        .iter()
        .map(|l| {
            Ok(SimLog {
                address: req(l, "address", address_of)?,
                topics: field(l, "topics")?
                    .as_array()
                    .ok_or("topics are not a list")?
                    .iter()
                    .map(|t| b256_of(t, "topic"))
                    .collect::<Result<Vec<_>, String>>()?,
                data: req(l, "data", bytes_of)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SimResult {
        status: req(v, "status", u64_of)?,
        return_data: req(v, "returnData", bytes_of)?,
        gas_used: req(v, "gasUsed", u64_of)?,
        logs,
        error: present(v, "error").map(|e| {
            bounded(
                e.get("message").and_then(Json::as_str).unwrap_or("error"),
                200,
            )
        }),
    })
}

fn call_params(to: Address, input: &[u8], pin: &Pin) -> String {
    format!(
        "[{{\"to\":{},\"data\":{}}},{}]",
        data(to.as_slice()),
        data(input),
        pinned(pin)
    )
}

/// A JSON-RPC error answer, or a transport/decoding failure (`code` None).
#[derive(Debug)]
struct Failure {
    code: Option<i64>,
    text: String,
}

fn bounded(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn response(method: &str, bytes: &[u8]) -> Result<Json, Failure> {
    let v: Json = serde_json::from_slice(bytes).map_err(|e| Failure {
        code: None,
        text: format!("invalid JSON-RPC response for {method}: {e}"),
    })?;
    answer(method, &v)
}

fn answer(method: &str, v: &Json) -> Result<Json, Failure> {
    if let Some(err) = v.get("error") {
        let code = err.get("code").and_then(Json::as_i64);
        let message = err.get("message").and_then(Json::as_str).unwrap_or("");
        return Err(Failure {
            code,
            text: format!(
                "JSON-RPC {method}: error {}: {}",
                code.unwrap_or(0),
                bounded(message, 200)
            ),
        });
    }
    v.get("result").cloned().ok_or_else(|| Failure {
        code: None,
        text: format!("JSON-RPC {method}: missing result"),
    })
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
    u64::from_str_radix(digits(v, what)?, 16).map_err(|_| format!("{what} is not a u64 quantity"))
}

fn u256_of(v: &Json, what: &str) -> Result<U256, String> {
    let d = digits(v, what)?;
    if d.is_empty() {
        return Err(format!("{what} is an empty quantity"));
    }
    U256::from_str_radix(d, 16).map_err(|_| format!("{what} is not a U256 quantity"))
}

fn bytes_of(v: &Json, what: &str) -> Result<Vec<u8>, String> {
    hex::decode(digits(v, what)?).map_err(|_| format!("{what} is not hex data"))
}

fn b256_of(v: &Json, what: &str) -> Result<B256, String> {
    B256::try_from(bytes_of(v, what)?.as_slice()).map_err(|_| format!("{what} is not 32 bytes"))
}

fn address_of(v: &Json, what: &str) -> Result<Address, String> {
    Address::try_from(bytes_of(v, what)?.as_slice()).map_err(|_| format!("{what} is not 20 bytes"))
}

fn req<T>(v: &Json, name: &str, decode: fn(&Json, &str) -> Result<T, String>) -> Result<T, String> {
    decode(field(v, name)?, name)
}

fn opt<T>(
    v: &Json,
    name: &str,
    decode: fn(&Json, &str) -> Result<T, String>,
) -> Result<Option<T>, String> {
    present(v, name).map(|f| decode(f, name)).transpose()
}

fn header_of(v: &Json) -> Result<Header, String> {
    let bloom = bytes_of(field(v, "logsBloom")?, "logsBloom")?;
    if bloom.len() != 256 {
        return Err("logsBloom is not 256 bytes".into());
    }
    Ok(Header {
        number: req(v, "number", u64_of)?,
        hash: req(v, "hash", b256_of)?,
        parent_hash: req(v, "parentHash", b256_of)?,
        timestamp: req(v, "timestamp", u64_of)?,
        logs_bloom: Bloom::from_slice(&bloom),
    })
}

fn log_of(v: &Json, receipt: &Json) -> Result<Log, String> {
    for key in [
        "blockHash",
        "blockNumber",
        "transactionHash",
        "transactionIndex",
    ] {
        if field(v, key)? != field(receipt, key)? {
            return Err(format!("log {key} differs from its receipt"));
        }
    }
    if v.get("removed").and_then(Json::as_bool) != Some(false) {
        return Err("removed or unqualified log".into());
    }
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
        .map(|l| log_of(l, v))
        .collect::<Result<Vec<_>, _>>()?;
    let mut indices = std::collections::BTreeSet::new();
    if !logs.iter().all(|l| indices.insert(l.log_index)) {
        return Err("duplicate log index".into());
    }
    Ok(Receipt {
        block_number: req(v, "blockNumber", u64_of)?,
        block_hash: req(v, "blockHash", b256_of)?,
        transaction_index: req(v, "transactionIndex", u64_of)?,
        transaction_hash: req(v, "transactionHash", b256_of)?,
        from: req(v, "from", address_of)?,
        to: opt(v, "to", address_of)?,
        contract_address: opt(v, "contractAddress", address_of)?,
        status: req(v, "status", u64_of)?,
        logs,
    })
}

fn tx_of(v: &Json) -> Result<Tx, String> {
    Ok(Tx {
        hash: req(v, "hash", b256_of)?,
        block_hash: opt(v, "blockHash", b256_of)?,
        block_number: opt(v, "blockNumber", u64_of)?,
        transaction_index: opt(v, "transactionIndex", u64_of)?,
        from: req(v, "from", address_of)?,
        to: opt(v, "to", address_of)?,
        input: req(v, "input", bytes_of)?,
        value: req(v, "value", u256_of)?,
    })
}

fn full_block_of(v: &Json) -> Result<FullBlock, String> {
    let number = req(v, "number", u64_of)?;
    let hash = req(v, "hash", b256_of)?;
    let transactions = field(v, "transactions")?
        .as_array()
        .ok_or("transactions are not a list")?
        .iter()
        .map(|t| {
            if !t.is_object() {
                return Err("a hashes-only block is not a full block".to_string());
            }
            let tx = tx_of(t)?;
            if tx.block_hash != Some(hash) || tx.block_number != Some(number) {
                return Err(format!("transaction {} is not of this block", tx.hash));
            }
            Ok(tx)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FullBlock {
        number,
        hash,
        transactions,
    })
}

/// A batch answer: one result per id `0..n`, whatever order it came in.
fn batch_results(method: &str, v: &Json, n: usize) -> Result<Vec<Json>, String> {
    let list = v
        .as_array()
        .ok_or_else(|| format!("{method} batch: the answer is not a list"))?;
    if list.len() != n {
        return Err(format!(
            "{method} batch: {} answers for {n} requests",
            list.len()
        ));
    }
    let mut out = vec![Json::Null; n];
    let mut seen = vec![false; n];
    for item in list {
        let id = item
            .get("id")
            .and_then(Json::as_u64)
            .and_then(|i| usize::try_from(i).ok())
            .filter(|i| *i < n)
            .ok_or_else(|| format!("{method} batch: missing or foreign id"))?;
        if seen[id] {
            return Err(format!("{method} batch: duplicate id {id}"));
        }
        seen[id] = true;
        out[id] = answer(method, item).map_err(|f| f.text)?;
    }
    Ok(out)
}

impl Endpoint {
    pub fn new(url: &str, limits: Limits, attempts: u32, retry_pause: Duration) -> Self {
        Self {
            url: url.to_string(),
            limits,
            attempts: attempts.max(1),
            retry_pause,
        }
    }

    /// One POST, retried on transport failure only (a JSON-RPC answer,
    /// error or not, is final).
    fn post(&self, body: &str) -> Result<Vec<u8>, String> {
        let mut last = String::new();
        for attempt in 1..=self.attempts {
            let mut curl = Command::new("curl");
            curl.env_clear();
            if let Some(path) = env::var_os("PATH") {
                curl.env("PATH", path);
            }
            match rpc_wire::post(curl, &self.url, body, self.limits) {
                Ok(bytes) => return Ok(bytes),
                Err(e) => last = e,
            }
            if attempt < self.attempts {
                thread::sleep(self.retry_pause);
            }
        }
        Err(format!("{} attempts: {last}", self.attempts))
    }

    fn call(&self, method: &str, params: &str) -> Result<Json, Failure> {
        let bytes = self
            .post(&body(1, method, params))
            .map_err(|text| Failure { code: None, text })?;
        response(method, &bytes)
    }

    fn value(&self, method: &str, params: &str) -> Result<Json, String> {
        self.call(method, params).map_err(|f| f.text)
    }

    pub fn chain_id(&self) -> Result<u64, String> {
        u64_of(&self.value("eth_chainId", "[]")?, "eth_chainId")
    }

    pub fn block_number(&self) -> Result<u64, String> {
        u64_of(&self.value("eth_blockNumber", "[]")?, "eth_blockNumber")
    }

    /// The canonical header at `number` (`eth_getBlockByNumber`, hashes only).
    pub fn header(&self, number: u64) -> Result<Header, String> {
        let v = self.value(
            "eth_getBlockByNumber",
            &format!("[{},false]", quantity(number)),
        )?;
        if v.is_null() {
            return Err(format!("block {number} unknown to the endpoint"));
        }
        let header = header_of(&v).map_err(|e| format!("block {number}: {e}"))?;
        pin_of(number, &header)?;
        Ok(header)
    }

    /// The headers `from..=to` in one batch, each for exactly its height.
    pub fn headers(&self, from: u64, to: u64) -> Result<Vec<Header>, String> {
        let n = usize::try_from(to - from + 1).map_err(|_| "header range too large")?;
        let requests: Vec<String> = (0..n as u64)
            .map(|i| {
                body(
                    i,
                    "eth_getHeaderByNumber",
                    &format!("[{}]", quantity(from + i)),
                )
            })
            .collect();
        let bytes = self.post(&format!("[{}]", requests.join(",")))?;
        let v: Json = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid eth_getHeaderByNumber batch: {e}"))?;
        batch_results("eth_getHeaderByNumber", &v, n)?
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let number = from + i as u64;
                if h.is_null() {
                    return Err(format!("header {number} unknown to the endpoint"));
                }
                let header = header_of(h).map_err(|e| format!("header {number}: {e}"))?;
                pin_of(number, &header)?;
                Ok(header)
            })
            .collect()
    }

    /// The blocks `from..=to` with their full transactions in one batch;
    /// each must be the answer for its height and carry its transactions.
    pub fn full_blocks(&self, from: u64, to: u64) -> Result<Vec<FullBlock>, String> {
        let n = usize::try_from(to - from + 1).map_err(|_| "block range too large")?;
        let requests: Vec<String> = (0..n as u64)
            .map(|i| {
                body(
                    i,
                    "eth_getBlockByNumber",
                    &format!("[{},true]", quantity(from + i)),
                )
            })
            .collect();
        let bytes = self.post(&format!("[{}]", requests.join(",")))?;
        let v: Json = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid eth_getBlockByNumber batch: {e}"))?;
        batch_results("eth_getBlockByNumber", &v, n)?
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let number = from + i as u64;
                if b.is_null() {
                    return Err(format!("block {number} unknown to the endpoint"));
                }
                let block = full_block_of(b).map_err(|e| format!("block {number}: {e}"))?;
                if block.number != number {
                    return Err(format!(
                        "unpinned answer: asked for block {number}, got {}",
                        block.number
                    ));
                }
                Ok(block)
            })
            .collect()
    }

    /// Every receipt of exactly the block of `header` (by hash).
    pub fn block_receipts(&self, header: &Header) -> Result<Vec<Receipt>, String> {
        let v = self.value(
            "eth_getBlockReceipts",
            &format!("[{}]", data(header.hash.as_slice())),
        )?;
        let receipts = v
            .as_array()
            .ok_or_else(|| format!("receipts of block {} are not a list", header.number))?
            .iter()
            .map(receipt_of)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("receipts of block {}: {e}", header.number))?;
        require_receipts_of(header, &receipts)?;
        Ok(receipts)
    }

    pub fn transaction(&self, hash: B256) -> Result<Tx, String> {
        let v = self.value(
            "eth_getTransactionByHash",
            &format!("[{}]", data(hash.as_slice())),
        )?;
        if v.is_null() {
            return Err(format!("transaction {hash} unknown to the endpoint"));
        }
        let tx = tx_of(&v).map_err(|e| format!("transaction {hash}: {e}"))?;
        if tx.hash != hash {
            return Err(format!("asked for transaction {hash}, got {}", tx.hash));
        }
        Ok(tx)
    }

    pub fn receipt(&self, hash: B256) -> Result<Receipt, String> {
        let v = self.value(
            "eth_getTransactionReceipt",
            &format!("[{}]", data(hash.as_slice())),
        )?;
        if v.is_null() {
            return Err(format!("receipt {hash} unknown to the endpoint"));
        }
        let r = receipt_of(&v).map_err(|e| format!("receipt {hash}: {e}"))?;
        if r.transaction_hash != hash {
            return Err(format!(
                "asked for receipt {hash}, got {}",
                r.transaction_hash
            ));
        }
        Ok(r)
    }

    pub fn code(&self, address: Address, pin: &Pin) -> Result<Vec<u8>, String> {
        bytes_of(
            &self.value("eth_getCode", &code_params(address, pin))?,
            "eth_getCode",
        )
    }

    pub fn storage(&self, address: Address, slot: B256, pin: &Pin) -> Result<B256, String> {
        b256_of(
            &self.value("eth_getStorageAt", &storage_params(address, slot, pin))?,
            "eth_getStorageAt",
        )
    }

    /// `eth_call` at the pin; a revert is an outcome, every other error fails.
    pub fn call_at(&self, to: Address, input: &[u8], pin: &Pin) -> Result<CallOutcome, String> {
        match self.call("eth_call", &call_params(to, input, pin)) {
            Ok(v) => bytes_of(&v, "eth_call").map(CallOutcome::Returned),
            Err(f) if f.code == Some(3) || f.text.contains("revert") => {
                Ok(CallOutcome::Reverted(f.text))
            }
            Err(f) => Err(f.text),
        }
    }

    pub fn host(&self) -> String {
        host(&self.url)
    }

    pub fn estimate_gas(
        &self,
        from: Address,
        to: Address,
        value: U256,
        input: &[u8],
        pin: &Pin,
    ) -> Result<u64, String> {
        u64_of(
            &self.value(
                "eth_estimateGas",
                &estimate_params(from, to, value, input, pin),
            )?,
            "eth_estimateGas",
        )
    }

    /// The node's execution of `calls`, in order, in one simulated block on
    /// top of the pinned block; nothing is signed, sent or persisted.
    pub fn simulate(
        &self,
        pin: &Pin,
        balances: &[(Address, U256)],
        storage: &[StorageOverride],
        calls: &[SimCall],
    ) -> Result<Vec<SimResult>, String> {
        let v = self.value(
            "eth_simulateV1",
            &simulate_params(pin, balances, storage, calls),
        )?;
        let block = v
            .as_array()
            .and_then(|b| b.first())
            .ok_or("eth_simulateV1 returned no block")?;
        let results = field(block, "calls")?
            .as_array()
            .ok_or("simulated calls are not a list")?
            .iter()
            .map(sim_result_of)
            .collect::<Result<Vec<_>, _>>()?;
        if results.len() != calls.len() {
            return Err(format!(
                "eth_simulateV1 answered {} results for {} calls",
                results.len(),
                calls.len()
            ));
        }
        Ok(results)
    }
}

/// The host of a URL: what a record names (never a path or query).
pub fn host(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .to_string()
}

// ---- Sourcify (read-only GET) -------------------------------------------------

/// What a Sourcify v2 contract lookup says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourcifyFacts {
    /// `EXACT_MATCH`, `MATCH` or `NO_MATCH`.
    pub status: String,
    pub creation_match: Option<String>,
    pub runtime_match: Option<String>,
    pub contract_name: Option<String>,
    pub compiler_version: Option<String>,
    pub proxy_implementations: Vec<Address>,
    pub deployment_tx: Option<B256>,
    pub deployment_block: Option<u64>,
    pub deployer: Option<Address>,
    pub abi_signatures: Vec<String>,
}

/// The fields the freeze asks Sourcify for (no sources).
pub const SOURCIFY_FIELDS: &str =
    "abi,compilation.name,compilation.compilerVersion,proxyResolution,deployment";

pub fn sourcify_url(base: &str, chain_id: u64, address: Address) -> String {
    format!(
        "{}/v2/contract/{chain_id}/0x{}?fields={SOURCIFY_FIELDS}",
        base.trim_end_matches('/'),
        hex::encode(address)
    )
}

/// The fields of a storage-layout lookup (no sources).
pub const SOURCIFY_LAYOUT_FIELDS: &str = "storageLayout,compilation.name";

pub fn sourcify_layout_url(base: &str, chain_id: u64, address: Address) -> String {
    format!(
        "{}/v2/contract/{chain_id}/0x{}?fields={SOURCIFY_LAYOUT_FIELDS}",
        base.trim_end_matches('/'),
        hex::encode(address)
    )
}

/// One state variable of a Sourcify storage layout, with its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutVariable {
    pub label: String,
    pub slot: u64,
    pub offset: u64,
    /// The compiler's type id (`t_mapping(t_address,t_bool)`).
    pub type_id: String,
    /// The type's label (`mapping(address => bool)`).
    pub type_label: String,
    /// `inplace`, `mapping`, `dynamic_array` or `bytes`.
    pub encoding: String,
}

/// The facts of a storage-layout lookup: the match and the variable named
/// `label` (`None` when the layout has no such variable).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcifyLayout {
    /// `EXACT_MATCH`, `MATCH` or `NO_MATCH`.
    pub status: String,
    pub runtime_match: Option<String>,
    pub contract_name: Option<String>,
    pub variable: Option<LayoutVariable>,
}

/// The variable `label` of a Sourcify storage-layout answer. A label that
/// occurs more than once (an inherited shadow) is refused, never chosen.
pub fn sourcify_layout(raw: &[u8], label: &str) -> Result<SourcifyLayout, String> {
    let v: Json =
        serde_json::from_slice(raw).map_err(|e| format!("Sourcify answer is not JSON: {e}"))?;
    let status = match present(&v, "match").and_then(Json::as_str) {
        Some("exact_match") => "EXACT_MATCH",
        Some("match") => "MATCH",
        None => "NO_MATCH",
        Some(other) => return Err(format!("unknown Sourcify match {other:?}")),
    };
    let mut layout = SourcifyLayout {
        status: status.into(),
        runtime_match: text(&v, &["runtimeMatch"]),
        contract_name: text(&v, &["compilation", "name"]),
        variable: None,
    };
    let Some(storage) = present(&v, "storageLayout") else {
        return Ok(layout);
    };
    let entries = field(storage, "storage")?
        .as_array()
        .ok_or("storageLayout.storage is not a list")?;
    let mut found = entries
        .iter()
        .filter(|e| e.get("label").and_then(Json::as_str) == Some(label));
    let Some(entry) = found.next() else {
        return Ok(layout);
    };
    if found.next().is_some() {
        return Err(format!("storage layout names {label} more than once"));
    }
    let decimal = |name: &str| -> Result<u64, String> {
        field(entry, name)?
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| entry.get(name).and_then(Json::as_u64))
            .ok_or_else(|| format!("{label}.{name} is not a decimal number"))
    };
    let type_id = field(entry, "type")?
        .as_str()
        .ok_or_else(|| format!("{label}.type is not text"))?
        .to_string();
    let types = field(storage, "types")?;
    let ty = field(types, &type_id)?;
    layout.variable = Some(LayoutVariable {
        label: label.to_string(),
        slot: decimal("slot")?,
        offset: decimal("offset")?,
        type_label: text(ty, &["label"]).ok_or_else(|| format!("{type_id} has no label"))?,
        encoding: text(ty, &["encoding"]).ok_or_else(|| format!("{type_id} has no encoding"))?,
        type_id,
    });
    Ok(layout)
}

/// One GET through curl, the URL on stdin (`-K -`), never on argv; at most
/// `max_bytes` of body. Returns the HTTP status and the body.
pub fn get(url: &str, limits: Limits, max_bytes: u64) -> Result<(u16, Vec<u8>), String> {
    if url.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        return Err("URL contains a control character or quote".into());
    }
    let mut curl = Command::new("curl");
    curl.env_clear();
    if let Some(path) = env::var_os("PATH") {
        curl.env("PATH", path);
    }
    curl.arg("-q")
        .arg("-sS")
        .args(["--connect-timeout", &limits.connect_timeout_s.to_string()])
        .args(["--max-time", &limits.max_time_s.to_string()])
        .args(["--max-filesize", &max_bytes.to_string()])
        .args(["-w", "\n%{http_code}"])
        .args(["-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = curl.spawn().map_err(|e| format!("execute curl: {e}"))?;
    let config = format!("url = \"{url}\"\n");
    let written = child
        .stdin
        .take()
        .ok_or_else(|| "curl stdin unavailable".to_string())
        .and_then(|mut stdin| {
            stdin
                .write_all(config.as_bytes())
                .map_err(|e| e.to_string())
        });
    let out = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
    written?;
    if !out.status.success() {
        return Err(format!(
            "GET failed (curl exit {:?}): {}",
            out.status.code(),
            bounded(
                &rpc_wire::redact(url, &String::from_utf8_lossy(&out.stderr)),
                200
            )
        ));
    }
    split_status(&out.stdout)
}

/// The body and the status curl's `-w "\n%{http_code}"` appended.
fn split_status(out: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let at = out
        .iter()
        .rposition(|b| *b == b'\n')
        .ok_or("no HTTP status after the body")?;
    let status = std::str::from_utf8(&out[at + 1..])
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .ok_or("no HTTP status after the body")?;
    Ok((status, out[..at].to_vec()))
}

fn text(v: &Json, path: &[&str]) -> Option<String> {
    let mut at = v;
    for key in path {
        at = present(at, key)?;
    }
    at.as_str().map(str::to_string)
}

/// The facts of a Sourcify v2 answer.
pub fn sourcify_facts(raw: &[u8]) -> Result<SourcifyFacts, String> {
    let v: Json =
        serde_json::from_slice(raw).map_err(|e| format!("Sourcify answer is not JSON: {e}"))?;
    let status = match present(&v, "match").and_then(Json::as_str) {
        Some("exact_match") => "EXACT_MATCH",
        Some("match") => "MATCH",
        None => "NO_MATCH",
        Some(other) => return Err(format!("unknown Sourcify match {other:?}")),
    };
    let mut facts = SourcifyFacts {
        status: status.into(),
        creation_match: text(&v, &["creationMatch"]),
        runtime_match: text(&v, &["runtimeMatch"]),
        contract_name: text(&v, &["compilation", "name"]),
        compiler_version: text(&v, &["compilation", "compilerVersion"]),
        ..SourcifyFacts::default()
    };
    if let Some(list) = v
        .get("proxyResolution")
        .and_then(|p| p.get("implementations"))
        .and_then(Json::as_array)
    {
        for i in list {
            facts
                .proxy_implementations
                .push(req(i, "address", address_of)?);
        }
    }
    if let Some(d) = present(&v, "deployment") {
        facts.deployment_tx = opt(d, "transactionHash", b256_of)?;
        facts.deployment_block = present(d, "blockNumber")
            .and_then(Json::as_str)
            .map(|s| {
                s.parse::<u64>()
                    .map_err(|_| "deployment.blockNumber is not a number")
            })
            .transpose()?;
        facts.deployer = opt(d, "deployer", address_of)?;
    }
    if let Some(abi) = present(&v, "abi") {
        facts.abi_signatures = abi_entries_of(abi)?
            .into_iter()
            .map(|e| e.signature)
            .collect();
    }
    Ok(facts)
}

// ---- ABI JSON ---------------------------------------------------------------------

/// A function or event of a JSON ABI with its canonical signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbiEntry {
    pub event: bool,
    pub name: String,
    pub signature: String,
    pub mutability: Option<String>,
}

fn canonical_type(input: &Json) -> Result<String, String> {
    let ty = input
        .get("type")
        .and_then(Json::as_str)
        .ok_or("ABI parameter without type")?;
    match ty.strip_prefix("tuple") {
        Some(suffix) => {
            let parts = input
                .get("components")
                .and_then(Json::as_array)
                .ok_or("tuple without components")?
                .iter()
                .map(canonical_type)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("({}){suffix}", parts.join(",")))
        }
        None => Ok(ty.to_string()),
    }
}

fn abi_entries_of(abi: &Json) -> Result<Vec<AbiEntry>, String> {
    let mut out = Vec::new();
    for e in abi.as_array().ok_or("an ABI is a list")? {
        let kind = e.get("type").and_then(Json::as_str).unwrap_or("function");
        if kind != "function" && kind != "event" {
            continue;
        }
        let name = e
            .get("name")
            .and_then(Json::as_str)
            .ok_or("ABI entry without name")?;
        let inputs = e
            .get("inputs")
            .and_then(Json::as_array)
            .map(|list| {
                list.iter()
                    .map(canonical_type)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        out.push(AbiEntry {
            event: kind == "event",
            name: name.to_string(),
            signature: format!("{name}({})", inputs.join(",")),
            mutability: e
                .get("stateMutability")
                .and_then(Json::as_str)
                .map(str::to_string),
        });
    }
    Ok(out)
}

/// The entries of an archived ABI file.
pub fn abi_entries(bytes: &[u8]) -> Result<Vec<AbiEntry>, String> {
    let v: Json = serde_json::from_slice(bytes).map_err(|e| format!("ABI file: {e}"))?;
    abi_entries_of(&v)
}

// ---- the archived 2026-09-27 evidence -------------------------------------------

/// One line of the archived `web3_sha3` comparison: label and the
/// endpoint's hash.
pub fn keccak_reference(bytes: &[u8]) -> Result<Vec<(String, B256)>, String> {
    let v: Json = serde_json::from_slice(bytes).map_err(|e| format!("keccak reference: {e}"))?;
    v.as_array()
        .ok_or("keccak reference is a list")?
        .iter()
        .map(|e| {
            let label = e
                .get("label")
                .and_then(Json::as_str)
                .ok_or("reference entry without label")?;
            Ok((label.to_string(), req(e, "reference_hash", b256_of)?))
        })
        .collect()
}

/// The archived Classic launch: block number/hash and the transaction.
pub fn archived_transaction(bytes: &[u8]) -> Result<(u64, B256, Tx), String> {
    let v: Json = serde_json::from_slice(bytes).map_err(|e| format!("archived tx: {e}"))?;
    let number = v
        .get("block_number")
        .and_then(Json::as_u64)
        .ok_or("archived block_number missing")?;
    let hash = req(&v, "block_hash", b256_of)?;
    Ok((number, hash, tx_of(field(&v, "tx")?)?))
}

pub fn archived_receipt(bytes: &[u8]) -> Result<Receipt, String> {
    let v: Json = serde_json::from_slice(bytes).map_err(|e| format!("archived receipt: {e}"))?;
    receipt_of(v.get("result").unwrap_or(&v))
}

/// `contracts.four.implementation_address` of the archived summary.
pub fn archived_classic_implementation(bytes: &[u8]) -> Result<Address, String> {
    let v: Json = serde_json::from_slice(bytes).map_err(|e| format!("archived summary: {e}"))?;
    let four = v
        .get("contracts")
        .and_then(|c| c.get("four"))
        .ok_or("contracts.four missing")?;
    req(four, "implementation_address", address_of)
}

/// (proxy, implementation) pairs of the archived EIP-1967 slot batch.
pub fn archived_slot_reads(
    request: &[u8],
    response: &[u8],
) -> Result<Vec<(Address, Address)>, String> {
    let req_v: Json = serde_json::from_slice(request).map_err(|e| format!("slot request: {e}"))?;
    let resp_v: Json =
        serde_json::from_slice(response).map_err(|e| format!("slot response: {e}"))?;
    let mut out = Vec::new();
    for r in req_v.as_array().ok_or("slot request is a list")? {
        let id = r.get("id").and_then(Json::as_u64).ok_or("request id")?;
        let proxy = address_of(
            r.get("params")
                .and_then(|p| p.get(0))
                .ok_or("request params")?,
            "proxy",
        )?;
        let answer = resp_v
            .as_array()
            .ok_or("slot response is a list")?
            .iter()
            .find(|a| a.get("id").and_then(Json::as_u64) == Some(id))
            .ok_or("no answer for a request")?;
        let word = req(answer, "result", b256_of)?;
        out.push((proxy, Address::from_slice(&word[12..])));
    }
    Ok(out)
}

/// The archived decoded Flap launch (`flap_v6_fixture_decoded.json`):
/// transaction hash, `newTokenV6` salt and `tokenVersion`.
pub fn archived_flap_launch(bytes: &[u8]) -> Result<(B256, B256, u64), String> {
    let v: Json = serde_json::from_slice(bytes).map_err(|e| format!("archived launch: {e}"))?;
    let params = field(&v, "params")?;
    let version = params
        .get("tokenVersion")
        .and_then(Json::as_u64)
        .ok_or("archived tokenVersion missing")?;
    Ok((
        req(&v, "txHash", b256_of)?,
        req(params, "salt", b256_of)?,
        version,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(byte: &str) -> String {
        format!("0x{}", byte.repeat(32))
    }

    fn header_json(number: u64, hash: &str) -> Json {
        serde_json::from_str(&format!(
            r#"{{"number":"0x{number:x}","hash":"{}","parentHash":"{}","timestamp":"0x64","logsBloom":"0x{}"}}"#,
            h(hash),
            h("99"),
            "00".repeat(256)
        ))
        .unwrap()
    }

    #[test]
    fn a_wrong_chain_is_refused() {
        assert!(require_bsc(56).is_ok());
        assert_eq!(
            require_bsc(1).unwrap_err(),
            "wrong chain: eth_chainId 1, not 56"
        );
        let v = response("eth_chainId", br#"{"jsonrpc":"2.0","id":1,"result":"0x1"}"#).unwrap();
        assert!(require_bsc(u64_of(&v, "eth_chainId").unwrap()).is_err());
    }

    #[test]
    fn an_unpinned_answer_is_refused() {
        // A header answered for another height (e.g. `latest` substituted).
        let header = header_of(&header_json(11, "aa")).unwrap();
        assert!(pin_of(11, &header).is_ok());
        assert!(
            pin_of(10, &header)
                .unwrap_err()
                .starts_with("unpinned answer")
        );
        // A pending block has no hash: refused at decoding.
        let mut pending = header_json(11, "aa");
        pending["hash"] = Json::Null;
        assert!(header_of(&pending).is_err());
        // A batch with a missing or foreign id is refused.
        let batch: Json = serde_json::from_str(&format!(
            r#"[{{"jsonrpc":"2.0","id":0,"result":{}}},{{"jsonrpc":"2.0","id":7,"result":{}}}]"#,
            header_json(10, "aa"),
            header_json(11, "bb")
        ))
        .unwrap();
        assert!(batch_results("eth_getHeaderByNumber", &batch, 2).is_err());
        // Receipts of another block are refused.
        let receipt = Receipt {
            block_number: 11,
            block_hash: B256::repeat_byte(0xbb),
            transaction_index: 0,
            transaction_hash: B256::repeat_byte(1),
            from: Address::ZERO,
            to: None,
            contract_address: None,
            status: 1,
            logs: vec![],
        };
        assert!(require_receipts_of(&header, std::slice::from_ref(&receipt)).is_err());
        let mut same = receipt.clone();
        same.block_hash = header.hash;
        assert!(require_receipts_of(&header, std::slice::from_ref(&same)).is_ok());
        assert!(require_receipts_of(&header, &[same.clone(), same.clone()]).is_err());
        // A transaction must be mined where its receipt says.
        let tx = Tx {
            hash: same.transaction_hash,
            block_hash: Some(header.hash),
            block_number: Some(11),
            transaction_index: Some(1),
            from: Address::ZERO,
            to: None,
            input: vec![],
            value: U256::ZERO,
        };
        assert!(require_tx_at(&tx, &same).is_err());
    }

    #[test]
    fn every_state_read_names_one_canonical_block_by_hash() {
        let pin = Pin {
            number: 5,
            hash: B256::repeat_byte(0xab),
        };
        let tag = format!(r#"{{"blockHash":"{}","requireCanonical":true}}"#, h("ab"));
        for params in [
            code_params(Address::repeat_byte(1), &pin),
            storage_params(Address::repeat_byte(1), B256::repeat_byte(2), &pin),
            call_params(Address::repeat_byte(2), &[0x70, 0xa0], &pin),
        ] {
            assert!(params.ends_with(&format!("{tag}]")), "{params}");
            assert!(
                !params.contains("latest") && !params.contains("pending"),
                "{params}"
            );
            let parsed: Json = serde_json::from_str(&params).unwrap();
            assert!(parsed.as_array().unwrap().last().unwrap()["requireCanonical"] == true);
        }
    }

    #[test]
    fn a_revert_is_an_outcome_and_a_missing_trie_node_an_error() {
        let revert = response(
            "eth_call",
            br#"{"jsonrpc":"2.0","id":1,"error":{"code":3,"message":"execution reverted"}}"#,
        )
        .unwrap_err();
        assert_eq!(revert.code, Some(3));
        let missing = response(
            "eth_call",
            br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"missing trie node"}}"#,
        )
        .unwrap_err();
        assert!(missing.text.contains("missing trie node") && !missing.text.contains("revert"));
    }

    #[test]
    fn sourcify_answers_decode_or_fail() {
        let none = sourcify_facts(br#"{"match":null,"creationMatch":null,"runtimeMatch":null,"chainId":"56","address":"0xF32eE25A7A94F5858692B90815493bDA9f992eee"}"#).unwrap();
        assert_eq!(none.status, "NO_MATCH");
        let proxy = sourcify_facts(br#"{"compilation":{"name":"ERC1967Proxy","compilerVersion":"0.8.24"},"proxyResolution":{"isProxy":true,"implementations":[{"address":"0x02d714Cb43d1544Abe1fFc2405ec00b7235B7F26"}]},"deployment":{"transactionHash":"0xe90902be9d0b415e1f6029f89b845a63b5a14bb6c3700b75a133579ef09fd437","blockNumber":"102418465","deployer":"0x8558fE0C40cCad66f7e6c2900BF4D8b5c96CbD19"},"abi":[{"type":"event","name":"Upgraded","inputs":[{"name":"implementation","type":"address","indexed":true}]}],"match":"exact_match"}"#).unwrap();
        assert_eq!(proxy.status, "EXACT_MATCH");
        assert_eq!(proxy.deployment_block, Some(102418465));
        assert_eq!(proxy.abi_signatures, ["Upgraded(address)"]);
        assert_eq!(proxy.proxy_implementations.len(), 1);
        assert!(sourcify_facts(b"<html>").is_err());
        assert!(sourcify_facts(br#"{"match":"maybe"}"#).is_err());
    }

    #[test]
    fn tuple_signatures_are_canonical() {
        let entries = abi_entries(br#"[{"type":"function","name":"f","stateMutability":"view","inputs":[{"type":"tuple[]","components":[{"type":"address"},{"type":"tuple","components":[{"type":"uint256"},{"type":"bytes"}]}]},{"type":"uint8"}]},{"type":"constructor","inputs":[]}]"#).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].signature, "f((address,(uint256,bytes))[],uint8)");
        assert_eq!(entries[0].mutability.as_deref(), Some("view"));
    }

    #[test]
    fn a_full_block_carries_only_its_own_transactions() {
        let block = |tx_block: &str| -> Json {
            serde_json::from_str(&format!(
                r#"{{"number":"0x10","hash":"{}","transactions":[{{"hash":"{}","blockHash":"{}","blockNumber":"0x10","transactionIndex":"0x0","from":"{}","to":null,"input":"0x8cb5772c","value":"0x0"}}]}}"#,
                h("aa"),
                h("bb"),
                h(tx_block),
                format_args!("0x{}", "11".repeat(20))
            ))
            .unwrap()
        };
        let ok = full_block_of(&block("aa")).unwrap();
        assert_eq!((ok.number, ok.transactions.len()), (16, 1));
        assert!(full_block_of(&block("cc")).is_err());
        let hashes_only: Json = serde_json::from_str(&format!(
            r#"{{"number":"0x10","hash":"{}","transactions":["{}"]}}"#,
            h("aa"),
            h("bb")
        ))
        .unwrap();
        assert!(full_block_of(&hashes_only).is_err());
    }

    #[test]
    fn simulation_and_estimates_are_pinned_and_unsigned() {
        let pin = Pin {
            number: 5,
            hash: B256::repeat_byte(0xab),
        };
        let call = SimCall {
            from: Address::repeat_byte(1),
            to: Address::repeat_byte(2),
            value: U256::from(1000u64),
            data: vec![0x8c, 0xb5],
            gas: 21_000,
        };
        let p: Json = serde_json::from_str(&simulate_params(
            &pin,
            &[(Address::repeat_byte(1), U256::from(5u8))],
            &[(
                Address::repeat_byte(2),
                B256::repeat_byte(0x0c),
                B256::with_last_byte(1),
            )],
            std::slice::from_ref(&call),
        ))
        .unwrap();
        assert_eq!(p[1]["blockHash"], h("ab"));
        assert_eq!(p[0]["validation"], false);
        let c = &p[0]["blockStateCalls"][0]["calls"][0];
        assert_eq!(c["value"], "0x3e8");
        assert_eq!(c["gas"], "0x5208");
        assert!(c.get("signature").is_none() && c.get("r").is_none());
        assert_eq!(
            p[0]["blockStateCalls"][0]["stateOverrides"][format!("0x{}", "01".repeat(20))]["balance"],
            "0x5"
        );
        // A storage override is a stateDiff word of its account.
        assert_eq!(
            p[0]["blockStateCalls"][0]["stateOverrides"][format!("0x{}", "02".repeat(20))]["stateDiff"]
                [format!("0x{}", "0c".repeat(32))],
            format!("0x{}01", "00".repeat(31))
        );
        let e: Json = serde_json::from_str(&estimate_params(
            call.from, call.to, call.value, &call.data, &pin,
        ))
        .unwrap();
        assert_eq!(e[1]["requireCanonical"], true);
        let r = sim_result_of(
            &serde_json::from_str(r#"{"status":"0x0","returnData":"0x","gasUsed":"0x10","logs":[],"error":{"message":"execution reverted"}}"#)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            (r.status, r.error.as_deref()),
            (0, Some("execution reverted"))
        );
    }

    /// A storage-layout answer gives the named variable with its type;
    /// a missing variable stays absent, a shadowed one is refused.
    #[test]
    fn a_storage_layout_names_its_variable_or_nothing() {
        let raw = br#"{"match":"match","runtimeMatch":"match","compilation":{"name":"Portal"},
            "storageLayout":{"storage":[
                {"label":"_blockedSpammers","slot":"176","offset":0,"type":"t_m1"},
                {"label":"_feeExemptedTraders","slot":"177","offset":0,"type":"t_mapping(t_address,t_bool)"}],
             "types":{"t_mapping(t_address,t_bool)":{"label":"mapping(address => bool)","encoding":"mapping","key":"t_address","value":"t_bool","numberOfBytes":"32"},
                      "t_m1":{"label":"x","encoding":"mapping"}}}}"#;
        let l = sourcify_layout(raw, "_feeExemptedTraders").unwrap();
        assert_eq!(
            (l.status.as_str(), l.contract_name.as_deref()),
            ("MATCH", Some("Portal"))
        );
        let v = l.variable.unwrap();
        assert_eq!((v.slot, v.offset), (177, 0));
        assert_eq!(v.type_label, "mapping(address => bool)");
        assert_eq!(v.encoding, "mapping");
        assert_eq!(sourcify_layout(raw, "_absent").unwrap().variable, None);
        let shadowed = String::from_utf8(raw.to_vec())
            .unwrap()
            .replace("_blockedSpammers", "_feeExemptedTraders");
        assert!(sourcify_layout(shadowed.as_bytes(), "_feeExemptedTraders").is_err());
        let unverified = sourcify_layout(br#"{"match":null}"#, "_feeExemptedTraders").unwrap();
        assert_eq!(
            (unverified.status.as_str(), unverified.variable),
            ("NO_MATCH", None)
        );
        assert!(sourcify_layout_url("https://s/", 56, Address::ZERO)
            .ends_with("/v2/contract/56/0x0000000000000000000000000000000000000000?fields=storageLayout,compilation.name"));
    }

    #[test]
    fn a_get_answer_splits_into_body_and_status() {
        assert_eq!(
            split_status(b"{\"match\":null}\n404").unwrap(),
            (404, b"{\"match\":null}".to_vec())
        );
        assert!(split_status(b"{}").is_err());
    }

    #[test]
    fn hosts_never_carry_paths_queries_or_credentials() {
        assert_eq!(
            host("https://bsc-dataseed.bnbchain.org"),
            "bsc-dataseed.bnbchain.org"
        );
        assert_eq!(
            host("https://user:pw@rpc.example.org/v1/KEY?x=1"),
            "rpc.example.org"
        );
    }
}
