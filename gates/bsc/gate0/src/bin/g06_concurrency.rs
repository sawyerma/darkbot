use darkbot_gate0::intent_lane::IntentLane;
use alloy_primitives::{keccak256, Address, Bytes, U256};
use alloy_sol_types::{sol, SolCall};
use k256::ecdsa::SigningKey;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    env,
    fs,
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Barrier, Mutex,
    },
    thread,
    time::Instant,
};

sol! {
    struct SwapParams {
        address inputToken;
        address outputToken;
        uint256 inputAmount;
        uint256 minOutputAmount;
        bytes permit;
    }
    function swapExactInput(SwapParams params) external payable returns(uint256);
}

fn n(s: &str) -> U256 {
    U256::from_str(s).unwrap()
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

fn rlp_list(items: Vec<Vec<u8>>) -> Vec<u8> {
    let payload_len: usize = items.iter().map(Vec::len).sum();
    let mut payload = Vec::with_capacity(payload_len);
    for item in items {
        payload.extend_from_slice(&item);
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

fn derive_token(base: Address, launch_id: u64) -> Address {
    let mut preimage = Vec::with_capacity(28);
    preimage.extend_from_slice(base.as_slice());
    preimage.extend_from_slice(&launch_id.to_be_bytes());
    let d = keccak256(preimage);
    Address::from_slice(&d.as_slice()[12..])
}

fn make_calldata(token: Address, input: U256, minimum: U256) -> Vec<u8> {
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

fn sign_legacy(
    nonce: u64,
    portal: Address,
    input: U256,
    calldata: &[u8],
    signer: &SigningKey,
) -> Vec<u8> {
    let unsigned = rlp_list(vec![
        rlp_u64(nonce),
        rlp_u64(310_000_000),
        rlp_u64(1_500_000),
        rlp_bytes(portal.as_slice()),
        rlp_u256(input),
        rlp_bytes(calldata),
        rlp_u64(56),
        rlp_bytes(&[]),
        rlp_bytes(&[]),
    ]);
    let sighash = keccak256(unsigned);
    let (sig, recovery_id) = signer
        .sign_prehash_recoverable(sighash.as_slice())
        .unwrap();
    let sig_bytes = sig.to_bytes();
    let v = 35u64 + 2 * 56 + recovery_id.to_byte() as u64;
    rlp_list(vec![
        rlp_u64(nonce),
        rlp_u64(310_000_000),
        rlp_u64(1_500_000),
        rlp_bytes(portal.as_slice()),
        rlp_u256(input),
        rlp_bytes(calldata),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sig_bytes[..32])),
        rlp_u256(U256::from_be_slice(&sig_bytes[32..])),
    ])
}

#[derive(Clone)]
struct ResultRow {
    launch_id: u64,
    nonce: u64,
    token: Address,
    tx_hash: [u8; 32],
    calldata_hash: [u8; 32],
    adapter_projection_ok: bool,
    elapsed_ns: u64,
}

fn pct(mut v: Vec<u64>, num: usize) -> u64 {
    v.sort_unstable();
    let idx = ((v.len() - 1) * num) / 100;
    v[idx]
}

fn main() {
    let args: Vec<String> = env::args().collect();
    assert!(
        (3..=5).contains(&args.len()),
        "g06_concurrency <g03_rust_inputs.json> <g03_rust_output.json> [workers] [intents]"
    );

    let fi: Value =
        serde_json::from_str(&fs::read_to_string(&args[1]).unwrap()).unwrap();
    let fo: Value =
        serde_json::from_str(&fs::read_to_string(&args[2]).unwrap()).unwrap();

    let workers: usize = args
        .get(3)
        .map(|x| x.parse().unwrap())
        .unwrap_or_else(|| thread::available_parallelism().map(|x| x.get()).unwrap_or(4).min(8));
    let intents: usize = args
        .get(4)
        .map(|x| x.parse().unwrap())
        .unwrap_or(1000);

    assert!(workers >= 2, "workers must be >= 2");
    assert!(intents >= workers, "intents must be >= workers");

    let portal = Address::from_str(
        fo["portal"]
            .as_str()
            .unwrap_or("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0"),
    )
    .unwrap();
    let base_token = Address::from_str(fi["expected_token"].as_str().unwrap()).unwrap();
    assert_eq!(
        base_token,
        Address::from_str(fo["token"].as_str().unwrap()).unwrap(),
        "G0.3 input/output fixture mismatch"
    );

    let input = n(fi["native_input"].as_str().unwrap());
    let minimum = n(fo["min_output"].as_str().unwrap());

    // Fixed non-funded test key. No network submission exists in this binary.
    let signer = SigningKey::from_bytes((&[0x22u8; 32]).into()).unwrap();

    let next_index = AtomicUsize::new(0);
    let public_key = signer.verifying_key().to_encoded_point(false);
    let sender_hash = keccak256(&public_key.as_bytes()[1..]);
    let sender = Address::from_slice(&sender_hash.as_slice()[12..]);
    let lane = Mutex::new(IntentLane::new(56, sender, BASE_NONCE, intents));
    let results: Mutex<Vec<ResultRow>> = Mutex::new(Vec::with_capacity(intents));
    let start_barrier = Barrier::new(workers);
    const BASE_NONCE: u64 = 0x700000;

    let wall = Instant::now();

    thread::scope(|scope| {
        for _ in 0..workers {
            let signer = signer.clone();
            let next_index = &next_index;
            let lane = &lane;
            let results = &results;
            let barrier = &start_barrier;

            scope.spawn(move || {
                barrier.wait();
                loop {
                    let idx = next_index.fetch_add(1, Ordering::Relaxed);
                    if idx >= intents {
                        break;
                    }

                    let launch_id = idx as u64;
                    let t0 = Instant::now();

                    let token = derive_token(base_token, launch_id);
                    let ticket = lane.lock().unwrap().reserve(token).unwrap();
                    let nonce = ticket.nonce();
                    let calldata = make_calldata(token, input, minimum);
                    let calldata_hash = keccak256(&calldata);
                    let signed = sign_legacy(nonce, portal, input, &calldata, &signer);
                    let tx_hash = keccak256(&signed);
                    lane.lock().unwrap().prepare(ticket, tx_hash).unwrap();
                    let raw_hex = hex::encode(&signed);

                    // G0.6 adapter projection only: same signed transaction must be
                    // represented identically in both candidate builder envelopes.
                    let br = format!(
                        r#"{{"jsonrpc":"2.0","id":2,"method":"eth_sendBundle","params":[{{"txs":["0x{}"]}}]}}"#,
                        raw_hex
                    );
                    let c48 = format!(
                        r#"{{"jsonrpc":"2.0","id":2,"method":"eth_sendBundle","params":[{{"txs":["0x{}"],"noMerge":true}}]}}"#,
                        raw_hex
                    );
                    let needle = format!("0x{}", raw_hex);
                    let adapter_projection_ok =
                        br.matches(&needle).count() == 1 && c48.matches(&needle).count() == 1;

                    results.lock().unwrap().push(ResultRow {
                        launch_id,
                        nonce,
                        token,
                        tx_hash: tx_hash.into(),
                        calldata_hash: calldata_hash.into(),
                        adapter_projection_ok,
                        elapsed_ns: t0.elapsed().as_nanos() as u64,
                    });
                }
            });
        }
    });

    let wall_ns = wall.elapsed().as_nanos() as u64;
    let mut rows = results.into_inner().unwrap();
    rows.sort_by_key(|r| r.launch_id);

    assert_eq!(rows.len(), intents, "lost or duplicated work item");

    let mut nonces = HashSet::with_capacity(intents);
    let mut tokens = HashSet::with_capacity(intents);
    let mut tx_hashes = HashSet::with_capacity(intents);
    let mut cross_wiring = 0usize;
    let mut overwritten_inputs = 0usize;
    let mut adapter_projection_mismatch = 0usize;

    for row in &rows {
        nonces.insert(row.nonce);
        tokens.insert(row.token);
        tx_hashes.insert(row.tx_hash);

        let expected_token = derive_token(base_token, row.launch_id);
        if expected_token != row.token {
            cross_wiring += 1;
        }

        let expected_calldata = make_calldata(row.token, input, minimum);
        let expected_hash: [u8; 32] = keccak256(expected_calldata).into();
        if expected_hash != row.calldata_hash {
            overwritten_inputs += 1;
        }

        if !row.adapter_projection_ok {
            adapter_projection_mismatch += 1;
        }
    }

    // Dry permits only. Confirmation below is simulated, never a receipt claim.
    let mut lane = lane.into_inner().unwrap();
    for expected_nonce in BASE_NONCE..BASE_NONCE + intents as u64 {
        let (ticket, hash) = lane.begin_send().unwrap();
        assert_eq!(ticket.nonce(), expected_nonce);
        assert!(rows.iter().any(|r| r.nonce == ticket.nonce() && r.tx_hash == hash.0));
        lane.confirm(ticket, hash).unwrap();
    }
    assert!(lane.begin_send().is_err());

    let duplicate_nonce_count = intents - nonces.len();
    let duplicate_token_count = intents - tokens.len();
    let duplicate_tx_hash_count = intents - tx_hashes.len();

    let min_nonce = rows.iter().map(|r| r.nonce).min().unwrap();
    let max_nonce = rows.iter().map(|r| r.nonce).max().unwrap();
    let contiguous_nonce_lane =
        min_nonce == BASE_NONCE && max_nonce == BASE_NONCE + intents as u64 - 1;

    let latencies: Vec<u64> = rows.iter().map(|r| r.elapsed_ns).collect();
    let p50 = pct(latencies.clone(), 50);
    let p95 = pct(latencies.clone(), 95);
    let p99 = pct(latencies.clone(), 99);
    let max = *latencies.iter().max().unwrap();
    let over_1ms = latencies.iter().filter(|x| **x > 1_000_000).count();

    let invariants_pass =
        duplicate_nonce_count == 0
        && duplicate_token_count == 0
        && duplicate_tx_hash_count == 0
        && cross_wiring == 0
        && overwritten_inputs == 0
        && adapter_projection_mismatch == 0
        && contiguous_nonce_lane;

    let latency_target_met = p99 < 1_000_000;
    let result = if invariants_pass && latency_target_met {
        "PASS_SYNTHETIC_DRY_ONLY"
    } else {
        "ADAPT"
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "G0.6_CONCURRENCY_DRY_QUALIFICATION",
            "result": result,
            "scope": "Concurrency/nonce/cross-wiring/adapter-projection dry qualification using the frozen G0.3 Flap BUY shape; synthetic distinct launch identities; no network write",
            "workers": workers,
            "intents": intents,
            "wall_time_ms": wall_ns as f64 / 1_000_000.0,
            "g03_fixture": {
                "base_token": base_token.to_string(),
                "portal": portal.to_string(),
                "input": input.to_string(),
                "min_output": minimum.to_string()
            },
            "invariants": {
                "unique_nonce_count": nonces.len(),
                "duplicate_nonce_count": duplicate_nonce_count,
                "contiguous_nonce_lane": contiguous_nonce_lane,
                "unique_token_count": tokens.len(),
                "duplicate_token_count": duplicate_token_count,
                "unique_tx_hash_count": tx_hashes.len(),
                "duplicate_tx_hash_count": duplicate_tx_hash_count,
                "cross_wiring_count": cross_wiring,
                "overwritten_buy_input_count": overwritten_inputs,
                "adapter_projection_mismatch_count": adapter_projection_mismatch,
                "dry_ordered_permit_count": intents,
                "actual_submission_lifecycle": "NOT_TESTED",
                "confirmation": "SIMULATED_OFFLINE_ONLY"
            },
            "local_concurrent_intent_build_sign": {
                "p50_us": p50 as f64 / 1000.0,
                "p95_us": p95 as f64 / 1000.0,
                "p99_us": p99 as f64 / 1000.0,
                "max_us": max as f64 / 1000.0,
                "over_1ms_count": over_1ms,
                "over_1ms_fraction": over_1ms as f64 / intents as f64,
                "p99_target_under_1ms": latency_target_met
            },
            "candidate_inclusion_projection": [
                "BlockRazor eth_sendBundle",
                "48Club eth_sendBundle noMerge=true"
            ],
            "safety": {
                "network_access": false,
                "transaction_submission": false,
                "value_bearing_buy": false,
                "signing_key": "fixed non-funded test key"
            },
            "non_claims": [
                "No ingress coverage PASS",
                "No builder inclusion or ordering proof",
                "No Mainnet BUY",
                "No T_buy measurement",
                "Synthetic launch identities are used only to stress concurrency isolation"
            ]
        }))
        .unwrap()
    );
}


#[cfg(test)]
mod qualification_tests {
    use super::*;
    use alloy_rlp::Header;
    use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
    use std::sync::{mpsc, atomic::AtomicU64};

    fn fields(raw: &[u8]) -> Vec<&[u8]> {
        let mut envelope = raw;
        let mut body = Header::decode_bytes(&mut envelope, true).unwrap();
        assert!(envelope.is_empty());
        let mut fields = Vec::new();
        while !body.is_empty() {
            fields.push(Header::decode_bytes(&mut body, false).unwrap());
        }
        assert_eq!(fields.len(), 9);
        fields
    }

    fn fixture() -> (Address, Vec<u8>) {
        let portal = Address::from_str("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0").unwrap();
        let token = Address::from_str("0xabe1fe16cf6bc48c9524713054efe6e954b17777").unwrap();
        (portal, make_calldata(token, U256::from(1000), U256::from(1)))
    }

    #[test]
    fn signed_envelopes_are_canonical_and_recover_the_correct_sender() {
        let (portal, calldata) = fixture();
        let mut shortened_scalars = 0;
        // Offline signatures only, no transport; these are disposable test keys.
        for key_byte in [0x22, 0x33] {
            let signer = SigningKey::from_bytes((&[key_byte; 32]).into()).unwrap();
            for nonce in 0..512 {
                let raw = sign_legacy(nonce, portal, U256::from(1000), &calldata, &signer);
                let f = fields(&raw);
                for i in [0, 1, 2, 4, 6, 7, 8] {
                    assert_ne!(f[i].first(), Some(&0), "noncanonical integer: key={key_byte} nonce={nonce} field={i}");
                }
                assert_eq!(f[3], portal.as_slice());
                assert_eq!(f[5], calldata);
                let parity = U256::from_be_slice(f[6]).to::<u64>() - 147;
                assert!(parity <= 1);
                let signature = Signature::from_scalars(
                    U256::from_be_slice(f[7]).to_be_bytes::<32>(),
                    U256::from_be_slice(f[8]).to_be_bytes::<32>(),
                ).unwrap();
                assert!(signature.normalize_s().is_none(), "high-s signature");
                let mut unsigned: Vec<Vec<u8>> = f[..6].iter().map(|x| rlp_bytes(x)).collect();
                unsigned.extend([rlp_u64(56), rlp_bytes(&[]), rlp_bytes(&[])]);
                let digest = keccak256(rlp_list(unsigned));
                let recovered = VerifyingKey::recover_from_prehash(
                    digest.as_slice(), &signature, RecoveryId::from_byte(parity as u8).unwrap()
                ).unwrap();
                assert_eq!(&recovered, signer.verifying_key());
                shortened_scalars += usize::from(f[7].len() < 32) + usize::from(f[8].len() < 32);
            }
        }
        assert!(shortened_scalars > 0, "regression vectors must exercise short signature scalars");
        println!("1024 envelopes validated; short signature scalars={shortened_scalars}");
    }

    #[test]
    fn dry_lane_orders_existing_signed_buy_envelopes() {
        use darkbot_gate0::intent_lane::Error;
        let (portal, calldata) = fixture();
        let signer=SigningKey::from_bytes((&[0x22;32]).into()).unwrap();
        let pk=signer.verifying_key().to_encoded_point(false);
        let digest=keccak256(&pk.as_bytes()[1..]);
        let sender=Address::from_slice(&digest.as_slice()[12..]);
        let mut lane=IntentLane::new(56,sender,7,2);
        let token_a=Address::repeat_byte(1); let token_b=Address::repeat_byte(2);
        let a=lane.reserve(token_a).unwrap(); let b=lane.reserve(token_b).unwrap();
        assert_eq!(lane.reserve(token_a),Err(Error::Duplicate));
        let mut envelopes=std::collections::BTreeMap::new();
        for (ticket,token) in [(b,token_b),(a,token_a)] {
            let input=make_calldata(token,U256::from(1000),U256::from(1));
            let raw=sign_legacy(ticket.nonce(),portal,U256::from(1000),&input,&signer);
            lane.prepare(ticket,keccak256(&raw)).unwrap();
            envelopes.insert(ticket.nonce(),(token,raw));
        }
        for expected_nonce in [7,8] {
            let (ticket,hash)=lane.begin_send().unwrap();
            assert_eq!(ticket.nonce(),expected_nonce);
            let (token,raw)=&envelopes[&expected_nonce];
            assert_eq!(keccak256(raw),hash);
            let f=fields(raw);
            assert_eq!(U256::from_be_slice(f[0]).to::<u64>(),expected_nonce);
            let decoded=swapExactInputCall::abi_decode_validate(f[5]).unwrap();
            assert_eq!(decoded.params.outputToken,*token);
            assert_eq!(lane.begin_send(),Err(Error::NotReady));
            lane.confirm(ticket,hash).unwrap();
        }
        assert_eq!(lane.begin_send(),Err(Error::NotReady));
        let _=calldata;
    }

    #[test]
    fn diagnostic_duplicate_intent_has_distinct_hash_when_nonce_changes() {
        let (portal, calldata) = fixture();
        let signer = SigningKey::from_bytes((&[0x22; 32]).into()).unwrap();
        let a = sign_legacy(7, portal, U256::from(1000), &calldata, &signer);
        let b = sign_legacy(8, portal, U256::from(1000), &calldata, &signer);
        assert_ne!(keccak256(&a), keccak256(&b));
        assert_eq!(fields(&a)[5], fields(&b)[5]);
        println!("LIMITATION_REPRODUCED: same economic intent, two distinct hashes; hash uniqueness is not deduplication");
    }

    #[test]
    fn diagnostic_atomic_nonce_allocation_does_not_order_completion() {
        let next = AtomicU64::new(7);
        let (allocated_tx, allocated_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let completed = Mutex::new(Vec::new());
        thread::scope(|scope| {
            let completed = &completed;
            let next = &next;
            scope.spawn(move || {
                let first = next.fetch_add(1, Ordering::SeqCst);
                allocated_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                completed.lock().unwrap().push(first);
            });
            allocated_rx.recv().unwrap();
            let second = next.fetch_add(1, Ordering::SeqCst);
            completed.lock().unwrap().push(second);
            release_tx.send(()).unwrap();
        });
        assert_eq!(*completed.lock().unwrap(), vec![8, 7]);
        println!("LIMITATION_REPRODUCED: unique nonces complete out of order; no dispatch or nonce-gap guarantee");
    }
}
