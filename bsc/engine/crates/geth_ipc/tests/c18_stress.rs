//! C1.8 concurrency and state-safety stress, Rust side.
//!
//! These tests provoke queueing, saturation, churn and corruption on purpose
//! and check the ownership and state invariants under that load. A failure
//! here is a silent loss, an overwrite, an unaccounted gap, a lane reused
//! before its drain, an accepted corrupt record or a broken generation rule.
//! They prove invariants, not latency; C1.9 measures latency.
//!
//! The geth half (simultaneous peer handlers, canonical/reorg under
//! transaction load, slow and failing outbound peers, the Chain lane's
//! single-producer ownership) is in the overlay's `stress_test.go`,
//! `outbound_test.go` and `chain_v2_test.go`.

use darkbot_bsc_geth_ipc::RecordKind;
use darkbot_bsc_geth_ipc::hot0::{Hot0Config, Hot0Decision, Hot0Outcome, Hot0Poller};
use darkbot_bsc_geth_ipc::ingress::{SOURCE_ETH_BROADCAST, SOURCE_TXPOOL, ingress_aux};
use darkbot_bsc_geth_ipc::v2::{
    ArenaV2, CLOCK_ID_MONOTONIC, ConsumerV2, DIRECTORY_ENTRY_SIZE_V2, DirectionV2,
    LANE_HEADER_SIZE_V2, LANE_STATE_BOUND, LANE_STATE_FREE, LANE_STATE_QUARANTINED, LaneClassV2,
    LaneConfigV2, LaneSetV2, ProducerV2, RecordV2, SLOT_HEADER_SIZE_V2, SUPER_HEADER_SIZE_V2,
    V2Error, arena_path_v2,
};
use std::fs;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

const GENERATION: u64 = 1808;
const CAPACITY: usize = 64;
const STRIDE: usize = 512;
const FIRST_PEER: u16 = 16;
const TXPOOL_LANE: u16 = 1;

static ID: AtomicU64 = AtomicU64::new(0);

fn peer_lane(lane_id: u16) -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id,
        direction: DirectionV2::GethToRust,
        class: LaneClassV2::PeerIngress,
        flags: 0,
        capacity: CAPACITY,
        slot_stride: STRIDE,
    }
}

fn txpool_lane() -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id: TXPOOL_LANE,
        direction: DirectionV2::GethToRust,
        class: LaneClassV2::TxpoolEvidence,
        flags: 0,
        capacity: CAPACITY,
        slot_stride: STRIDE,
    }
}

struct Stress {
    dir: PathBuf,
    path: PathBuf,
    arena: ArenaV2,
    peers: Vec<LaneConfigV2>,
}

impl Stress {
    /// An arena with the Txpool lane and `peers` peer lanes, all BOUND.
    fn new(name: &str, peers: u16) -> Self {
        let id = ID.fetch_add(1, Ordering::Relaxed);
        let name = format!("darkbot-fastlane-c18-{}-{id}-{name}", std::process::id());
        let dir = std::env::temp_dir().join(name);
        fs::create_dir_all(&dir).unwrap();
        let path = arena_path_v2(&dir, GENERATION);
        let peers: Vec<LaneConfigV2> = (0..peers).map(|i| peer_lane(FIRST_PEER + i)).collect();
        let mut lanes = vec![txpool_lane()];
        lanes.extend_from_slice(&peers);
        let mut arena = ArenaV2::create(&path, GENERATION, &lanes).unwrap();
        for lane in &peers {
            let owner = [lane.lane_id as u8; 32];
            arena.bind_peer_lane(lane.lane_id, owner).unwrap();
        }
        Self {
            dir,
            path,
            arena,
            peers,
        }
    }

    fn producer(&self, lane: LaneConfigV2) -> ProducerV2 {
        ProducerV2::open(&self.path, GENERATION, lane).unwrap()
    }

    fn consumer(&self, lane: LaneConfigV2) -> ConsumerV2 {
        ConsumerV2::open(&self.path, GENERATION, lane).unwrap()
    }

    /// File offset of slot `index` of `lane_id`, read from the directory.
    fn slot_offset(&self, lane_id: u16, index: usize) -> u64 {
        let file = fs::File::open(&self.path).unwrap();
        let mut count = [0u8; 4];
        file.read_exact_at(&mut count, 24).unwrap();
        for entry in 0..u32::from_le_bytes(count) as usize {
            let base = (SUPER_HEADER_SIZE_V2 + entry * DIRECTORY_ENTRY_SIZE_V2) as u64;
            let mut id = [0u8; 2];
            let mut offset = [0u8; 8];
            file.read_exact_at(&mut id, base).unwrap();
            file.read_exact_at(&mut offset, base + 16).unwrap();
            if u16::from_le_bytes(id) == lane_id {
                let lane_offset = u64::from_le_bytes(offset);
                return lane_offset + (LANE_HEADER_SIZE_V2 + index * STRIDE) as u64;
            }
        }
        panic!("lane {lane_id} not in the directory");
    }

    /// Flips one bit of the arena file at `offset`.
    fn flip(&self, offset: u64) {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path);
        let file = file.unwrap();
        let mut byte = [0u8; 1];
        file.read_exact_at(&mut byte, offset).unwrap();
        byte[0] ^= 0x10;
        file.write_all_at(&byte, offset).unwrap();
    }
}

impl Drop for Stress {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A payload that names its lane and sequence, of a length that varies.
fn payload_for(lane_id: u16, sequence: u64, buf: &mut [u8; 240]) -> usize {
    let len = 16 + (sequence as usize * 7) % 200;
    buf[..2].copy_from_slice(&lane_id.to_le_bytes());
    buf[2..10].copy_from_slice(&sequence.to_le_bytes());
    for (i, byte) in buf[10..len].iter_mut().enumerate() {
        *byte = (sequence as u8).wrapping_add(i as u8);
    }
    len
}

fn check_payload(lane_id: u16, sequence: u64, payload: &[u8]) {
    let mut expected = [0u8; 240];
    let len = payload_for(lane_id, sequence, &mut expected);
    assert_eq!(payload, &expected[..len], "lane {lane_id} seq {sequence}");
}

fn early(hash: [u8; 32], payload: &[u8]) -> RecordV2<'_> {
    RecordV2 {
        kind: RecordKind::EarlyTxSeen,
        flags: 0,
        timestamp_ns: 1_000,
        clock_id: CLOCK_ID_MONOTONIC,
        source: SOURCE_ETH_BROADCAST,
        peer: [0x22; 32],
        hash,
        aux: ingress_aux(999),
        payload,
    }
}

fn hash_of(lane_id: u16, sequence: u64) -> [u8; 32] {
    let mut hash = [0u8; 32];
    hash[..8].copy_from_slice(&sequence.to_le_bytes());
    hash[8..10].copy_from_slice(&lane_id.to_le_bytes());
    hash
}

/// Publishes `records` records, retrying while the lane is full. Returns how
/// often the lane was full.
fn publish_all(producer: &mut ProducerV2, lane_id: u16, records: u64) -> u64 {
    let mut full = 0;
    let mut buf = [0u8; 240];
    for sequence in 0..records {
        let len = payload_for(lane_id, sequence, &mut buf);
        loop {
            let record = early(hash_of(lane_id, sequence), &buf[..len]);
            match producer.publish(&record) {
                Ok(published) => {
                    assert_eq!(published, sequence);
                    break;
                }
                Err(V2Error::QueueFull { .. }) => {
                    full += 1;
                    thread::yield_now();
                }
                Err(other) => panic!("lane {lane_id}: {other}"),
            }
        }
    }
    full
}

#[test]
fn simultaneous_peer_producers_lose_nothing_and_keep_lane_order() {
    const LANES: u16 = 8;
    const RECORDS: u64 = 3_000;
    let stress = Stress::new("simultaneous", LANES);
    let mut set = LaneSetV2::open(&stress.path, GENERATION, &stress.peers).unwrap();
    let mut next = [0u64; LANES as usize];
    let full: Vec<u64> = thread::scope(|scope| {
        let handles: Vec<_> = stress
            .peers
            .iter()
            .map(|lane| {
                let mut producer = stress.producer(*lane);
                let lane_id = lane.lane_id;
                scope.spawn(move || publish_all(&mut producer, lane_id, RECORDS))
            })
            .collect();
        // One consumer for all lanes, as HOT0: round robin over the set.
        let mut remaining = u64::from(LANES) * RECORDS;
        while remaining > 0 {
            for (index, expected) in next.iter_mut().enumerate() {
                let Some(guard) = set.try_acquire(index).unwrap() else {
                    continue;
                };
                let record = guard.record();
                let lane_id = FIRST_PEER + index as u16;
                assert_eq!((record.lane_id, record.sequence), (lane_id, *expected));
                assert_eq!(record.hash, &hash_of(lane_id, *expected)[..]);
                check_payload(lane_id, *expected, record.payload);
                guard.release().unwrap();
                *expected += 1;
                remaining -= 1;
            }
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for (index, lane) in stress.peers.iter().enumerate() {
        let stats = stress.arena.lane_stats(lane.lane_id).unwrap();
        assert_eq!(
            (stats.write_sequence, stats.read_sequence),
            (RECORDS, RECORDS)
        );
        // Every refused publish was counted; nothing was dropped silently.
        assert_eq!(stats.dropped_full, full[index]);
        assert_eq!((stats.corrupt_records, stats.oversize), (0, 0));
        assert_eq!(stats.state, LANE_STATE_BOUND);
    }
}

#[test]
fn a_stalled_consumer_causes_counted_refusals_and_no_overwrite() {
    let stress = Stress::new("stall", 1);
    let lane = stress.peers[0];
    let mut producer = stress.producer(lane);
    let mut buf = [0u8; 240];
    let mut refused = 0u64;
    let attempts = CAPACITY as u64 * 10;
    for sequence in 0..attempts {
        let len = payload_for(lane.lane_id, sequence, &mut buf);
        let record = early(hash_of(lane.lane_id, sequence), &buf[..len]);
        match producer.publish(&record) {
            Ok(published) => assert_eq!(published, sequence),
            Err(V2Error::QueueFull { sequence: at }) => {
                assert_eq!(at, CAPACITY as u64);
                refused += 1;
            }
            Err(other) => panic!("{other}"),
        }
    }
    assert_eq!(refused, CAPACITY as u64 * 9);
    let stats = stress.arena.lane_stats(lane.lane_id).unwrap();
    assert_eq!(stats.dropped_full, refused);
    assert_eq!(stats.write_sequence, CAPACITY as u64);
    // The stalled lane still holds exactly the first records, untouched.
    let mut consumer = stress.consumer(lane);
    for sequence in 0..CAPACITY as u64 {
        let guard = consumer.try_acquire().unwrap().unwrap();
        assert_eq!(guard.sequence(), sequence);
        check_payload(lane.lane_id, sequence, guard.record().payload);
        guard.release().unwrap();
    }
    assert!(consumer.try_acquire().unwrap().is_none());
    // And accepts again at once.
    let record = early(hash_of(lane.lane_id, 999), b"after the stall");
    assert_eq!(producer.publish(&record).unwrap(), CAPACITY as u64);
}

#[test]
fn lane_churn_never_reuses_a_lane_before_its_drain() {
    let mut stress = Stress::new("churn", 3);
    // Start from FREE lanes: close and drain the bindings of `new`.
    for lane in stress.peers.clone() {
        let owner = [lane.lane_id as u8; 32];
        stress
            .arena
            .begin_close_peer_lane(lane.lane_id, owner, 2)
            .unwrap();
        let consumer = stress.consumer(lane);
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        stress
            .arena
            .release_peer_lane(lane.lane_id, owner, 2)
            .unwrap();
    }
    let mut buf = [0u8; 240];
    for cycle in 0..300u64 {
        let lane = stress.peers[(cycle % 3) as usize];
        let lane_id = lane.lane_id;
        let owner = [(cycle % 251) as u8 + 1; 32];
        let epoch = stress.arena.bind_peer_lane(lane_id, owner).unwrap();
        // Epochs only grow: 3 after the first rebind of each lane.
        assert_eq!(epoch, 3 + cycle / 3, "cycle {cycle}");
        // A bound lane cannot be bound again.
        assert!(stress.arena.bind_peer_lane(lane_id, owner).is_err());
        let mut producer = stress.producer(lane);
        let before = stress.arena.lane_stats(lane_id).unwrap().write_sequence;
        let records = cycle % 5;
        for i in 0..records {
            let len = payload_for(lane_id, before + i, &mut buf);
            producer
                .publish(&early(hash_of(lane_id, before + i), &buf[..len]))
                .unwrap();
        }
        let final_write = stress
            .arena
            .begin_close_peer_lane(lane_id, owner, epoch)
            .unwrap();
        assert_eq!(final_write, before + records);
        // A closing lane takes no record and is not released undrained.
        assert!(producer.publish(&early([9; 32], b"late")).is_err());
        assert!(
            stress
                .arena
                .release_peer_lane(lane_id, owner, epoch)
                .is_err()
        );
        assert!(stress.arena.bind_peer_lane(lane_id, [0xee; 32]).is_err());
        let mut consumer = stress.consumer(lane);
        for i in 0..records {
            let guard = consumer.try_acquire().unwrap().unwrap();
            let record = guard.record();
            assert_eq!((record.sequence, record.lane_epoch), (before + i, epoch));
            check_payload(lane_id, before + i, record.payload);
            guard.release().unwrap();
        }
        // A wrong owner or an earlier epoch never releases the lane.
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        // 0xee collides with the real owner at cycle 237. Derive a
        // guaranteed distinct owner so all 300 negative checks are valid.
        let mut wrong_owner = owner;
        wrong_owner[0] ^= 0xff;
        assert!(
            stress
                .arena
                .release_peer_lane(lane_id, wrong_owner, epoch)
                .is_err()
        );
        assert!(
            stress
                .arena
                .release_peer_lane(lane_id, owner, epoch - 1)
                .is_err()
        );
        stress
            .arena
            .release_peer_lane(lane_id, owner, epoch)
            .unwrap();
        let stats = stress.arena.lane_stats(lane_id).unwrap();
        assert_eq!(stats.state, LANE_STATE_FREE);
        assert_eq!((stats.corrupt_records, stats.dropped_full), (0, 0));
    }
}

#[test]
fn consumer_and_producer_restarts_keep_the_sequence() {
    let stress = Stress::new("restart", 1);
    let lane = stress.peers[0];
    let mut buf = [0u8; 240];
    let mut published = 0u64;
    let mut consumed = 0u64;
    for round in 0..40u64 {
        // A new producer handle and a new consumer handle every round.
        let mut producer = stress.producer(lane);
        for _ in 0..(round % 7) + 1 {
            let len = payload_for(lane.lane_id, published, &mut buf);
            let record = early(hash_of(lane.lane_id, published), &buf[..len]);
            assert_eq!(producer.publish(&record).unwrap(), published);
            published += 1;
        }
        drop(producer);
        let mut consumer = stress.consumer(lane);
        // Leave some records unread across the consumer restart.
        let take = (published - consumed).min((round % 4) + 2);
        for _ in 0..take {
            let guard = consumer.try_acquire().unwrap().unwrap();
            assert_eq!(guard.sequence(), consumed);
            check_payload(lane.lane_id, consumed, guard.record().payload);
            guard.release().unwrap();
            consumed += 1;
        }
    }
    let mut consumer = stress.consumer(lane);
    while let Some(guard) = consumer.try_acquire().unwrap() {
        assert_eq!(guard.sequence(), consumed);
        consumed += 1;
    }
    assert_eq!(consumed, published);
}

#[test]
fn a_wrong_generation_is_refused_by_every_endpoint() {
    let stress = Stress::new("generation", 2);
    let lane = stress.peers[0];
    let wrong = GENERATION + 1;
    assert!(ProducerV2::open(&stress.path, wrong, lane).is_err());
    assert!(ConsumerV2::open(&stress.path, wrong, lane).is_err());
    assert!(LaneSetV2::open(&stress.path, wrong, &stress.peers).is_err());
    assert!(ArenaV2::open(&stress.path, wrong).is_err());
    let config = Hot0Config {
        intent_slots: 1,
        dedup_sets: 4,
    };
    assert!(Hot0Poller::open(&stress.path, wrong, &stress.peers, None, config).is_err());
    assert!(Hot0Poller::open(&stress.path, 0, &stress.peers, None, config).is_err());
    // A lane the arena does not have, or has with another shape, is refused.
    let mut reshaped = lane;
    reshaped.capacity = CAPACITY / 2;
    assert!(ConsumerV2::open(&stress.path, GENERATION, reshaped).is_err());
    assert!(ConsumerV2::open(&stress.path, GENERATION, peer_lane(99)).is_err());
    assert!(Hot0Poller::open(&stress.path, GENERATION, &stress.peers, None, config).is_ok());
}

#[test]
fn torn_and_corrupt_records_are_never_accepted() {
    const RECORDS: u64 = 40;
    const BAD: usize = 17;
    let stress = Stress::new("corrupt", 4);
    for lane in &stress.peers {
        let mut producer = stress.producer(*lane);
        assert_eq!(publish_all(&mut producer, lane.lane_id, RECORDS), 0);
    }
    // Lane 16: a payload bit. Lane 17: a metadata bit (timestamp). Lane 18:
    // a torn commit (committed_sequence). Lane 19 stays intact.
    let payload = stress.slot_offset(16, BAD) + SLOT_HEADER_SIZE_V2 as u64 + 5;
    stress.flip(payload);
    stress.flip(stress.slot_offset(17, BAD) + 16);
    stress.flip(stress.slot_offset(18, BAD));

    let mut set = LaneSetV2::open(&stress.path, GENERATION, &stress.peers).unwrap();
    for index in 0..4usize {
        let lane_id = FIRST_PEER + index as u16;
        let mut delivered = 0u64;
        let defect = loop {
            match set.try_acquire(index) {
                Ok(Some(guard)) => {
                    check_payload(lane_id, delivered, guard.record().payload);
                    guard.release().unwrap();
                    delivered += 1;
                }
                Ok(None) => break None,
                Err(err) => break Some(err),
            }
        };
        let stats = set.stats(index);
        if index == 3 {
            assert!(defect.is_none());
            assert_eq!((delivered, stats.corrupt_records), (RECORDS, 0));
            continue;
        }
        // Everything before the bad record, nothing at or after it.
        assert_eq!(delivered, BAD as u64, "lane {lane_id}");
        let defect = defect.unwrap();
        let expected = match index {
            0 | 1 => matches!(defect, V2Error::ChecksumMismatch { .. }),
            _ => matches!(defect, V2Error::CorruptSequence { .. }),
        };
        assert!(expected, "lane {lane_id}: {defect}");
        assert_eq!(stats.state, LANE_STATE_QUARANTINED);
        assert_eq!(
            (stats.read_sequence, stats.corrupt_records),
            (BAD as u64, 1)
        );
        // Asking again neither delivers nor counts again.
        let again = set.try_acquire(index).map(|guard| guard.is_some());
        assert!(matches!(again, Err(V2Error::LaneQuarantined { .. })));
        assert_eq!(set.stats(index).corrupt_records, 1);
    }
}

#[test]
fn oversize_records_are_refused_with_evidence() {
    let stress = Stress::new("oversize", 1);
    let lane = stress.peers[0];
    let mut producer = stress.producer(lane);
    let fits = vec![1u8; lane.max_payload()];
    let too_large = vec![1u8; lane.max_payload() + 1];
    assert_eq!(producer.publish(&early([1; 32], &fits)).unwrap(), 0);
    for _ in 0..5 {
        let refused = producer.publish(&early([2; 32], &too_large));
        assert!(matches!(refused, Err(V2Error::PayloadTooLarge { .. })));
    }
    let stats = stress.arena.lane_stats(lane.lane_id).unwrap();
    assert_eq!((stats.oversize, stats.write_sequence), (5, 1));
    let mut consumer = stress.consumer(lane);
    let guard = consumer.try_acquire().unwrap().unwrap();
    assert_eq!(guard.record().payload.len(), lane.max_payload());
    drop(guard);
    assert!(consumer.try_acquire().unwrap().is_none());
}

#[test]
fn hot0_accounts_for_every_record_under_burst_and_background_load() {
    const LANES: u16 = 6;
    const RECORDS: u64 = 1_500;
    let stress = Stress::new("burst", LANES);
    let config = Hot0Config {
        intent_slots: 8,
        dedup_sets: 1024,
    };
    let mut poller = Hot0Poller::open(&stress.path, GENERATION, &stress.peers, None, config);
    let poller = poller.as_mut().unwrap();
    let stop = AtomicBool::new(false);
    let recorded = AtomicU64::new(0);
    // Scoped threads take these by reference; their handles are moved in.
    let (stop, recorded) = (&stop, &recorded);
    let total = u64::from(LANES) * RECORDS;
    thread::scope(|scope| {
        // Peer handlers: each record's first payload byte asks for admission
        // or not. Even lanes share their hashes with the next lane, so half
        // of the launches arrive twice.
        for lane in &stress.peers {
            let mut producer = stress.producer(*lane);
            let lane_id = lane.lane_id;
            let hash_lane = lane_id - (lane_id - FIRST_PEER) % 2;
            scope.spawn(move || {
                for sequence in 0..RECORDS {
                    let payload = [sequence.is_multiple_of(3) as u8, 7, 7, 7];
                    let record = early(hash_of(hash_lane, sequence), &payload);
                    while producer.publish(&record).is_err() {
                        thread::yield_now();
                    }
                }
            });
        }
        // Recorder load: the Txpool lane is flooded and drained by the owned
        // audit path on its own threads.
        let mut txpool = stress.producer(txpool_lane());
        scope.spawn(move || {
            let payload = [9u8; 128];
            while !stop.load(Ordering::Relaxed) {
                let mut record = early([3; 32], &payload);
                record.kind = RecordKind::TxpoolAccepted;
                record.source = SOURCE_TXPOOL;
                let _ = txpool.publish(&record);
                thread::yield_now();
            }
        });
        let mut recorder = stress.consumer(txpool_lane());
        scope.spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match recorder.try_consume() {
                    Ok(Some(_)) => {
                        recorded.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => thread::yield_now(),
                }
            }
        });
        // Background load: allocation and memory traffic.
        for _ in 0..2 {
            scope.spawn(move || {
                let mut sink = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    let block = vec![sink as u8; 64 * 1024];
                    sink = sink.wrapping_add(block.iter().map(|b| u64::from(*b)).sum());
                }
                sink
            });
        }
        // HOT0 on this thread.
        let mut admitted = 0u64;
        while poller.stats().peer_records < total {
            let outcome = poller.poll_once(
                |_| {},
                |record| {
                    if record.payload[0] == 1 {
                        Hot0Decision::Admit
                    } else {
                        Hot0Decision::Reject
                    }
                },
            );
            if let Hot0Outcome::Admitted(handle) = outcome {
                admitted += 1;
                assert!(poller.intents_mut().release(handle));
            }
        }
        stop.store(true, Ordering::Relaxed);
        assert_eq!(admitted, poller.stats().admitted);
    });
    let stats = poller.stats();
    assert_eq!(stats.peer_records, total);
    // Every record is in exactly one outcome.
    let outcomes = stats.duplicates
        + stats.rejected
        + stats.admitted
        + stats.dropped_pool_exhausted
        + stats.dropped_launch_too_large
        + stats.dropped_unstable_slot
        + stats.invalid_records
        + stats.rejected_chain_unavailable;
    assert_eq!(outcomes, total);
    // Each of the three hash groups was seen twice: half are duplicates.
    assert_eq!(stats.duplicates, total / 2);
    assert_eq!((stats.lane_faults, stats.invalid_records), (0, 0));
    assert_eq!(
        stats.dropped_pool_exhausted + stats.dropped_unstable_slot,
        0
    );
    assert_eq!(stats.admitted, (total / 2) / 3);
    for lane in &stress.peers {
        let lane_stats = stress.arena.lane_stats(lane.lane_id).unwrap();
        assert_eq!(lane_stats.read_sequence, RECORDS);
        assert_eq!(
            (lane_stats.corrupt_records, lane_stats.state),
            (0, LANE_STATE_BOUND)
        );
    }
    assert!(recorded.load(Ordering::Relaxed) > 0);
}
