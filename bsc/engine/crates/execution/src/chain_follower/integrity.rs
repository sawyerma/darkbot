//! REV.1.0 R7 (BSC3-056): a block's own consistency. Its hash is keccak of
//! its header's RLP, its transactions and receipts are the ones its
//! `transactionsRoot` and `receiptsRoot` commit to, each transaction's
//! signer is recovered from its signed bytes ([`super::jsonrpc`]). That is
//! one provider's inner consistency; the chain is the canonical one where
//! the second source reports the same hash ([`crate::canonical_confirm`]),
//! which now binds all of it.

use alloy_primitives::keccak256;
use darkbot_bsc_tx_encoding::rlp::{encode_bytes, encode_list, encode_u64};

use super::jsonrpc::{Log, Receipt};

/// The root of the ordered trie of `values` (keys `rlp(index)`): a
/// header's `transactionsRoot` or `receiptsRoot`.
pub(super) fn ordered_trie_root(values: &[Vec<u8>]) -> [u8; 32] {
    if values.is_empty() {
        return keccak256(encode_bytes(&[])).0;
    }
    let mut items: Vec<(Vec<u8>, &[u8])> = values
        .iter()
        .enumerate()
        .map(|(i, value)| (nibbles(&encode_u64(i as u64)), value.as_slice()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    keccak256(node(&items, 0)).0
}

fn nibbles(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().flat_map(|b| [b >> 4, b & 0x0f]).collect()
}

/// The hex-prefix encoding of `path` (a leaf's or an extension's).
fn compact(path: &[u8], leaf: bool) -> Vec<u8> {
    let flag = if leaf { 2 } else { 0 };
    let (mut out, rest) = match path.len() % 2 {
        1 => (vec![((flag + 1) << 4) | path[0]], &path[1..]),
        _ => (vec![flag << 4], path),
    };
    out.extend(rest.chunks_exact(2).map(|pair| (pair[0] << 4) | pair[1]));
    out
}

/// A child as its parent holds it: embedded below 32 bytes, else its hash.
fn reference(encoded: Vec<u8>) -> Vec<u8> {
    match encoded.len() {
        0..32 => encoded,
        _ => encode_bytes(&keccak256(&encoded).0),
    }
}

/// The encoded node of the sorted `items` below `depth` nibbles.
fn node(items: &[(Vec<u8>, &[u8])], depth: usize) -> Vec<u8> {
    if let [(key, value)] = items {
        return encode_list(&[
            encode_bytes(&compact(&key[depth..], true)),
            encode_bytes(value),
        ]);
    }
    let first = &items[0].0;
    let shared = (depth..first.len())
        .take_while(|&i| items.iter().all(|(key, _)| key.get(i) == Some(&first[i])))
        .count();
    if shared > 0 {
        let path = compact(&first[depth..depth + shared], false);
        return encode_list(&[encode_bytes(&path), reference(node(items, depth + shared))]);
    }
    // A branch: a key ending here is its value (it sorts first), the rest
    // grouped by their next nibble.
    let (value, mut rest): (&[u8], _) = match items {
        [(key, value), rest @ ..] if key.len() == depth => (value, rest),
        _ => (&[], items),
    };
    let mut slots = Vec::with_capacity(17);
    for nibble in 0..16 {
        let (group, after) = rest.split_at(rest.partition_point(|(key, _)| key[depth] == nibble));
        slots.push(match group {
            [] => encode_bytes(&[]),
            _ => reference(node(group, depth + 1)),
        });
        rest = after;
    }
    slots.push(encode_bytes(value));
    encode_list(&slots)
}

/// The logs bloom of `logs` (each address and topic, three bits).
pub(super) fn logs_bloom(logs: &[Log]) -> [u8; 256] {
    let mut bloom = [0u8; 256];
    for log in logs {
        let items =
            std::iter::once(log.address.as_slice()).chain(log.topics.iter().map(|t| t.as_slice()));
        for item in items {
            let hash = keccak256(item);
            for pair in hash[..6].chunks_exact(2) {
                let bit = usize::from(u16::from_be_bytes([pair[0], pair[1]]) & 2047);
                bloom[255 - bit / 8] |= 1 << (bit % 8);
            }
        }
    }
    bloom
}

/// A receipt as its block commits to it: `rlp([status, cumulativeGasUsed,
/// logsBloom, logs])`, led by its transaction's type unless legacy.
fn receipt_encoding(receipt: &Receipt) -> Vec<u8> {
    let logs: Vec<Vec<u8>> = receipt
        .logs
        .iter()
        .map(|log| {
            let topics: Vec<Vec<u8>> = log.topics.iter().map(|t| encode_bytes(t)).collect();
            encode_list(&[
                encode_bytes(&log.address),
                encode_list(&topics),
                encode_bytes(&log.data),
            ])
        })
        .collect();
    let body = encode_list(&[
        encode_u64(u64::from(receipt.succeeded)),
        encode_u64(receipt.cumulative_gas),
        encode_bytes(&logs_bloom(&receipt.logs)),
        encode_list(&logs),
    ]);
    match receipt.tx_type {
        0 => body,
        t => [vec![t], body].concat(),
    }
}

/// The `receiptsRoot` of a block with these `receipts`, in order.
pub(super) fn receipts_root(receipts: &[Receipt]) -> [u8; 32] {
    let encoded: Vec<Vec<u8>> = receipts.iter().map(receipt_encoding).collect();
    ordered_trie_root(&encoded)
}

#[cfg(test)]
mod tests {
    use super::super::jsonrpc::{parse_block, parse_header, parse_receipts};
    use super::super::{Field, Integrity, RequestError};
    use super::*;
    use alloy_primitives::hex;
    use serde_json::{Value, json};

    /// Real BSC blocks as the node sent them, with their receipts: legacy,
    /// type 1, 2, 3 (blob) and 4 (EIP-7702) transactions among them.
    const BLOCKS: [(u64, &str, &str); 5] = [
        (
            129_774_331,
            include_str!("../../tests/fixtures/chain/block_129774331.full.json"),
            include_str!("../../tests/fixtures/chain/block_129774331.receipts.json"),
        ),
        (
            129_781_119,
            include_str!("../../tests/fixtures/chain/block_129781119.full.json"),
            include_str!("../../tests/fixtures/chain/block_129781119.receipts.json"),
        ),
        (
            129_781_248,
            include_str!("../../tests/fixtures/chain/block_129781248.full.json"),
            include_str!("../../tests/fixtures/chain/block_129781248.receipts.json"),
        ),
        (
            129_842_141,
            include_str!("../../tests/fixtures/chain/block_129842141.full.json"),
            include_str!("../../tests/fixtures/chain/block_129842141.receipts.json"),
        ),
        (
            129_891_515,
            include_str!("../../tests/fixtures/chain/block_129891515.full.json"),
            include_str!("../../tests/fixtures/chain/block_129891515.receipts.json"),
        ),
    ];

    /// R7: every real block is its header's hash, every transaction object
    /// the transaction its hash names signed by its `from`, all of them the
    /// header's `transactionsRoot`, the receipts its `receiptsRoot`; the
    /// header alone (the second source's form) the same header.
    #[test]
    fn every_real_block_commits_to_its_transactions_and_receipts() {
        assert_eq!(
            ordered_trie_root(&[]),
            hex!("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421")
        );
        for (number, block, receipts) in BLOCKS {
            let full = parse_block(block.as_bytes(), 1, number).unwrap().unwrap();
            let listed = parse_receipts(receipts.as_bytes(), 1).unwrap().unwrap();
            assert_eq!(listed.len(), full.head.txs.len());
            assert_eq!(receipts_root(&listed), full.head.header.receipts_root);
            let header = parse_header(block.as_bytes(), 1, number).unwrap().unwrap();
            assert_eq!(header, full.head.header);
        }
    }

    /// R7: a block that contradicts itself is refused: a header field
    /// changed (its hash), missing, or an optional one after a missing one;
    /// a transaction's nonce (not the transaction its hash names), its
    /// `from` (not its signer), two of them swapped (not the root); a
    /// receipt's status, gas or log is not the receipts root.
    #[test]
    fn a_block_that_contradicts_itself_is_refused() {
        let (number, block, receipts) = BLOCKS[0];
        let edited = |edit: &dyn Fn(&mut Value)| {
            let mut answer: Value = serde_json::from_str(block).unwrap();
            edit(&mut answer["result"]);
            parse_block(answer.to_string().as_bytes(), 1, number).map(|b| b.map(|_| ()))
        };
        let integrity = |i| Err(RequestError::Integrity(i));
        let header_field = Err(RequestError::Shape(Field::Header));
        assert_eq!(edited(&|_| {}), Ok(Some(())));
        assert_eq!(
            edited(&|b| b["stateRoot"] = json!(format!("0x{}", "11".repeat(32)))),
            integrity(Integrity::HeaderHash)
        );
        for name in ["sha3Uncles", "withdrawalsRoot"] {
            let removed = edited(&|b| {
                b.as_object_mut().unwrap().remove(name);
            });
            assert_eq!(removed, header_field, "{name}");
        }
        let tx = |b: &mut Value, i: usize, field: &str, value: Value| {
            b["transactions"][i][field] = value;
        };
        assert_eq!(
            edited(&|b| tx(b, 3, "nonce", json!("0x7fffffff"))),
            integrity(Integrity::TxHash)
        );
        assert_eq!(
            edited(&|b| tx(b, 3, "from", json!(format!("0x{}", "11".repeat(20))))),
            integrity(Integrity::TxSigner)
        );
        // Review LOW-5: a field this build does not know with another hash
        // (a fork), a type it does not rebuild; a node's extra key alone
        // changes nothing.
        let other_hash = json!(format!("0x{}", "22".repeat(32)));
        assert_eq!(
            edited(&|b| {
                b["futureRoot"] = json!(format!("0x{}", "33".repeat(32)));
                b["hash"] = other_hash.clone();
            }),
            integrity(Integrity::UnknownHeaderField)
        );
        assert_eq!(
            edited(&|b| b["hash"] = other_hash.clone()),
            integrity(Integrity::HeaderHash)
        );
        assert_eq!(edited(&|b| b["nodeNote"] = json!("x")), Ok(Some(())));
        assert_eq!(
            edited(&|b| tx(b, 3, "type", json!("0x5"))),
            Err(RequestError::UnsupportedTxType(5))
        );
        // A typed object whose `v` is not its `yParity`.
        assert_eq!(
            edited(&|b| tx(b, 2, "v", json!("0x0"))),
            Err(RequestError::Shape(Field::TxFields))
        );
        assert_eq!(
            edited(&|b| {
                let txs = b["transactions"].as_array_mut().unwrap();
                txs.swap(3, 4);
                txs[3]["transactionIndex"] = json!("0x3");
                txs[4]["transactionIndex"] = json!("0x4");
            }),
            integrity(Integrity::TransactionsRoot)
        );
        let head = parse_block(block.as_bytes(), 1, number)
            .unwrap()
            .unwrap()
            .head;
        let root = head.header.receipts_root;
        let listed = parse_receipts(receipts.as_bytes(), 1).unwrap().unwrap();
        let i = listed.iter().position(|r| !r.logs.is_empty()).unwrap();
        let changes: [&dyn Fn(&mut Receipt); 3] = [
            &|r| r.succeeded = !r.succeeded,
            &|r| r.cumulative_gas += 1,
            &|r| r.logs[0].data.push(0),
        ];
        for change in changes {
            let mut other = listed.clone();
            change(&mut other[i]);
            assert_ne!(receipts_root(&other), root);
            assert!(!super::super::follow::receipts_match(&head, &other));
        }
        assert_eq!(receipts_root(&listed), root);
        assert!(super::super::follow::receipts_match(&head, &listed));
        // Review M1: a stated cost the root does not commit to is its gas
        // and its transaction's price, or the receipts are refused.
        let costs: [&dyn Fn(&mut Receipt); 2] =
            [&|r| r.cost.as_mut().unwrap().gas_used += 1, &|r| {
                r.cost.as_mut().unwrap().effective_gas_price += 1
            }];
        for change in costs {
            let mut other = listed.clone();
            change(&mut other[i]);
            assert_eq!(receipts_root(&other), root);
            assert!(!super::super::follow::receipts_match(&head, &other));
        }
    }

    /// R7: a legacy transaction without EIP-155 (`v` 27/28) is signed over
    /// its six fields alone: its signer is recovered, its bytes its hash.
    #[test]
    fn a_transaction_before_eip155_recovers_its_signer() {
        use super::super::tests_seal::{address_of, signed_legacy};
        for key in [[0x11u8; 32], [0x12; 32], [0x13; 32]] {
            let object = signed_legacy(&key, b"pre-155", 4, None);
            let v = object["v"].as_str().unwrap();
            assert!(v == "0x1b" || v == "0x1c", "{v}");
            let (raw, signature) =
                super::super::jsonrpc::signed_tx(object.as_object().unwrap()).unwrap();
            assert_eq!(signature.signer(), Ok(address_of(&key)));
            assert_eq!(
                hex::encode(keccak256(&raw)),
                object["hash"].as_str().unwrap()[2..]
            );
        }
    }
}
