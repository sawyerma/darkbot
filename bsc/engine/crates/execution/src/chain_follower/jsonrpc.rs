//! The JSON-RPC wire boundary. A response body goes in, typed values come
//! out; nothing past this file sees JSON.
//!
//! Hex follows the JSON-RPC encoding (EIP-1474) as BSC nodes write it, in
//! lowercase: a quantity is `0x` with the shortest non-empty digit string
//! (`0x0` for zero), data is `0x` with two digits per byte, of exactly the
//! field's width where it has one. Anything else is refused.

use serde_json::{Map, Value};

use super::{Call, Field, RequestError};
use crate::wss::{BSC_MAINNET_CHAIN_ID, RateLimitSignal, VerifiedTx, verify_tx_map};

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
}

/// `eth_getBlockByNumber(n, false)`: the header and the block's
/// transaction hashes in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BlockHead {
    pub(super) header: Header,
    pub(super) transactions: Vec<[u8; 32]>,
}

/// One receipt of `eth_getBlockReceipts`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Receipt {
    pub(super) tx_hash: [u8; 32],
    pub(super) tx_index: u64,
    pub(super) block_hash: [u8; 32],
    pub(super) block_number: u64,
    /// `status` is `0x1`.
    pub(super) succeeded: bool,
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

/// Where a launch transaction must be: its receipt's hash, block and
/// index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TxPlace {
    pub(super) hash: [u8; 32],
    pub(super) block_hash: [u8; 32],
    pub(super) block_number: u64,
    pub(super) tx_index: u64,
}

/// The request body of `call` with `id`.
pub(super) fn request(id: u64, call: Call) -> String {
    match call {
        Call::BlockNumber => {
            format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_blockNumber","params":[]}}"#)
        }
        Call::Header(n) => format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_getBlockByNumber","params":["0x{n:x}",false]}}"#
        ),
        Call::Receipts(n) => format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_getBlockReceipts","params":["0x{n:x}"]}}"#
        ),
        Call::Transaction(hash) => {
            let hash = alloy_primitives::hex::encode_prefixed(hash);
            format!(
                r#"{{"jsonrpc":"2.0","id":{id},"method":"eth_getTransactionByHash","params":["{hash}"]}}"#
            )
        }
    }
}

/// The answer to `eth_blockNumber`.
pub(super) fn parse_block_number(body: &[u8], id: u64) -> Result<u64, RequestError> {
    match envelope(body, id)? {
        Value::String(s) => quantity_u64(&s).ok_or(RequestError::Shape(Field::Result)),
        _ => Err(RequestError::Shape(Field::Result)),
    }
}

/// The answer to `eth_getBlockByNumber(number, false)`; `None` for `null`.
pub(super) fn parse_header(
    body: &[u8],
    id: u64,
    number: u64,
) -> Result<Option<BlockHead>, RequestError> {
    let block = match envelope(body, id)? {
        Value::Null => return Ok(None),
        Value::Object(block) => block,
        _ => return Err(RequestError::Shape(Field::Result)),
    };
    let timestamp_s = quantity(&block, "timestamp", Field::Timestamp)?;
    let ms = mix_hash_ms(fixed(&block, "mixHash", Field::MixHash)?)
        .ok_or(RequestError::Shape(Field::MixHash))?;
    let header = Header {
        number: quantity(&block, "number", Field::Number)?,
        hash: fixed(&block, "hash", Field::Hash)?,
        parent_hash: fixed(&block, "parentHash", Field::ParentHash)?,
        timestamp_s,
        block_ms: timestamp_s
            .checked_mul(MS_PER_S)
            .and_then(|t| t.checked_add(ms))
            .ok_or(RequestError::Shape(Field::Timestamp))?,
    };
    if header.number != number {
        return Err(RequestError::Shape(Field::Number));
    }
    let Some(Value::Array(txs)) = block.get("transactions") else {
        return Err(RequestError::Shape(Field::Transactions));
    };
    let transactions = txs
        .iter()
        .map(|tx| tx.as_str().and_then(data_fixed::<32>))
        .collect::<Option<Vec<_>>>()
        .ok_or(RequestError::Shape(Field::Transactions))?;
    Ok(Some(BlockHead {
        header,
        transactions,
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

/// The answer to `eth_getTransactionByHash` for the launch at `place`:
/// bound to its receipt (hash, block hash and number, index) and verified
/// by the WSS source's verifier for BSC Mainnet. `None` for `null`.
pub(super) fn parse_transaction(
    body: &[u8],
    id: u64,
    place: &TxPlace,
) -> Result<Option<VerifiedTx>, RequestError> {
    let tx = match envelope(body, id)? {
        Value::Null => return Ok(None),
        Value::Object(tx) => tx,
        _ => return Err(RequestError::Shape(Field::Result)),
    };
    // A backend that has not seen the block yet answers the transaction as
    // pending (`blockHash` null): not there yet, retried like `null`.
    if tx.get("blockHash") == Some(&Value::Null) {
        return Ok(None);
    }
    let found = TxPlace {
        hash: fixed(&tx, "hash", Field::TxBinding)?,
        block_hash: fixed(&tx, "blockHash", Field::TxBinding)?,
        block_number: quantity(&tx, "blockNumber", Field::TxBinding)?,
        tx_index: quantity(&tx, "transactionIndex", Field::TxBinding)?,
    };
    if found != *place {
        return Err(RequestError::Shape(Field::TxBinding));
    }
    let verified = verify_tx_map(&tx, BSC_MAINNET_CHAIN_ID).map_err(RequestError::Unverifiable)?;
    // The verifier proved Keccak-256 of the signed bytes is the object's
    // hash, which is the receipt's.
    if verified.hash != place.hash {
        return Err(RequestError::Shape(Field::TxBinding));
    }
    Ok(Some(verified))
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

fn receipt(r: &Map<String, Value>) -> Result<Receipt, RequestError> {
    let tx_hash = fixed(r, "transactionHash", Field::TxHash)?;
    let tx_index = quantity(r, "transactionIndex", Field::TxIndex)?;
    let block_hash = fixed(r, "blockHash", Field::BlockHash)?;
    let block_number = quantity(r, "blockNumber", Field::BlockNumber)?;
    let succeeded = match quantity(r, "status", Field::Status)? {
        0 => false,
        1 => true,
        _ => return Err(RequestError::Shape(Field::Status)),
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
        block_hash,
        block_number,
        succeeded,
        logs,
    })
}

/// The `result` of a JSON-RPC 2.0 answer to request `id`, or its error.
fn envelope(body: &[u8], id: u64) -> Result<Value, RequestError> {
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
pub(super) fn quantity_u64(s: &str) -> Option<u64> {
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

/// `0x` and two digits per byte.
pub(super) fn data_vec(s: &str) -> Option<Vec<u8>> {
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
pub(super) fn data_fixed<const N: usize>(s: &str) -> Option<[u8; N]> {
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
