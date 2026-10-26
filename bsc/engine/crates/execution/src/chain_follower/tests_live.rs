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
    let mut tx_counts: Vec<usize> = Vec::new();
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
            txs,
        } => {
            assert!(observed >= started);
            assert!(txs.iter().enumerate().all(|(i, tx)| tx.index == i as u64));
            tx_counts.push(txs.len());
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
        None,
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
    tx_counts.sort_unstable();
    println!(
        "txs per block over {} blocks: min={:?} p50={:?} max={:?}",
        tx_counts.len(),
        tx_counts.first(),
        tx_counts.get(tx_counts.len() / 2),
        tx_counts.last()
    );
    let recent: Vec<u64> = chain.keys().rev().take(20).copied().collect();
    let (mut block_bytes, mut receipt_bytes) = response_sizes(&limits, &recent);
    for (name, sizes) in [
        ("eth_getBlockByNumber(n, true)", &mut block_bytes),
        ("eth_getBlockReceipts(n)", &mut receipt_bytes),
    ] {
        sizes.sort_unstable();
        println!(
            "{name} body bytes over {} blocks: min={:?} p50={:?} max={:?}",
            sizes.len(),
            sizes.first(),
            sizes.get(sizes.len() / 2),
            sizes.last()
        );
    }
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

/// The body sizes of `eth_getBlockByNumber(n, true)` and
/// `eth_getBlockReceipts(n)` for `numbers`, fetched again through the
/// follower's own client (failed requests are left out).
fn response_sizes(limits: &FollowerLimits, numbers: &[u64]) -> (Vec<usize>, Vec<usize>) {
    let target = Endpoint::parse("wss://bsc-dataseed.bnbchain.org").unwrap();
    let tls = tls_config().unwrap();
    let (authority, path) = authority_and_path(target.url());
    let bounds = connect_bounds(limits);
    let connect = |stop: &AtomicBool| connect_tls(&target, &tls, &bounds, stop, Instant::now());
    let mut client = http::Client::new(connect, authority, path, limits);
    let stop = AtomicBool::new(false);
    let (mut blocks, mut receipts) = (Vec::new(), Vec::new());
    for (id, number) in (1u64..).zip(numbers) {
        let block = jsonrpc::request(id, Call::Block(*number));
        if let Ok(body) = client.post(block.as_bytes(), &stop) {
            blocks.push(body.len());
        }
        let receipt = jsonrpc::request(id, Call::Receipts(*number));
        if let Ok(body) = client.post(receipt.as_bytes(), &stop) {
            receipts.push(body.len());
        }
    }
    (blocks, receipts)
}
