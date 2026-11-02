//! R7 test support: synthetic blocks that are consistent in themselves
//! (signed transactions, both roots, the header hash), as a provider that
//! tells the truth about its own chain sends them.

use alloy_primitives::{hex, keccak256};
use darkbot_bsc_tx_encoding::rlp::{encode_bytes, encode_list, encode_u64};
use serde_json::{Value, json};

use super::integrity::{ordered_trie_root, receipts_root};
use super::jsonrpc::{header_hash, receipt, signed_tx};

/// The key of every synthetic transaction (test only).
const KEY: [u8; 32] = [0x5a; 32];

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

/// The address of `key`.
pub(super) fn address_of(key: &[u8; 32]) -> [u8; 20] {
    darkbot_bsc_trading::signer::Signer::from_secret_bytes(key)
        .unwrap()
        .address()
        .into_array()
}

/// The address of [`KEY`].
pub(super) fn synth_sender() -> [u8; 20] {
    address_of(&KEY)
}

/// A legacy transaction object signed by [`KEY`] for `chain_id` (EIP-155):
/// nonce `nonce`, `input` its data; its hash is its bytes'.
pub(super) fn synth_tx(input: &[u8], nonce: u64, chain_id: u64) -> Value {
    signed_legacy(&KEY, input, nonce, Some(chain_id))
}

/// [`synth_tx`] signed by `key`; without a chain id before EIP-155 (`v`
/// 27/28).
pub(super) fn signed_legacy(
    key: &[u8; 32],
    input: &[u8],
    nonce: u64,
    chain_id: Option<u64>,
) -> Value {
    let to = [0x77u8; 20];
    let fields = [
        encode_u64(nonce),
        encode_u64(0),
        encode_u64(21_000),
        encode_bytes(&to),
        encode_u64(0),
        encode_bytes(input),
    ];
    let mut signing = fields.to_vec();
    if let Some(chain_id) = chain_id {
        signing.extend([encode_u64(chain_id), encode_bytes(&[]), encode_bytes(&[])]);
    }
    let signing_key = k256::ecdsa::SigningKey::from_slice(key).unwrap();
    let (sig, id) = signing_key
        .sign_prehash_recoverable(keccak256(encode_list(&signing)).as_slice())
        .unwrap();
    let v = chain_id.map_or(27, |c| c * 2 + 35) + u64::from(id.is_y_odd());
    let (r, s) = (sig.r().to_bytes(), sig.s().to_bytes());
    let object = json!({
        "type": "0x0",
        "nonce": format!("0x{nonce:x}"),
        "gasPrice": "0x0",
        "gas": "0x5208",
        "to": hx(&to),
        "value": "0x0",
        "input": hx(input),
        "v": format!("0x{v:x}"),
        "r": quantity(&r),
        "s": quantity(&s),
        "from": hx(&address_of(key)),
    });
    let (raw, _) = signed_tx(object.as_object().unwrap()).unwrap();
    let mut object = object;
    object["hash"] = json!(hx(&keccak256(&raw).0));
    object
}

/// An EIP-7702 (type 4) transaction object signed by [`KEY`] for BSC,
/// with one authorization: a type the WSS verifier does not rebuild.
pub(super) fn synth_7702(input: &[u8], nonce: u64) -> Value {
    let to = [0x77u8; 20];
    let auth = [
        encode_u64(56),
        encode_bytes(&[0x22; 20]),
        encode_u64(0),
        encode_u64(0),
        encode_u64(1),
        encode_u64(1),
    ];
    let fields = [
        encode_u64(56),
        encode_u64(nonce),
        encode_u64(1),
        encode_u64(1),
        encode_u64(50_000),
        encode_bytes(&to),
        encode_u64(0),
        encode_bytes(input),
        encode_list::<Vec<u8>>(&[]),
        encode_list(&[encode_list(&auth)]),
    ];
    let key = k256::ecdsa::SigningKey::from_slice(&KEY).unwrap();
    let payload = [vec![4u8], encode_list(&fields)].concat();
    let (sig, id) = key
        .sign_prehash_recoverable(keccak256(&payload).as_slice())
        .unwrap();
    let y = format!("0x{:x}", u8::from(id.is_y_odd()));
    let object = json!({
        "type": "0x4",
        "chainId": "0x38",
        "nonce": format!("0x{nonce:x}"),
        "maxPriorityFeePerGas": "0x1",
        "maxFeePerGas": "0x1",
        "gas": "0xc350",
        "to": hx(&to),
        "value": "0x0",
        "input": hx(input),
        "accessList": [],
        "authorizationList": [{
            "chainId": "0x38",
            "address": hx(&[0x22; 20]),
            "nonce": "0x0",
            "yParity": "0x0",
            "r": "0x1",
            "s": "0x1",
        }],
        "v": y,
        "yParity": y,
        "r": quantity(&sig.r().to_bytes()),
        "s": quantity(&sig.s().to_bytes()),
        "from": hx(&synth_sender()),
    });
    let (raw, _) = signed_tx(object.as_object().unwrap()).unwrap();
    let mut object = object;
    object["hash"] = json!(hx(&keccak256(&raw).0));
    object
}

/// Big-endian bytes as a JSON-RPC quantity.
fn quantity(be: &[u8]) -> String {
    let digits = hex::encode(be);
    let digits = digits.trim_start_matches('0');
    format!("0x{}", if digits.is_empty() { "0" } else { digits })
}

/// Seals `block` (a header object, its `transactions` full objects or
/// hashes): the mandatory header fields it lacks with neutral values, its
/// `transactionsRoot` from signed objects, its `receiptsRoot` from
/// `receipts` (given), then its `hash` (unless a field of it is refused);
/// every `blockHash` that named the old hash names the new one.
pub(super) fn seal(block: &mut Value, receipts: Option<&mut Value>) {
    let defaults = [
        ("sha3Uncles", hx(&keccak256(encode_list::<Vec<u8>>(&[])).0)),
        ("miner", hx(&[0u8; 20])),
        ("stateRoot", hx(&[0u8; 32])),
        ("transactionsRoot", hx(&ordered_trie_root(&[]))),
        ("receiptsRoot", hx(&ordered_trie_root(&[]))),
        ("logsBloom", hx(&[0u8; 256])),
        ("difficulty", "0x2".to_owned()),
        ("gasLimit", "0x42c1d80".to_owned()),
        ("gasUsed", "0x0".to_owned()),
        ("extraData", "0x".to_owned()),
        ("nonce", hx(&[0u8; 8])),
    ];
    for (name, value) in defaults {
        if block.get(name).is_none() {
            block[name] = json!(value);
        }
    }
    if let Some(Value::Array(txs)) = block.get("transactions") {
        let raws: Option<Vec<Vec<u8>>> = txs
            .iter()
            .map(|tx| Some(signed_tx(tx.as_object()?).ok()?.0))
            .collect();
        if let Some(raws) = raws {
            block["transactionsRoot"] = json!(hx(&ordered_trie_root(&raws)));
        }
    }
    if let Some(Value::Array(list)) = receipts.as_deref() {
        let typed: Vec<_> = list
            .iter()
            .map(|r| receipt(r.as_object().unwrap()).unwrap())
            .collect();
        block["receiptsRoot"] = json!(hx(&receipts_root(&typed)));
    }
    // A header whose fields are refused before its hash keeps its hash.
    let Ok(hash) = header_hash(block.as_object().unwrap()) else {
        return;
    };
    let old = block.get("hash").cloned();
    let new = json!(hx(&hash));
    block["hash"] = new.clone();
    let rebind = |v: &mut Value| {
        if v.get("blockHash") == old.as_ref() && old.is_some() {
            v["blockHash"] = new.clone();
        }
    };
    if let Some(Value::Array(txs)) = block.get_mut("transactions") {
        txs.iter_mut().filter(|t| t.is_object()).for_each(rebind);
    }
    if let Some(Value::Array(list)) = receipts {
        for r in list {
            rebind(r);
            if let Some(Value::Array(logs)) = r.get_mut("logs") {
                logs.iter_mut().for_each(rebind);
            }
        }
    }
}
