//! The JSON-RPC wire boundary. A frame's bytes go in, typed values come
//! out; nothing past this file sees JSON.
//!
//! Hex follows the JSON-RPC encoding (EIP-1474): a quantity is `0x` with
//! the shortest non-empty digit string (`0x0` for zero), data is `0x` with
//! two digits per byte, of exactly the field's width where it has one.

use serde_json::{Map, Value};

use super::verify::{AccessItem, TxKind, WireTx};
use super::{RejectReason, TxField, VerifiedTx};

/// The JSON-RPC id of the one request this source sends.
pub(super) const SUBSCRIBE_ID: u64 = 1;

/// `eth_subscribe` for full pending transactions, with [`SUBSCRIBE_ID`].
pub(super) const SUBSCRIBE_REQUEST: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newPendingTransactions",true]}"#;

/// The JSON-RPC error codes that mean rate limiting: EIP-1474 "limit
/// exceeded" and the HTTP status some providers reuse.
const RATE_LIMIT_CODES: [i64; 2] = [-32005, 429];

pub(super) fn is_rate_limit_code(code: i64) -> bool {
    RATE_LIMIT_CODES.contains(&code)
}

/// One inbound frame, typed.
#[derive(Debug)]
pub(super) enum Inbound {
    /// A response with a string `result`: the subscription id.
    Subscribed { id: u64, subscription: String },
    /// Any message with an `error` object and an integer `code`.
    RpcError { code: i64 },
    /// An `eth_subscription` notification.
    Notification { subscription: String, item: Item },
    /// Valid JSON of no other shape.
    Unexpected,
}

/// The `result` of one notification.
#[derive(Debug)]
pub(super) enum Item {
    Hash([u8; 32]),
    Tx(Box<WireTx>),
    Rejected(RejectReason),
}

/// The frame is not a JSON value.
#[derive(Debug)]
pub(super) struct MalformedJson;

pub(super) fn parse_inbound(bytes: &[u8]) -> Result<Inbound, MalformedJson> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| MalformedJson)?;
    let Value::Object(msg) = value else {
        return Ok(Inbound::Unexpected);
    };
    if let Some(error) = msg.get("error") {
        return Ok(match error.get("code").and_then(Value::as_i64) {
            Some(code) => Inbound::RpcError { code },
            None => Inbound::Unexpected,
        });
    }
    if msg.get("method").and_then(Value::as_str) == Some("eth_subscription") {
        let Some(params) = msg.get("params").and_then(Value::as_object) else {
            return Ok(Inbound::Unexpected);
        };
        let (Some(subscription), Some(result)) = (
            params.get("subscription").and_then(Value::as_str),
            params.get("result"),
        ) else {
            return Ok(Inbound::Unexpected);
        };
        let item = match result {
            Value::String(hash) => match data_fixed::<32>(hash) {
                Some(hash) => Item::Hash(hash),
                None => Item::Rejected(RejectReason::Malformed(TxField::Hash)),
            },
            Value::Object(tx) => match parse_tx(tx) {
                Ok(tx) => Item::Tx(Box::new(tx)),
                Err(reason) => Item::Rejected(reason),
            },
            _ => Item::Rejected(RejectReason::NotATransaction),
        };
        return Ok(Inbound::Notification {
            subscription: subscription.to_owned(),
            item,
        });
    }
    match (
        msg.get("id").and_then(Value::as_u64),
        msg.get("result").and_then(Value::as_str),
    ) {
        (Some(id), Some(subscription)) => Ok(Inbound::Subscribed {
            id,
            subscription: subscription.to_owned(),
        }),
        _ => Ok(Inbound::Unexpected),
    }
}

/// Parses one transaction object as JSON text (the public boundary of
/// [`super::verify_tx_object`]).
pub(super) fn parse_tx_json(bytes: &[u8]) -> Result<WireTx, RejectReason> {
    match serde_json::from_slice::<Value>(bytes) {
        Ok(Value::Object(tx)) => parse_tx(&tx),
        _ => Err(RejectReason::NotATransaction),
    }
}

/// Verifies one transaction object the caller already parsed (the
/// canonical follower's `eth_getTransactionByHash` answer): the same
/// conversion and checks as every full notification.
pub(crate) fn verify_tx_map(
    tx: &Map<String, Value>,
    expected_chain_id: u64,
) -> Result<VerifiedTx, RejectReason> {
    super::verify::verify(&parse_tx(tx)?, expected_chain_id)
}

/// Converts a transaction object into typed fields. Only the fields the
/// signed envelope of its type contains are read; for a type this source
/// does not rebuild nothing else is read.
pub(super) fn parse_tx(tx: &Map<String, Value>) -> Result<WireTx, RejectReason> {
    let tx_type = quantity_u64(text(tx, "type", TxField::Type)?)
        .ok_or(RejectReason::Malformed(TxField::Type))?;
    let tx_type = u8::try_from(tx_type).map_err(|_| RejectReason::Malformed(TxField::Type))?;
    if tx_type > 2 {
        return Err(RejectReason::UnsupportedType(tx_type));
    }
    let hash = data_fixed::<32>(text(tx, "hash", TxField::Hash)?)
        .ok_or(RejectReason::Malformed(TxField::Hash))?;
    let from = data_fixed::<20>(text(tx, "from", TxField::From)?)
        .ok_or(RejectReason::Malformed(TxField::From))?;
    let to = match tx.get("to") {
        None => return Err(RejectReason::Missing(TxField::To)),
        Some(Value::Null) => None,
        Some(Value::String(to)) => {
            Some(data_fixed::<20>(to).ok_or(RejectReason::Malformed(TxField::To))?)
        }
        Some(_) => return Err(RejectReason::Malformed(TxField::To)),
    };
    let nonce = u64_field(tx, "nonce", TxField::Nonce)?;
    let gas = u64_field(tx, "gas", TxField::Gas)?;
    let value = u256_field(tx, "value", TxField::Value)?;
    let input = data_vec(text(tx, "input", TxField::Input)?)
        .ok_or(RejectReason::Malformed(TxField::Input))?;
    let r = u256_field(tx, "r", TxField::R)?;
    let s = u256_field(tx, "s", TxField::S)?;
    let kind = match tx_type {
        0 => TxKind::Legacy {
            gas_price: u256_field(tx, "gasPrice", TxField::GasPrice)?,
            v: u64_field(tx, "v", TxField::V)?,
            chain_id: match tx.get("chainId") {
                None | Some(Value::Null) => None,
                Some(_) => Some(u64_field(tx, "chainId", TxField::ChainId)?),
            },
        },
        1 => TxKind::AccessList {
            chain_id: u64_field(tx, "chainId", TxField::ChainId)?,
            gas_price: u256_field(tx, "gasPrice", TxField::GasPrice)?,
            access_list: access_list(tx)?,
            y_parity: y_parity(tx)?,
        },
        _ => TxKind::DynamicFee {
            chain_id: u64_field(tx, "chainId", TxField::ChainId)?,
            max_priority_fee: u256_field(
                tx,
                "maxPriorityFeePerGas",
                TxField::MaxPriorityFeePerGas,
            )?,
            max_fee: u256_field(tx, "maxFeePerGas", TxField::MaxFeePerGas)?,
            access_list: access_list(tx)?,
            y_parity: y_parity(tx)?,
        },
    };
    Ok(WireTx {
        hash,
        from,
        to,
        nonce,
        gas,
        value,
        input,
        r,
        s,
        kind,
    })
}

/// The signature parity of a typed transaction: `yParity` and/or `v`, each
/// `0x0` or `0x1`, equal when both are present.
fn y_parity(tx: &Map<String, Value>) -> Result<bool, RejectReason> {
    let parity = |name: &str, field: TxField| -> Result<Option<bool>, RejectReason> {
        match tx.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(_) => match u64_field(tx, name, field)? {
                0 => Ok(Some(false)),
                1 => Ok(Some(true)),
                _ => Err(RejectReason::YParityMismatch),
            },
        }
    };
    match (
        parity("yParity", TxField::YParity)?,
        parity("v", TxField::V)?,
    ) {
        (Some(a), Some(b)) if a != b => Err(RejectReason::YParityMismatch),
        (Some(p), _) | (None, Some(p)) => Ok(p),
        (None, None) => Err(RejectReason::Missing(TxField::YParity)),
    }
}

fn access_list(tx: &Map<String, Value>) -> Result<Vec<AccessItem>, RejectReason> {
    const BAD: RejectReason = RejectReason::Malformed(TxField::AccessList);
    let entries = match tx.get("accessList") {
        None | Some(Value::Null) => return Err(RejectReason::Missing(TxField::AccessList)),
        Some(Value::Array(entries)) => entries,
        Some(_) => return Err(BAD),
    };
    let mut list = Vec::with_capacity(entries.len());
    for entry in entries {
        let address = entry
            .get("address")
            .and_then(Value::as_str)
            .and_then(data_fixed::<20>)
            .ok_or(BAD)?;
        let keys = entry
            .get("storageKeys")
            .and_then(Value::as_array)
            .ok_or(BAD)?;
        let mut storage_keys = Vec::with_capacity(keys.len());
        for key in keys {
            storage_keys.push(key.as_str().and_then(data_fixed::<32>).ok_or(BAD)?);
        }
        list.push(AccessItem {
            address,
            storage_keys,
        });
    }
    Ok(list)
}

fn text<'a>(
    tx: &'a Map<String, Value>,
    name: &str,
    field: TxField,
) -> Result<&'a str, RejectReason> {
    match tx.get(name) {
        None | Some(Value::Null) => Err(RejectReason::Missing(field)),
        Some(Value::String(s)) => Ok(s),
        Some(_) => Err(RejectReason::Malformed(field)),
    }
}

fn u64_field(tx: &Map<String, Value>, name: &str, field: TxField) -> Result<u64, RejectReason> {
    quantity_u64(text(tx, name, field)?).ok_or(RejectReason::Malformed(field))
}

fn u256_field(
    tx: &Map<String, Value>,
    name: &str,
    field: TxField,
) -> Result<[u8; 32], RejectReason> {
    quantity_u256(text(tx, name, field)?).ok_or(RejectReason::Malformed(field))
}

fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// The digits of a canonical quantity of at most `max_digits` digits.
fn quantity_digits(s: &str, max_digits: usize) -> Option<&[u8]> {
    let digits = s.strip_prefix("0x")?.as_bytes();
    let canonical = match digits {
        [] => false,
        [_] => true,
        [first, ..] => *first != b'0',
    };
    (canonical && digits.len() <= max_digits).then_some(digits)
}

pub(super) fn quantity_u64(s: &str) -> Option<u64> {
    let mut v = 0u64;
    for &d in quantity_digits(s, 16)? {
        v = (v << 4) | u64::from(nibble(d)?);
    }
    Some(v)
}

/// A canonical quantity of up to 256 bits, as 32 big-endian bytes.
pub(super) fn quantity_u256(s: &str) -> Option<[u8; 32]> {
    let digits = quantity_digits(s, 64)?;
    let mut out = [0u8; 32];
    for (i, &d) in digits.iter().rev().enumerate() {
        let n = nibble(d)?;
        out[31 - i / 2] |= if i % 2 == 0 { n } else { n << 4 };
    }
    Some(out)
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
