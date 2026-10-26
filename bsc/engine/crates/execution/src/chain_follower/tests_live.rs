//! The real provider path (TLS, keep-alive HTTP/1.1, JSON-RPC, receipt
//! check, launch detection) against the free public BSC endpoint. Ignored
//! by default: it needs the network. Run it with
//! `cargo test -p darkbot-bsc-execution --features runtime --lib chain_follower::tests_live -- --ignored --nocapture`.

use super::*;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread;

#[test]
#[ignore = "network: follows BSC Mainnet at bsc-dataseed.bnbchain.org for ~30 s"]
fn live_dataseed_follows_consecutive_canonical_blocks() {
    let limits = FollowerLimits {
        poll: Duration::from_millis(200),
        reorg_depth: 16,
        max_catch_up: 64,
        connect_timeout: Duration::from_secs(5),
        handshake_timeout: Duration::from_secs(10),
        read_tick: Duration::from_millis(100),
        write_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(10),
        max_response_bytes: 16 << 20,
        backoff_initial: Duration::from_millis(250),
        backoff_max: Duration::from_secs(5),
        tx_attempts: 3,
    };
    let run_for = Duration::from_secs(30);
    let stop = Arc::new(AtomicBool::new(false));
    let timer = {
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            thread::sleep(run_for);
            let at = Instant::now();
            stop.store(true, Ordering::Release);
            at
        })
    };
    // The reported canonical chain, its continuity checked as it grows.
    let mut chain: BTreeMap<u64, [u8; 32]> = BTreeMap::new();
    let mut last: Option<u64> = None;
    let mut linked = 0usize;
    let (mut blocks, mut reorgs, mut gaps) = (0usize, Vec::new(), Vec::new());
    let mut launches = Vec::new();
    let mut faults = Vec::new();
    // Wall clock when first held minus the block's millisecond timestamp.
    let mut lags_ms: Vec<i64> = Vec::new();
    let started = Instant::now();
    let mut sink = |event: CanonicalEvent| match event {
        CanonicalEvent::Block {
            number,
            hash,
            parent_hash,
            timestamp_s,
            timestamp_ms,
            observed,
            observed_unix_ms,
        } => {
            assert!(observed >= started);
            assert_eq!(timestamp_ms / 1000, timestamp_s);
            let held = observed_unix_ms.expect("the wall clock is readable");
            lags_ms.push(i64::try_from(held).unwrap() - i64::try_from(timestamp_ms).unwrap());
            if let Some(prev) = last {
                assert_eq!(number, prev + 1, "blocks follow in order");
            }
            if let Some(parent) = chain.get(&(number - 1)) {
                assert_eq!(
                    *parent, parent_hash,
                    "block {number} extends the reported chain"
                );
                linked += 1;
            }
            chain.insert(number, hash);
            last = Some(number);
            blocks += 1;
        }
        CanonicalEvent::Launch(launch) => launches.push(launch),
        CanonicalEvent::Reorg { orphaned, new_head } => {
            for (number, hash) in &orphaned {
                assert_eq!(chain.remove(number), Some(*hash));
            }
            last = orphaned.first().map(|(n, _)| n - 1);
            reorgs.push((orphaned, new_head));
        }
        CanonicalEvent::ReorgTooDeep { orphaned, new_head } => {
            chain.clear();
            last = None;
            reorgs.push((orphaned, new_head));
        }
        CanonicalEvent::Gap { from, to } => {
            chain.clear();
            last = None;
            gaps.push((from, to));
        }
        CanonicalEvent::Fault(fault) => faults.push(fault),
    };
    run_follower(
        "https://bsc-dataseed.bnbchain.org",
        &limits,
        &stop,
        &mut sink,
    )
    .unwrap();
    let returned = Instant::now();
    let stop_at = timer.join().unwrap();

    let (first, last_block) = (chain.keys().next().copied(), chain.keys().last().copied());
    lags_ms.sort_unstable();
    println!(
        "lag observed_unix_ms - timestamp_ms over {} blocks: min={:?} p50={:?} max={:?} ms",
        lags_ms.len(),
        lags_ms.first(),
        lags_ms.get(lags_ms.len() / 2),
        lags_ms.last()
    );
    println!(
        "live {:?}: blocks={blocks} linked={linked} first={first:?} last={last_block:?} \
         reorgs={reorgs:?} gaps={gaps:?} faults={} {faults:?} launches={}",
        returned - started,
        faults.len(),
        launches.len(),
    );
    for launch in &launches {
        println!(
            "  {:?} block {} tx {} token {:?} buys {:?} sender {:?} nonce {:?} c1 {} unverifiable {:?}",
            launch.venue,
            launch.block_number,
            launch.tx_index,
            launch.token.map(alloy_primitives::hex::encode),
            launch.external_buys_after,
            launch.sender.map(alloy_primitives::hex::encode),
            launch.nonce,
            launch.c1_scope,
            launch.unverifiable
        );
    }
    assert!(blocks >= 10, "followed only {blocks} blocks");
    assert!(
        linked + 1 + reorgs.len() + gaps.len() >= blocks,
        "parent links checked"
    );
    assert!(returned - stop_at < limits.read_tick * 2 + limits.connect_timeout);
}
