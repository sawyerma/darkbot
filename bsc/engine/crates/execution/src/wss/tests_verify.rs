//! Verification against real BSC mainnet transactions (fixtures fetched
//! once with `eth_getTransactionByHash` from a public read-only RPC; the
//! JSON is the provider wire format, unchanged).

use super::jsonrpc::{data_fixed, data_vec, quantity_u64, quantity_u256};
use super::*;
use alloy_primitives::keccak256;
use darkbot_bsc_geth_ipc::ingress::decode_tx_target;
use serde_json::{Value, json};

const LEGACY: &str = include_str!("../../tests/fixtures/wss/legacy.json");
const TYPE1: &str = include_str!("../../tests/fixtures/wss/type1.json");
const TYPE2: &str = include_str!("../../tests/fixtures/wss/type2.json");

fn fixtures(text: &str) -> Vec<Value> {
    serde_json::from_str(text).unwrap()
}

fn all_fixtures() -> Vec<Value> {
    [LEGACY, TYPE1, TYPE2]
        .iter()
        .flat_map(|t| fixtures(t))
        .collect()
}

fn verify_value(tx: &Value) -> Result<VerifiedTx, RejectReason> {
    verify_tx_object(&serde_json::to_vec(tx).unwrap(), BSC_MAINNET_CHAIN_ID)
}

fn s<'a>(tx: &'a Value, key: &str) -> &'a str {
    tx[key].as_str().unwrap()
}

/// Flips the lowest bit of the last hex digit (stays canonical hex).
fn flip_last(hex: &str) -> String {
    let (head, last) = hex.split_at(hex.len() - 1);
    let d = u8::from_str_radix(last, 16).unwrap() ^ 1;
    format!("{head}{d:x}")
}

fn with(tx: &Value, key: &str, value: Value) -> Value {
    let mut t = tx.clone();
    t[key] = value;
    t
}

fn without(tx: &Value, key: &str) -> Value {
    let mut t = tx.clone();
    t.as_object_mut().unwrap().remove(key);
    t
}

#[test]
fn real_bsc_transactions_verify_and_bind_every_reported_field() {
    let mut per_type = [0usize; 3];
    let mut creations = 0;
    let mut access_list_keys = 0;
    for tx in all_fixtures() {
        let v = verify_value(&tx).unwrap_or_else(|r| panic!("{} rejected: {r:?}", s(&tx, "hash")));
        let ty = quantity_u64(s(&tx, "type")).unwrap();
        per_type[ty as usize] += 1;
        assert_eq!(v.hash, data_fixed::<32>(s(&tx, "hash")).unwrap());
        assert_eq!(keccak256(&v.raw).0, v.hash, "raw hashes to the hash");
        assert_eq!(v.from, data_fixed::<20>(s(&tx, "from")).unwrap());
        assert_eq!(
            v.to,
            tx["to"].as_str().map(|t| data_fixed::<20>(t).unwrap())
        );
        assert_eq!(v.nonce, quantity_u64(s(&tx, "nonce")).unwrap());
        assert_eq!(v.chain_id, Some(56));
        assert_eq!(v.tx_type, (ty != 0).then_some(ty as u8));
        let target = decode_tx_target(&v.raw).unwrap();
        assert_eq!(target.data, data_vec(s(&tx, "input")).unwrap().as_slice());
        creations += usize::from(v.to.is_none());
        access_list_keys += tx["accessList"].as_array().map_or(0, |l| {
            l.iter()
                .map(|e| e["storageKeys"].as_array().unwrap().len())
                .sum()
        });
    }
    assert_eq!(per_type, [5, 5, 5], "legacy EIP-155, type 1, type 2");
    assert!(creations >= 1, "a contract creation is covered");
    assert!(access_list_keys > 0, "a non-empty access list is covered");
}

/// Every single-field change of a verified object is rejected, each by the
/// check that binds that field.
#[test]
fn every_single_field_tamper_is_rejected() {
    use RejectReason::*;
    let mut checked = 0;
    for tx in all_fixtures() {
        let legacy = s(&tx, "type") == "0x0";
        let flip = |key: &str| with(&tx, key, json!(flip_last(s(&tx, key))));
        let mut cases: Vec<(&str, Value, RejectReason)> = vec![
            ("hash", flip("hash"), HashMismatch),
            ("from", flip("from"), SignerMismatch),
            ("value", flip("value"), HashMismatch),
            ("nonce", flip("nonce"), HashMismatch),
            ("gas", flip("gas"), HashMismatch),
            ("r", flip("r"), HashMismatch),
            ("s", flip("s"), HashMismatch),
        ];
        let input = s(&tx, "input");
        let tampered_input = if input == "0x" {
            "0x00".to_owned()
        } else {
            flip_last(input)
        };
        cases.push((
            "input",
            with(&tx, "input", json!(tampered_input)),
            HashMismatch,
        ));
        let tampered_to = match tx["to"].as_str() {
            Some(to) => json!(flip_last(to)),
            None => json!(format!("0x{}", "11".repeat(20))),
        };
        cases.push(("to", with(&tx, "to", tampered_to), HashMismatch));
        if legacy {
            let v = quantity_u64(s(&tx, "v")).unwrap();
            let other_parity = if v == 147 { 148 } else { 147 };
            cases.push((
                "v parity",
                with(&tx, "v", json!(format!("0x{other_parity:x}"))),
                HashMismatch,
            ));
            // v of chain 97 (Chapel), same parity.
            let chapel_v = 35 + 2 * 97 + (v - 35) % 2;
            cases.push((
                "v chain",
                with(
                    &without(&tx, "chainId"),
                    "v",
                    json!(format!("0x{chapel_v:x}")),
                ),
                WrongChain(97),
            ));
            cases.push((
                "chainId",
                with(&tx, "chainId", json!("0x61")),
                ChainIdMismatch,
            ));
            cases.push(("gasPrice", flip("gasPrice"), HashMismatch));
        } else {
            cases.push((
                "chainId",
                with(&tx, "chainId", json!("0x61")),
                WrongChain(97),
            ));
            cases.push(("v only", flip("v"), YParityMismatch));
            cases.push((
                "v and yParity",
                with(&flip("v"), "yParity", json!(flip_last(s(&tx, "yParity")))),
                HashMismatch,
            ));
            let mut extended = tx["accessList"].as_array().unwrap().clone();
            extended.push(json!({"address": format!("0x{}", "22".repeat(20)), "storageKeys": []}));
            cases.push((
                "accessList",
                with(&tx, "accessList", Value::Array(extended)),
                HashMismatch,
            ));
            if s(&tx, "type") == "0x1" {
                cases.push(("gasPrice", flip("gasPrice"), HashMismatch));
            } else {
                cases.push(("maxFeePerGas", flip("maxFeePerGas"), HashMismatch));
                cases.push((
                    "maxPriorityFeePerGas",
                    flip("maxPriorityFeePerGas"),
                    HashMismatch,
                ));
            }
        }
        for (name, tampered, want) in cases {
            assert_eq!(
                verify_value(&tampered),
                Err(want),
                "{} tampered {name}",
                s(&tx, "hash")
            );
            checked += 1;
        }
    }
    assert!(checked >= 15 * 12, "{checked} tamper cases");
}

#[test]
fn unsupported_pre_eip155_missing_and_malformed_are_rejected() {
    use RejectReason::*;
    let legacy = fixtures(LEGACY).remove(0);
    let typed = fixtures(TYPE2).remove(0);
    for ty in ["0x3", "0x4", "0x7e"] {
        let t = u8::from_str_radix(&ty[2..], 16).unwrap();
        assert_eq!(
            verify_value(&with(&typed, "type", json!(ty))),
            Err(UnsupportedType(t))
        );
    }
    assert_eq!(
        verify_value(&with(&typed, "type", json!("0x100"))),
        Err(Malformed(TxField::Type))
    );
    for v in ["0x1b", "0x1c"] {
        assert_eq!(
            verify_value(&with(&without(&legacy, "chainId"), "v", json!(v))),
            Err(PreEip155)
        );
    }
    assert_eq!(
        verify_value(&with(&legacy, "v", json!("0x22"))),
        Err(Malformed(TxField::V))
    );
    for (key, field) in [
        ("type", TxField::Type),
        ("hash", TxField::Hash),
        ("from", TxField::From),
        ("to", TxField::To),
        ("r", TxField::R),
        ("chainId", TxField::ChainId),
        ("accessList", TxField::AccessList),
        ("maxFeePerGas", TxField::MaxFeePerGas),
    ] {
        assert_eq!(
            verify_value(&without(&typed, key)),
            Err(Missing(field)),
            "{key}"
        );
    }
    assert_eq!(
        verify_value(&without(&without(&typed, "v"), "yParity")),
        Err(Missing(TxField::YParity))
    );
    // Non-canonical quantities, odd or non-hex data, wrong widths.
    let long_hash = format!("0x{}", "0".repeat(66));
    let long_r = format!("0x1{}", "0".repeat(64));
    for (key, bad, field) in [
        ("nonce", "0x01", TxField::Nonce),
        ("value", "0x", TxField::Value),
        ("gas", "12", TxField::Gas),
        ("input", "0x0", TxField::Input),
        ("input", "0xzz", TxField::Input),
        ("from", "0x1234", TxField::From),
        ("hash", long_hash.as_str(), TxField::Hash),
        ("r", long_r.as_str(), TxField::R),
    ] {
        assert_eq!(
            verify_value(&with(&typed, key, json!(bad))),
            Err(Malformed(field)),
            "{key}={bad}"
        );
    }
    assert_eq!(
        verify_value(&with(&typed, "yParity", json!("0x2"))),
        Err(YParityMismatch)
    );
    assert_eq!(verify_tx_object(b"[1,2]", 56), Err(NotATransaction));
    assert_eq!(verify_tx_object(b"not json", 56), Err(NotATransaction));
    // A real mainnet object is not accepted for another expected chain.
    assert_eq!(
        verify_tx_object(&serde_json::to_vec(&typed).unwrap(), 97),
        Err(WrongChain(56))
    );
}

#[test]
fn hex_quantities_and_data_are_strict() {
    assert_eq!(quantity_u64("0x0"), Some(0));
    assert_eq!(quantity_u64("0xffffffffffffffff"), Some(u64::MAX));
    assert_eq!(quantity_u64("0x1ffffffffffffffff"), None);
    assert_eq!(quantity_u64("0x00"), None);
    assert_eq!(quantity_u64("0x"), None);
    assert_eq!(quantity_u64("ff"), None);
    assert_eq!(quantity_u64("0xg"), None);
    let mut one = [0u8; 32];
    one[31] = 1;
    assert_eq!(quantity_u256("0x1"), Some(one));
    let mut odd = [0u8; 32];
    odd[30] = 0x01;
    odd[31] = 0x23;
    assert_eq!(quantity_u256("0x123"), Some(odd));
    assert_eq!(
        quantity_u256(&format!("0x{}", "f".repeat(64))),
        Some([0xff; 32])
    );
    assert_eq!(quantity_u256(&format!("0x{}", "f".repeat(65))), None);
    assert_eq!(data_vec("0x"), Some(vec![]));
    assert_eq!(data_vec("0xABcd"), Some(vec![0xab, 0xcd]));
    assert_eq!(data_vec("0xabc"), None);
    assert_eq!(data_fixed::<2>("0xabcd"), Some([0xab, 0xcd]));
    assert_eq!(data_fixed::<2>("0xab"), None);
}

/// BSC3-063: the source's libsecp256k1 recovery accepts and refuses exactly
/// what the core's k256 `recover_signer` does, with the same error: k256
/// signatures of many keys, their other parity, their high-`s` twin, `r`
/// or `s` zero, at the order or one below it, `s` at and above n/2, an `r`
/// that is no point's x, prehashes 0, n, n + 1 and 2^256 - 1, and a
/// signature whose recovered key is the point at infinity.
#[test]
fn the_source_recovers_exactly_what_the_core_recovers() {
    use darkbot_bsc_trading::signer::{SignerError, recover_signer};
    use darkbot_bsc_trading::transactions::legacy::RecoverableSignature;
    use k256::elliptic_curve::PrimeField;
    use k256::elliptic_curve::point::AffineCoordinates;
    use k256::{ProjectivePoint, Scalar};
    let u256 = alloy_primitives::U256::from_be_bytes::<32>;
    let bytes = |x: alloy_primitives::U256| x.to_be_bytes::<32>();
    let order: [u8; 32] =
        alloy_primitives::hex!("fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141");
    let half: [u8; 32] =
        alloy_primitives::hex!("7fffffffffffffffffffffffffffffff5d576e7357a4501ddfe92f46681b20a0");
    let one = alloy_primitives::U256::from(1u8);
    let one_less = bytes(u256(order) - one);
    let above_half = bytes(u256(half) + one);
    let edge_prehashes = [[0u8; 32], order, bytes(u256(order) + one), [0xff; 32]];
    let mut errors = std::collections::BTreeSet::new();
    let mut recovered = 0;
    let mut check = |prehash: [u8; 32], r: [u8; 32], s: [u8; 32], y_parity: bool| {
        let core = recover_signer(
            &alloy_primitives::B256::from(prehash),
            &RecoverableSignature { r, s, y_parity },
        )
        .map(|a| a.0.0);
        let source = verify::recover(&prehash, &r, &s, y_parity);
        assert_eq!(
            source, core,
            "prehash {prehash:02x?} r {r:02x?} s {s:02x?} parity {y_parity}"
        );
        match source {
            Ok(_) => recovered += 1,
            Err(e) => {
                errors.insert(format!("{e:?}"));
            }
        }
        source
    };
    let mut seed = [0x5au8; 32];
    for n in 0..100u32 {
        seed = keccak256(seed).0;
        let key = k256::ecdsa::SigningKey::from_slice(&seed).unwrap();
        let prehash = keccak256(n.to_be_bytes()).0;
        let (signature, id) = key.sign_prehash_recoverable(&prehash).unwrap();
        let sig = signature.to_bytes();
        let (r, s): ([u8; 32], [u8; 32]) =
            (sig[..32].try_into().unwrap(), sig[32..].try_into().unwrap());
        let parity = id.is_y_odd();
        for (r, s, y_parity) in [
            (r, s, parity),
            (r, s, !parity),
            (r, bytes(u256(order) - u256(s)), !parity),
            ([0; 32], s, parity),
            (r, [0; 32], parity),
            (order, s, parity),
            (r, order, parity),
            (one_less, s, parity),
            (r, half, parity),
            (r, above_half, parity),
            (keccak256(seed).0, s, parity),
        ] {
            let _ = check(prehash, r, s, y_parity);
        }
        for prehash in edge_prehashes {
            let _ = check(prehash, r, s, parity);
            let _ = check(prehash, r, s, !parity);
        }
        // The point at infinity: R = kG, s = 1 and z = k (mod n) make
        // r⁻¹(sR − zG) the identity.
        let k = Scalar::from_repr(keccak256(seed).0.into())
            .into_option()
            .filter(|k| !bool::from(k.is_zero()))
            .unwrap_or(Scalar::ONE);
        let big_r = (ProjectivePoint::GENERATOR * k).to_affine();
        let x: [u8; 32] = big_r.x().into();
        if x < order {
            let mut s_one = [0u8; 32];
            s_one[31] = 1;
            let infinity = check(k.to_repr().into(), x, s_one, bool::from(big_r.y_is_odd()));
            assert_eq!(infinity, Err(SignerError::RecoveryFailed));
        }
    }
    // Most signatures and the boundary ones recover; every refusal of the
    // core occurs.
    assert!(recovered > 300, "{recovered}");
    for e in [
        SignerError::InvalidSignature,
        SignerError::HighS,
        SignerError::RecoveryFailed,
    ] {
        assert!(
            errors.contains(&format!("{e:?}")),
            "{e:?} never occurred: {errors:?}"
        );
    }
}
