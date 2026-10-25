//! The real provider path (TLS, upgrade, subscription, verification)
//! against a public, free, read-only BSC endpoint. Ignored by default: it
//! needs the network. Run it with
//! `cargo test -p darkbot-bsc-execution --features runtime --lib wss::tests_live -- --ignored --nocapture`.

use super::*;
use alloy_primitives::keccak256;
use darkbot_bsc_geth_ipc::ingress::decode_tx_target;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

#[test]
#[ignore = "network: subscribes to a public BSC WebSocket endpoint for ~20 s"]
fn live_publicnode_pending_transactions_verify() {
    let source = WssSource {
        name: "BSC_INGRESS_PUBLICNODE",
        url: "wss://bsc-rpc.publicnode.com".to_owned(),
    };
    let limits = WssLimits {
        connect_timeout: Duration::from_secs(5),
        handshake_timeout: Duration::from_secs(10),
        read_tick: Duration::from_millis(100),
        write_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(15),
        max_frame_bytes: 1 << 20,
        max_message_bytes: 4 << 20,
        backoff_initial: Duration::from_millis(500),
        backoff_max: Duration::from_secs(5),
        stable_after: Duration::from_secs(10),
    };
    let run_for = Duration::from_secs(20);
    let stop = Arc::new(AtomicBool::new(false));
    let timer = {
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            thread::sleep(run_for);
            stop.store(true, Ordering::Release);
        })
    };
    let (mut connected, mut full, mut hash_only) = (0usize, 0usize, 0usize);
    let mut rejected = Vec::new();
    let mut disconnects = Vec::new();
    let mut types = [0usize; 3];
    let started = Instant::now();
    let mut sink = |event: WssEvent| match event {
        WssEvent::Connected => connected += 1,
        WssEvent::Full(tx) => {
            // Re-check what verification guarantees, independently.
            assert_eq!(keccak256(&tx.raw).0, tx.hash);
            let target = decode_tx_target(&tx.raw).unwrap();
            assert_eq!((target.tx_type, target.to), (tx.tx_type, tx.to));
            assert_eq!(tx.chain_id, Some(BSC_MAINNET_CHAIN_ID));
            types[usize::from(tx.tx_type.unwrap_or(0))] += 1;
            full += 1;
        }
        WssEvent::HashOnly(_) => hash_only += 1,
        WssEvent::Rejected(reason) => rejected.push(reason),
        WssEvent::Disconnected(reason) => disconnects.push(reason),
    };
    run_source(&source, &limits, &stop, &mut sink).unwrap();
    let elapsed = started.elapsed();
    timer.join().unwrap();

    let mut reasons: Vec<(RejectReason, usize)> = Vec::new();
    for r in &rejected {
        match reasons.iter_mut().find(|(k, _)| k == r) {
            Some((_, n)) => *n += 1,
            None => reasons.push((*r, 1)),
        }
    }
    println!(
        "live {:?}: connected={connected} full={full} (legacy={} type1={} type2={}) \
         hash_only={hash_only} rejected={} {reasons:?} disconnects={disconnects:?}",
        elapsed,
        types[0],
        types[1],
        types[2],
        rejected.len(),
    );
    assert!(
        connected >= 1,
        "no confirmed subscription; disconnects {disconnects:?}"
    );
    assert_eq!(disconnects.last(), Some(&DisconnectReason::Stopped));
    assert!(elapsed < run_for + limits.read_tick + limits.write_timeout);
}
