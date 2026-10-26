//! Phase I gate M4: local measurement of the local node's adapter over the
//! FastLane egress lane (C1.7) on an isolated arena: a prepared single-raw
//! intent → the production dispatch (`begin_send`, the pre-permit and write
//! checks, the permit's write) → the egress lane commit (`T_queue`), read
//! back by a consumer in the role of the node's coordinator outside the
//! measured region. Measured: the dispatch's start to `T_queue`, and the
//! write check to `T_queue` (the commit itself); allocations of each
//! measured region are counted. Run with
//! `cargo test --release -p darkbot-bsc-execution --features runtime --test egress_latency -- --ignored --nocapture`;
//! with `BSC_C3_LATENCY_EVIDENCE_DIR` set, the statistics and the raw totals
//! are written there as one binary record (gate L-c). `T_queue` is a local
//! queue commit: no node reads the arena here, nothing is sent; not
//! `T_send`, not the Linux target.
#![cfg(feature = "runtime")]

mod common;

use common::*;
use darkbot_bsc_execution::paper_exec::FnClock;
use darkbot_bsc_execution::submission::egress::EgressTransport;
use darkbot_bsc_execution::submission::send::{Armed, SendOutcome, dispatch_diagnostic};
use darkbot_bsc_geth_ipc::outbound::egress_lane;
use darkbot_bsc_geth_ipc::protocol_v2::{ArenaV2, ConsumerV2, arena_path_v2};
use darkbot_bsc_record::schema::bsc_c3_latency::c3_latency as c;
use darkbot_bsc_record::schema::c1_qualification::latency_stats as s;
use darkbot_bsc_record::{Draft, Value};
use darkbot_bsc_trading::submission_policy::plan::TransportStartDeadline;
use darkbot_bsc_trading::submission_policy::release::ReleaseProfile;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::time::Instant;

const N: usize = 2_000;
const WARMUP: usize = 100;
const GENERATION: u64 = 4_243;

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
#[ignore = "Phase I egress measurement; run in release"]
fn egress_queue_commit_latency() {
    let dir = std::env::temp_dir().join(format!("bsc-egress-latency-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = arena_path_v2(&dir, GENERATION);
    let arena = ArenaV2::create(&path, GENERATION, &[egress_lane()]).unwrap();
    let mut node = EgressTransport::open(&path, GENERATION).unwrap();
    let mut coordinator = ConsumerV2::open(&path, GENERATION, egress_lane()).unwrap();
    let table = frozen_table();
    let start = Instant::now();
    let clock = FnClock {
        id: CLOCK,
        now: || start.elapsed().as_nanos() as u64,
    };
    let (mut total, mut commit) = (Vec::with_capacity(N), Vec::with_capacity(N));
    let (mut cold, mut alloc_min, mut alloc_max) = (0, u64::MAX, 0);
    // The first dispatch is the cold sample, then `WARMUP` unmeasured.
    for i in 0..=WARMUP + N {
        // A fresh prepared intent each time (outside the measured region).
        let mut world = World::prepared();
        let plan = plan(&world, &table, single_inputs(&world));
        let mut frame = local_frame(&plan, &table);
        let current = world.snapshot;
        let before = allocations();
        let begin = (clock.now)();
        let deadline = TransportStartDeadline::for_profile(
            ReleaseProfile::BirthEntryV1,
            CLOCK,
            begin,
            LIMIT_NS,
        )
        .unwrap();
        let report = dispatch_diagnostic(
            &mut world.lane,
            &plan,
            &context(&table, &current),
            &clock,
            &deadline,
            &mut [Armed {
                adapter: adapter(LOCAL),
                transport: &mut node,
                frame: &mut frame,
            }],
        )
        .unwrap();
        let used = allocations() - before;
        let record = *report.adapter(adapter(LOCAL)).unwrap();
        assert_eq!(record.outcome, SendOutcome::HandedOff);
        let queued = record.t_queue_commit_ns().unwrap();
        let checked = record.t_write_check_ns.unwrap();
        // The node's coordinator takes the record (frees the slot).
        let received = coordinator.try_consume().unwrap().unwrap();
        assert_eq!(&received.payload[..], &world.raw_buy[..]);
        if i == 0 {
            cold = queued - begin;
        } else if i > WARMUP {
            total.push(queued - begin);
            commit.push(queued - checked);
            alloc_min = alloc_min.min(used);
            alloc_max = alloc_max.max(used);
        }
    }
    println!(
        "Local node egress (C1.7) on an isolated arena, {N} dispatches after a cold one and {WARMUP} warm-up, this machine (T_queue, not T_send):"
    );
    println!("cold first dispatch: {cold} ns; allocations per dispatch {alloc_min}..={alloc_max}");
    let mut d = Draft::new(&c::SCHEMA);
    for (name, values) in [
        ("dispatch_start_to_queue_commit", &total),
        ("write_check_to_queue_commit", &commit),
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
        let totals: Vec<u8> = total.iter().flat_map(|v| v.to_le_bytes()).collect();
        d.set(c::HOST, host.as_str())
            .set(
                c::BUILD,
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
            )
            .set(c::PROVENANCE_TOOL, "egress_latency")
            .set(c::PROVENANCE_OBSERVED_UNIX_MS, observed)
            .set(c::LAUNCHES, total.len() as u64)
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
            "NOT_T_SEND: T_queue is the egress lane's local commit; no node reads the arena, nothing is sent",
            "DEV_HOST: this host, not the dedicated Linux target hardware",
            "NO_ACQUISITION: no BUY was sent or included; the BUY is signed by a test key",
            "DISPATCH_START: measured from the dispatch's start on a prepared intent, not from a launch's T0",
        ] {
            d.push(c::NON_CLAIMS, n);
        }
        let out = Path::new(&out);
        fs::create_dir_all(out).unwrap();
        let path = out.join(format!("egress_latency_{observed}.dbrec"));
        fs::write(&path, d.encode().unwrap()).unwrap();
        println!("evidence: {}", path.display());
    }
    drop((coordinator, node, arena));
    std::fs::remove_dir_all(&dir).unwrap();
}
