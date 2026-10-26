//! Phase I local measurement of Strategy C's PAPER path on this machine:
//! a PASS on a token's exact curve → curve admission on C2's canonical
//! snapshot → BUY expectation → PAPER signature → strategy lane → the local
//! node's single-raw plan → frame → diagnostic dispatch → PAPER sink
//! (`T_shadow_sink`, never `T_send`) → journal record; and an exit trigger →
//! SELL admission → the approval and the SELL, each the same way.
//!
//! Arming and the C2 anchor happen before the measured loop; the first
//! entry is the cold sample, then `WARMUP` entries and exits are not
//! measured. Allocations of each measured region are counted. Run with
//! `cargo test --release -p darkbot-bsc-execution --features runtime --test strategy_paper_latency -- --ignored --nocapture`;
//! with `BSC_C3_LATENCY_EVIDENCE_DIR` set, the statistics and the raw
//! entry totals are written there as one binary record. A local metric
//! only: no transport, no acquisition, no T0→T_buy claim.
#![cfg(feature = "runtime")]

use alloy_primitives::{Address, B256, U256};
use darkbot_bsc_execution::paper::PaperIdentity;
use darkbot_bsc_execution::paper_exec::{
    Arming, CanonicalContext, ExecConfig, FnClock, PaperExecutor, StrategyCurve,
};
use darkbot_bsc_execution::runtime_journal::{JournalSetup, RuntimeJournal};
use darkbot_bsc_execution::runtime_strategy::paper_intent_payload;
use darkbot_bsc_geth_ipc::protocol_v2::CLOCK_ID_MONOTONIC;
use darkbot_bsc_journal::RecordKind as JournalKind;
use darkbot_bsc_record::schema::bsc_c3_latency::c3_latency as c;
use darkbot_bsc_record::schema::c1_qualification::latency_stats as s;
use darkbot_bsc_record::{Draft, Value};
use darkbot_bsc_trading::chain_state::canonical::ExecutionStatus;
use darkbot_bsc_trading::chain_state::engine::{ChainState, StateLimits};
use darkbot_bsc_trading::chain_state::input::{CanonicalBlock, CanonicalTx, ChainInput};
use darkbot_bsc_trading::economics::flap::native_curve20::{
    HoldingCap, NativeBuyInputs, PricedToken, WITNESSED_MAINNET_TERMS, profile_terms,
    quote_native_buy,
};
use darkbot_bsc_trading::submission_policy::plan::ChainHead;
use darkbot_bsc_trading::venues::flap::deployment::{ObservedDeployment, bsc_mainnet_v5_27_0};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::time::Instant;

const N: usize = 2_000;
const WARMUP: usize = 100;
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
    if n >= 1_000 {
        d.set(s::P99_9_NS, i(rank(0.999)));
    } else {
        d.set(s::P99_9_NOTE, "fewer than 1000 samples");
    }
    d.set(s::P99_99_NOTE, "fewer than 10000 samples");
    let line = format!(
        "{segment}: n={n} min={} mean={mean} p50={} p95={} p99={} p99.9={} max={} ns; >=0.5ms {} >=1ms {}",
        v[0],
        rank(0.50),
        rank(0.95),
        rank(0.99),
        rank(0.999),
        v[n - 1],
        at_least(500_000),
        at_least(1_000_000),
    );
    (d, line)
}

#[test]
#[ignore = "Phase I latency measurement; run in release"]
fn strategy_paper_path_latency() {
    let dir = std::env::temp_dir().join(format!("bsc-strategy-latency-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let entries = 1 + WARMUP + N;

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
    let mut exec = PaperExecutor::arm(
        identity,
        validated,
        WITNESSED_MAINNET_TERMS,
        ExecConfig {
            input_wei: 1_000_000_000_000_000,
            slippage_bps: 100,
            gas_price_wei: 50_000_000,
            gas_limit: 3_000_000,
            session_cap_wei: 1_150_000_000_000_000,
            lanes: 1,
            max_calldata: 16_384,
            latency_samples: 16,
            policy_generation: 3,
            capability_generation: 1,
            validity_blocks: 2,
            start_limit_ns: 1_000_000,
            max_frame: 65_536,
            // BUY, approval, SELL: three nonces of the sequence each.
            strategy_lanes: 3 * entries,
            strategy_session_cap_wei: u128::MAX / 4,
            approve_gas_limit: 100_000,
            sell_gas_limit: 1_500_000,
            strategy_max_head_age_ms: 30_000,
        },
        Arming {
            next_nonce: 0,
            fence_dir: &dir.join("fences"),
            session: 1,
            arm: 0,
        },
    )
    .unwrap();

    // C2: anchored on a canonical block, the PAPER sender reconciled.
    let mut state = ChainState::new(
        56,
        StateLimits {
            max_txs: 64,
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
    // The head's own time: a fresh head (the age bound is not measured).
    let c2 = CanonicalContext {
        snapshot: &|policy, codehash| state.canonical_snapshot_id(policy, codehash),
        head,
        now_unix_ms: head.map_or(0, |h| h.timestamp * 1_000),
    };

    let mut journal = RuntimeJournal::open(
        &dir.join("journal"),
        1,
        1,
        JournalSetup::whole(3 * entries + 8, 4096),
    )
    .unwrap();
    // A curve after 1 BNB of real buys; every entry is another token.
    let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
    let q = quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(1_000_000_000_000_000_000u128),
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    let curve = StrategyCurve {
        terms,
        reserve: q.reserve_after,
        circulating: q.supply_after,
    };
    let input = U256::from(10_000_000_000_000_000u64);
    let tokens = U256::from(1_000_000u64) * U256::from(10u64).pow(U256::from(18u8));
    let start = Instant::now();
    let now = || start.elapsed().as_nanos() as u64;
    let clock = FnClock {
        id: CLOCK_ID_MONOTONIC,
        now,
    };

    let (mut buys, mut exits) = (Vec::with_capacity(N), Vec::with_capacity(N));
    let (mut alloc_min, mut alloc_max, mut cold) = (u64::MAX, 0u64, 0u64);
    for i in 0..entries {
        let mut token = [0u8; 20];
        token[..8].copy_from_slice(&(i as u64 + 1).to_le_bytes());
        let before = allocations();
        let decided = now();
        let buy = exec
            .on_strategy_buy(token, curve, input, &c2, &clock, decided)
            .unwrap();
        journal.record(
            JournalKind::Runtime,
            buy.times.sink_ns,
            paper_intent_payload(&buy),
        );
        let buy_ns = buy.times.sink_ns - decided;
        let triggered = now();
        let (exit, refused) = exec.on_strategy_exit(token, tokens, curve, &c2, &clock, triggered);
        assert_eq!(refused, None);
        let [approve, sell]: [_; 2] = exit.try_into().unwrap();
        for x in [&approve, &sell] {
            journal.record(
                JournalKind::Runtime,
                x.times.sink_ns,
                paper_intent_payload(x),
            );
        }
        let exit_ns = sell.times.sink_ns - triggered;
        let used = allocations() - before;
        if i == 0 {
            cold = buy_ns;
        } else if i > WARMUP {
            buys.push(buy_ns);
            exits.push(exit_ns);
            alloc_min = alloc_min.min(used);
            alloc_max = alloc_max.max(used);
        }
    }
    journal.finish().unwrap();
    println!(
        "Strategy C PAPER path, {} entries and exits measured after a cold one and {WARMUP} warm-up, this machine (not T_send):",
        buys.len()
    );
    println!(
        "cold first entry: {cold} ns; allocations per entry and exit {alloc_min}..={alloc_max}"
    );
    let mut d = Draft::new(&c::SCHEMA);
    for (name, values) in [
        ("strategy_buy_decided_to_sink", &buys),
        ("strategy_exit_triggered_to_sell_sink", &exits),
    ] {
        let (stage, line) = stats(name, values);
        println!("{line}");
        d.push(c::STAGES, stage.into_nested());
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
        let totals: Vec<u8> = buys.iter().flat_map(|v| v.to_le_bytes()).collect();
        d.set(c::HOST, host.as_str())
            .set(
                c::BUILD,
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
            )
            .set(c::PROVENANCE_TOOL, "strategy_paper_latency")
            .set(c::PROVENANCE_OBSERVED_UNIX_MS, observed)
            .set(c::LAUNCHES, buys.len() as u64)
            .set(c::WARMUP, WARMUP as u64)
            .set(c::COLD_TOTAL_NS, cold)
            .set(c::ALLOCATIONS_MIN, alloc_min)
            .set(c::ALLOCATIONS_MAX, alloc_max)
            .set(c::TOTALS_NS, Value::Bytes(totals))
            .set(c::DEADLINE_MISSES, 0u64);
        if let Ok(o) = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
        {
            d.set(c::GIT_HEAD, String::from_utf8_lossy(&o.stdout).trim());
        }
        for n in [
            "NOT_T_SEND: T_shadow_sink is the PAPER sink's transport write, not a wire boundary",
            "DEV_HOST: this host, not the dedicated Linux target hardware",
            "NO_ACQUISITION: no BUY or SELL was sent or included; nothing is signed with a funded key",
            "DECISION_START: measured from the strategy's decision, not from a launch's T0",
        ] {
            d.push(c::NON_CLAIMS, n);
        }
        let out = Path::new(&out);
        fs::create_dir_all(out).unwrap();
        let path = out.join(format!("strategy_paper_latency_{observed}.dbrec"));
        fs::write(&path, d.encode().unwrap()).unwrap();
        println!("evidence: {}", path.display());
    }
    exec.disarm().unwrap();
    fs::remove_dir_all(&dir).unwrap();
}
