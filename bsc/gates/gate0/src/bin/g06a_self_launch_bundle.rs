//! G0.6a controlled Mainnet self-launch ordered-bundle DRY harness.
//!
//! IMPORTANT: this binary has deliberately no networking code. It may build valid
//! Mainnet transaction bytes in memory, but it never writes them to a socket or
//! external process. Before live execution a separate, explicitly armed binary/path
//! must pass acceptance_guard::verify_live_authorization().

use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use alloy_sol_types::{sol, SolCall};
use darkbot_gate0::acceptance_guard::{inspect_v2, sha256_hex, FROZEN_APPROVED_V2_SHA256};
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};
use serde_json::{json, Value};
use std::{env, fs, path::Path, str::FromStr};

const CHAIN_ID: u64 = 56;
const PORTAL: &str = "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0";
const V2_PATH: &str =
    "bsc/gates/gate0/evidence/g01_mainnet_acceptance_proposal_v2.json";
const BUY_VALUE_WEI_DEFAULT: u64 = 1_000_000_000_000_000; // 0.001 BNB
const SLIPPAGE_BPS: u64 = 100;

sol! {
    struct LaunchParams {
        string name;
        string symbol;
        string uri;
        uint8 dexThresh;
        bytes32 salt;
        uint8 migratorType;
        address quoteToken;
        uint256 quoteAmt;
        address beneficiary;
        bytes permit;
        bytes32 extensionId;
        bytes extensionData;
        uint8 dexId;
        uint8 lpFeeProfile;
        uint16 buyTax;
        uint16 sellTax;
        uint64 taxDuration;
        uint64 antiFarmerDuration;
        uint16 mktBps;
        uint16 deflationBps;
        uint16 dividendBps;
        uint16 lpBps;
        uint256 minimumShareBalance;
        address dividendToken;
        address commissionReceiver;
        uint8 tokenVersion;
    }

    function newTokenV6(LaunchParams params) external payable returns(address);

    struct SwapParams {
        address inputToken;
        address outputToken;
        uint256 inputAmount;
        uint256 minOutputAmount;
        bytes permit;
    }

    function swapExactInput(SwapParams params) external payable returns(uint256);
}

fn req_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn env_u64(name: &str) -> u64 {
    req_env(name)
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be decimal u64"))
}

fn env_u128(name: &str) -> u128 {
    req_env(name)
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be decimal u128"))
}

fn key_from_env(name: &str) -> SigningKey {
    let raw = req_env(name);
    let bytes = hex::decode(raw.trim().trim_start_matches("0x"))
        .unwrap_or_else(|_| panic!("{name} is not valid hex"));
    assert_eq!(bytes.len(), 32, "{name} must be exactly 32 bytes");
    SigningKey::from_slice(&bytes).unwrap_or_else(|_| panic!("{name} is not a valid secp256k1 key"))
}

fn signer_address(key: &SigningKey) -> Address {
    let point = key.verifying_key().to_encoded_point(false);
    let h = keccak256(&point.as_bytes()[1..]);
    Address::from_slice(&h.as_slice()[12..])
}

fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    if b.len() == 1 && b[0] < 0x80 {
        return b.to_vec();
    }
    if b.len() <= 55 {
        let mut out = Vec::with_capacity(1 + b.len());
        out.push(0x80 + b.len() as u8);
        out.extend_from_slice(b);
        return out;
    }
    let len_bytes = b.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];
    let mut out = Vec::with_capacity(1 + lb.len() + b.len());
    out.push(0xb7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(b);
    out
}

fn rlp_u64(v: u64) -> Vec<u8> {
    if v == 0 {
        return rlp_bytes(&[]);
    }
    let b = v.to_be_bytes();
    let first = b.iter().position(|x| *x != 0).unwrap();
    rlp_bytes(&b[first..])
}

fn rlp_u256(v: U256) -> Vec<u8> {
    if v == U256::ZERO {
        return rlp_bytes(&[]);
    }
    let b = v.to_be_bytes::<32>();
    let first = b.iter().position(|x| *x != 0).unwrap();
    rlp_bytes(&b[first..])
}

fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len: usize = items.iter().map(Vec::len).sum();
    let mut payload = Vec::with_capacity(payload_len);
    for item in items {
        payload.extend_from_slice(item);
    }
    if payload.len() <= 55 {
        let mut out = Vec::with_capacity(1 + payload.len());
        out.push(0xc0 + payload.len() as u8);
        out.extend_from_slice(&payload);
        return out;
    }
    let len_bytes = payload.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];
    let mut out = Vec::with_capacity(1 + lb.len() + payload.len());
    out.push(0xf7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(&payload);
    out
}

#[derive(Clone)]
struct SignedLegacy {
    raw: Vec<u8>,
    hash: B256,
    nonce: u64,
    gas_price: u64,
    gas_limit: u64,
    to: Address,
    value: U256,
    calldata: Vec<u8>,
}

fn sign_legacy(
    signer: &SigningKey,
    nonce: u64,
    gas_price: u64,
    gas_limit: u64,
    to: Address,
    value: U256,
    calldata: Vec<u8>,
) -> SignedLegacy {
    let unsigned_items = vec![
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(to.as_slice()),
        rlp_u256(value),
        rlp_bytes(&calldata),
        rlp_u64(CHAIN_ID),
        rlp_bytes(&[]),
        rlp_bytes(&[]),
    ];
    let sighash = keccak256(rlp_list(&unsigned_items));
    let (sig, rid) = signer
        .sign_prehash_recoverable(sighash.as_slice())
        .expect("signing failed");
    let sb = sig.to_bytes();
    let v = 35u64 + 2 * CHAIN_ID + rid.to_byte() as u64;

    let raw = rlp_list(&[
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(to.as_slice()),
        rlp_u256(value),
        rlp_bytes(&calldata),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sb[..32])),
        rlp_u256(U256::from_be_slice(&sb[32..])),
    ]);

    SignedLegacy {
        hash: keccak256(&raw),
        raw,
        nonce,
        gas_price,
        gas_limit,
        to,
        value,
        calldata,
    }
}

fn rlp_item(input: &[u8], mut p: usize) -> (Vec<u8>, usize) {
    let b = input[p];
    if b <= 0x7f {
        return (vec![b], p + 1);
    }
    if b <= 0xb7 {
        let len = (b - 0x80) as usize;
        p += 1;
        return (input[p..p + len].to_vec(), p + len);
    }
    let ll = (b - 0xb7) as usize;
    p += 1;
    let mut len = 0usize;
    for x in &input[p..p + ll] {
        len = (len << 8) | *x as usize;
    }
    p += ll;
    (input[p..p + len].to_vec(), p + len)
}

fn decode_rlp_list_payload(raw: &[u8]) -> &[u8] {
    let b = raw[0];
    if (0xc0..=0xf7).contains(&b) {
        let len = (b - 0xc0) as usize;
        assert_eq!(raw.len(), 1 + len, "bad short RLP list length");
        &raw[1..]
    } else {
        assert!(b >= 0xf8, "expected RLP list");
        let ll = (b - 0xf7) as usize;
        let mut len = 0usize;
        for x in &raw[1..1 + ll] {
            len = (len << 8) | *x as usize;
        }
        assert_eq!(raw.len(), 1 + ll + len, "bad long RLP list length");
        &raw[1 + ll..]
    }
}

fn bytes_u64(b: &[u8]) -> u64 {
    assert!(b.len() <= 8);
    b.iter().fold(0u64, |a, x| (a << 8) | *x as u64)
}

fn recover_sender(tx: &SignedLegacy) -> Address {
    let payload = decode_rlp_list_payload(&tx.raw);
    let mut p = 0usize;
    let mut fields = Vec::with_capacity(9);
    for _ in 0..9 {
        let (v, next) = rlp_item(payload, p);
        fields.push(v);
        p = next;
    }
    assert_eq!(p, payload.len(), "trailing legacy RLP bytes");

    let nonce = bytes_u64(&fields[0]);
    let gas_price = bytes_u64(&fields[1]);
    let gas_limit = bytes_u64(&fields[2]);
    let to = Address::from_slice(&fields[3]);
    let value = if fields[4].is_empty() { U256::ZERO } else { U256::from_be_slice(&fields[4]) };
    let calldata = fields[5].clone();
    let v = bytes_u64(&fields[6]);

    assert_eq!(nonce, tx.nonce);
    assert_eq!(gas_price, tx.gas_price);
    assert_eq!(gas_limit, tx.gas_limit);
    assert_eq!(to, tx.to);
    assert_eq!(value, tx.value);
    assert_eq!(calldata, tx.calldata);

    let base = 35 + 2 * CHAIN_ID;
    assert!(v == base || v == base + 1, "wrong EIP-155 v/chain");
    let rid = RecoveryId::from_byte((v - base) as u8).expect("bad recovery id");
    let r = U256::from_be_slice(&fields[7]).to_be_bytes::<32>();
    let s = U256::from_be_slice(&fields[8]).to_be_bytes::<32>();
    let sig = Signature::from_scalars(r, s).expect("bad signature scalars");

    let unsigned = rlp_list(&[
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(to.as_slice()),
        rlp_u256(value),
        rlp_bytes(&calldata),
        rlp_u64(CHAIN_ID),
        rlp_bytes(&[]),
        rlp_bytes(&[]),
    ]);
    let sighash = keccak256(unsigned);
    let vk = VerifyingKey::recover_from_prehash(sighash.as_slice(), &sig, rid)
        .expect("signature recovery failed");
    let point = vk.to_encoded_point(false);
    let h = keccak256(&point.as_bytes()[1..]);
    Address::from_slice(&h.as_slice()[12..])
}

fn predicted_token(portal: Address, impl_addr: Address, salt: B256) -> Address {
    let mut init = hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73").unwrap();
    init.extend_from_slice(impl_addr.as_slice());
    init.extend_from_slice(&hex::decode("5af43d82803e903d91602b57fd5bf3").unwrap());

    let mut preimage = Vec::with_capacity(85);
    preimage.push(0xff);
    preimage.extend_from_slice(portal.as_slice());
    preimage.extend_from_slice(salt.as_slice());
    preimage.extend_from_slice(keccak256(init).as_slice());
    let digest = keccak256(preimage);
    Address::from_slice(&digest.as_slice()[12..])
}

fn launch_calldata(launcher: Address, salt: B256) -> Vec<u8> {
    newTokenV6Call {
        params: LaunchParams {
            name: "DarkBot G06A Self Launch".into(),
            symbol: "DBG06A".into(),
            uri: "ipfs://darkbot-g06a-dry".into(),
            dexThresh: 1,
            salt,
            migratorType: 1,
            quoteToken: Address::ZERO,
            quoteAmt: U256::ZERO,
            beneficiary: launcher,
            permit: Bytes::new(),
            extensionId: B256::ZERO,
            extensionData: Bytes::new(),
            dexId: 0,
            lpFeeProfile: 0,
            buyTax: 100,
            sellTax: 100,
            taxDuration: 31_536_000,
            antiFarmerDuration: 86_400,
            mktBps: 10_000,
            deflationBps: 0,
            dividendBps: 0,
            lpBps: 0,
            minimumShareBalance: U256::ZERO,
            dividendToken: Address::ZERO,
            commissionReceiver: Address::ZERO,
            tokenVersion: 6,
        },
    }
    .abi_encode()
}

fn buy_calldata(token: Address, input: U256, minimum: U256) -> Vec<u8> {
    swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: input,
            minOutputAmount: minimum,
            permit: Bytes::new(),
        },
    }
    .abi_encode()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    assert!(
        args.len() == 1 || (args.len() == 3 && args[1] == "--verify-evidence"),
        "usage: g06a_self_launch_bundle [--verify-evidence <local_simulation.json>]"
    );

    let contract = inspect_v2(Path::new(V2_PATH)).expect("v2 contract invalid");
    assert!(
        !contract.approved && !contract.execution_ready,
        "DRY harness expects v2 to remain unapproved/unready"
    );
    assert_eq!(
        FROZEN_APPROVED_V2_SHA256, "UNARMED",
        "DRY harness must not run after source guard is armed"
    );

    let launcher_key = key_from_env("DARKBOT_G06A_LAUNCHER_PRIVATE_KEY");
    let buyer_key = key_from_env("DARKBOT_G06A_BUYER_PRIVATE_KEY");
    let launcher = signer_address(&launcher_key);
    let buyer = signer_address(&buyer_key);
    assert_ne!(launcher, buyer, "DRY-1 fail: launcher and buyer are identical");

    let portal = Address::from_str(PORTAL).unwrap();
    let impl_addr = Address::from_str(&req_env("DARKBOT_G06A_TAX_V3_IMPL"))
        .expect("DARKBOT_G06A_TAX_V3_IMPL invalid");
    let launcher_nonce = env_u64("DARKBOT_G06A_LAUNCHER_NONCE");
    let buyer_nonce = env_u64("DARKBOT_G06A_BUYER_NONCE");
    let gas_price = env_u64("DARKBOT_G06A_GAS_PRICE_WEI");
    let launch_gas_limit = env_u64("DARKBOT_G06A_LAUNCH_GAS_LIMIT");
    let buy_gas_limit = env_u64("DARKBOT_G06A_BUY_GAS_LIMIT");
    let launch_value = env_u128("DARKBOT_G06A_LAUNCH_VALUE_WEI");
    let buy_value = env::var("DARKBOT_G06A_BUY_VALUE_WEI")
        .ok()
        .map(|x| x.parse::<u64>().expect("DARKBOT_G06A_BUY_VALUE_WEI invalid"))
        .unwrap_or(BUY_VALUE_WEI_DEFAULT);
    let local_expected_output = U256::from_str(&req_env("DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT"))
        .expect("DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT invalid");
    assert!(local_expected_output > U256::ZERO);

    let salt = B256::from_str(&req_env("DARKBOT_G06A_SALT"))
        .expect("DARKBOT_G06A_SALT must be 32-byte hex");
    let token = predicted_token(portal, impl_addr, salt);
    let min_output = local_expected_output * U256::from(10_000 - SLIPPAGE_BPS)
        / U256::from(10_000u64);
    assert!(min_output > U256::ZERO);

    let launch = sign_legacy(
        &launcher_key,
        launcher_nonce,
        gas_price,
        launch_gas_limit,
        portal,
        U256::from(launch_value),
        launch_calldata(launcher, salt),
    );
    let buy = sign_legacy(
        &buyer_key,
        buyer_nonce,
        gas_price,
        buy_gas_limit,
        portal,
        U256::from(buy_value),
        buy_calldata(token, U256::from(buy_value), min_output),
    );

    // Independent decode+recover from final RLP bytes, not from the original key objects.
    let recovered_launcher = recover_sender(&launch);
    let recovered_buyer = recover_sender(&buy);
    assert_eq!(recovered_launcher, launcher, "DRY-1 launch signer recovery mismatch");
    assert_eq!(recovered_buyer, buyer, "DRY-1 buyer signer recovery mismatch");
    assert_ne!(recovered_launcher, recovered_buyer, "DRY-1 recovered wallets identical");

    let send_head = env_u64("DARKBOT_G06A_SEND_HEAD");
    let max_block = env_u64("DARKBOT_G06A_MAX_BLOCK");
    assert!(
        max_block >= send_head && max_block <= send_head + 2,
        "DRY-4 expiry must be within send_head..=send_head+2"
    );

    // Exact same transaction bytes are projected into both builder envelopes.
    // These strings remain in RAM only. This binary has no networking code.
    let launch_hex = format!("0x{}", hex::encode(&launch.raw));
    let buy_hex = format!("0x{}", hex::encode(&buy.raw));
    let br = json!({
        "jsonrpc":"2.0",
        "id":1,
        "method":"eth_sendBundle",
        "params":[{
            "txs":[launch_hex.clone(), buy_hex.clone()],
            "maxBlockNumber": format!("0x{max_block:x}")
        }]
    });
    let c48 = json!({
        "jsonrpc":"2.0",
        "id":1,
        "method":"eth_sendBundle",
        "params":[{
            "txs":[launch_hex.clone(), buy_hex.clone()],
            "maxBlockNumber": max_block,
            "noMerge": true,
            "positionFirst": true
        }]
    });

    let br_txs = br.pointer("/params/0/txs").unwrap().as_array().unwrap();
    let c48_txs = c48.pointer("/params/0/txs").unwrap().as_array().unwrap();
    assert_eq!(br_txs, c48_txs, "DRY-1 builder transaction bytes differ");
    assert_eq!(br_txs.len(), 2);
    assert_eq!(br_txs[0].as_str().unwrap(), launch_hex);
    assert_eq!(br_txs[1].as_str().unwrap(), buy_hex);

    let br_digest = sha256_hex(serde_json::to_vec(&br).unwrap().as_slice());
    let c48_digest = sha256_hex(serde_json::to_vec(&c48).unwrap().as_slice());

    let worst_case_gas = (launch_gas_limit as u128 + buy_gas_limit as u128)
        .checked_mul(gas_price as u128)
        .expect("gas budget overflow");
    let reserved = launch_value
        .checked_add(buy_value as u128)
        .and_then(|x| x.checked_add(worst_case_gas))
        .expect("attempt budget overflow");
    assert!(
        reserved <= 11_000_000_000_000_000u128,
        "DRY-3 precheck: attempt reserve exceeds v2 proposal ceiling"
    );

    let mut dry2 = false;
    let mut dry3 = false;
    let mut dry4_nonce = false;

    if args.len() == 3 {
        let evidence: Value = serde_json::from_slice(
            &fs::read(&args[2]).expect("cannot read local simulation evidence")
        ).expect("invalid local simulation evidence JSON");

        assert_eq!(
            evidence["launch_tx_hash"].as_str().unwrap().to_ascii_lowercase(),
            launch.hash.to_string().to_ascii_lowercase(),
            "DRY-2 evidence launch hash mismatch"
        );
        assert_eq!(
            evidence["buy_tx_hash"].as_str().unwrap().to_ascii_lowercase(),
            buy.hash.to_string().to_ascii_lowercase(),
            "DRY-2 evidence buy hash mismatch"
        );
        assert_eq!(
            evidence["token"].as_str().unwrap().to_ascii_lowercase(),
            token.to_string().to_ascii_lowercase(),
            "DRY-2 evidence token mismatch"
        );

        let simulated_output =
            U256::from_str(evidence["simulated_received_amount"].as_str().unwrap()).unwrap();
        assert_eq!(
            simulated_output, local_expected_output,
            "DRY-2 independent simulated output != local calculation"
        );
        assert!(min_output <= simulated_output && min_output > U256::ZERO);
        assert_eq!(evidence["launch_success"].as_bool(), Some(true));
        assert_eq!(evidence["buy_success"].as_bool(), Some(true));
        dry2 = true;

        let launch_gas_used = evidence["launch_gas_used"].as_u64().unwrap();
        let buy_gas_used = evidence["buy_gas_used"].as_u64().unwrap();
        assert!(launch_gas_used <= launch_gas_limit);
        assert!(buy_gas_used <= buy_gas_limit);
        let strict_floor = evidence["strictest_verified_builder_floor_wei"].as_u64().unwrap();
        assert!(
            gas_price >= strict_floor,
            "DRY-3 signed gas price below strictest verified builder floor"
        );
        let simulated_cost = launch_value
            + buy_value as u128
            + (launch_gas_used as u128 + buy_gas_used as u128) * gas_price as u128;
        assert!(
            simulated_cost <= 11_000_000_000_000_000u128,
            "DRY-3 simulated attempt cost exceeds v2 proposal ceiling"
        );
        dry3 = true;

        assert_eq!(
            evidence["launcher_pending_nonce"].as_u64(),
            Some(launcher_nonce),
            "DRY-4 launcher nonce mismatch"
        );
        assert_eq!(
            evidence["buyer_pending_nonce"].as_u64(),
            Some(buyer_nonce),
            "DRY-4 buyer nonce mismatch"
        );
        dry4_nonce = true;
    }

    let dry1 = true;
    // Structural egress proof: this binary contains no TcpStream, UDP, HTTP client,
    // process spawn or file write of valid transaction/bundle bytes.
    let dry4_zero_egress = true;
    let dry4 = dry4_nonce && dry4_zero_egress;
    let all = dry1 && dry2 && dry3 && dry4;

    println!("{}", serde_json::to_string_pretty(&json!({
        "status": "G0.6A_SELF_LAUNCH_BUNDLE_DRY",
        "contract": {
            "path": V2_PATH,
            "sha256": contract.sha256,
            "approved": contract.approved,
            "execution_ready": contract.execution_ready,
            "compiled_live_hash": FROZEN_APPROVED_V2_SHA256
        },
        "wallets": {
            "launcher": launcher.to_string(),
            "buyer": buyer.to_string(),
            "distinct": launcher != buyer
        },
        "transactions": {
            "chain_id": CHAIN_ID,
            "portal": PORTAL,
            "token": token.to_string(),
            "launch_tx_hash": launch.hash.to_string(),
            "buy_tx_hash": buy.hash.to_string(),
            "min_output": min_output.to_string(),
            "local_expected_output": local_expected_output.to_string(),
            "gas_price_wei": gas_price,
            "launch_gas_limit": launch_gas_limit,
            "buy_gas_limit": buy_gas_limit,
            "launch_value_wei": launch_value.to_string(),
            "buy_value_wei": buy_value.to_string()
        },
        "bundle_projection": {
            "order": ["launch_A","buy_B"],
            "identical_txs_across_builders": true,
            "blockrazor_payload_sha256": br_digest,
            "fortyeightclub_payload_sha256": c48_digest,
            "send_head": send_head,
            "max_block": max_block,
            "fortyeightclub_noMerge": true,
            "fortyeightclub_positionFirst": true
        },
        "budget": {
            "worst_case_reserved_wei": reserved.to_string(),
            "v2_proposal_per_attempt_ceiling_wei": "11000000000000000"
        },
        "dry_pass": {
            "DRY_1_bytes_and_signers": dry1,
            "DRY_2_independent_local_simulation": dry2,
            "DRY_3_gas_floor_and_budget": dry3,
            "DRY_4_expiry_nonces_zero_egress": dry4,
            "zero_egress_structural": dry4_zero_egress,
            "all": all
        },
        "network_write_capability": "NONE_IN_THIS_BINARY",
        "live_submission": "IMPOSSIBLE_FROM_G06A_DRY_BINARY"
    })).unwrap());

    if !all {
        std::process::exit(2);
    }
}
