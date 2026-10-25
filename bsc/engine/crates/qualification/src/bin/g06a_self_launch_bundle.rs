//! G0.6a controlled Mainnet self-launch ordered-bundle DRY harness.
//!
//! IMPORTANT: this binary has deliberately no networking code. It may build valid
//! Mainnet transaction bytes in memory, but it never writes them to a socket or
//! external process. Before live execution a separate, explicitly armed binary/path
//! must pass acceptance_guard::verify_live_authorization().
//!
//! It reads the binary local-simulation record and writes the binary DRY
//! record (DBREC001, native schema with provenance); stdout is the record's
//! derived view. No JSON: the builder envelopes are projected with the same
//! fixed wire templates G0.7 sends, held in RAM only.

use alloy_primitives::{Address, B256, Bytes, U256, keccak256};
use alloy_sol_types::{SolCall, sol};
use darkbot_bsc_execution::acceptance_guard::{
    FROZEN_APPROVED_V5_SHA256, V3_RELATIVE_PATH, inspect_v3,
};
use darkbot_bsc_record::schema::{
    PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL,
    g06a::{dry, local_simulation as sim},
};
use darkbot_bsc_record::{Draft, Value, decode};
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};
use k256::elliptic_curve::zeroize::Zeroizing;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, path::Path, str::FromStr};

const CHAIN_ID: u64 = 56;
const PORTAL: &str = "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0";
// The single (binary v3) contract path is owned by the acceptance guard.
const CONTRACT_PATH: &str = V3_RELATIVE_PATH;
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

// BSC-007 (BSC_Bug_V2 P2-D5): the live runner's key rule (g07 inputs.rs).
// Both keys are read once into zeroizing strings, every key variable is
// cleared before parsing (also on a missing or malformed key), and the hex
// is decoded only into a zeroizing fixed buffer.
fn parse_key(raw: &str) -> Result<SigningKey, &'static str> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(raw.trim().trim_start_matches("0x"), &mut bytes[..])
        .map_err(|_| "a wallet key must be exactly 32 bytes of hex")?;
    SigningKey::from_slice(&bytes[..]).map_err(|_| "a wallet key is not a valid secp256k1 key")
}

// Callbacks let the test check the clearing order without mutating the
// multithreaded test harness's environment.
fn read_and_clear_keys(
    read: impl Fn(&str) -> Option<String>,
    clear: impl Fn(&str),
) -> Result<(SigningKey, SigningKey), &'static str> {
    let launcher = read("DARKBOT_G06A_LAUNCHER_PRIVATE_KEY").map(Zeroizing::new);
    let buyer = read("DARKBOT_G06A_BUYER_PRIVATE_KEY").map(Zeroizing::new);
    for name in [
        "DARKBOT_G06A_LAUNCHER_PRIVATE_KEY",
        "DARKBOT_G06A_BUYER_PRIVATE_KEY",
        "BSC_LAUNCHER_PRIVATE_KEY",
        "BSC_BUYER_PRIVATE_KEY",
    ] {
        clear(name);
    }
    let launcher = launcher.ok_or("DARKBOT_G06A_LAUNCHER_PRIVATE_KEY is required")?;
    let buyer = buyer.ok_or("DARKBOT_G06A_BUYER_PRIVATE_KEY is required")?;
    Ok((parse_key(&launcher)?, parse_key(&buyer)?))
}

/// # Safety
/// `main` only, while the process is single-threaded: this binary starts no
/// thread and no child process.
unsafe fn take_keys() -> (SigningKey, SigningKey) {
    read_and_clear_keys(
        |name| env::var(name).ok(),
        // SAFETY: the caller guarantees exclusive access to the environment.
        |name| unsafe { env::remove_var(name) },
    )
    .unwrap_or_else(|e| panic!("{e}"))
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
    let value = if fields[4].is_empty() {
        U256::ZERO
    } else {
        U256::from_be_slice(&fields[4])
    };
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

/// The BlockRazor `eth_sendBundle` body exactly as G0.7 frames it.
fn blockrazor_body(request_id: &str, txs: [&str; 2], max_block: u64) -> String {
    format!(
        r#"{{"id":"{request_id}","jsonrpc":"2.0","method":"eth_sendBundle","params":[{{"maxBlockNumber":{max_block},"txs":["{}","{}"]}}]}}"#,
        txs[0], txs[1]
    )
}

/// The 48Club `eth_sendBundle` body exactly as G0.7 frames it. G0.7 sets
/// maxTimestamp from the send head at the release; the DRY projection has
/// no send head and uses 0, so its digest describes the template only.
fn club48_body(request_id: &str, txs: [&str; 2], max_block: u64, max_timestamp: u64) -> String {
    format!(
        r#"{{"id":"{request_id}","jsonrpc":"2.0","method":"eth_sendBundle","params":[{{"maxBlockNumber":{max_block},"maxTimestamp":{max_timestamp},"noMerge":true,"positionFirst":true,"txs":["{}","{}"]}}]}}"#,
        txs[0], txs[1]
    )
}

fn is_wire_hex(text: &str) -> bool {
    text.strip_prefix("0x")
        .is_some_and(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn tag(name: &str) -> u16 {
    dry::NATIVE
        .fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| f.tag)
        .unwrap_or_else(|| panic!("native DRY schema has no field {name}"))
}

fn word(x: U256) -> Value {
    Value::U256(x.to_be_bytes())
}

const MAX_ATTEMPT_WEI: u128 = 481_163_500_000_000;
fn within_attempt_budget(cost: Option<u128>) -> bool {
    cost.is_some_and(|cost| cost <= MAX_ATTEMPT_WEI)
}

#[cfg(test)]
mod budget_regression {
    use super::*;
    #[test]
    fn frozen_cap_and_overflow_fail_closed() {
        assert!(within_attempt_budget(Some(MAX_ATTEMPT_WEI)));
        assert!(!within_attempt_budget(Some(MAX_ATTEMPT_WEI+1)));
        assert!(!within_attempt_budget(u128::MAX.checked_add(1)));
        assert!(within_attempt_budget(Some((6_603_270+3_000_000)*50_000_000+1_000_000_000_000)));
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let (evidence_path, out_path) = match args.as_slice() {
        [flag, out] if flag == "--out" => (None, out.clone()),
        [verify, evidence, flag, out] if verify == "--verify-evidence" && flag == "--out" => {
            (Some(evidence.clone()), out.clone())
        }
        _ => panic!(
            "usage: g06a_self_launch_bundle [--verify-evidence <local_simulation.dbrec>] --out <dry_result.dbrec>"
        ),
    };

    // The DRY of attempt 1 is complete and frozen; this harness only runs
    // while the guard is unarmed (the compiled hash, not the contract's
    // approval flags, is the execution authority).
    let contract = inspect_v3(Path::new(CONTRACT_PATH)).expect("v3 contract invalid");
    assert_eq!(
        FROZEN_APPROVED_V5_SHA256, "UNARMED",
        "DRY harness must not run after source guard is armed"
    );

    let allow_same_wallet_test = env::var("DARKBOT_G06A_ALLOW_SAME_WALLET_TEST")
        .ok()
        .as_deref()
        == Some("YES");

    // SAFETY: single-threaded main; no thread or child process exists.
    let (launcher_key, buyer_key) = unsafe { take_keys() };
    let launcher = signer_address(&launcher_key);
    let buyer = signer_address(&buyer_key);
    let wallets_distinct = launcher != buyer;
    if !wallets_distinct {
        assert!(
            allow_same_wallet_test,
            "DRY-1 fail: launcher and buyer are identical; set DARKBOT_G06A_ALLOW_SAME_WALLET_TEST=YES only for the non-qualifying local test mode"
        );
    }

    let portal = Address::from_str(PORTAL).unwrap();
    let impl_addr = Address::from_str(&req_env("DARKBOT_G06A_TAX_V3_IMPL"))
        .expect("DARKBOT_G06A_TAX_V3_IMPL invalid");
    let launcher_nonce = env_u64("DARKBOT_G06A_LAUNCHER_NONCE");
    let buyer_nonce = env_u64("DARKBOT_G06A_BUYER_NONCE");
    if wallets_distinct {
        // Independent wallets may have unrelated pending nonces.
    } else {
        assert_eq!(
            buyer_nonce,
            launcher_nonce + 1,
            "same-wallet test requires BUY nonce = launch nonce + 1"
        );
    }
    let gas_price = env_u64("DARKBOT_G06A_GAS_PRICE_WEI");
    let launch_gas_limit = env_u64("DARKBOT_G06A_LAUNCH_GAS_LIMIT");
    let buy_gas_limit = env_u64("DARKBOT_G06A_BUY_GAS_LIMIT");
    let launch_value = env_u128("DARKBOT_G06A_LAUNCH_VALUE_WEI");
    let buy_value = env::var("DARKBOT_G06A_BUY_VALUE_WEI")
        .ok()
        .map(|x| {
            x.parse::<u64>()
                .expect("DARKBOT_G06A_BUY_VALUE_WEI invalid")
        })
        .unwrap_or(BUY_VALUE_WEI_DEFAULT);
    let local_expected_output = U256::from_str(&req_env("DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT"))
        .expect("DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT invalid");
    assert!(local_expected_output > U256::ZERO);

    let salt = B256::from_str(&req_env("DARKBOT_G06A_SALT"))
        .expect("DARKBOT_G06A_SALT must be 32-byte hex");
    let token = predicted_token(portal, impl_addr, salt);
    let min_output =
        local_expected_output * U256::from(10_000 - SLIPPAGE_BPS) / U256::from(10_000u64);
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
    // The signing keys end with the two signatures (SigningKey zeroizes on drop).
    drop((launcher_key, buyer_key));

    // Independent decode+recover from final RLP bytes, not from the original key objects.
    let recovered_launcher = recover_sender(&launch);
    let recovered_buyer = recover_sender(&buy);
    assert_eq!(
        recovered_launcher, launcher,
        "DRY-1 launch signer recovery mismatch"
    );
    assert_eq!(
        recovered_buyer, buyer,
        "DRY-1 buyer signer recovery mismatch"
    );
    if wallets_distinct {
        assert_ne!(
            recovered_launcher, recovered_buyer,
            "DRY-1 recovered wallets identical"
        );
    } else {
        assert_eq!(
            recovered_launcher, recovered_buyer,
            "same-wallet test recovery mismatch"
        );
    }

    let send_head = env_u64("DARKBOT_G06A_SEND_HEAD");
    let max_block = env_u64("DARKBOT_G06A_MAX_BLOCK");
    assert!(
        max_block >= send_head && max_block <= send_head + 2,
        "DRY-4 expiry must be within send_head..=send_head+2"
    );

    // Exact same transaction bytes are projected into both builder envelopes,
    // framed with G0.7's fixed templates. These strings remain in RAM only.
    // This binary has no networking code.
    let launch_hex = format!("0x{}", hex::encode(&launch.raw));
    let buy_hex = format!("0x{}", hex::encode(&buy.raw));
    let request_id = buy.hash.to_string();
    assert!(is_wire_hex(&launch_hex) && is_wire_hex(&buy_hex) && is_wire_hex(&request_id));
    let txs = [launch_hex.as_str(), buy_hex.as_str()];
    let br = blockrazor_body(&request_id, txs, max_block);
    let c48 = club48_body(&request_id, txs, max_block, 0);
    let tx_list = format!("\"txs\":[\"{launch_hex}\",\"{buy_hex}\"]");
    assert!(
        br.contains(&tx_list) && c48.contains(&tx_list),
        "DRY-1 builder transaction bytes differ"
    );
    let br_digest = darkbot_bsc_record::sha256(br.as_bytes());
    let c48_digest = darkbot_bsc_record::sha256(c48.as_bytes());

    let worst_case_gas = (launch_gas_limit as u128 + buy_gas_limit as u128)
        .checked_mul(gas_price as u128)
        .expect("gas budget overflow");
    let reserved = launch_value
        .checked_add(buy_value as u128)
        .and_then(|x| x.checked_add(worst_case_gas))
        .expect("attempt budget overflow");
    assert!(
        within_attempt_budget(Some(reserved)),
        "DRY-3 precheck: attempt reserve exceeds frozen G0.7 ceiling"
    );

    let mut dry2 = false;
    let mut dry3 = false;
    let mut dry4_nonce = false;

    if let Some(path) = &evidence_path {
        let evidence = decode(
            &sim::NATIVE,
            &fs::read(path).expect("cannot read local simulation record"),
        )
        .expect("invalid local simulation record");
        let field = |name: &str| {
            let tag = sim::NATIVE
                .fields
                .iter()
                .find(|f| f.name == name)
                .map(|f| f.tag)
                .unwrap_or_else(|| panic!("local simulation schema has no field {name}"));
            evidence
                .get(tag)
                .cloned()
                .unwrap_or_else(|| panic!("local simulation lacks {name}"))
        };
        let u64_of = |name: &str| match field(name) {
            Value::U64(n) => n,
            other => panic!("{name} is {other:?}"),
        };

        assert_eq!(
            field("launch_tx_hash"),
            Value::B256(launch.hash.0),
            "DRY-2 evidence launch hash mismatch"
        );
        assert_eq!(
            field("buy_tx_hash"),
            Value::B256(buy.hash.0),
            "DRY-2 evidence buy hash mismatch"
        );
        assert_eq!(
            field("token"),
            Value::Address(token.into_array()),
            "DRY-2 evidence token mismatch"
        );

        let simulated_output = match field("simulated_received_amount") {
            Value::U256(w) => U256::from_be_bytes(w),
            other => panic!("simulated_received_amount is {other:?}"),
        };
        assert_eq!(
            simulated_output, local_expected_output,
            "DRY-2 independent simulated output != local calculation"
        );
        assert!(min_output <= simulated_output && min_output > U256::ZERO);
        assert_eq!(field("launch_success"), Value::Bool(true));
        assert_eq!(field("buy_success"), Value::Bool(true));
        assert_eq!(field("output_difference"), Value::Text("0".into()));
        dry2 = true;

        let launch_gas_used = u64_of("launch_gas_used");
        let buy_gas_used = u64_of("buy_gas_used");
        assert!(launch_gas_used <= launch_gas_limit);
        assert!(buy_gas_used <= buy_gas_limit);
        let strict_floor = match field("strictest_verified_builder_floor_wei") {
            Value::U256(w) => U256::from_be_bytes(w),
            other => panic!("strictest_verified_builder_floor_wei is {other:?}"),
        };
        assert!(
            U256::from(gas_price) >= strict_floor,
            "DRY-3 signed gas price below strictest verified builder floor"
        );
        let simulated_cost = (launch_gas_used as u128 + buy_gas_used as u128).checked_mul(gas_price as u128)
            .and_then(|gas| launch_value.checked_add(buy_value as u128).and_then(|value| value.checked_add(gas)));
        assert!(
            within_attempt_budget(simulated_cost),
            "DRY-3 simulated attempt cost exceeds v2 proposal ceiling"
        );
        dry3 = true;

        assert_eq!(
            u64_of("launcher_pending_nonce"),
            launcher_nonce,
            "DRY-4 launcher nonce mismatch"
        );
        if wallets_distinct {
            assert_eq!(
                u64_of("buyer_pending_nonce"),
                buyer_nonce,
                "DRY-4 buyer nonce mismatch"
            );
            dry4_nonce = true;
        } else {
            // With one sender, both read-only pending-nonce queries return the same current nonce.
            // The BUY is intentionally signed at N+1 and therefore cannot satisfy the two-wallet
            // DRY-4 contract. This mode is diagnostic only.
            assert_eq!(
                u64_of("buyer_pending_nonce"),
                launcher_nonce,
                "same-wallet pending nonce mismatch"
            );
            dry4_nonce = false;
        }
    }

    let dry1 = wallets_distinct;
    // Structural egress proof: this binary contains no TcpStream, UDP, HTTP client,
    // process spawn or file write of valid transaction/bundle bytes.
    let dry4_zero_egress = true;
    let dry4 = dry4_nonce && dry4_zero_egress;
    let all = dry1 && dry2 && dry3 && dry4;

    let observed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut d = Draft::new(&dry::NATIVE);
    d.set(PROVENANCE_TOOL, "g06a_self_launch_bundle")
        .set(PROVENANCE_OBSERVED_UNIX_MS, observed)
        .set(dry::STATUS, "G0.6A_SELF_LAUNCH_BUNDLE_DRY")
        .set(tag("contract.path"), CONTRACT_PATH)
        .set(
            tag("contract.sha256"),
            Value::B256(B256::from_str(&contract.sha256).expect("contract sha").0),
        )
        .set(tag("contract.approved"), contract.approved)
        .set(tag("contract.execution_ready"), contract.execution_ready)
        .set(tag("contract.compiled_live_hash"), FROZEN_APPROVED_V5_SHA256)
        .set(dry::LAUNCHER, Value::Address(launcher.into_array()))
        .set(dry::BUYER, Value::Address(buyer.into_array()))
        .set(dry::DISTINCT, wallets_distinct)
        .set(
            tag("wallets.same_wallet_test_mode"),
            allow_same_wallet_test && !wallets_distinct,
        )
        .set(dry::CHAIN_ID, CHAIN_ID)
        .set(dry::PORTAL, Value::Address(portal.into_array()))
        .set(dry::TOKEN, Value::Address(token.into_array()))
        .set(dry::LAUNCH_TX_HASH, Value::B256(launch.hash.0))
        .set(dry::BUY_TX_HASH, Value::B256(buy.hash.0))
        .set(dry::MIN_OUTPUT, word(min_output))
        .set(dry::LOCAL_EXPECTED_OUTPUT, word(local_expected_output))
        .set(dry::GAS_PRICE_WEI, gas_price)
        .set(dry::LAUNCH_GAS_LIMIT, launch_gas_limit)
        .set(dry::BUY_GAS_LIMIT, buy_gas_limit)
        .set(dry::LAUNCH_VALUE_WEI, Value::u128(launch_value))
        .set(dry::BUY_VALUE_WEI, Value::u128(u128::from(buy_value)))
        .push(tag("bundle_projection.order"), "launch_A")
        .push(tag("bundle_projection.order"), "buy_B")
        .set(tag("bundle_projection.identical_txs_across_builders"), true)
        .set(
            tag("bundle_projection.blockrazor_payload_sha256"),
            Value::B256(br_digest),
        )
        .set(
            tag("bundle_projection.fortyeightclub_payload_sha256"),
            Value::B256(c48_digest),
        )
        .set(tag("bundle_projection.send_head"), send_head)
        .set(tag("bundle_projection.max_block"), max_block)
        .set(tag("bundle_projection.fortyeightclub_noMerge"), true)
        .set(tag("bundle_projection.fortyeightclub_positionFirst"), true)
        .set(tag("budget.worst_case_reserved_wei"), Value::u128(reserved))
        .set(
            tag("budget.v2_proposal_per_attempt_ceiling_wei"),
            Value::u128(10_603_270_000_000_000),
        )
        .set(dry::DRY_1, dry1)
        .set(dry::DRY_2, dry2)
        .set(dry::DRY_3, dry3)
        .set(dry::DRY_4, dry4)
        .set(dry::ZERO_EGRESS_STRUCTURAL, dry4_zero_egress)
        .set(dry::DRY_ALL, all)
        .set(
            tag("qualification_note"),
            if wallets_distinct {
                darkbot_bsc_qualification::DRY_STRUCTURAL_SCOPE_NOTE
            } else {
                "SAME_WALLET_TEST_ONLY: construction/recovery diagnostics only; DRY-1 and DRY-4 intentionally cannot pass"
            },
        )
        .set(dry::NETWORK_WRITE_CAPABILITY, "NONE_IN_THIS_BINARY")
        .set(tag("live_submission"), "IMPOSSIBLE_FROM_G06A_DRY_BINARY");
    let record = d.encode().expect("encode DRY record");
    let view = decode(&dry::NATIVE, &record)
        .expect("decode DRY record")
        .render();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&out_path)
        .expect("create-new DRY record");
    file.write_all(&record)
        .and_then(|()| file.sync_all())
        .expect("write DRY record");
    print!("{view}");

    if !all {
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BSC-007 (BSC_Bug_V2 P2-D5): both keys are read once, every key
    /// variable is cleared before parsing -- also when a key is missing or
    /// malformed -- and decoded only into a zeroizing fixed buffer.
    #[test]
    fn keys_are_read_once_and_cleared_before_parsing() {
        use std::cell::RefCell;
        let one = format!("0x{}1", "0".repeat(63));
        let two = format!("0x{}2", "0".repeat(63));
        let cleared = RefCell::new(Vec::new());
        let take = |launcher: Option<&str>, buyer: Option<&str>| {
            cleared.borrow_mut().clear();
            let keys = read_and_clear_keys(
                |name| match name {
                    "DARKBOT_G06A_LAUNCHER_PRIVATE_KEY" => launcher.map(str::to_string),
                    "DARKBOT_G06A_BUYER_PRIVATE_KEY" => buyer.map(str::to_string),
                    other => panic!("unexpected read of {other}"),
                },
                |name| cleared.borrow_mut().push(name.to_string()),
            );
            let mut names = cleared.borrow().clone();
            names.sort();
            assert_eq!(
                names,
                [
                    "BSC_BUYER_PRIVATE_KEY",
                    "BSC_LAUNCHER_PRIVATE_KEY",
                    "DARKBOT_G06A_BUYER_PRIVATE_KEY",
                    "DARKBOT_G06A_LAUNCHER_PRIVATE_KEY"
                ]
            );
            keys
        };
        let (launcher, buyer) = take(Some(&one), Some(&two)).unwrap();
        assert_eq!(
            signer_address(&launcher),
            Address::from_str("0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf").unwrap()
        );
        assert_eq!(
            signer_address(&buyer),
            Address::from_str("0x2B5AD5c4795c026514f8317c7a215E218DcCD6cF").unwrap()
        );
        for (launcher, buyer) in [
            (None, Some(two.as_str())),
            (Some(one.as_str()), None),
            (Some("0x1234"), Some(two.as_str())),
            (Some(one.as_str()), Some(&two.replace('2', "g"))),
            (Some(&format!("0x{}", "0".repeat(64))), Some(two.as_str())),
        ] {
            assert!(take(launcher, buyer).is_err());
        }
        let source = include_str!("g06a_self_launch_bundle.rs");
        let source = &source[..source.find(concat!("#[cfg(test)]\n", "mod tests")).unwrap()];
        assert!(!source.contains(concat!("fn key_", "from_env")));
        assert!(source.contains("let (launcher_key, buyer_key) = unsafe { take_keys() };"));
        let dropped = source.find("drop((launcher_key, buyer_key));").unwrap();
        assert!(source.rfind("&buyer_key,").unwrap() < dropped);
    }

    /// The projection bodies are byte-identical to G0.7's golden wire format
    /// (crates/execution .../builder_submission.rs).
    #[test]
    fn projection_bodies_are_the_g07_wire_format() {
        assert_eq!(
            club48_body("0xb2", ["0xaa", "0xbb"], 103, 1_002),
            r#"{"id":"0xb2","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":103,"maxTimestamp":1002,"noMerge":true,"positionFirst":true,"txs":["0xaa","0xbb"]}]}"#
        );
        assert_eq!(
            blockrazor_body("0xb2", ["0xaa", "0xbb"], 103),
            r#"{"id":"0xb2","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":103,"txs":["0xaa","0xbb"]}]}"#
        );
        assert!(
            is_wire_hex("0x0a") && !is_wire_hex("0x") && !is_wire_hex("0xg1") && !is_wire_hex("aa")
        );
        // Every native DRY field the harness sets exists.
        for name in [
            "contract.path",
            "bundle_projection.order",
            "live_submission",
        ] {
            let _ = tag(name);
        }
    }
}
