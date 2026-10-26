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
