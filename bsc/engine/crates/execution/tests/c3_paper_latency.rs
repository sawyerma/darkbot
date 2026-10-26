//! C3 local execution measurement (PHASENPLAN §11, master plan §13 C3) of
//! the production path in PAPER mode on this machine:
//!
//! FastLane EARLY_TX_SEEN record (mmap/SPSC arena) → HOT0 poller (borrowed)
//! → envelope decode → C2 pending input and snapshot → venue decoder →
//! profile → fresh-launch admission → expectation → build/sign → Unit-4
//! lane → PAPER sink (`T_shadow_sink`, never `T_send`).
//!
//! `T_ingress` is the poller's delivery of the record. Arming (fences,
//! lanes, the C2 anchor and the PAPER sender's nonce fact) happens before
//! the measured loop. Run with
//! `cargo test --release -p darkbot-bsc-execution --features runtime --test c3_paper_latency -- --ignored --nocapture`.
//! A local metric only: no transport, no acquisition, no T0→T_buy claim.
#![cfg(feature = "runtime")]

use alloy_primitives::{Address, B256, U256, hex};
use darkbot_bsc_execution::paper::PaperIdentity;
use darkbot_bsc_execution::paper_exec::{ExecConfig, PaperExecutor};
use darkbot_bsc_geth_ipc::RecordKind;
use darkbot_bsc_geth_ipc::hot0::{Hot0Config, Hot0Decision, Hot0Poller};
use darkbot_bsc_geth_ipc::ingress::{SOURCE_ETH_BROADCAST, decode_tx_target, ingress_aux};
use darkbot_bsc_geth_ipc::protocol_v2::{
    ArenaV2, CLOCK_ID_MONOTONIC, DirectionV2, LaneClassV2, LaneConfigV2, ProducerV2, RecordV2,
    arena_path_v2,
};
use darkbot_bsc_trading::chain_state::canonical::ExecutionStatus;
use darkbot_bsc_trading::chain_state::engine::{ChainState, StateLimits};
use darkbot_bsc_trading::chain_state::input::{
    CanonicalBlock, CanonicalTx, ChainInput, FirstSeen, PendingObservation,
};
use darkbot_bsc_trading::economics::flap::native_curve20::WITNESSED_MAINNET_TERMS;
use darkbot_bsc_trading::venues::flap::abi::{LaunchParamsV6, encode_new_token_v6};
use darkbot_bsc_trading::venues::flap::deployment::{
    ObservedDeployment, bsc_mainnet_v5_24_0, bsc_mainnet_v5_27_0,
};
use darkbot_bsc_tx_encoding::rlp::{encode_bytes, encode_list, encode_u64};
use std::fs;
use std::path::Path;
use std::time::Instant;

const N: usize = 2_000;
const GENERATION: u64 = 1_591;
const PEER: u16 = 16;
const CHAIN: u16 = 2;

fn lane(lane_id: u16, class: LaneClassV2) -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id,
        direction: DirectionV2::GethToRust,
        class,
        flags: 0,
        capacity: 8,
        slot_stride: 4096,
    }
}

/// The witnessed native self-launch (the qualified profile) in a legacy
/// envelope to the Portal, from `launcher` with `nonce`.
fn launch_tx(nonce: u64) -> Vec<u8> {
    let mut beneficiary = [0u8; 20];
    beneficiary[18] = 0xa1;
    beneficiary[19] = 0x1c;
    let launch = LaunchParamsV6 {
        name: b"DarkBot G06A Economics",
        symbol: b"DBG06E",
        uri: b"ipfs://darkbot-g06a-economics",
        dex_thresh: 1,
        salt: B256::from(hex!(
            "09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb"
        )),
        migrator_type: 1,
        quote_token: Address::ZERO,
        quote_amt: U256::ZERO,
        beneficiary: Address::from(beneficiary),
        permit: &[],
        extension_id: B256::ZERO,
        extension_data: &[],
        dex_id: 0,
        lp_fee_profile: 0,
        buy_tax: 100,
        sell_tax: 100,
        tax_duration: 31_536_000,
        anti_farmer_duration: 86_400,
        mkt_bps: 10_000,
        deflation_bps: 0,
        dividend_bps: 0,
        lp_bps: 0,
        minimum_share_balance: U256::ZERO,
        dividend_token: Address::ZERO,
        commission_receiver: Address::ZERO,
        token_version: 6,
    };
    let mut calldata = Vec::new();
    encode_new_token_v6(&launch, &mut calldata);
    encode_list(&[
        encode_u64(nonce),
        encode_u64(1_000_000_000),
        encode_u64(3_000_000),
        encode_bytes(bsc_mainnet_v5_27_0().portal.as_slice()),
        encode_u64(0),
        encode_bytes(&calldata),
        encode_u64(147),
        encode_bytes(&[1u8; 32]),
        encode_bytes(&[2u8; 32]),
    ])
}

fn quantiles(name: &str, mut v: Vec<u64>) {
    v.sort_unstable();
    let q = |p: f64| v[((v.len() - 1) as f64 * p) as usize];
    println!(
        "{name}: n={} min={} p50={} p95={} p99={} p99.9={} max={} ns",
        v.len(),
        v[0],
        q(0.5),
        q(0.95),
        q(0.99),
        q(0.999),
        v[v.len() - 1]
    );
}

#[test]
#[ignore = "C3 latency measurement; run in release"]
fn c3_paper_path_latency() {
    let dir = std::env::temp_dir().join(format!("bsc-c3-latency-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // PAPER identity and the armed executor (N single-intent lanes).
    let key = dir.join("identity/paper.key");
    PaperIdentity::create(&key, Path::new("/nonexistent-repo")).unwrap();
    let identity = PaperIdentity::load(&key, Path::new("/nonexistent-repo"), &[]).unwrap();
    let sender = Address::from(identity.address());
    // The deployment whose economics G0.6a witnessed: the live v5.27.0 is
    // decode-only until reconfirmed (the decode path is the same Portal).
    let d = bsc_mainnet_v5_24_0();
    let validated = d
        .validate(
            &ObservedDeployment {
                chain_id: d.chain_id,
                portal_version: d.portal_version,
                portal_runtime_sha256: d.portal_runtime_sha256,
                portal_implementation: d.portal_implementation,
                portal_implementation_runtime_sha256: d.portal_implementation_runtime_sha256,
                tax_v3_implementation_runtime_sha256: d.tax_v3_implementation_runtime_sha256,
            },
            5,
        )
        .unwrap();
    let mut exec = PaperExecutor::arm(
        identity,
        validated,
        WITNESSED_MAINNET_TERMS,
        ExecConfig {
            input_wei: 1_000_000_000_000_000,
            slippage_bps: 500,
            gas_price_wei: 1_000_000_000,
            gas_limit: 300_000,
            session_cap_wei: 10_000_000_000_000_000,
            lanes: N,
            max_calldata: 4_096,
            latency_samples: N,
            policy_generation: 3,
        },
        0,
        &dir.join("fences"),
        1,
    )
    .unwrap();

    // C2: anchored, the PAPER sender reconciled (coherent).
    let mut state = ChainState::new(
        56,
        StateLimits {
            max_txs: 4 * N,
            max_candidates_per_nonce: 4,
            max_watched_senders: 1,
            reorg_window: 64,
            finality_depth: 16,
        },
        &[sender],
    )
    .unwrap();
    let block = B256::repeat_byte(0xb1);
    state.apply(&ChainInput::Block(CanonicalBlock {
        number: 1,
        hash: block,
        parent_hash: B256::ZERO,
        timestamp_ms: 1,
        txs: vec![CanonicalTx {
            hash: B256::repeat_byte(0x01),
            index: 0,
            sender: Address::repeat_byte(0x09),
            nonce: 0,
            status: ExecutionStatus::Succeeded,
        }],
    }));
    state.apply(&ChainInput::NonceFact {
        sender,
        next_nonce: 0,
        block_number: 1,
        block_hash: block,
    });
    assert!(state.coherent());

    // FastLane arena: one bound peer lane, the chain lane.
    let path = arena_path_v2(&dir, GENERATION);
    let peer = lane(PEER, LaneClassV2::PeerIngress);
    let chain = lane(CHAIN, LaneClassV2::ChainEvidence);
    let mut arena = ArenaV2::create(&path, GENERATION, &[chain, peer]).unwrap();
    arena.bind_peer_lane(PEER, [7; 32]).unwrap();
    let mut producer = ProducerV2::open(&path, GENERATION, peer).unwrap();
    let mut poller = Hot0Poller::open(
        &path,
        GENERATION,
        &[peer],
        Some(chain),
        Hot0Config {
            intent_slots: 8,
            dedup_sets: 64,
        },
    )
    .unwrap();
    let launcher = Address::repeat_byte(0x5a);
    let start = Instant::now();
    let now = || start.elapsed().as_nanos() as u64;
    let txs: Vec<Vec<u8>> = (0..N as u64).map(launch_tx).collect();

    let (mut c2, mut decode, mut admit, mut sign, mut sink, mut total) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    for (i, raw) in txs.iter().enumerate() {
        let mut hash = [0u8; 32];
        hash[..8].copy_from_slice(&(i as u64 + 1).to_le_bytes());
        let published = now() + 1;
        producer
            .publish(&RecordV2 {
                kind: RecordKind::EarlyTxSeen,
                flags: 0,
                timestamp_ns: published,
                clock_id: CLOCK_ID_MONOTONIC,
                source: SOURCE_ETH_BROADCAST,
                peer: [0x22; 32],
                hash,
                // The packet was decoded just before the ingress record.
                aux: ingress_aux(published - 1),
                payload: raw,
            })
            .unwrap();
        let mut done = false;
        poller.poll_once(
            |_| {},
            |record| {
                let ingress = now();
                let record_hash: [u8; 32] = record.hash.try_into().unwrap();
                let target = decode_tx_target(record.payload).unwrap();
                state.apply(&ChainInput::Pending(PendingObservation {
                    hash: B256::from(record_hash),
                    sender: launcher,
                    nonce: i as u64,
                    first_seen: FirstSeen {
                        source: 0,
                        at_ns: ingress,
                    },
                }));
                let c2_ns = now();
                let buy = exec
                    .on_launch(
                        record_hash,
                        target.to,
                        target.data,
                        ingress,
                        |c, p, g| state.snapshot_id(c, p, g),
                        now,
                    )
                    .unwrap();
                let t = buy.times;
                c2.push(c2_ns - ingress);
                decode.push(t.decoded_ns - c2_ns);
                admit.push(t.admitted_ns - t.decoded_ns);
                sign.push(t.signed_ns - t.admitted_ns);
                sink.push(t.sink_ns - t.signed_ns);
                total.push(t.sink_ns - ingress);
                done = true;
                Hot0Decision::Reject
            },
        );
        assert!(done, "record {i} not delivered");
    }
    assert_eq!(exec.counters().handoffs, N as u64);
    println!("C3 PAPER path, {N} launches, this machine (not T_send):");
    quantiles("C2 pending apply", c2);
    quantiles("decode (recognize+V6)", decode);
    quantiles("profile+snapshot+admission", admit);
    quantiles("expectation+build+sign", sign);
    quantiles("lane prepare/begin/write+sink", sink);
    quantiles("T_ingress -> T_shadow_sink", total);
    drop(arena);
    fs::remove_dir_all(&dir).unwrap();
}
