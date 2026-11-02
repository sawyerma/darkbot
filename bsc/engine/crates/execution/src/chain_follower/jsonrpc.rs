//! The JSON-RPC wire boundary. A response body goes in, typed values come
//! out; nothing past this file sees JSON.
//!
//! Hex follows the JSON-RPC encoding (EIP-1474) as BSC nodes write it, in
//! lowercase: a quantity is `0x` with the shortest non-empty digit string
//! (`0x0` for zero), data is `0x` with two digits per byte, of exactly the
//! field's width where it has one. Anything else is refused.

use alloy_primitives::keccak256;
use darkbot_bsc_tx_encoding::rlp::{encode_bytes, encode_list, encode_u64, encode_uint_be};
use serde_json::{Map, Value};

use super::{Call, Field, Integrity, RequestError};
use crate::wss::{ParsedTx, RateLimitSignal, RejectReason, parse_tx_map};

/// The JSON-RPC error codes that mean rate limiting: EIP-1474 "limit
/// exceeded" and the HTTP status some providers reuse (the codes of the
/// WSS source's `jsonrpc::RATE_LIMIT_CODES`).
const RATE_LIMIT_CODES: [i64; 2] = [-32005, 429];

/// Most topics of one log (`LOG4`).
const MAX_TOPICS: usize = 4;

/// Milliseconds per second: `timestamp_ms = timestamp * 1000 + ms`, and the
/// millisecond part of a verified BSC header is below it.
const MS_PER_S: u64 = 1000;

/// A block header as the follower keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Header {
    pub(super) number: u64,
    pub(super) hash: [u8; 32],
    pub(super) parent_hash: [u8; 32],
    pub(super) timestamp_s: u64,
    /// bsc-geth `Header.MilliTimestamp()`.
    pub(super) block_ms: u64,
    /// R7: what the header commits the block's transactions and receipts to.
    pub(super) transactions_root: [u8; 32],
    pub(super) receipts_root: [u8; 32],
    /// R7 review M1: its base fee (none before London).
    pub(super) base_fee: Option<u128>,
}

/// `eth_getBlockByNumber(n, true)`: the header and the block's
/// transactions in order (the index is the position).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BlockHead {
    pub(super) header: Header,
    pub(super) txs: Vec<BlockTx>,
}

/// One transaction of a block, as its object states it (bound to the
/// block's hash, number and its position).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BlockTx {
    pub(super) hash: [u8; 32],
    pub(super) from: [u8; 20],
    pub(super) nonce: u64,
    /// R7 review M1: what a unit of its gas may cost.
    pub(super) price: Price,
}

/// R7 review M1: what a unit of a transaction's gas costs: a fixed price
/// (legacy, type 1) or a cap and a tip over the block's base fee (2–4);
/// amounts above 128 bits saturate (such a cap caps nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Price {
    Fixed(u128),
    Capped { max_fee: u128, tip: u128 },
}

impl Price {
    /// The price its receipt states under `base_fee`; a capped price has
    /// none without a base fee.
    pub(super) fn effective(self, base_fee: Option<u128>) -> Option<u128> {
        match self {
            Price::Fixed(price) => Some(price),
            Price::Capped { max_fee, tip } => Some(max_fee.min(base_fee?.saturating_add(tip))),
        }
    }
}

/// R7: a transaction's signature over its signing hash, recovered apart
/// (review LOW-4: after the cheaper hash and root checks).
#[derive(Clone, Copy, Debug)]
pub(super) struct TxSignature {
    prehash: [u8; 32],
    r: [u8; 32],
    s: [u8; 32],
    y_parity: bool,
}

impl TxSignature {
    /// The signer it recovers to.
    pub(super) fn signer(&self) -> Result<[u8; 20], RequestError> {
        crate::wss::recover(&self.prehash, &self.r, &self.s, self.y_parity)
            .map_err(|_| RequestError::Integrity(Integrity::TxSigner))
    }
}

/// A block with each transaction object also converted into the WSS
/// verifier's typed fields (verified only where needed: the launches).
#[derive(Debug)]
pub(super) struct FullBlock {
    pub(super) head: BlockHead,
    /// Per transaction, in order: its typed fields, or why the WSS source
    /// cannot rebuild it.
    pub(super) objects: Vec<Result<ParsedTx, RejectReason>>,
}

/// One receipt of `eth_getBlockReceipts`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Receipt {
    pub(super) tx_hash: [u8; 32],
    pub(super) tx_index: u64,
    pub(super) from: [u8; 20],
    pub(super) block_hash: [u8; 32],
    pub(super) block_number: u64,
    /// `status` is `0x1`.
    pub(super) succeeded: bool,
    /// R7: its transaction's type and the block's gas used up to it.
    pub(super) tx_type: u8,
    pub(super) cumulative_gas: u64,
    /// `gasUsed` and `effectiveGasPrice`, when the node states both.
    pub(super) cost: Option<super::TxCost>,
    pub(super) logs: Vec<Log>,
}

/// One log of a receipt (bound to its receipt's block and transaction).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Log {
    pub(super) address: [u8; 20],
    pub(super) topics: Vec<[u8; 32]>,
    pub(super) data: Vec<u8>,
    pub(super) log_index: u64,
}

/// The request body of `call` with `id`.
pub(super) fn request(id: u64, call: Call) -> String {
    match call {
        Call::BlockNumber => {
            format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_blockNumber","params":[]}}"#)
        }
        Call::Block(n) => format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_getBlockByNumber","params":["0x{n:x}",true]}}"#
        ),
        Call::Receipts(n) => format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_getBlockReceipts","params":["0x{n:x}"]}}"#
        ),
        Call::Header(n) => format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_getBlockByNumber","params":["0x{n:x}",false]}}"#
        ),
        Call::Receipt(hash) => format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_getTransactionReceipt","params":["0x{}"]}}"#,
            alloy_primitives::hex::encode(hash)
        ),
        Call::TransactionCount { sender, block_hash } => format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_getTransactionCount","params":["0x{}",{{"blockHash":"0x{}"}}]}}"#,
            alloy_primitives::hex::encode(sender),
            alloy_primitives::hex::encode(block_hash)
        ),
    }
}

/// The answer to `eth_blockNumber` or `eth_getTransactionCount`: one quantity.
pub(crate) fn parse_quantity(body: &[u8], id: u64) -> Result<u64, RequestError> {
    match envelope(body, id)? {
        Value::String(s) => quantity_u64(&s).ok_or(RequestError::Shape(Field::Result)),
        _ => Err(RequestError::Shape(Field::Result)),
    }
}

/// BSC3-056: the answer to `eth_getBlockByNumber(number, false)`, its
/// header; `None` for `null`. The transactions are not read.
pub(super) fn parse_header(
    body: &[u8],
    id: u64,
    number: u64,
) -> Result<Option<Header>, RequestError> {
    let block = match envelope(body, id)? {
        Value::Null => return Ok(None),
        Value::Object(block) => block,
        _ => return Err(RequestError::Shape(Field::Result)),
    };
    let header = header_of(&block)?;
    if header.number != number {
        return Err(RequestError::Shape(Field::Number));
    }
    Ok(Some(header))
}

/// The header fields of a block object; R7: its hash is keccak of its RLP.
fn header_of(block: &Map<String, Value>) -> Result<Header, RequestError> {
    let timestamp_s = quantity(block, "timestamp", Field::Timestamp)?;
    let ms = mix_hash_ms(fixed(block, "mixHash", Field::MixHash)?)
        .ok_or(RequestError::Shape(Field::MixHash))?;
    let header = Header {
        number: quantity(block, "number", Field::Number)?,
        hash: fixed(block, "hash", Field::Hash)?,
        parent_hash: fixed(block, "parentHash", Field::ParentHash)?,
        timestamp_s,
        block_ms: timestamp_s
            .checked_mul(MS_PER_S)
            .and_then(|t| t.checked_add(ms))
            .ok_or(RequestError::Shape(Field::Timestamp))?,
        transactions_root: fixed(block, "transactionsRoot", Field::Header)?,
        receipts_root: fixed(block, "receiptsRoot", Field::Header)?,
        base_fee: match block.get("baseFeePerGas") {
            None | Some(Value::Null) => None,
            Some(_) => Some(wide(block, "baseFeePerGas", Field::Header)?),
        },
    };
    if header_hash(block)? != header.hash {
        // Review LOW-5: a header with a field this build does not know (a
        // fork it does not follow, or a forgery) apart from one that
        // contradicts itself.
        let unknown = block.keys().any(|k| !BLOCK_KEYS.contains(&k.as_str()));
        return Err(RequestError::Integrity(match unknown {
            true => Integrity::UnknownHeaderField,
            false => Integrity::HeaderHash,
        }));
    }
    Ok(header)
}

/// The keys of a BSC block object: its header's RLP fields, then what the
/// node adds.
const BLOCK_KEYS: [&str; 28] = [
    "parentHash",
    "sha3Uncles",
    "miner",
    "stateRoot",
    "transactionsRoot",
    "receiptsRoot",
    "logsBloom",
    "difficulty",
    "number",
    "gasLimit",
    "gasUsed",
    "timestamp",
    "extraData",
    "mixHash",
    "nonce",
    "baseFeePerGas",
    "withdrawalsRoot",
    "blobGasUsed",
    "excessBlobGas",
    "parentBeaconBlockRoot",
    "requestsHash",
    "hash",
    "size",
    "totalDifficulty",
    "transactions",
    "uncles",
    "withdrawals",
    "milliTimestamp",
];

/// R7 (BSC3-056): keccak of the header's RLP, its fields in bsc-geth's
/// order; the optional ones from `baseFeePerGas` on each only after the
/// one before.
pub(super) fn header_hash(block: &Map<String, Value>) -> Result<[u8; 32], RequestError> {
    let f = Field::Header;
    let mut items = vec![
        encode_bytes(&fixed::<32>(block, "parentHash", f)?),
        encode_bytes(&fixed::<32>(block, "sha3Uncles", f)?),
        encode_bytes(&fixed::<20>(block, "miner", f)?),
        encode_bytes(&fixed::<32>(block, "stateRoot", f)?),
        encode_bytes(&fixed::<32>(block, "transactionsRoot", f)?),
        encode_bytes(&fixed::<32>(block, "receiptsRoot", f)?),
        encode_bytes(&fixed::<256>(block, "logsBloom", f)?),
        big(block, "difficulty", f)?,
        big(block, "number", f)?,
        big(block, "gasLimit", f)?,
        big(block, "gasUsed", f)?,
        big(block, "timestamp", f)?,
        encode_bytes(&data_vec(text(block, "extraData", f)?).ok_or(RequestError::Shape(f))?),
        encode_bytes(&fixed::<32>(block, "mixHash", f)?),
        encode_bytes(&fixed::<8>(block, "nonce", f)?),
    ];
    let optional = [
        ("baseFeePerGas", false),
        ("withdrawalsRoot", true),
        ("blobGasUsed", false),
        ("excessBlobGas", false),
        ("parentBeaconBlockRoot", true),
        ("requestsHash", true),
    ];
    let mut ended = false;
    for (name, word) in optional {
        match (block.get(name).is_some_and(|v| !v.is_null()), ended, word) {
            (false, _, _) => ended = true,
            (true, true, _) => return Err(RequestError::Shape(f)),
            (true, false, true) => items.push(encode_bytes(&fixed::<32>(block, name, f)?)),
            (true, false, false) => items.push(big(block, name, f)?),
        }
    }
    Ok(keccak256(encode_list(&items)).0)
}

/// R7 (BSC3-056): a transaction object's signed bytes, rebuilt from its
/// fields (legacy with or without EIP-155, types 1–4), and its signature.
pub(super) fn signed_tx(tx: &Map<String, Value>) -> Result<(Vec<u8>, TxSignature), RequestError> {
    let f = Field::TxFields;
    let tx_type = u8::try_from(quantity(tx, "type", f)?).map_err(|_| RequestError::Shape(f))?;
    let to = match tx.get("to") {
        None | Some(Value::Null) => Vec::new(),
        Some(_) => fixed::<20>(tx, "to", f)?.to_vec(),
    };
    let input = data_vec(text(tx, "input", f)?).ok_or(RequestError::Shape(f))?;
    let tail = [
        big(tx, "gas", f)?,
        encode_bytes(&to),
        big(tx, "value", f)?,
        encode_bytes(&input),
    ];
    let mut fields = match tx_type {
        0 => vec![big(tx, "nonce", f)?, big(tx, "gasPrice", f)?],
        1 => vec![
            big(tx, "chainId", f)?,
            big(tx, "nonce", f)?,
            big(tx, "gasPrice", f)?,
        ],
        2..=4 => vec![
            big(tx, "chainId", f)?,
            big(tx, "nonce", f)?,
            big(tx, "maxPriorityFeePerGas", f)?,
            big(tx, "maxFeePerGas", f)?,
        ],
        t => return Err(RequestError::UnsupportedTxType(t)),
    };
    fields.extend(tail);
    if tx_type > 0 {
        fields.push(access_list(tx)?);
    }
    if tx_type == 3 {
        fields.push(big(tx, "maxFeePerBlobGas", f)?);
        fields.push(words(tx, "blobVersionedHashes")?);
    }
    if tx_type == 4 {
        fields.push(authorizations(tx)?);
    }
    let (r, s) = (word(tx, "r", f)?, word(tx, "s", f)?);
    let (payload, v, y_parity) = match tx_type {
        0 => {
            let v = quantity(tx, "v", f)?;
            match v {
                27 | 28 => (encode_list(&fields), v, v == 28),
                _ => {
                    let offset = v.checked_sub(35).ok_or(RequestError::Shape(f))?;
                    let mut signed = fields.clone();
                    signed.extend([encode_u64(offset / 2), encode_bytes(&[]), encode_bytes(&[])]);
                    (encode_list(&signed), v, offset % 2 == 1)
                }
            }
        }
        t => {
            let y = match (tx.get("yParity"), tx.get("v")) {
                (Some(_), Some(_)) if quantity(tx, "yParity", f)? != quantity(tx, "v", f)? => {
                    return Err(RequestError::Shape(f));
                }
                (Some(_), _) => quantity(tx, "yParity", f)?,
                (None, _) => quantity(tx, "v", f)?,
            };
            if y > 1 {
                return Err(RequestError::Shape(f));
            }
            ([vec![t], encode_list(&fields)].concat(), y, y == 1)
        }
    };
    fields.extend([encode_u64(v), encode_uint_be(&r), encode_uint_be(&s)]);
    let raw = match tx_type {
        0 => encode_list(&fields),
        t => [vec![t], encode_list(&fields)].concat(),
    };
    let signature = TxSignature {
        prehash: keccak256(&payload).0,
        r,
        s,
        y_parity,
    };
    Ok((raw, signature))
}

/// R7 review M1: the [`Price`] of a transaction object of a known type.
fn price_of(tx: &Map<String, Value>) -> Result<Price, RequestError> {
    let f = Field::TxFields;
    match u8::try_from(quantity(tx, "type", f)?).map_err(|_| RequestError::Shape(f))? {
        0 | 1 => Ok(Price::Fixed(wide(tx, "gasPrice", f)?)),
        2..=4 => Ok(Price::Capped {
            max_fee: wide(tx, "maxFeePerGas", f)?,
            tip: wide(tx, "maxPriorityFeePerGas", f)?,
        }),
        t => Err(RequestError::UnsupportedTxType(t)),
    }
}

/// A quantity of at most 256 bits as a `u128`, saturated above it.
fn wide(obj: &Map<String, Value>, name: &str, field: Field) -> Result<u128, RequestError> {
    let be = quantity_be(text(obj, name, field)?).ok_or(RequestError::Shape(field))?;
    Ok(match be.len() {
        0..=16 => be.iter().fold(0u128, |v, b| (v << 8) | u128::from(*b)),
        _ => u128::MAX,
    })
}

/// An access list: `[[address, [storageKey, ...]], ...]`.
fn access_list(tx: &Map<String, Value>) -> Result<Vec<u8>, RequestError> {
    let f = Field::TxFields;
    let Some(Value::Array(entries)) = tx.get("accessList") else {
        return Err(RequestError::Shape(f));
    };
    let mut items = Vec::with_capacity(entries.len());
    for entry in entries {
        let Value::Object(entry) = entry else {
            return Err(RequestError::Shape(f));
        };
        items.push(encode_list(&[
            encode_bytes(&fixed::<20>(entry, "address", f)?),
            words(entry, "storageKeys")?,
        ]));
    }
    Ok(encode_list(&items))
}

/// An EIP-7702 authorization list: `[[chainId, address, nonce, yParity,
/// r, s], ...]`.
fn authorizations(tx: &Map<String, Value>) -> Result<Vec<u8>, RequestError> {
    let f = Field::TxFields;
    let Some(Value::Array(entries)) = tx.get("authorizationList") else {
        return Err(RequestError::Shape(f));
    };
    let mut items = Vec::with_capacity(entries.len());
    for entry in entries {
        let Value::Object(a) = entry else {
            return Err(RequestError::Shape(f));
        };
        items.push(encode_list(&[
            big(a, "chainId", f)?,
            encode_bytes(&fixed::<20>(a, "address", f)?),
            big(a, "nonce", f)?,
            big(a, "yParity", f)?,
            big(a, "r", f)?,
            big(a, "s", f)?,
        ]));
    }
    Ok(encode_list(&items))
}

/// A list of 32-byte data, RLP-encoded.
fn words(obj: &Map<String, Value>, name: &str) -> Result<Vec<u8>, RequestError> {
    let f = Field::TxFields;
    let Some(Value::Array(entries)) = obj.get(name) else {
        return Err(RequestError::Shape(f));
    };
    let items = entries
        .iter()
        .map(|w| {
            w.as_str()
                .and_then(data_fixed::<32>)
                .map(|w| encode_bytes(&w))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(RequestError::Shape(f))?;
    Ok(encode_list(&items))
}

/// A quantity of at most 256 bits, RLP-encoded.
fn big(obj: &Map<String, Value>, name: &str, field: Field) -> Result<Vec<u8>, RequestError> {
    let be = quantity_be(text(obj, name, field)?).ok_or(RequestError::Shape(field))?;
    Ok(encode_uint_be(&be))
}

/// A signature scalar: a quantity of at most 256 bits, as 32 bytes.
fn word(obj: &Map<String, Value>, name: &str, field: Field) -> Result<[u8; 32], RequestError> {
    let be = quantity_be(text(obj, name, field)?).ok_or(RequestError::Shape(field))?;
    let mut out = [0u8; 32];
    out[32 - be.len()..].copy_from_slice(&be);
    Ok(out)
}

/// The answer to `eth_getBlockByNumber(number, true)`; `None` for `null`.
/// Every transaction must be a full object bound to this block at its
/// position.
pub(super) fn parse_block(
    body: &[u8],
    id: u64,
    number: u64,
) -> Result<Option<FullBlock>, RequestError> {
    let block = match envelope(body, id)? {
        Value::Null => return Ok(None),
        Value::Object(block) => block,
        _ => return Err(RequestError::Shape(Field::Result)),
    };
    let header = header_of(&block)?;
    if header.number != number {
        return Err(RequestError::Shape(Field::Number));
    }
    let Some(Value::Array(entries)) = block.get("transactions") else {
        return Err(RequestError::Shape(Field::Transactions));
    };
    let mut txs = Vec::with_capacity(entries.len());
    let mut objects = Vec::with_capacity(entries.len());
    let mut raws = Vec::with_capacity(entries.len());
    let mut signatures = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        // The hash-only form (`false`) is refused here.
        let Value::Object(tx) = entry else {
            return Err(RequestError::Shape(Field::Transactions));
        };
        let bound = quantity(tx, "transactionIndex", Field::TxIndex)? == index as u64
            && fixed::<32>(tx, "blockHash", Field::BlockHash)? == header.hash
            && quantity(tx, "blockNumber", Field::BlockNumber)? == header.number;
        if !bound {
            return Err(RequestError::Shape(Field::TxBinding));
        }
        let (hash, from, nonce) = (
            fixed(tx, "hash", Field::TxHash)?,
            fixed(tx, "from", Field::From)?,
            quantity(tx, "nonce", Field::Nonce)?,
        );
        // R7: the object is the transaction its hash names.
        let (raw, signature) = signed_tx(tx)?;
        if keccak256(&raw).0 != hash {
            return Err(RequestError::Integrity(Integrity::TxHash));
        }
        let price = price_of(tx)?;
        raws.push(raw);
        signatures.push(signature);
        txs.push(BlockTx {
            hash,
            from,
            nonce,
            price,
        });
        objects.push(parse_tx_map(tx));
    }
    if super::integrity::ordered_trie_root(&raws) != header.transactions_root {
        return Err(RequestError::Integrity(Integrity::TransactionsRoot));
    }
    // Review LOW-4: the signers last (the dearest check): each its `from`.
    for (tx, signature) in txs.iter().zip(&signatures) {
        if signature.signer()? != tx.from {
            return Err(RequestError::Integrity(Integrity::TxSigner));
        }
    }
    Ok(Some(FullBlock {
        head: BlockHead { header, txs },
        objects,
    }))
}

/// The millisecond part of a BSC header, as bsc-geth v1.7.8
/// `Header.MilliTimestamp()` reads it: 0 for a zero `mixHash`, otherwise the
/// 256-bit big-endian value, which a verified header keeps below 1000.
/// `None` for 1000 or more.
fn mix_hash_ms(mix_hash: [u8; 32]) -> Option<u64> {
    let (high, low) = mix_hash.split_at(30);
    if high.iter().any(|b| *b != 0) {
        return None;
    }
    let ms = u64::from(u16::from_be_bytes([low[0], low[1]]));
    (ms < MS_PER_S).then_some(ms)
}

/// The answer to `eth_getBlockReceipts`; `None` for `null`.
pub(super) fn parse_receipts(body: &[u8], id: u64) -> Result<Option<Vec<Receipt>>, RequestError> {
    let receipts = match envelope(body, id)? {
        Value::Null => return Ok(None),
        Value::Array(receipts) => receipts,
        _ => return Err(RequestError::Shape(Field::Result)),
    };
    receipts
        .iter()
        .map(|r| match r {
            Value::Object(r) => receipt(r),
            _ => Err(RequestError::Shape(Field::Result)),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// REV.1.0 R3b: the answer to `eth_getTransactionReceipt`; `None` for
/// `null` (no receipt known to that node).
pub(super) fn parse_receipt(body: &[u8], id: u64) -> Result<Option<Receipt>, RequestError> {
    match envelope(body, id)? {
        Value::Null => Ok(None),
        Value::Object(r) => receipt(&r).map(Some),
        _ => Err(RequestError::Shape(Field::Result)),
    }
}

pub(super) fn receipt(r: &Map<String, Value>) -> Result<Receipt, RequestError> {
    let tx_hash = fixed(r, "transactionHash", Field::TxHash)?;
    let tx_index = quantity(r, "transactionIndex", Field::TxIndex)?;
    let from = fixed(r, "from", Field::From)?;
    let block_hash = fixed(r, "blockHash", Field::BlockHash)?;
    let block_number = quantity(r, "blockNumber", Field::BlockNumber)?;
    let succeeded = match quantity(r, "status", Field::Status)? {
        0 => false,
        1 => true,
        _ => return Err(RequestError::Shape(Field::Status)),
    };
    let tx_type = u8::try_from(quantity(r, "type", Field::ReceiptFields)?)
        .map_err(|_| RequestError::Shape(Field::ReceiptFields))?;
    let cumulative_gas = quantity(r, "cumulativeGasUsed", Field::ReceiptFields)?;
    // Optional on the wire; present, it must be a quantity.
    let cost = match (r.get("gasUsed"), r.get("effectiveGasPrice")) {
        (Some(_), Some(_)) => Some(super::TxCost {
            gas_used: quantity(r, "gasUsed", Field::GasCost)?,
            effective_gas_price: quantity(r, "effectiveGasPrice", Field::GasCost)?,
        }),
        _ => None,
    };
    let Some(Value::Array(entries)) = r.get("logs") else {
        return Err(RequestError::Shape(Field::Logs));
    };
    let mut logs = Vec::with_capacity(entries.len());
    for entry in entries {
        let Value::Object(entry) = entry else {
            return Err(RequestError::Shape(Field::Logs));
        };
        let bound = fixed::<32>(entry, "blockHash", Field::LogBinding)? == block_hash
            && quantity(entry, "blockNumber", Field::LogBinding)? == block_number
            && fixed::<32>(entry, "transactionHash", Field::LogBinding)? == tx_hash
            && quantity(entry, "transactionIndex", Field::LogBinding)? == tx_index;
        if !bound {
            return Err(RequestError::Shape(Field::LogBinding));
        }
        if entry.get("removed") != Some(&Value::Bool(false)) {
            return Err(RequestError::Shape(Field::LogRemoved));
        }
        let Some(Value::Array(topics)) = entry.get("topics") else {
            return Err(RequestError::Shape(Field::LogTopics));
        };
        if topics.len() > MAX_TOPICS {
            return Err(RequestError::Shape(Field::LogTopics));
        }
        let topics = topics
            .iter()
            .map(|t| t.as_str().and_then(data_fixed::<32>))
            .collect::<Option<Vec<_>>>()
            .ok_or(RequestError::Shape(Field::LogTopics))?;
        logs.push(Log {
            address: fixed(entry, "address", Field::LogAddress)?,
            topics,
            data: data_vec(text(entry, "data", Field::LogData)?)
                .ok_or(RequestError::Shape(Field::LogData))?,
            log_index: quantity(entry, "logIndex", Field::LogIndex)?,
        });
    }
    Ok(Receipt {
        tx_hash,
        tx_index,
        from,
        block_hash,
        block_number,
        succeeded,
        tx_type,
        cumulative_gas,
        cost,
        logs,
    })
}

/// The `result` of a JSON-RPC 2.0 answer to request `id`, or its error.
pub(crate) fn envelope(body: &[u8], id: u64) -> Result<Value, RequestError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| RequestError::MalformedJson)?;
    let Value::Object(mut msg) = value else {
        return Err(RequestError::Shape(Field::Envelope));
    };
    if msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(RequestError::Shape(Field::Envelope));
    }
    // An error is the answer to the one request in flight, whatever its id
    // (providers answer rate limits with a null id).
    if let Some(error) = msg.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .ok_or(RequestError::Shape(Field::ErrorCode))?;
        return Err(if RATE_LIMIT_CODES.contains(&code) {
            RequestError::RateLimited(RateLimitSignal::RpcError(code))
        } else {
            RequestError::Rpc(code)
        });
    }
    if msg.get("id").and_then(Value::as_u64) != Some(id) {
        return Err(RequestError::Shape(Field::Id));
    }
    msg.remove("result")
        .ok_or(RequestError::Shape(Field::Result))
}

fn text<'a>(
    obj: &'a Map<String, Value>,
    name: &str,
    field: Field,
) -> Result<&'a str, RequestError> {
    obj.get(name)
        .and_then(Value::as_str)
        .ok_or(RequestError::Shape(field))
}

fn quantity(obj: &Map<String, Value>, name: &str, field: Field) -> Result<u64, RequestError> {
    quantity_u64(text(obj, name, field)?).ok_or(RequestError::Shape(field))
}

fn fixed<const N: usize>(
    obj: &Map<String, Value>,
    name: &str,
    field: Field,
) -> Result<[u8; N], RequestError> {
    data_fixed::<N>(text(obj, name, field)?).ok_or(RequestError::Shape(field))
}

fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// A canonical quantity of at most 64 bits.
pub(crate) fn quantity_u64(s: &str) -> Option<u64> {
    let digits = s.strip_prefix("0x")?.as_bytes();
    let canonical = match digits {
        [] => false,
        [_] => true,
        [first, ..] => *first != b'0',
    };
    if !canonical || digits.len() > 16 {
        return None;
    }
    digits
        .iter()
        .try_fold(0u64, |v, &d| Some((v << 4) | u64::from(nibble(d)?)))
}

/// R7: a canonical quantity of at most 256 bits, its big-endian bytes
/// without leading zeros (none for zero).
pub(crate) fn quantity_be(s: &str) -> Option<Vec<u8>> {
    let digits = s.strip_prefix("0x")?.as_bytes();
    let canonical = match digits {
        [] => false,
        [_] => true,
        [first, ..] => *first != b'0',
    };
    if !canonical || digits.len() > 64 {
        return None;
    }
    let (head, rest) = digits.split_at(digits.len() % 2);
    let mut out = Vec::with_capacity(digits.len().div_ceil(2));
    if let [d] = head {
        out.push(nibble(*d)?);
    }
    for pair in rest.chunks_exact(2) {
        out.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    if out == [0] {
        out.clear();
    }
    Some(out)
}

/// `0x` and two digits per byte.
pub(crate) fn data_vec(s: &str) -> Option<Vec<u8>> {
    let digits = s.strip_prefix("0x")?.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks_exact(2)
        .map(|pair| Some((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}

/// `0x` and exactly `2 * N` digits.
pub(crate) fn data_fixed<const N: usize>(s: &str) -> Option<[u8; N]> {
    let digits = s.strip_prefix("0x")?.as_bytes();
    if digits.len() != 2 * N {
        return None;
    }
    let mut out = [0u8; N];
    for (byte, pair) in out.iter_mut().zip(digits.chunks_exact(2)) {
        *byte = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(out)
}
