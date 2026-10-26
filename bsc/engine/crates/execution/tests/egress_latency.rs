//! Phase I gate M4: local measurement of the local node's adapter over the
//! FastLane egress lane (C1.7) on an isolated arena: a prepared single-raw
//! intent → the production dispatch (`begin_send`, the pre-permit and write
//! checks, the permit's write) → the egress lane commit (`T_queue`), read
//! back by a consumer in the role of the node's coordinator outside the
//! measured region. Measured: the dispatch's start to `T_queue`, and the
//! write check to `T_queue` (the commit itself). Run with
//! `cargo test --release -p darkbot-bsc-execution --features runtime --test egress_latency -- --ignored --nocapture`.
//! `T_queue` is a local queue commit: no node reads the arena here, nothing
//! is sent; not `T_send`, not the Linux target.
#![cfg(feature = "runtime")]

mod common;

use common::*;
use darkbot_bsc_execution::paper_exec::FnClock;
use darkbot_bsc_execution::submission::egress::EgressTransport;
use darkbot_bsc_execution::submission::send::{Armed, SendOutcome, dispatch_diagnostic};
use darkbot_bsc_geth_ipc::outbound::egress_lane;
use darkbot_bsc_geth_ipc::protocol_v2::{ArenaV2, ConsumerV2, arena_path_v2};
use darkbot_bsc_trading::submission_policy::plan::TransportStartDeadline;
use darkbot_bsc_trading::submission_policy::release::ReleaseProfile;
use std::time::Instant;

const N: usize = 2_000;
const WARMUP: usize = 100;
const GENERATION: u64 = 4_243;

fn quantiles(name: &str, values: &mut [u64]) {
    values.sort_unstable();
    let n = values.len();
    let rank = |p: f64| values[(((n as f64) * p).ceil() as usize).clamp(1, n) - 1];
    println!(
        "{name}: n={n} min={} p50={} p95={} p99={} p99.9={} max={} ns",
        values[0],
        rank(0.50),
        rank(0.95),
        rank(0.99),
        rank(0.999),
        values[n - 1]
    );
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
    for i in 0..WARMUP + N {
        // A fresh prepared intent each time (outside the measured region).
        let mut world = World::prepared();
        let plan = plan(&world, &table, single_inputs(&world));
        let mut frame = local_frame(&plan, &table);
        let current = world.snapshot;
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
        let record = *report.adapter(adapter(LOCAL)).unwrap();
        assert_eq!(record.outcome, SendOutcome::HandedOff);
        let queued = record.t_queue_commit_ns().unwrap();
        let checked = record.t_write_check_ns.unwrap();
        // The node's coordinator takes the record (frees the slot).
        let received = coordinator.try_consume().unwrap().unwrap();
        assert_eq!(&received.payload[..], &world.raw_buy[..]);
        if i >= WARMUP {
            total.push(queued - begin);
            commit.push(queued - checked);
        }
    }
    println!(
        "Local node egress (C1.7) on an isolated arena, {N} dispatches after {WARMUP} warm-up, this machine (T_queue, not T_send):"
    );
    quantiles("dispatch_start_to_queue_commit", &mut total);
    quantiles("write_check_to_queue_commit", &mut commit);
    drop((coordinator, node, arena));
    std::fs::remove_dir_all(&dir).unwrap();
}
