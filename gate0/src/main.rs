use alloy_primitives::{keccak256, Address, Bytes, U256};
use alloy_sol_types::{sol, SolCall};
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use std::str::FromStr;

sol! {
    struct SwapParams {
        address inputToken;
        address outputToken;
        uint256 inputAmount;
        uint256 minOutputAmount;
        bytes permit;
    }

    function swapExactInput(SwapParams params) external payable returns (uint256);
}

fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    if b.len() == 1 && b[0] < 0x80 {
        return b.to_vec();
    }

    if b.len() <= 55 {
        let mut out = vec![0x80 + b.len() as u8];
        out.extend_from_slice(b);
        return out;
    }

    let len = b.len();
    let len_bytes = len.to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];

    let mut out = vec![0xb7 + lb.len() as u8];
    out.extend_from_slice(lb);
    out.extend_from_slice(b);
    out
}

fn rlp_hex(s: &str) -> Vec<u8> {
    let mut b = hex::decode(s.trim_start_matches("0x")).unwrap();
    while b.first() == Some(&0) {
        b.remove(0);
    }
    rlp_bytes(&b)
}

fn rlp_list(items: Vec<Vec<u8>>) -> Vec<u8> {
    let payload: Vec<u8> = items.into_iter().flatten().collect();

    if payload.len() <= 55 {
        let mut out = vec![0xc0 + payload.len() as u8];
        out.extend(payload);
        return out;
    }

    let len_bytes = payload.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];

    let mut out = vec![0xf7 + lb.len() as u8];
    out.extend_from_slice(lb);
    out.extend(payload);
    out
}

fn main() {
    let portal = Address::from_str(
        "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0"
    ).unwrap();

    let token = Address::from_str(
        "0xabe1fe16cf6bc48c9524713054efe6e954b17777"
    ).unwrap();

    let call = swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: U256::from(150_000_000_000_000_000u128),
            minOutputAmount: U256::from_str(
                "6000000000000000000000000"
            ).unwrap(),
            permit: Bytes::new(),
        },
    };

    let calldata = call.abi_encode();

    // EIP-155 signing preimage:
    // nonce, gasPrice, gasLimit, to, value, data, chainId, 0, 0
    let unsigned = rlp_list(vec![
        rlp_hex("032e"),
        rlp_hex("127a3980"),
        rlp_hex("16e360"),
        rlp_bytes(portal.as_slice()),
        rlp_hex("0214e8348c4f0000"),
        rlp_bytes(&calldata),
        rlp_hex("38"),
        rlp_bytes(&[]),
        rlp_bytes(&[]),
    ]);

    let sighash = keccak256(&unsigned);

    let r = hex::decode(
        "66e5778143debc8df45dd1f47d2fb5cea41d4a8c49abd312e0847a30ef4bcda0"
    ).unwrap();

    let s = hex::decode(
        "6a90536f139884940cacd213f0aebf1873569889f800cbfc802056329de86771"
    ).unwrap();

    let mut rs = Vec::with_capacity(64);
    rs.extend_from_slice(&r);
    rs.extend_from_slice(&s);

    let sig = Signature::from_slice(&rs).unwrap();

    // v = 0x93 = 147
    // 147 - (35 + 2*56) = 0
    let recovery_id = RecoveryId::from_byte(0).unwrap();

    let vk =
        VerifyingKey::recover_from_prehash(sighash.as_slice(), &sig, recovery_id).unwrap();

    let pubkey = vk.to_encoded_point(false);
    let pubkey_hash = keccak256(&pubkey.as_bytes()[1..]);
    let recovered_sender = Address::from_slice(&pubkey_hash.as_slice()[12..]);

    let expected_sender = Address::from_str(
        "0xb6864c017e9d9d89012d4f90ee382bf1461e4c90"
    ).unwrap();

    assert_eq!(recovered_sender, expected_sender);

    // Signed historical raw TX:
    let signed = rlp_list(vec![
        rlp_hex("032e"),
        rlp_hex("127a3980"),
        rlp_hex("16e360"),
        rlp_bytes(portal.as_slice()),
        rlp_hex("0214e8348c4f0000"),
        rlp_bytes(&calldata),
        rlp_hex("93"),
        rlp_bytes(&r),
        rlp_bytes(&s),
    ]);

    let tx_hash = keccak256(&signed);

    let expected_tx_hash = hex::decode(
        "8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b"
    ).unwrap();

    assert_eq!(tx_hash.as_slice(), expected_tx_hash.as_slice());

    println!("G0.3 STEP4 PASS");
    println!("recovered_sender={recovered_sender}");
    println!("tx_hash=0x{}", hex::encode(tx_hash));
    println!("chain_id=56");
    println!("y_parity=0");
}
