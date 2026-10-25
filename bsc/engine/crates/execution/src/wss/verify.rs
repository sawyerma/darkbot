//! Rebuilds the canonical signed bytes of a typed transaction and verifies
//! them. No JSON here: the input is what `jsonrpc` converted.
//!
//! Per type (`fields` are the unsigned fields in envelope order):
//!
//! | type    | signed bytes                                  | signing payload               |
//! |---------|-----------------------------------------------|-------------------------------|
//! | legacy  | `rlp([fields, v, r, s])`                      | `rlp([fields, chainId, 0, 0])` |
//! | 1, 2    | `type ‖ rlp([chainId, fields, yParity, r, s])` | `type ‖ rlp([chainId, fields])` |
//!
//! legacy `fields`: nonce, gasPrice, gas, to, value, input (EIP-155,
//! `v = 35 + 2·chainId + parity`); type 1 (EIP-2930): nonce, gasPrice, gas,
//! to, value, input, accessList; type 2 (EIP-1559): nonce,
//! maxPriorityFeePerGas, maxFeePerGas, gas, to, value, input, accessList.
//!
//! Order of checks: chain, then Keccak-256 of the signed bytes against the
//! announced hash (cheap, binds every field), then signer recovery against
//! the announced sender, then the independent envelope decoder.

use alloy_primitives::keccak256;
use darkbot_bsc_geth_ipc::ingress::decode_tx_target;
use darkbot_bsc_trading::signer::recover_signer;
use darkbot_bsc_trading::transactions::legacy::RecoverableSignature;
use darkbot_bsc_tx_encoding::rlp::{
    bytes_encoded_len, list_encoded_len, put_bytes, put_list_header, put_u64, put_uint_be,
};

use super::{RejectReason, TxField, VerifiedTx};

/// The EIP-155 `v` offset: `v = 35 + 2·chainId + parity`.
const EIP155_V_OFFSET: u64 = 35;
/// The pre-EIP-155 `v` values (27 + parity).
const PRE_EIP155_V: [u64; 2] = [27, 28];

/// The typed fields of one transaction object.
#[derive(Debug)]
pub(super) struct WireTx {
    pub hash: [u8; 32],
    pub from: [u8; 20],
    pub to: Option<[u8; 20]>,
    pub nonce: u64,
    pub gas: u64,
    pub value: [u8; 32],
    pub input: Vec<u8>,
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub kind: TxKind,
}

#[derive(Debug)]
pub(super) enum TxKind {
    Legacy {
        gas_price: [u8; 32],
        v: u64,
        /// The object's `chainId`, when it carries one.
        chain_id: Option<u64>,
    },
    /// EIP-2930 (type 1).
    AccessList {
        chain_id: u64,
        gas_price: [u8; 32],
        access_list: Vec<AccessItem>,
        y_parity: bool,
    },
    /// EIP-1559 (type 2).
    DynamicFee {
        chain_id: u64,
        max_priority_fee: [u8; 32],
        max_fee: [u8; 32],
        access_list: Vec<AccessItem>,
        y_parity: bool,
    },
}

#[derive(Debug)]
pub(super) struct AccessItem {
    pub address: [u8; 20],
    pub storage_keys: Vec<[u8; 32]>,
}

/// Parses one transaction object (JSON text, the provider's wire format)
/// and verifies it against `expected_chain_id`. The same path the source
/// uses for every full notification.
pub fn verify_tx_object(json: &[u8], expected_chain_id: u64) -> Result<VerifiedTx, RejectReason> {
    verify(&super::jsonrpc::parse_tx_json(json)?, expected_chain_id)
}

pub(super) fn verify(tx: &WireTx, expected_chain_id: u64) -> Result<VerifiedTx, RejectReason> {
    let (tx_type, chain_id, y_parity) = match tx.kind {
        TxKind::Legacy { v, chain_id, .. } => {
            if PRE_EIP155_V.contains(&v) {
                return Err(RejectReason::PreEip155);
            }
            let offset = v
                .checked_sub(EIP155_V_OFFSET)
                .ok_or(RejectReason::Malformed(TxField::V))?;
            let from_v = offset / 2;
            if from_v != expected_chain_id {
                return Err(RejectReason::WrongChain(from_v));
            }
            if chain_id.is_some_and(|c| c != from_v) {
                return Err(RejectReason::ChainIdMismatch);
            }
            (None, from_v, offset % 2 == 1)
        }
        TxKind::AccessList {
            chain_id, y_parity, ..
        } => (Some(1u8), chain_id, y_parity),
        TxKind::DynamicFee {
            chain_id, y_parity, ..
        } => (Some(2u8), chain_id, y_parity),
    };
    if chain_id != expected_chain_id {
        return Err(RejectReason::WrongChain(chain_id));
    }

    let mut fields = Vec::with_capacity(tx.input.len() + 256);
    put_unsigned_fields(tx, chain_id, &mut fields);
    let fields_len = fields.len();

    // Signing payload: legacy appends (chainId, 0, 0) per EIP-155.
    if tx_type.is_none() {
        put_u64(&mut fields, chain_id);
        put_u64(&mut fields, 0);
        put_u64(&mut fields, 0);
    }
    let prehash = keccak256(envelope(tx_type, &fields));

    fields.truncate(fields_len);
    match tx.kind {
        TxKind::Legacy { v, .. } => put_u64(&mut fields, v),
        _ => put_u64(&mut fields, u64::from(y_parity)),
    }
    put_uint_be(&mut fields, &tx.r);
    put_uint_be(&mut fields, &tx.s);
    let raw = envelope(tx_type, &fields);

    if keccak256(&raw).0 != tx.hash {
        return Err(RejectReason::HashMismatch);
    }
    let signature = RecoverableSignature {
        r: tx.r,
        s: tx.s,
        y_parity,
    };
    let signer = recover_signer(&prehash, &signature).map_err(RejectReason::Signature)?;
    if signer.0.0 != tx.from {
        return Err(RejectReason::SignerMismatch);
    }
    match decode_tx_target(&raw) {
        Ok(target)
            if target.tx_type == tx_type
                && target.to == tx.to
                && target.data == tx.input.as_slice() => {}
        _ => return Err(RejectReason::Envelope),
    }
    Ok(VerifiedTx {
        hash: tx.hash,
        from: tx.from,
        to: tx.to,
        nonce: tx.nonce,
        chain_id: Some(chain_id),
        tx_type,
        raw,
    })
}

/// The unsigned fields in envelope order, typed ones led by the chain id.
fn put_unsigned_fields(tx: &WireTx, chain_id: u64, out: &mut Vec<u8>) {
    let to: &[u8] = match &tx.to {
        Some(to) => to,
        None => &[],
    };
    match &tx.kind {
        TxKind::Legacy { gas_price, .. } => {
            put_u64(out, tx.nonce);
            put_uint_be(out, gas_price);
            put_u64(out, tx.gas);
            put_bytes(out, to);
            put_uint_be(out, &tx.value);
            put_bytes(out, &tx.input);
        }
        TxKind::AccessList {
            gas_price,
            access_list,
            ..
        } => {
            put_u64(out, chain_id);
            put_u64(out, tx.nonce);
            put_uint_be(out, gas_price);
            put_u64(out, tx.gas);
            put_bytes(out, to);
            put_uint_be(out, &tx.value);
            put_bytes(out, &tx.input);
            put_access_list(out, access_list);
        }
        TxKind::DynamicFee {
            max_priority_fee,
            max_fee,
            access_list,
            ..
        } => {
            put_u64(out, chain_id);
            put_u64(out, tx.nonce);
            put_uint_be(out, max_priority_fee);
            put_uint_be(out, max_fee);
            put_u64(out, tx.gas);
            put_bytes(out, to);
            put_uint_be(out, &tx.value);
            put_bytes(out, &tx.input);
            put_access_list(out, access_list);
        }
    }
}

/// `rlp([[address, [key, ...]], ...])`.
fn put_access_list(out: &mut Vec<u8>, list: &[AccessItem]) {
    let keys_payload = |item: &AccessItem| -> usize {
        item.storage_keys.iter().map(|k| bytes_encoded_len(k)).sum()
    };
    let item_payload = |item: &AccessItem| -> usize {
        bytes_encoded_len(&item.address) + list_encoded_len(keys_payload(item))
    };
    let payload: usize = list
        .iter()
        .map(|item| list_encoded_len(item_payload(item)))
        .sum();
    put_list_header(out, payload);
    for item in list {
        put_list_header(out, item_payload(item));
        put_bytes(out, &item.address);
        put_list_header(out, keys_payload(item));
        for key in &item.storage_keys {
            put_bytes(out, key);
        }
    }
}

/// `type ‖ rlp(list)` for a typed transaction, `rlp(list)` for legacy.
fn envelope(tx_type: Option<u8>, list_payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + list_encoded_len(list_payload.len()));
    if let Some(t) = tx_type {
        out.push(t);
    }
    put_list_header(&mut out, list_payload.len());
    out.extend_from_slice(list_payload);
    out
}
