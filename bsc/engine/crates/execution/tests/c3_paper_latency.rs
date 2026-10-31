//! C3 local execution measurement (PHASENPLAN §11, master plan §13 C3) of
//! the production path in PAPER mode on this machine:
//!
//! FastLane EARLY_TX_SEEN record of a really signed launch (mmap/SPSC
//! arena) → HOT0 poller (borrowed; `T_ingress` is its delivery) → envelope
//! decode and sender recovery → C2 pending input and snapshot → venue
//! decoder → profile → fresh-launch admission → expectation → build/sign →
//! Unit-4 lane → submission plan (48Club bundle of the original launch) →
//! request frame → diagnostic dispatch → PAPER sink as transport
//! (`T_shadow_sink`, never `T_send`) → journal record of the handoff.
//!
//! Arming (fence, lanes, the C2 anchor and the PAPER sender's nonce fact)
//! happens before the measured loop; the first launch after arming is
//! reported separately as the cold sample, then `WARMUP` launches are not
//! measured. Allocations of each BUY's measured region are counted. Run with
//! `cargo test --release -p darkbot-bsc-execution --features runtime --test c3_paper_latency -- --ignored --nocapture`;
//! with `BSC_C3_LATENCY_EVIDENCE_DIR` set, the statistics and the raw
//! totals are written there as one binary record. A local metric only: no
//! transport, no acquisition, no T0→T_buy claim.
//!
//! G0.6 head-of-line: with `BSC_C3_LATENCY_BURST=k` (2..=8, the lane
//! capacity) the launches arrive `k` at a time (published together, then
//! polled one by one, as the single executor takes them); each burst
//! position also reports its latency from the burst's first publish to its
//! PAPER sink handoff.
#![cfg(feature = "runtime")]

use alloy_primitives::{Address, B256, U256, hex, keccak256};
use darkbot_bsc_execution::paper::PaperIdentity;
use darkbot_bsc_execution::paper_exec::{
    Arming, ExecConfig, FnClock, LaunchContext, PaperExecutor, PendingLaunch, Refusal,
};
use darkbot_bsc_execution::runtime_journal::{JournalSetup, RuntimeJournal};
use darkbot_bsc_geth_ipc::RecordKind;
use darkbot_bsc_geth_ipc::hot0::{Hot0Config, Hot0Decision, Hot0Poller};
use darkbot_bsc_geth_ipc::ingress::{SOURCE_ETH_BROADCAST, decode_tx_target, ingress_aux};
use darkbot_bsc_geth_ipc::protocol_v2::{
    ArenaV2, CLOCK_ID_MONOTONIC, DirectionV2, LaneClassV2, LaneConfigV2, ProducerV2, RecordV2,
    arena_path_v2,
};
use darkbot_bsc_journal::RecordKind as JournalKind;
use darkbot_bsc_record::schema::bsc_c3_latency::c3_latency as c;
use darkbot_bsc_record::schema::c1_qualification::latency_stats as s;
use darkbot_bsc_record::{Draft, Value};
use darkbot_bsc_trading::chain_state::canonical::ExecutionStatus;
use darkbot_bsc_trading::chain_state::engine::{ChainState, StateLimits};
use darkbot_bsc_trading::chain_state::input::{
    CanonicalBlock, CanonicalTx, ChainInput, FirstSeen, PendingObservation,
};
use darkbot_bsc_trading::economics::flap::native_curve20::WITNESSED_MAINNET_TERMS;
use darkbot_bsc_trading::signer::{Signer, recover_legacy_sender};
use darkbot_bsc_trading::submission_policy::plan::ChainHead;
use darkbot_bsc_trading::transactions::legacy::{LegacyTx, decode_signed};
use darkbot_bsc_trading::venues::flap::abi::{LaunchParamsV6, encode_new_token_v6};
use darkbot_bsc_trading::venues::flap::deployment::{ObservedDeployment, bsc_mainnet_v5_27_0};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::time::Instant;

const N: usize = 2_000;
const WARMUP: usize = 100;
const GENERATION: u64 = 1_591;
const PEER: u16 = 16;
const CHAIN: u16 = 2;
const HEAD: u64 = 129_900_000;

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

fn count() {
    let _ = ALLOCATIONS.try_with(|a| a.set(a.get() + 1));
}

// SAFETY: every call is forwarded unchanged to the system allocator; the
// counter is a const-initialised thread local without a destructor, so
// counting neither allocates nor re-enters the allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

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

/// The witnessed native self-launch (the qualified profile) with a salt of
/// its own, really signed by `launcher` with `nonce`.
fn launch_tx(launcher: &Signer, nonce: u64) -> Vec<u8> {
    let mut beneficiary = [0u8; 20];
    beneficiary[18] = 0xa1;
    beneficiary[19] = 0x1c;
    let mut salt = hex!("09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb");
    salt[24..].copy_from_slice(&nonce.to_be_bytes());
    let launch = LaunchParamsV6 {
        name: b"DarkBot G06A Economics",
        symbol: b"DBG06E",
        uri: b"ipfs://darkbot-g06a-economics",
        dex_thresh: 1,
        salt: B256::from(salt),
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
    let (mut scratch, mut raw) = (Vec::new(), Vec::new());
    launcher
        .sign_legacy(
            &LegacyTx {
                chain_id: 56,
                nonce,
                gas_price: 1_000_000_000,
                gas_limit: 6_000_000,
                to: bsc_mainnet_v5_27_0().portal,
                value: U256::ZERO,
                input: &calldata,
            },
            &mut scratch,
            &mut raw,
        )
        .unwrap();
    raw
}

/// One handoff for the G0.6 validity check: the delivered launch hash, the
/// launch the BUY answers, its token and the signed BUY.
type Handoff = ([u8; 32], [u8; 32], [u8; 20], Vec<u8>);

/// Nearest-rank statistics of one stage as the C1 latency record.
fn stats(segment: &str, values: &[u64]) -> (Draft, String) {
    let mut v = values.to_vec();
    v.sort_unstable();
    let n = v.len();
    let rank = |p: f64| v[(((n as f64) * p).ceil() as usize).clamp(1, n) - 1];
    let mean = v.iter().map(|x| u128::from(*x)).sum::<u128>() / n as u128;
    let at_least = |ns: u64| v.iter().filter(|x| **x >= ns).count() as u64;
    let band = |lo: u64, hi: u64| v.iter().filter(|x| **x >= lo && **x < hi).count() as u64;
    let i = |x: u64| i64::try_from(x).unwrap_or(i64::MAX);
    let mut d = Draft::new(&s::SCHEMA);
    d.set(s::SEGMENT, segment)
        .set(s::SAMPLES, n as u64)
        .set(s::MIN_NS, i(v[0]))
        .set(s::MEAN_NS_FLOOR, i(mean as u64))
        .set(s::P50_NS, i(rank(0.50)))
        .set(s::P95_NS, i(rank(0.95)))
        .set(s::P99_NS, i(rank(0.99)))
        .set(s::MAX_NS, i(v[n - 1]))
        .set(s::MISSES_GE_1MS, at_least(1_000_000))
        .set(s::BAND_LT_200US, band(0, 200_000))
        .set(s::BAND_200US_TO_LT_500US, band(200_000, 500_000))
        .set(s::BAND_500US_TO_LT_1MS, band(500_000, 1_000_000))
        .set(s::BAND_GE_1MS, at_least(1_000_000));
    // Nearest rank needs 1 / (1 - q) samples for a tail quantile.
    if n >= 1_000 {
        d.set(s::P99_9_NS, i(rank(0.999)));
    } else {
        d.set(s::P99_9_NOTE, "fewer than 1000 samples");
    }
    if n >= 10_000 {
        d.set(s::P99_99_NS, i(rank(0.9999)));
    } else {
        d.set(s::P99_99_NOTE, "fewer than 10000 samples");
    }
    let line = format!(
        "{segment}: n={n} min={} mean={mean} p50={} p95={} p99={} p99.9={} max={} ns; >=0.5ms {} >=1ms {} >=3ms {}",
        v[0],
        rank(0.50),
        rank(0.95),
        rank(0.99),
        rank(0.999),
        v[n - 1],
        at_least(500_000),
        at_least(1_000_000),
        at_least(3_000_000)
    );
    (d, line)
}

#[test]
#[ignore = "C3 latency measurement; run in release"]
fn c3_paper_path_latency() {
    let dir = std::env::temp_dir().join(format!("bsc-c3-latency-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let launches = 1 + WARMUP + N;

    // PAPER identity and the armed executor: one lane per launch and a
    // session cap of every BUY (each launch is another token).
    let key = dir.join("identity/paper.key");
    PaperIdentity::create(&key, Path::new("/nonexistent-repo")).unwrap();
    let identity = PaperIdentity::load(&key, Path::new("/nonexistent-repo"), &[]).unwrap();
    let sender = Address::from(identity.address());
    let d = bsc_mainnet_v5_27_0();
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
    let (gas_price_wei, gas_limit) = (50_000_000u64, 3_000_000u64);
    let one = 1_000_000_000_000_000u128 + u128::from(gas_price_wei) * u128::from(gas_limit);
    let mut exec = PaperExecutor::arm(
        identity,
        validated,
        WITNESSED_MAINNET_TERMS,
        ExecConfig {
            input_wei: 1_000_000_000_000_000,
            slippage_bps: 100,
            exit_slippage_bps: 100,
            gas_price_wei,
            gas_limit,
            session_cap_wei: one * launches as u128,
            lanes: launches,
            max_calldata: 16_384,
            latency_samples: launches,
            policy_generation: 3,
            capability_generation: 1,
            validity_blocks: 2,
            start_limit_ns: 1_000_000,
            max_frame: 65_536,
            strategy_lanes: launches,
            strategy_session_cap_wei: one * launches as u128,
            approve_gas_limit: 100_000,
            sell_gas_limit: 1_500_000,
            strategy_max_head_age_ms: 30_000,
            live_max_head_age_ms: 86_400_000,
            live_exit_reverts_max: 2,
            sender_paths: darkbot_bsc_execution::paper_exec::SenderPaths::C3,
        },
        Arming {
            next_nonce: 0,
            fence_dir: &dir.join("fences"),
            session: 1,
            arm: 0,
        },
    )
    .unwrap();

    // C2: anchored, the PAPER sender reconciled (coherent).
    let mut state = ChainState::new(
        56,
        StateLimits {
            max_txs: 4 * launches,
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
        number: HEAD,
        hash: block,
        parent_hash: B256::ZERO,
        timestamp_ms: 1_793_000_000_000,
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
        block_number: HEAD,
        block_hash: block,
    });
    assert!(state.coherent());
    let head = state.head_block().map(|(number, _, ms)| ChainHead {
        number,
        timestamp: ms / 1_000,
    });

    // The runtime journal takes each handoff record (recorder load).
    let mut journal = RuntimeJournal::open(
        &dir.join("journal"),
        1,
        1,
        JournalSetup::whole(launches + 8, 4096),
    )
    .unwrap();

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
    let launcher = Signer::from_secret_bytes(&[0x5a; 32]).unwrap();
    let txs: Vec<Vec<u8>> = (0..launches as u64)
        .map(|i| launch_tx(&launcher, i))
        .collect();
    let start = Instant::now();
    let now = || start.elapsed().as_nanos() as u64;
    let clock = FnClock {
        id: CLOCK_ID_MONOTONIC,
        now,
    };

    let stages = [
        "recover+C2 pending",
        "decode",
        "profile+snapshot+admission",
        "expectation+build+sign",
        "lane prepare (final BUY check)",
        "plan+frame",
        "dispatch to the sink",
        "journal record",
        "T_ingress -> T_shadow_sink",
    ];
    let burst: usize = std::env::var("BSC_C3_LATENCY_BURST")
        .map(|v| v.parse().expect("BSC_C3_LATENCY_BURST is a number"))
        .unwrap_or(1);
    assert!(
        (1..=8).contains(&burst),
        "BSC_C3_LATENCY_BURST is 1..=8 (the lane capacity)"
    );
    // Per burst position: the burst's first publish -> this sink handoff.
    let mut positions: Vec<Vec<u64>> = vec![Vec::with_capacity(N / burst + 1); burst];
    let mut samples: Vec<Vec<u64>> = vec![Vec::with_capacity(N); stages.len()];
    let (mut cold, mut misses) = (0u64, 0u64);
    let (mut alloc_min, mut alloc_max) = (u64::MAX, 0u64);
    let mut scratch = Vec::with_capacity(512);
    // G0.6 validity of every handoff, checked after the loop: the launch it
    // answers, its token and its signed BUY (moved, not copied).
    let mut handoffs: Vec<Handoff> = Vec::with_capacity(launches);
    // The cold launch alone, then the rest in bursts.
    let chunks = std::iter::once(0..1).chain(
        (1..launches)
            .step_by(burst)
            .map(|a| a..(a + burst).min(launches)),
    );
    for chunk in chunks {
        let first_published = now() + 1;
        for raw in &txs[chunk.clone()] {
            let published = now() + 1;
            producer
                .publish(&RecordV2 {
                    kind: RecordKind::EarlyTxSeen,
                    flags: 0,
                    timestamp_ns: published,
                    clock_id: CLOCK_ID_MONOTONIC,
                    source: SOURCE_ETH_BROADCAST,
                    peer: [0x22; 32],
                    hash: keccak256(raw).0,
                    // The packet was decoded just before the ingress record.
                    aux: ingress_aux(published - 1),
                    payload: raw,
                })
                .unwrap();
        }
        for (position, i) in chunk.clone().enumerate() {
            let hash = keccak256(&txs[i]).0;
            let mut done = false;
            poller.poll_once(
                |_| {},
                |record| {
                    let ingress = now();
                    let allocs = allocations();
                    // The signed envelope and its sender, as C2 needs them.
                    let signed = decode_signed(record.payload).unwrap();
                    let from = recover_legacy_sender(&signed, 56, &mut scratch).unwrap();
                    state.apply(&ChainInput::Pending(PendingObservation {
                        hash: B256::from(hash),
                        sender: from,
                        nonce: signed.tx.nonce,
                        first_seen: FirstSeen {
                            source: 0,
                            at_ns: ingress,
                        },
                    }));
                    let c2_ns = now();
                    let target = decode_tx_target(record.payload).unwrap();
                    let result = exec.on_launch(
                        PendingLaunch {
                            hash,
                            raw: record.payload,
                            to: target.to,
                            input: target.data,
                            ingress_ns: ingress,
                        },
                        &LaunchContext {
                            snapshot: &|cand, p, g| state.snapshot_id(cand, p, g),
                            head,
                            // PAPER: its transactions never leave.
                            next_nonce: None,
                            exit_reserve_wei: None,
                        },
                        &clock,
                    );
                    done = true;
                    let buy = match result {
                        Ok(buy) => buy,
                        Err(Refusal::Deadline) => {
                            misses += 1;
                            return Hot0Decision::Reject;
                        }
                        Err(r) => panic!("launch {i}: {r:?}"),
                    };
                    let mut payload = Vec::with_capacity(64);
                    payload.extend_from_slice(&buy.handoff.tx_hash);
                    payload.extend_from_slice(&buy.handoff.sequence.to_le_bytes());
                    assert!(journal.record(JournalKind::Runtime, buy.times.sink_ns, payload));
                    let journal_ns = now();
                    let used = allocations() - allocs;
                    let t = buy.times;
                    let total = t.sink_ns - ingress;
                    if i == 0 {
                        cold = total;
                    } else if chunk.start > WARMUP {
                        positions[position].push(t.sink_ns - first_published);
                    }
                    if i > WARMUP {
                        alloc_min = alloc_min.min(used);
                        alloc_max = alloc_max.max(used);
                        for (k, v) in [
                            c2_ns - ingress,
                            t.decoded_ns - c2_ns,
                            t.admitted_ns - t.decoded_ns,
                            t.signed_ns - t.admitted_ns,
                            t.prepared_ns - t.signed_ns,
                            t.planned_ns - t.prepared_ns,
                            t.sink_ns - t.planned_ns,
                            journal_ns - t.sink_ns,
                            total,
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            samples[k].push(v);
                        }
                    }
                    handoffs.push((hash, buy.launch_hash, buy.token, buy.raw));
                    Hot0Decision::Reject
                },
            );
            assert!(done, "record {i} not delivered");
        }
    }
    drop(arena);
    journal.finish().unwrap();
    // G0.6: launches taken together are not cross-wired, overwritten or
    // duplicated — one BUY per launch, its own token in its calldata, signed
    // by the PAPER sender. Every C3 PAPER BUY is an alternative from the same
    // canonical state (never mined), so all carry the arm's next nonce: a
    // LIVE nonce sequence of simultaneous BUYs is not exercised here.
    assert_eq!(handoffs.len() as u64, launches as u64 - misses);
    let mut tokens = std::collections::HashSet::new();
    for (k, (delivered, answered, token, raw)) in handoffs.iter().enumerate() {
        assert_eq!(delivered, answered, "handoff {k} answers another launch");
        assert!(tokens.insert(*token), "handoff {k} repeats a token");
        let signed = decode_signed(raw).unwrap();
        assert_eq!(signed.tx.nonce, 0, "handoff {k}: the arm's next nonce");
        assert_eq!(
            recover_legacy_sender(&signed, 56, &mut scratch).unwrap(),
            sender,
            "handoff {k} signer"
        );
        assert!(
            signed.tx.input.windows(20).any(|w| w == token),
            "handoff {k} calldata misses its token"
        );
    }
    println!(
        "G0.6 validity: {} handoffs, one per launch, distinct tokens in their calldata, PAPER signer, all at the arm's next nonce (PAPER alternatives)",
        handoffs.len()
    );
    println!(
        "C3 PAPER path, {} launches measured after a cold one and {WARMUP} warm-up, this machine (not T_send):",
        samples[0].len()
    );
    println!(
        "cold first launch: {cold} ns; deadline misses: {misses}; allocations per BUY {alloc_min}..={alloc_max}"
    );
    let mut d = Draft::new(&c::SCHEMA);
    for (name, values) in stages.iter().zip(&samples) {
        let (stage, line) = stats(name, values);
        println!("{line}");
        d.push(c::STAGES, stage.into_nested());
    }
    if burst > 1 {
        for (p, values) in positions.iter().enumerate() {
            let name = format!(
                "burst {burst} position {p}: first publish -> T_shadow_sink (head-of-line)"
            );
            let (stage, line) = stats(&name, values);
            println!("{line}");
            d.push(c::STAGES, stage.into_nested());
        }
    }
    if let Some(out) = std::env::var_os("BSC_C3_LATENCY_EVIDENCE_DIR") {
        let host = format!(
            "{} {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::process::Command::new("sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| "cpu unknown".into())
        );
        let observed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let totals: Vec<u8> = samples[stages.len() - 1]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        d.set(c::HOST, host.as_str())
            .set(
                c::BUILD,
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
            )
            .set(c::PROVENANCE_TOOL, "c3_paper_latency")
            .set(c::PROVENANCE_OBSERVED_UNIX_MS, observed)
            .set(c::LAUNCHES, samples[0].len() as u64)
            .set(c::WARMUP, WARMUP as u64)
            .set(c::COLD_TOTAL_NS, cold)
            .set(c::ALLOCATIONS_MIN, alloc_min)
            .set(c::ALLOCATIONS_MAX, alloc_max)
            .set(c::TOTALS_NS, Value::Bytes(totals))
            .set(c::DEADLINE_MISSES, misses);
        if let Ok(o) = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
        {
            d.set(c::GIT_HEAD, String::from_utf8_lossy(&o.stdout).trim());
        }
        for n in [
            "NOT_T_SEND: T_shadow_sink is the PAPER sink's transport write, not a wire boundary",
            "DEV_HOST: this host, not the dedicated Linux target hardware",
            "NO_ACQUISITION: no BUY was sent or included; nothing is signed with a funded key",
            "FASTLANE_LOCAL: the FastLane record is published locally, not by a synced geth",
        ] {
            d.push(c::NON_CLAIMS, n);
        }
        if burst > 1 {
            d.push(
                c::NON_CLAIMS,
                format!(
                    "BURST {burst}: launches published {burst} at a time and taken one by one by the single executor; a synthetic arrival pattern, not an observed one; PAPER BUYs share the arm's next nonce (alternatives, never mined): no LIVE nonce sequence of simultaneous BUYs is exercised"
                )
                .as_str(),
            );
        }
        let out = Path::new(&out);
        fs::create_dir_all(out).unwrap();
        let path = out.join(format!("c3_paper_latency_{observed}.dbrec"));
        fs::write(&path, d.encode().unwrap()).unwrap();
        println!("evidence: {}", path.display());
    }
    fs::remove_dir_all(&dir).unwrap();
}
