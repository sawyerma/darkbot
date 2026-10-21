use super::*;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64 as TestAtomicU64, Ordering as TestOrdering};
use std::thread;
use std::time::{Duration, Instant};

static ID: TestAtomicU64 = TestAtomicU64::new(0);

fn tmp(name: &str) -> std::path::PathBuf {
    let id = ID.fetch_add(1, TestOrdering::Relaxed);
    private_test_dir("darkbot-fastlane-v2").join(format!("{id}-{name}.mmap"))
}

/// A per-process 0700 directory under the temp dir; arena files must not sit
/// directly in a shared sticky temp dir (E7-5 trust contract).
fn private_test_dir(prefix: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn peer_lane(id: u16) -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id: id,
        direction: DirectionV2::GethToRust,
        class: LaneClassV2::PeerIngress,
        flags: 0,
        capacity: 4,
        slot_stride: 512,
    }
}

fn txpool_lane(id: u16) -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id: id,
        direction: DirectionV2::GethToRust,
        class: LaneClassV2::TxpoolEvidence,
        flags: 0,
        capacity: 4,
        slot_stride: 512,
    }
}

fn owner(tag: u8) -> [u8; 32] {
    [tag; 32]
}

fn stats_of(arena: &ArenaV2, lane_id: u16) -> LaneStatsV2 {
    arena.lane_stats(lane_id).unwrap()
}

fn offset_of(arena: &ArenaV2, lane_id: u16) -> usize {
    locate_lane_by_id(&arena.mapping, lane_id).unwrap().1
}

fn drain_all(consumer: &mut ConsumerV2) -> Vec<OwnedRecordV2> {
    let mut out = Vec::new();
    while let Some(record) = consumer.try_consume().unwrap() {
        out.push(record);
    }
    out
}

fn record<'a>(payload: &'a [u8]) -> RecordV2<'a> {
    RecordV2 {
        kind: RecordKind::EarlyTxSeen,
        flags: 3,
        timestamp_ns: 123_456_789,
        clock_id: CLOCK_ID_MONOTONIC,
        source: [0x11; SOURCE_LEN],
        peer: [0x22; PEER_LEN],
        hash: [0x33; HASH_LEN],
        aux: [0x44; AUX_LEN],
        payload,
    }
}

#[test]
fn crc32c_standard_and_cross_language_vectors() {
    assert_eq!(crc32c(b"123456789"), 0xe306_9283);

    let mut slot = vec![0u8; 512];
    let r = record(b"signed-transaction");
    put_u16(&mut slot, SLOT_OFF_KIND, r.kind as u16);
    put_u16(&mut slot, SLOT_OFF_FLAGS, r.flags);
    put_u32(&mut slot, SLOT_OFF_PAYLOAD_LEN, r.payload.len() as u32);
    put_u64(&mut slot, SLOT_OFF_TIMESTAMP, r.timestamp_ns);
    put_u32(&mut slot, SLOT_OFF_CLOCK_ID, r.clock_id);
    slot[SLOT_OFF_SOURCE..SLOT_OFF_SOURCE + SOURCE_LEN].copy_from_slice(&r.source);
    slot[SLOT_OFF_PEER..SLOT_OFF_PEER + PEER_LEN].copy_from_slice(&r.peer);
    slot[SLOT_OFF_HASH..SLOT_OFF_HASH + HASH_LEN].copy_from_slice(&r.hash);
    slot[SLOT_OFF_AUX..SLOT_OFF_AUX + AUX_LEN].copy_from_slice(&r.aux);
    put_u64(&mut slot, SLOT_OFF_LANE_EPOCH, 7);
    slot[SLOT_HEADER_SIZE_V2..SLOT_HEADER_SIZE_V2 + r.payload.len()].copy_from_slice(r.payload);
    assert_eq!(record_crc32c(&slot, r.payload.len()), 0xde71_4cce);
}

#[test]
fn arena_roundtrip_wrap_and_directory_validation() {
    let path = tmp("roundtrip");
    let lanes = [peer_lane(10), peer_lane(11)];
    let mut arena = ArenaV2::create(&path, 7, &lanes).unwrap();
    assert_eq!(arena.mapped_len() % ALIGN_V2, 0);
    arena.bind_peer_lane(10, [0x10; 32]).unwrap();
    drop(arena);

    let mut producer = ProducerV2::open(&path, 7, lanes[0]).unwrap();
    let mut consumer = ConsumerV2::open(&path, 7, lanes[0]).unwrap();

    for n in 0..32u8 {
        producer.publish(&record(&[n])).unwrap();
        let out = consumer.try_consume().unwrap().unwrap();
        assert_eq!(out.sequence, u64::from(n));
        assert_eq!(out.lane_id, 10);
        assert_eq!(out.lane_epoch, 2);
        assert_eq!(&*out.payload, &[n]);
    }

    let mut wrong = lanes[0];
    wrong.capacity = 8;
    assert!(matches!(
        ProducerV2::open(&path, 7, wrong),
        Err(V2Error::LaneMismatch("capacity"))
    ));

    drop(producer);
    drop(consumer);
    fs::remove_file(path).unwrap();
}

#[test]
fn full_lane_never_overwrites_unread_records() {
    let path = tmp("full");
    let lane = peer_lane(1);
    let mut arena = ArenaV2::create(&path, 9, &[lane]).unwrap();
    arena.bind_peer_lane(1, [0x01; 32]).unwrap();
    drop(arena);
    let mut producer = ProducerV2::open(&path, 9, lane).unwrap();
    let mut consumer = ConsumerV2::open(&path, 9, lane).unwrap();

    for n in 0..lane.capacity {
        producer.publish(&record(&[n as u8])).unwrap();
    }
    assert!(matches!(
        producer.publish(&record(b"x")),
        Err(V2Error::QueueFull { .. })
    ));
    assert_eq!(producer.stats().dropped_full, 1);

    for n in 0..lane.capacity {
        let out = consumer.try_consume().unwrap().unwrap();
        assert_eq!(&*out.payload, &[n as u8]);
    }

    drop(producer);
    drop(consumer);
    fs::remove_file(path).unwrap();
}

#[test]
fn header_and_payload_corruption_are_both_detected() {
    for target in ["header", "payload"] {
        let path = tmp(target);
        let lane = peer_lane(2);
        let mut arena = ArenaV2::create(&path, 11, &[lane]).unwrap();
        arena.bind_peer_lane(2, [0x02; 32]).unwrap();
        drop(arena);
        let mut producer = ProducerV2::open(&path, 11, lane).unwrap();
        producer.publish(&record(b"abc")).unwrap();

        let (mut mapping, lane_offset) = open_lane(&path, 11, lane).unwrap();
        let slot_offset = lane_offset + LANE_HEADER_SIZE_V2;
        if target == "header" {
            mapping.bytes_mut()[slot_offset + SLOT_OFF_HASH] ^= 0x01;
        } else {
            mapping.bytes_mut()[slot_offset + SLOT_HEADER_SIZE_V2] ^= 0x01;
        }
        drop(mapping);

        let mut consumer = ConsumerV2::open(&path, 11, lane).unwrap();
        assert!(matches!(
            consumer.try_consume(),
            Err(V2Error::ChecksumMismatch { .. })
        ));
        assert_eq!(consumer.stats().corrupt_records, 1);
        assert!(matches!(
            consumer.try_consume(),
            Err(V2Error::LaneQuarantined {
                reason: QUARANTINE_CHECKSUM
            })
        ));
        let stats = consumer.stats();
        assert_eq!(
            (stats.corrupt_records, stats.read_sequence, stats.state),
            (1, 0, LANE_STATE_QUARANTINED)
        );

        drop(producer);
        drop(consumer);
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn independent_lanes_support_parallel_single_producers() {
    let path = tmp("parallel");
    let lanes = [peer_lane(20), peer_lane(21)];
    let mut arena = ArenaV2::create(&path, 13, &lanes).unwrap();
    arena.bind_peer_lane(20, [0x20; 32]).unwrap();
    arena.bind_peer_lane(21, [0x21; 32]).unwrap();
    drop(arena);

    let p0_path = path.clone();
    let p1_path = path.clone();
    let l0 = lanes[0];
    let l1 = lanes[1];

    let t0 = thread::spawn(move || {
        let mut p = ProducerV2::open(&p0_path, 13, l0).unwrap();
        for n in 0..100u8 {
            loop {
                match p.publish(&record(&[n])) {
                    Ok(_) => break,
                    Err(V2Error::QueueFull { .. }) => thread::yield_now(),
                    Err(e) => panic!("{e}"),
                }
            }
        }
    });
    let t1 = thread::spawn(move || {
        let mut p = ProducerV2::open(&p1_path, 13, l1).unwrap();
        for n in 0..100u8 {
            loop {
                match p.publish(&record(&[n])) {
                    Ok(_) => break,
                    Err(V2Error::QueueFull { .. }) => thread::yield_now(),
                    Err(e) => panic!("{e}"),
                }
            }
        }
    });

    let mut c0 = ConsumerV2::open(&path, 13, lanes[0]).unwrap();
    let mut c1 = ConsumerV2::open(&path, 13, lanes[1]).unwrap();
    let mut got0 = 0;
    let mut got1 = 0;
    while got0 < 100 || got1 < 100 {
        if got0 < 100 && c0.try_consume().unwrap().is_some() {
            got0 += 1;
        }
        if got1 < 100 && c1.try_consume().unwrap().is_some() {
            got1 += 1;
        }
        thread::yield_now();
    }
    t0.join().unwrap();
    t1.join().unwrap();
    assert_eq!(got0, 100);
    assert_eq!(got1, 100);

    drop(c0);
    drop(c1);
    fs::remove_file(path).unwrap();
}
#[test]
fn peer_lane_lifecycle_requires_drain_before_reuse() {
    let path = tmp("lifecycle");
    let lane = peer_lane(30);
    let mut arena = ArenaV2::create(&path, 21, &[lane]).unwrap();
    let owner_a = [0xa1; 32];
    let owner_b = [0xb2; 32];

    let (_, lane_offset) = locate_lane_by_id(&arena.mapping, 30).unwrap();
    assert_eq!(
        lane_stats(&arena.mapping, lane_offset).state,
        LANE_STATE_FREE
    );

    let epoch_a = arena.bind_peer_lane(30, owner_a).unwrap();
    assert_eq!(epoch_a, 2);

    let mut producer = ProducerV2::open(&path, 21, lane).unwrap();
    let mut consumer = ConsumerV2::open(&path, 21, lane).unwrap();
    producer.publish(&record(b"one")).unwrap();

    let final_write = arena.begin_close_peer_lane(30, owner_a, epoch_a).unwrap();
    assert_eq!(final_write, 1);
    assert!(matches!(
        arena.release_peer_lane(30, owner_a, epoch_a),
        Err(V2Error::LaneNotDrained { .. })
    ));
    assert!(!consumer.acknowledge_drained_if_closing().unwrap());

    assert!(consumer.try_consume().unwrap().is_some());
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    arena.release_peer_lane(30, owner_a, epoch_a).unwrap();

    let stats = lane_stats(&arena.mapping, lane_offset);
    assert_eq!(stats.state, LANE_STATE_FREE);
    assert_eq!(stats.owner, [0; 32]);
    assert_eq!(stats.write_sequence, stats.read_sequence);

    let epoch_b = arena.bind_peer_lane(30, owner_b).unwrap();
    assert_eq!(epoch_b, 3);

    drop(producer);
    drop(consumer);
    drop(arena);
    fs::remove_file(path).unwrap();
}

#[test]
fn peer_lane_lifecycle_rejects_wrong_owner_and_wrong_state() {
    let path = tmp("owner");
    let lane = peer_lane(31);
    let mut arena = ArenaV2::create(&path, 22, &[lane]).unwrap();
    let owner = [0x11; 32];
    let wrong = [0x22; 32];

    let epoch = arena.bind_peer_lane(31, owner).unwrap();
    assert!(matches!(
        arena.bind_peer_lane(31, owner),
        Err(V2Error::LifecycleState { .. })
    ));
    assert!(matches!(
        arena.begin_close_peer_lane(31, wrong, epoch),
        Err(V2Error::LaneMismatch("owner"))
    ));
    arena.begin_close_peer_lane(31, owner, epoch).unwrap();

    let consumer = ConsumerV2::open(&path, 22, lane).unwrap();
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    assert!(matches!(
        arena.release_peer_lane(31, wrong, epoch),
        Err(V2Error::LaneMismatch("owner"))
    ));
    arena.release_peer_lane(31, owner, epoch).unwrap();

    drop(consumer);
    drop(arena);
    fs::remove_file(path).unwrap();
}

#[test]
fn free_peer_lane_refuses_publish_and_wrong_generation_is_quarantined() {
    let path = tmp("generation");
    let lane = peer_lane(32);
    let arena = ArenaV2::create(&path, 23, &[lane]).unwrap();
    drop(arena);

    let mut producer = ProducerV2::open(&path, 23, lane).unwrap();
    assert!(matches!(
        producer.publish(&record(b"x")),
        Err(V2Error::LifecycleState {
            expected: LANE_STATE_BOUND,
            observed: LANE_STATE_FREE
        })
    ));
    assert!(matches!(
        ProducerV2::open(&path, 24, lane),
        Err(V2Error::GenerationMismatch { .. })
    ));

    drop(producer);
    fs::remove_file(path).unwrap();
}

// -----------------------------------------------------------------
// C1.3 lifecycle
// -----------------------------------------------------------------

#[test]
fn lifecycle_free_bound_closing_drained_free_and_second_bind() {
    let path = tmp("c13-cycle");
    let lane = peer_lane(40);
    let mut arena = ArenaV2::create(&path, 41, &[lane]).unwrap();
    let s = stats_of(&arena, 40);
    assert_eq!(
        (s.state, s.lane_epoch, s.owner),
        (LANE_STATE_FREE, 1, [0; 32])
    );

    // FREE -> BOUND: new epoch, owner written.
    let epoch_a = arena.bind_peer_lane(40, owner(0xa1)).unwrap();
    let s = stats_of(&arena, 40);
    assert_eq!(
        (epoch_a, s.state, s.lane_epoch, s.owner),
        (2, LANE_STATE_BOUND, 2, owner(0xa1))
    );

    let mut producer = ProducerV2::open(&path, 41, lane).unwrap();
    let mut consumer = ConsumerV2::open(&path, 41, lane).unwrap();
    producer.publish(&record(b"a0")).unwrap();
    producer.publish(&record(b"a1")).unwrap();

    // BOUND -> CLOSING: final_write frozen at write_sequence.
    let final_write = arena
        .begin_close_peer_lane(40, owner(0xa1), epoch_a)
        .unwrap();
    let s = stats_of(&arena, 40);
    assert_eq!(
        (
            final_write,
            s.state,
            s.final_write_sequence,
            s.write_sequence
        ),
        (2, LANE_STATE_CLOSING, 2, 2)
    );

    // CLOSING -> drained: every record of the old owner, then the ack.
    let drained = drain_all(&mut consumer);
    assert_eq!(drained.len(), 2);
    assert!(drained.iter().all(|r| r.lane_epoch == epoch_a));
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    assert_eq!(stats_of(&arena, 40).drained_epoch, epoch_a);

    // drained -> FREE: owner and final_write cleared, sequences kept.
    arena.release_peer_lane(40, owner(0xa1), epoch_a).unwrap();
    let s = stats_of(&arena, 40);
    assert_eq!(
        (s.state, s.owner, s.final_write_sequence),
        (LANE_STATE_FREE, [0; 32], 0)
    );
    assert_eq!((s.write_sequence, s.read_sequence), (2, 2));

    // FREE -> second BOUND: the epoch advances, sequences continue.
    let epoch_b = arena.bind_peer_lane(40, owner(0xb2)).unwrap();
    assert_eq!(epoch_b, epoch_a + 1);
    assert_eq!(stats_of(&arena, 40).owner, owner(0xb2));
    producer.publish(&record(b"b0")).unwrap();
    let out = consumer.try_consume().unwrap().unwrap();
    assert_eq!(
        (out.sequence, out.lane_epoch, &*out.payload),
        (2, epoch_b, &b"b0"[..])
    );

    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn publish_while_free_is_rejected_and_writes_nothing() {
    let path = tmp("c13-free-publish");
    let lane = peer_lane(41);
    let arena = ArenaV2::create(&path, 42, &[lane]).unwrap();
    let mut producer = ProducerV2::open(&path, 42, lane).unwrap();
    assert!(matches!(
        producer.publish(&record(b"x")),
        Err(V2Error::LifecycleState {
            expected: LANE_STATE_BOUND,
            observed: LANE_STATE_FREE
        })
    ));
    let s = producer.stats();
    assert_eq!((s.write_sequence, s.dropped_full, s.oversize), (0, 0, 0));
    let slot = offset_of(&arena, 41) + LANE_HEADER_SIZE_V2;
    assert_eq!(
        arena
            .mapping
            .atomic_u64(slot + SLOT_OFF_COMMITTED)
            .load(Ordering::Acquire),
        EMPTY_SEQUENCE
    );
    drop((producer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn publish_after_close_is_rejected() {
    let path = tmp("c13-closing-publish");
    let lane = peer_lane(42);
    let mut arena = ArenaV2::create(&path, 43, &[lane]).unwrap();
    let epoch = arena.bind_peer_lane(42, owner(1)).unwrap();
    let mut producer = ProducerV2::open(&path, 43, lane).unwrap();
    producer.publish(&record(b"last")).unwrap();
    let final_write = arena.begin_close_peer_lane(42, owner(1), epoch).unwrap();
    assert!(matches!(
        producer.publish(&record(b"late")),
        Err(V2Error::LifecycleState {
            expected: LANE_STATE_BOUND,
            observed: LANE_STATE_CLOSING
        })
    ));
    assert_eq!(producer.stats().write_sequence, final_write);
    drop((producer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn double_bind_is_rejected_without_side_effects() {
    let path = tmp("c13-double-bind");
    let lane = peer_lane(43);
    let mut arena = ArenaV2::create(&path, 44, &[lane]).unwrap();
    let epoch = arena.bind_peer_lane(43, owner(1)).unwrap();
    assert!(matches!(
        arena.bind_peer_lane(43, owner(2)),
        Err(V2Error::LifecycleState {
            expected: LANE_STATE_FREE,
            observed: LANE_STATE_BOUND
        })
    ));
    let s = stats_of(&arena, 43);
    assert_eq!(
        (s.state, s.lane_epoch, s.owner),
        (LANE_STATE_BOUND, epoch, owner(1))
    );
    drop(arena);
    fs::remove_file(path).unwrap();
}

#[test]
fn close_requires_the_owner_and_the_epoch_of_the_binding() {
    let path = tmp("c13-close-owner");
    let lane = peer_lane(44);
    let mut arena = ArenaV2::create(&path, 45, &[lane]).unwrap();
    let epoch = arena.bind_peer_lane(44, owner(1)).unwrap();
    assert!(matches!(
        arena.begin_close_peer_lane(44, owner(2), epoch),
        Err(V2Error::LaneMismatch("owner"))
    ));
    assert!(matches!(
        arena.begin_close_peer_lane(44, owner(1), epoch + 1),
        Err(V2Error::LeaseEpochMismatch { expected, observed })
            if expected == epoch && observed == epoch + 1
    ));
    let s = stats_of(&arena, 44);
    assert_eq!((s.state, s.final_write_sequence), (LANE_STATE_BOUND, 0));
    drop(arena);
    fs::remove_file(path).unwrap();
}

#[test]
fn release_requires_the_owner_and_the_epoch_of_the_binding() {
    let path = tmp("c13-release-owner");
    let lane = peer_lane(45);
    let mut arena = ArenaV2::create(&path, 46, &[lane]).unwrap();
    let epoch = arena.bind_peer_lane(45, owner(1)).unwrap();
    arena.begin_close_peer_lane(45, owner(1), epoch).unwrap();
    let consumer = ConsumerV2::open(&path, 46, lane).unwrap();
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    assert!(matches!(
        arena.release_peer_lane(45, owner(2), epoch),
        Err(V2Error::LaneMismatch("owner"))
    ));
    assert!(matches!(
        arena.release_peer_lane(45, owner(1), epoch - 1),
        Err(V2Error::LeaseEpochMismatch { expected, observed })
            if expected == epoch && observed == epoch - 1
    ));
    let s = stats_of(&arena, 45);
    assert_eq!((s.state, s.owner), (LANE_STATE_CLOSING, owner(1)));
    arena.release_peer_lane(45, owner(1), epoch).unwrap();
    drop((consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn release_before_the_drain_is_rejected() {
    let path = tmp("c13-undrained");
    let lane = peer_lane(46);
    let mut arena = ArenaV2::create(&path, 47, &[lane]).unwrap();
    let epoch = arena.bind_peer_lane(46, owner(1)).unwrap();
    let mut producer = ProducerV2::open(&path, 47, lane).unwrap();
    let consumer = ConsumerV2::open(&path, 47, lane).unwrap();
    producer.publish(&record(b"unread")).unwrap();
    arena.begin_close_peer_lane(46, owner(1), epoch).unwrap();
    assert!(!consumer.acknowledge_drained_if_closing().unwrap());
    assert!(matches!(
        arena.release_peer_lane(46, owner(1), epoch),
        Err(V2Error::LaneNotDrained {
            read: 0,
            final_write: 1
        })
    ));
    let s = stats_of(&arena, 46);
    assert_eq!((s.state, s.drained_epoch), (LANE_STATE_CLOSING, 0));
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn an_unread_record_prevents_reuse_until_it_is_consumed() {
    let path = tmp("c13-unread");
    let lane = peer_lane(47);
    let mut arena = ArenaV2::create(&path, 48, &[lane]).unwrap();
    let epoch = arena.bind_peer_lane(47, owner(1)).unwrap();
    let mut producer = ProducerV2::open(&path, 48, lane).unwrap();
    let mut consumer = ConsumerV2::open(&path, 48, lane).unwrap();
    for n in 0..3u8 {
        producer.publish(&record(&[n])).unwrap();
    }
    arena.begin_close_peer_lane(47, owner(1), epoch).unwrap();
    consumer.try_consume().unwrap().unwrap();
    consumer.try_consume().unwrap().unwrap();
    assert!(!consumer.acknowledge_drained_if_closing().unwrap());
    assert!(matches!(
        arena.release_peer_lane(47, owner(1), epoch),
        Err(V2Error::LaneNotDrained {
            read: 2,
            final_write: 3
        })
    ));
    assert!(matches!(
        arena.bind_peer_lane(47, owner(2)),
        Err(V2Error::LifecycleState {
            expected: LANE_STATE_FREE,
            observed: LANE_STATE_CLOSING
        })
    ));

    assert_eq!(&*consumer.try_consume().unwrap().unwrap().payload, &[2]);
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    arena.release_peer_lane(47, owner(1), epoch).unwrap();
    assert_eq!(arena.bind_peer_lane(47, owner(2)).unwrap(), epoch + 1);
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn an_empty_closing_lane_drains_at_once() {
    let path = tmp("c13-empty-closing");
    let lane = peer_lane(48);
    let mut arena = ArenaV2::create(&path, 49, &[lane]).unwrap();
    let epoch = arena.bind_peer_lane(48, owner(1)).unwrap();
    assert_eq!(arena.begin_close_peer_lane(48, owner(1), epoch).unwrap(), 0);
    let consumer = ConsumerV2::open(&path, 49, lane).unwrap();
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    assert_eq!(stats_of(&arena, 48).drained_epoch, epoch);
    arena.release_peer_lane(48, owner(1), epoch).unwrap();
    assert_eq!(stats_of(&arena, 48).state, LANE_STATE_FREE);
    drop((consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn a_wrong_arena_or_lane_generation_is_rejected() {
    let path = tmp("c13-generation");
    let lane = peer_lane(50);
    drop(ArenaV2::create(&path, 50, &[lane]).unwrap());
    assert!(matches!(
        ArenaV2::open(&path, 51),
        Err(V2Error::GenerationMismatch {
            expected: 51,
            observed: 50
        })
    ));
    assert!(matches!(
        ProducerV2::open(&path, 51, lane),
        Err(V2Error::GenerationMismatch { .. })
    ));
    assert!(matches!(
        ConsumerV2::open(&path, 51, lane),
        Err(V2Error::GenerationMismatch { .. })
    ));
    assert!(matches!(
        ArenaV2::open(&path, 0),
        Err(V2Error::ZeroGeneration)
    ));

    // A lane header that disagrees with the arena generation.
    let (mapping, lane_offset) = open_lane(&path, 50, lane).unwrap();
    mapping
        .atomic_u64(lane_offset + LH_OFF_GENERATION)
        .store(49, Ordering::Release);
    drop(mapping);
    assert!(matches!(
        ArenaV2::open(&path, 50),
        Err(V2Error::GenerationMismatch {
            expected: 50,
            observed: 49
        })
    ));
    assert!(matches!(
        ConsumerV2::open(&path, 50, lane),
        Err(V2Error::GenerationMismatch { .. })
    ));
    fs::remove_file(path).unwrap();
}

#[test]
fn a_record_of_another_epoch_quarantines_the_lane_once() {
    let path = tmp("c13-record-epoch");
    let lane = peer_lane(51);
    let mut arena = ArenaV2::create(&path, 52, &[lane]).unwrap();
    let epoch = arena.bind_peer_lane(51, owner(1)).unwrap();
    let mut producer = ProducerV2::open(&path, 52, lane).unwrap();
    producer.publish(&record(b"r")).unwrap();

    // Rewrite the record's epoch and re-seal its CRC, so that only the
    // epoch check can refuse it.
    let (mut mapping, lane_offset) = open_lane(&path, 52, lane).unwrap();
    let slot_offset = lane_offset + LANE_HEADER_SIZE_V2;
    {
        let slot = &mut mapping.bytes_mut()[slot_offset..slot_offset + lane.slot_stride];
        put_u64(slot, SLOT_OFF_LANE_EPOCH, epoch + 7);
        let payload_len = get_u32(slot, SLOT_OFF_PAYLOAD_LEN) as usize;
        let crc = record_crc32c(slot, payload_len);
        put_u32(slot, SLOT_OFF_CRC32C, crc);
    }
    drop(mapping);

    let mut consumer = ConsumerV2::open(&path, 52, lane).unwrap();
    assert!(matches!(
        consumer.try_consume(),
        Err(V2Error::LaneEpochMismatch { expected, observed })
            if expected == epoch && observed == epoch + 7
    ));
    for _ in 0..4 {
        assert!(matches!(
            consumer.try_consume(),
            Err(V2Error::LaneQuarantined {
                reason: QUARANTINE_EPOCH_MISMATCH
            })
        ));
    }
    let s = consumer.stats();
    assert_eq!(
        (
            s.state,
            s.corrupt_records,
            s.read_sequence,
            s.quarantine_reason
        ),
        (LANE_STATE_QUARANTINED, 1, 0, QUARANTINE_EPOCH_MISMATCH)
    );
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn lane_epoch_exhaustion_fails_closed() {
    let path = tmp("c13-epoch-exhaustion");
    let lane = peer_lane(52);
    let mut arena = ArenaV2::create(&path, 53, &[lane]).unwrap();
    let lane_offset = offset_of(&arena, 52);

    arena
        .mapping
        .atomic_u64(lane_offset + LH_OFF_LANE_EPOCH)
        .store(u64::MAX, Ordering::Release);
    assert!(matches!(
        arena.bind_peer_lane(52, owner(1)),
        Err(V2Error::EpochExhausted)
    ));
    let s = stats_of(&arena, 52);
    assert_eq!(
        (s.state, s.lane_epoch, s.owner),
        (LANE_STATE_FREE, u64::MAX, [0; 32])
    );

    // The last epoch is usable once; the bind after it is refused.
    arena
        .mapping
        .atomic_u64(lane_offset + LH_OFF_LANE_EPOCH)
        .store(u64::MAX - 1, Ordering::Release);
    let last = arena.bind_peer_lane(52, owner(1)).unwrap();
    assert_eq!(last, u64::MAX);
    arena.begin_close_peer_lane(52, owner(1), last).unwrap();
    let consumer = ConsumerV2::open(&path, 53, lane).unwrap();
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    arena.release_peer_lane(52, owner(1), last).unwrap();
    assert!(matches!(
        arena.bind_peer_lane(52, owner(2)),
        Err(V2Error::EpochExhausted)
    ));
    assert_eq!(stats_of(&arena, 52).state, LANE_STATE_FREE);
    drop((consumer, arena));
    fs::remove_file(path).unwrap();
}

fn peer_snapshot(pool: &PeerLanePoolV2, ids: &[u16]) -> Vec<LaneStatsV2> {
    ids.iter()
        .map(|id| pool.arena().lane_stats(*id).unwrap())
        .collect()
}

#[test]
fn the_pool_binds_only_free_lanes_and_reports_exhaustion_without_waiting() {
    let path = tmp("c13-pool");
    let ids = [60u16, 61, 62];
    let lanes = [
        peer_lane(60),
        peer_lane(61),
        peer_lane(62),
        txpool_lane(100),
    ];
    let arena = ArenaV2::create(&path, 60, &lanes).unwrap();
    let mut pool = PeerLanePoolV2::new(
        arena,
        PoolBoundsV2 {
            max_peers: 2,
            churn_reserve: 1,
        },
    )
    .unwrap();
    assert_eq!(pool.stats().peer_lanes, 3);

    let a = pool.allocate(owner(0xa)).unwrap();
    let b = pool.allocate(owner(0xb)).unwrap();
    assert_ne!(a.lane_id(), b.lane_id());
    assert!(ids.contains(&a.lane_id()) && ids.contains(&b.lane_id()));

    // The peer limit is explicit and changes nothing.
    let started = Instant::now();
    let before = peer_snapshot(&pool, &ids);
    assert!(matches!(
        pool.allocate(owner(0xc)),
        Err(V2Error::PeerLimitReached { max_peers: 2 })
    ));
    assert_eq!(peer_snapshot(&pool, &ids), before);

    // A closing (still draining) lane frees a peer slot; the reserve lane
    // takes the new peer.
    pool.begin_close(&a).unwrap();
    let c = pool.allocate(owner(0xc)).unwrap();
    assert!(c.lane_id() != a.lane_id() && c.lane_id() != b.lane_id());

    // No FREE lane left: explicit, nothing stolen.
    pool.begin_close(&b).unwrap();
    let before = peer_snapshot(&pool, &ids);
    assert!(matches!(
        pool.allocate(owner(0xd)),
        Err(V2Error::PoolExhausted { peer_lanes: 3 })
    ));
    assert_eq!(peer_snapshot(&pool, &ids), before);
    assert!(started.elapsed() < Duration::from_secs(1));

    let stats = pool.stats();
    assert_eq!(
        (stats.free, stats.bound, stats.closing, stats.quarantined),
        (0, 1, 2, 0)
    );
    assert_eq!(
        (
            stats.allocations,
            stats.exhausted,
            stats.peer_limit_refusals
        ),
        (3, 1, 1)
    );

    // Once a's lane is drained and released, it is the one reused.
    let consumer_a = ConsumerV2::open(&path, 60, peer_lane(a.lane_id())).unwrap();
    assert!(consumer_a.acknowledge_drained_if_closing().unwrap());
    pool.release(&a).unwrap();
    let d = pool.allocate(owner(0xd)).unwrap();
    assert_eq!((d.lane_id(), d.epoch()), (a.lane_id(), a.epoch() + 1));

    drop((consumer_a, pool));
    fs::remove_file(path).unwrap();
}

#[test]
fn the_pool_bounds_must_match_the_arena() {
    let path = tmp("c13-pool-bounds");
    let lanes = [peer_lane(63), peer_lane(64), txpool_lane(101)];
    drop(ArenaV2::create(&path, 61, &lanes).unwrap());
    for (max_peers, churn_reserve) in [(0, 2), (3, 0), (2, 1), (1, 0)] {
        let arena = ArenaV2::open(&path, 61).unwrap();
        assert!(matches!(
            PeerLanePoolV2::new(
                arena,
                PoolBoundsV2 {
                    max_peers,
                    churn_reserve
                }
            ),
            Err(V2Error::InvalidPoolBounds { peer_lanes: 2, .. })
        ));
    }
    let arena = ArenaV2::open(&path, 61).unwrap();
    let pool = PeerLanePoolV2::new(
        arena,
        PoolBoundsV2 {
            max_peers: 1,
            churn_reserve: 1,
        },
    )
    .unwrap();
    assert_eq!(pool.stats().peer_lanes, 2);
    drop(pool);
    fs::remove_file(path).unwrap();
}

#[test]
fn churn_keeps_epochs_monotonic_owners_apart_and_sequences_continuous() {
    let path = tmp("c13-churn");
    let lane = peer_lane(70);
    let arena = ArenaV2::create(&path, 70, &[lane]).unwrap();
    let mut pool = PeerLanePoolV2::new(
        arena,
        PoolBoundsV2 {
            max_peers: 1,
            churn_reserve: 0,
        },
    )
    .unwrap();
    let mut producer = ProducerV2::open(&path, 70, lane).unwrap();
    let mut consumer = ConsumerV2::open(&path, 70, lane).unwrap();
    let mut previous_epoch = 1u64;
    let mut next_sequence = 0u64;

    for cycle in 0..200u32 {
        let tag = cycle.to_le_bytes();
        let mut owner_bytes = [0u8; 32];
        owner_bytes[..4].copy_from_slice(&tag);
        let lease = pool.allocate(owner_bytes).unwrap();
        assert_eq!(lease.epoch(), previous_epoch + 1);
        previous_epoch = lease.epoch();

        let records = (cycle % 5) as usize;
        for index in 0..records {
            let mut payload = [0u8; 8];
            payload[..4].copy_from_slice(&tag);
            payload[4..].copy_from_slice(&(index as u32).to_le_bytes());
            producer.publish(&record(&payload)).unwrap();
        }
        let final_write = pool.begin_close(&lease).unwrap();
        assert_eq!(final_write, next_sequence + records as u64);

        // Only this binding's records, in order, under this epoch.
        let drained = drain_all(&mut consumer);
        assert_eq!(drained.len(), records);
        for (index, out) in drained.iter().enumerate() {
            assert_eq!(out.sequence, next_sequence + index as u64);
            assert_eq!(out.lane_epoch, lease.epoch());
            assert_eq!(&out.payload[..4], &tag);
            assert_eq!(&out.payload[4..], &(index as u32).to_le_bytes());
        }
        next_sequence += records as u64;

        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        pool.release(&lease).unwrap();
        let s = pool.arena().lane_stats(70).unwrap();
        assert_eq!(
            (s.state, s.owner, s.write_sequence, s.read_sequence),
            (LANE_STATE_FREE, [0; 32], next_sequence, next_sequence)
        );
    }
    let stats = pool.stats();
    assert_eq!((stats.allocations, stats.releases), (200, 200));
    drop((producer, consumer, pool));
    fs::remove_file(path).unwrap();
}

#[test]
fn a_stale_lease_never_acts_on_a_reused_lane_even_with_the_same_owner() {
    let path = tmp("c13-stale-lease");
    let lane = peer_lane(71);
    let arena = ArenaV2::create(&path, 71, &[lane]).unwrap();
    let mut pool = PeerLanePoolV2::new(
        arena,
        PoolBoundsV2 {
            max_peers: 1,
            churn_reserve: 0,
        },
    )
    .unwrap();
    let consumer = ConsumerV2::open(&path, 71, lane).unwrap();

    let first = pool.allocate(owner(7)).unwrap();
    pool.begin_close(&first).unwrap();
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    pool.release(&first).unwrap();
    let second = pool.allocate(owner(7)).unwrap();
    assert_eq!(second.epoch(), first.epoch() + 1);

    assert!(matches!(
        pool.begin_close(&first),
        Err(V2Error::LeaseEpochMismatch { expected, observed })
            if expected == second.epoch() && observed == first.epoch()
    ));
    assert!(matches!(
        pool.release(&first),
        Err(V2Error::LifecycleState {
            expected: LANE_STATE_CLOSING,
            observed: LANE_STATE_BOUND
        })
    ));
    let s = pool.arena().lane_stats(71).unwrap();
    assert_eq!(
        (s.state, s.lane_epoch, s.owner),
        (LANE_STATE_BOUND, second.epoch(), owner(7))
    );
    pool.begin_close(&second).unwrap();
    drop((consumer, pool));
    fs::remove_file(path).unwrap();
}

/// Binds `lane_id` once, fills all four slots (committed 1..=4, epoch
/// 2), drains, releases and binds again. Returns (first, second) epoch.
fn reuse_after_full_ring(
    arena: &mut ArenaV2,
    producer: &mut ProducerV2,
    consumer: &mut ConsumerV2,
    lane_id: u16,
) -> (u64, u64) {
    let first = arena.bind_peer_lane(lane_id, owner(1)).unwrap();
    for n in 0..4u8 {
        producer.publish(&record(&[n])).unwrap();
    }
    assert_eq!(drain_all(consumer).len(), 4);
    arena
        .begin_close_peer_lane(lane_id, owner(1), first)
        .unwrap();
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    arena.release_peer_lane(lane_id, owner(1), first).unwrap();
    let second = arena.bind_peer_lane(lane_id, owner(2)).unwrap();
    (first, second)
}

#[test]
fn slots_of_an_earlier_binding_are_never_accepted_after_reuse() {
    let path = tmp("c13-old-slots");
    let lanes = [peer_lane(72), peer_lane(75)];
    let mut arena = ArenaV2::create(&path, 72, &lanes).unwrap();

    // A write_sequence that runs ahead of its slot: slot 0 still holds
    // committed_sequence 1 from the first binding, never 5.
    let mut producer = ProducerV2::open(&path, 72, lanes[0]).unwrap();
    let mut consumer = ConsumerV2::open(&path, 72, lanes[0]).unwrap();
    reuse_after_full_ring(&mut arena, &mut producer, &mut consumer, 72);
    let (mapping, lane_offset) = open_lane(&path, 72, lanes[0]).unwrap();
    mapping
        .atomic_u64(lane_offset + LH_OFF_WRITE_SEQUENCE)
        .store(5, Ordering::Release);
    assert!(matches!(
        consumer.try_consume(),
        Err(V2Error::CorruptSequence {
            expected: 5,
            observed: 1
        })
    ));
    assert_eq!(consumer.stats().read_sequence, 4);
    assert_eq!(consumer.lifecycle_state(), LANE_STATE_QUARANTINED);

    // Even with a matching committed_sequence, the old record's epoch is
    // refused.
    let mut producer_b = ProducerV2::open(&path, 72, lanes[1]).unwrap();
    let mut consumer_b = ConsumerV2::open(&path, 72, lanes[1]).unwrap();
    let (first, second) = reuse_after_full_ring(&mut arena, &mut producer_b, &mut consumer_b, 75);
    let (mapping_b, lane_offset_b) = open_lane(&path, 72, lanes[1]).unwrap();
    mapping_b
        .atomic_u64(lane_offset_b + LH_OFF_WRITE_SEQUENCE)
        .store(5, Ordering::Release);
    mapping_b
        .atomic_u64(lane_offset_b + LANE_HEADER_SIZE_V2 + SLOT_OFF_COMMITTED)
        .store(5, Ordering::Release);
    assert!(matches!(
        consumer_b.try_consume(),
        Err(V2Error::LaneEpochMismatch { expected, observed })
            if expected == second && observed == first
    ));
    assert_eq!(consumer_b.stats().read_sequence, 4);
    drop((
        mapping, mapping_b, producer, consumer, producer_b, consumer_b, arena,
    ));
    fs::remove_file(path).unwrap();
}

#[test]
fn a_write_after_close_quarantines_the_lane() {
    let path = tmp("c13-late-write");
    let lanes = [peer_lane(73), peer_lane(74)];
    let mut arena = ArenaV2::create(&path, 73, &lanes).unwrap();

    // Detected by the consumer's drain acknowledgement.
    let epoch = arena.bind_peer_lane(73, owner(1)).unwrap();
    let mut producer = ProducerV2::open(&path, 73, lanes[0]).unwrap();
    let mut consumer = ConsumerV2::open(&path, 73, lanes[0]).unwrap();
    producer.publish(&record(b"0")).unwrap();
    assert_eq!(arena.begin_close_peer_lane(73, owner(1), epoch).unwrap(), 1);
    // A producer that passed its state check before the close lands one
    // more record.
    let (mapping, lane_offset) = open_lane(&path, 73, lanes[0]).unwrap();
    mapping
        .atomic_u64(lane_offset + LH_OFF_WRITE_SEQUENCE)
        .store(2, Ordering::Release);
    consumer.try_consume().unwrap().unwrap();
    assert!(matches!(
        consumer.acknowledge_drained_if_closing(),
        Err(V2Error::LateWriteAfterClose {
            final_write: 1,
            write: 2
        })
    ));
    let s = stats_of(&arena, 73);
    assert_eq!(
        (s.state, s.shutdown, s.quarantine_reason, s.corrupt_records),
        (LANE_STATE_QUARANTINED, true, QUARANTINE_LATE_WRITE, 0)
    );
    assert!(matches!(
        arena.release_peer_lane(73, owner(1), epoch),
        Err(V2Error::LifecycleState {
            expected: LANE_STATE_CLOSING,
            observed: LANE_STATE_QUARANTINED
        })
    ));
    assert!(matches!(
        producer.publish(&record(b"x")),
        Err(V2Error::LifecycleState {
            observed: LANE_STATE_QUARANTINED,
            ..
        })
    ));

    // Detected by the release after an acknowledged drain.
    let epoch = arena.bind_peer_lane(74, owner(2)).unwrap();
    arena.begin_close_peer_lane(74, owner(2), epoch).unwrap();
    let consumer_b = ConsumerV2::open(&path, 73, lanes[1]).unwrap();
    assert!(consumer_b.acknowledge_drained_if_closing().unwrap());
    let (mapping_b, lane_offset_b) = open_lane(&path, 73, lanes[1]).unwrap();
    mapping_b
        .atomic_u64(lane_offset_b + LH_OFF_WRITE_SEQUENCE)
        .store(1, Ordering::Release);
    assert!(matches!(
        arena.release_peer_lane(74, owner(2), epoch),
        Err(V2Error::LateWriteAfterClose {
            final_write: 0,
            write: 1
        })
    ));
    assert_eq!(stats_of(&arena, 74).state, LANE_STATE_QUARANTINED);
    assert!(matches!(
        arena.bind_peer_lane(74, owner(3)),
        Err(V2Error::LifecycleState {
            expected: LANE_STATE_FREE,
            observed: LANE_STATE_QUARANTINED
        })
    ));
    drop((mapping, mapping_b, producer, consumer, consumer_b, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn quarantine_is_terminal_and_a_restart_quarantines_the_old_generation() {
    let path = tmp("c13-quarantine");
    let lanes = [peer_lane(80), peer_lane(81), txpool_lane(102)];
    let mut arena = ArenaV2::create(&path, 80, &lanes).unwrap();
    let epoch = arena.bind_peer_lane(80, owner(1)).unwrap();
    let consumer = ConsumerV2::open(&path, 80, lanes[0]).unwrap();
    let mut producer = ProducerV2::open(&path, 80, lanes[0]).unwrap();

    assert_eq!(consumer.quarantine(), LANE_STATE_BOUND);
    assert_eq!(consumer.quarantine(), LANE_STATE_QUARANTINED);
    assert_eq!(consumer.lifecycle_state(), LANE_STATE_QUARANTINED);
    assert!(matches!(
        producer.publish(&record(b"x")),
        Err(V2Error::LifecycleState {
            observed: LANE_STATE_QUARANTINED,
            ..
        })
    ));
    assert!(matches!(
        arena.begin_close_peer_lane(80, owner(1), epoch),
        Err(V2Error::LifecycleState {
            observed: LANE_STATE_QUARANTINED,
            ..
        })
    ));
    assert!(matches!(
        arena.bind_peer_lane(80, owner(2)),
        Err(V2Error::LifecycleState {
            observed: LANE_STATE_QUARANTINED,
            ..
        })
    ));
    drop(arena);

    // Restart: the new runtime quarantines every lane of the old
    // generation before abandoning it. No record is destroyed.
    let mut old = ArenaV2::open(&path, 80).unwrap();
    assert_eq!(old.generation(), 80);
    assert_eq!(old.quarantine_lane(81).unwrap(), LANE_STATE_FREE);
    assert_eq!(old.quarantine_all().unwrap(), 1);
    assert_eq!(old.quarantine_all().unwrap(), 0);
    for (id, reason) in [
        (80u16, QUARANTINE_OPERATOR),
        (81, QUARANTINE_OPERATOR),
        (102, QUARANTINE_RESTART),
    ] {
        let s = old.lane_stats(id).unwrap();
        assert_eq!(
            (s.state, s.shutdown, s.quarantine_reason),
            (LANE_STATE_QUARANTINED, true, reason)
        );
    }
    let mut txpool = ProducerV2::open(&path, 80, lanes[2]).unwrap();
    assert!(matches!(
        txpool.publish(&record(b"t")),
        Err(V2Error::LifecycleState {
            observed: LANE_STATE_QUARANTINED,
            ..
        })
    ));
    let mut pool = PeerLanePoolV2::new(
        old,
        PoolBoundsV2 {
            max_peers: 1,
            churn_reserve: 1,
        },
    )
    .unwrap();
    assert!(matches!(
        pool.allocate(owner(9)),
        Err(V2Error::PoolExhausted { peer_lanes: 2 })
    ));
    assert_eq!(pool.stats().quarantined, 2);
    assert!(matches!(
        ArenaV2::open(&path, 81),
        Err(V2Error::GenerationMismatch { .. })
    ));
    drop((consumer, producer, txpool, pool));
    fs::remove_file(path).unwrap();
}

#[test]
fn independent_peer_lanes_cycle_concurrently() {
    const LANES: u16 = 4;
    const ROUNDS: u32 = 25;
    const PER_ROUND: u32 = 6;
    let path = tmp("c13-concurrent");
    let configs: Vec<LaneConfigV2> = (0..LANES).map(|i| peer_lane(90 + i)).collect();
    let mut arena = ArenaV2::create(&path, 90, &configs).unwrap();
    let bound_round: Arc<Vec<TestAtomicU64>> =
        Arc::new((0..LANES).map(|_| TestAtomicU64::new(0)).collect());
    let published_round: Arc<Vec<TestAtomicU64>> =
        Arc::new((0..LANES).map(|_| TestAtomicU64::new(0)).collect());

    let lane_owner = |lane: usize, round: u32| {
        let mut bytes = [0u8; 32];
        bytes[0] = lane as u8;
        bytes[1..5].copy_from_slice(&round.to_le_bytes());
        bytes
    };
    let mut owners = Vec::new();
    let mut epochs = Vec::new();
    for (lane, config) in configs.iter().enumerate() {
        owners.push(lane_owner(lane, 0));
        epochs.push(arena.bind_peer_lane(config.lane_id, owners[lane]).unwrap());
        bound_round[lane].store(1, TestOrdering::Release);
    }

    let mut producers = Vec::new();
    for (lane, config) in configs.iter().copied().enumerate() {
        let path = path.clone();
        let bound_round = Arc::clone(&bound_round);
        let published_round = Arc::clone(&published_round);
        producers.push(thread::spawn(move || {
            let mut producer = ProducerV2::open(&path, 90, config).unwrap();
            let deadline = Instant::now() + Duration::from_secs(30);
            for round in 0..ROUNDS {
                while bound_round[lane].load(TestOrdering::Acquire) != u64::from(round) + 1 {
                    assert!(Instant::now() < deadline, "producer {lane} waited too long");
                    thread::yield_now();
                }
                for index in 0..PER_ROUND {
                    let mut payload = [0u8; 12];
                    payload[..4].copy_from_slice(&(lane as u32).to_le_bytes());
                    payload[4..8].copy_from_slice(&round.to_le_bytes());
                    payload[8..].copy_from_slice(&index.to_le_bytes());
                    loop {
                        match producer.publish(&record(&payload)) {
                            Ok(_) => break,
                            Err(V2Error::QueueFull { .. }) => {
                                assert!(Instant::now() < deadline, "lane {lane} stayed full");
                                thread::yield_now();
                            }
                            Err(error) => panic!("lane {lane}: {error}"),
                        }
                    }
                }
                published_round[lane].store(u64::from(round) + 1, TestOrdering::Release);
            }
        }));
    }

    let mut consumers: Vec<ConsumerV2> = configs
        .iter()
        .map(|config| ConsumerV2::open(&path, 90, *config).unwrap())
        .collect();
    let mut rounds = vec![0u32; LANES as usize];
    let mut consumed = vec![0u32; LANES as usize];
    let mut finished = 0;
    let deadline = Instant::now() + Duration::from_secs(30);
    while finished < LANES as usize {
        for (lane, config) in configs.iter().enumerate() {
            if rounds[lane] == ROUNDS {
                continue;
            }
            while let Some(out) = consumers[lane].try_consume().unwrap() {
                assert_eq!(out.lane_epoch, epochs[lane]);
                let field =
                    |at: usize| u32::from_le_bytes(out.payload[at..at + 4].try_into().unwrap());
                assert_eq!(
                    (field(0), field(4), field(8)),
                    (lane as u32, rounds[lane], consumed[lane])
                );
                consumed[lane] += 1;
            }
            let producer_done =
                published_round[lane].load(TestOrdering::Acquire) == u64::from(rounds[lane]) + 1;
            if consumed[lane] == PER_ROUND && producer_done {
                let final_write = arena
                    .begin_close_peer_lane(config.lane_id, owners[lane], epochs[lane])
                    .unwrap();
                assert_eq!(final_write, u64::from((rounds[lane] + 1) * PER_ROUND));
                assert!(consumers[lane].acknowledge_drained_if_closing().unwrap());
                arena
                    .release_peer_lane(config.lane_id, owners[lane], epochs[lane])
                    .unwrap();
                rounds[lane] += 1;
                consumed[lane] = 0;
                if rounds[lane] == ROUNDS {
                    finished += 1;
                    continue;
                }
                owners[lane] = lane_owner(lane, rounds[lane]);
                let next = arena.bind_peer_lane(config.lane_id, owners[lane]).unwrap();
                assert_eq!(next, epochs[lane] + 1);
                epochs[lane] = next;
                bound_round[lane].store(u64::from(rounds[lane]) + 1, TestOrdering::Release);
            }
        }
        assert!(
            Instant::now() < deadline,
            "lifecycle stalled: rounds={rounds:?}"
        );
        thread::yield_now();
    }
    for producer in producers {
        producer.join().unwrap();
    }
    for (lane, config) in configs.iter().enumerate() {
        let s = stats_of(&arena, config.lane_id);
        assert_eq!(s.state, LANE_STATE_FREE);
        assert_eq!(s.lane_epoch, 1 + u64::from(ROUNDS));
        assert_eq!(s.read_sequence, u64::from(ROUNDS * PER_ROUND));
        assert_eq!(epochs[lane], 1 + u64::from(ROUNDS));
    }
    drop((consumers, arena));
    fs::remove_file(path).unwrap();
}

// -----------------------------------------------------------------
// K12: structural corruption quarantines once, never skips
// -----------------------------------------------------------------

#[test]
fn every_structural_defect_quarantines_once_and_is_never_skipped() {
    for (defect, reason) in [
        ("sequence", QUARANTINE_CORRUPT_SEQUENCE),
        ("payload-length", QUARANTINE_PAYLOAD_LENGTH),
        ("epoch", QUARANTINE_EPOCH_MISMATCH),
        ("crc", QUARANTINE_CHECKSUM),
        ("kind", QUARANTINE_RECORD_KIND),
    ] {
        let path = tmp(defect);
        let lane = peer_lane(110);
        let mut arena = ArenaV2::create(&path, 110, &[lane]).unwrap();
        let epoch = arena.bind_peer_lane(110, owner(1)).unwrap();
        let mut producer = ProducerV2::open(&path, 110, lane).unwrap();
        producer.publish(&record(b"bad")).unwrap();
        producer.publish(&record(b"good")).unwrap();

        let (mut mapping, lane_offset) = open_lane(&path, 110, lane).unwrap();
        let slot_offset = lane_offset + LANE_HEADER_SIZE_V2;
        match defect {
            "sequence" => mapping
                .atomic_u64(slot_offset + SLOT_OFF_COMMITTED)
                .store(7, Ordering::Release),
            _ => {
                let slot = &mut mapping.bytes_mut()[slot_offset..slot_offset + lane.slot_stride];
                match defect {
                    "payload-length" => {
                        put_u32(slot, SLOT_OFF_PAYLOAD_LEN, (lane.max_payload() + 1) as u32)
                    }
                    "epoch" => put_u64(slot, SLOT_OFF_LANE_EPOCH, epoch + 1),
                    "crc" => slot[SLOT_HEADER_SIZE_V2] ^= 0x01,
                    _ => put_u16(slot, SLOT_OFF_KIND, 99),
                }
                if defect == "epoch" || defect == "kind" {
                    // Re-seal so that only the targeted check can refuse it.
                    let len = get_u32(slot, SLOT_OFF_PAYLOAD_LEN) as usize;
                    let crc = record_crc32c(slot, len);
                    put_u32(slot, SLOT_OFF_CRC32C, crc);
                }
            }
        }
        drop(mapping);

        let mut consumer = ConsumerV2::open(&path, 110, lane).unwrap();
        let first = consumer.try_consume();
        let reported = matches!(
            (defect, &first),
            ("sequence", Err(V2Error::CorruptSequence { .. }))
                | ("payload-length", Err(V2Error::PayloadTooLarge { .. }))
                | ("epoch", Err(V2Error::LaneEpochMismatch { .. }))
                | ("crc", Err(V2Error::ChecksumMismatch { .. }))
                | ("kind", Err(V2Error::UnknownRecordKind(99)))
        );
        assert!(reported, "{defect}: first poll returned {first:?}");
        for _ in 0..5 {
            assert!(
                matches!(
                    consumer.try_consume(),
                    Err(V2Error::LaneQuarantined { reason: r }) if r == reason
                ),
                "{defect}"
            );
        }
        let s = consumer.stats();
        assert_eq!(
            (
                s.state,
                s.corrupt_records,
                s.read_sequence,
                s.quarantine_reason,
                s.shutdown
            ),
            (LANE_STATE_QUARANTINED, 1, 0, reason, true),
            "{defect}"
        );
        // Not consumed, not reusable, no publish.
        assert!(matches!(
            producer.publish(&record(b"x")),
            Err(V2Error::LifecycleState {
                observed: LANE_STATE_QUARANTINED,
                ..
            })
        ));
        assert!(matches!(
            arena.begin_close_peer_lane(110, owner(1), epoch),
            Err(V2Error::LifecycleState {
                observed: LANE_STATE_QUARANTINED,
                ..
            })
        ));
        assert!(matches!(
            arena.bind_peer_lane(110, owner(2)),
            Err(V2Error::LifecycleState {
                observed: LANE_STATE_QUARANTINED,
                ..
            })
        ));
        assert!(matches!(
            consumer.acknowledge_drained_if_closing(),
            Err(V2Error::LaneQuarantined { .. })
        ));
        drop((producer, consumer, arena));
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn records_outside_a_binding_quarantine_as_late_writes_without_counting_corruption() {
    let path = tmp("c13-outside-binding");
    let lanes = [peer_lane(111), peer_lane(112)];
    let mut arena = ArenaV2::create(&path, 111, &lanes).unwrap();

    // A record on a FREE lane.
    let (mapping, lane_offset) = open_lane(&path, 111, lanes[0]).unwrap();
    mapping
        .atomic_u64(lane_offset + LH_OFF_WRITE_SEQUENCE)
        .store(1, Ordering::Release);
    let mut consumer = ConsumerV2::open(&path, 111, lanes[0]).unwrap();
    assert!(matches!(
        consumer.try_consume(),
        Err(V2Error::LateWriteAfterClose {
            final_write: 0,
            write: 1
        })
    ));
    let s = consumer.stats();
    assert_eq!(
        (
            s.state,
            s.quarantine_reason,
            s.corrupt_records,
            s.read_sequence
        ),
        (LANE_STATE_QUARANTINED, QUARANTINE_LATE_WRITE, 0, 0)
    );

    // A record at final_write_sequence on a CLOSING lane.
    let epoch = arena.bind_peer_lane(112, owner(1)).unwrap();
    let mut producer = ProducerV2::open(&path, 111, lanes[1]).unwrap();
    producer.publish(&record(b"in")).unwrap();
    arena.begin_close_peer_lane(112, owner(1), epoch).unwrap();
    let (mapping_b, lane_offset_b) = open_lane(&path, 111, lanes[1]).unwrap();
    mapping_b
        .atomic_u64(lane_offset_b + LH_OFF_WRITE_SEQUENCE)
        .store(2, Ordering::Release);
    let mut consumer_b = ConsumerV2::open(&path, 111, lanes[1]).unwrap();
    assert_eq!(&*consumer_b.try_consume().unwrap().unwrap().payload, b"in");
    assert!(matches!(
        consumer_b.try_consume(),
        Err(V2Error::LateWriteAfterClose {
            final_write: 1,
            write: 2
        })
    ));
    let s = consumer_b.stats();
    assert_eq!(
        (
            s.state,
            s.quarantine_reason,
            s.corrupt_records,
            s.read_sequence
        ),
        (LANE_STATE_QUARANTINED, QUARANTINE_LATE_WRITE, 0, 1)
    );
    drop((mapping, mapping_b, consumer, consumer_b, producer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn the_pool_never_takes_bound_closing_or_quarantined_lanes() {
    let path = tmp("c13-pool-mixed");
    let ids = [120u16, 121, 122, 123];
    let lanes = ids.map(peer_lane);
    let mut arena = ArenaV2::create(&path, 120, &lanes).unwrap();
    let bound = arena.bind_peer_lane(120, owner(1)).unwrap();
    let closing = arena.bind_peer_lane(121, owner(2)).unwrap();
    arena.begin_close_peer_lane(121, owner(2), closing).unwrap();
    arena.quarantine_lane(122).unwrap();
    let mut pool = PeerLanePoolV2::new(
        arena,
        PoolBoundsV2 {
            max_peers: 3,
            churn_reserve: 1,
        },
    )
    .unwrap();

    let lease = pool.allocate(owner(4)).unwrap();
    assert_eq!(lease.lane_id(), 123);
    let before = peer_snapshot(&pool, &ids);
    assert!(matches!(
        pool.allocate(owner(5)),
        Err(V2Error::PoolExhausted { peer_lanes: 4 })
    ));
    assert_eq!(peer_snapshot(&pool, &ids), before);
    assert_eq!(before[0].lane_epoch, bound);
    let stats = pool.stats();
    assert_eq!(
        (stats.free, stats.bound, stats.closing, stats.quarantined),
        (0, 2, 1, 1)
    );
    drop(pool);
    fs::remove_file(path).unwrap();
}

// -----------------------------------------------------------------
// K11: shutdown refuses binds and publishes, never unmaps a live user
// -----------------------------------------------------------------

#[test]
fn shutdown_refuses_binds_and_publishes_while_live_handles_stay_valid() {
    let path = tmp("c13-shutdown");
    let lanes = [peer_lane(130), peer_lane(131)];
    let arena = ArenaV2::create(&path, 130, &lanes).unwrap();
    let mut pool = PeerLanePoolV2::new(
        arena,
        PoolBoundsV2 {
            max_peers: 1,
            churn_reserve: 1,
        },
    )
    .unwrap();
    let lease = pool.allocate(owner(1)).unwrap();
    let config = peer_lane(lease.lane_id());
    let published = Arc::new(TestAtomicU64::new(0));

    let producer_path = path.clone();
    let producer_count = Arc::clone(&published);
    let producer = thread::spawn(move || {
        let mut producer = ProducerV2::open(&producer_path, 130, config).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match producer.publish(&record(b"p")) {
                Ok(_) => {
                    producer_count.fetch_add(1, TestOrdering::Release);
                }
                Err(V2Error::QueueFull { .. }) => thread::yield_now(),
                Err(V2Error::Shutdown) => return,
                Err(error) => panic!("{error}"),
            }
            assert!(Instant::now() < deadline, "shutdown never observed");
        }
    });

    let mut consumer = ConsumerV2::open(&path, 130, config).unwrap();
    let mut consumed = 0u64;
    while consumed < 8 {
        if consumer.try_consume().unwrap().is_some() {
            consumed += 1;
        }
        thread::yield_now();
    }
    pool.begin_shutdown().unwrap();
    producer.join().unwrap();

    // Everything published is still readable, then the lane reports its end.
    loop {
        match consumer.try_consume() {
            Ok(Some(_)) => consumed += 1,
            Ok(None) => thread::yield_now(),
            Err(V2Error::Shutdown) => break,
            Err(error) => panic!("{error}"),
        }
    }
    assert_eq!(consumed, published.load(TestOrdering::Acquire));
    assert!(matches!(pool.allocate(owner(2)), Err(V2Error::Shutdown)));
    let other = if lease.lane_id() == 130 { 131 } else { 130 };
    assert!(matches!(
        pool.arena_mut().bind_peer_lane(other, owner(3)),
        Err(V2Error::Shutdown)
    ));
    // Close, drain and release stay deterministic during shutdown.
    pool.begin_close(&lease).unwrap();
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    pool.release(&lease).unwrap();
    assert!(matches!(
        pool.arena_mut().bind_peer_lane(lease.lane_id(), owner(4)),
        Err(V2Error::Shutdown)
    ));
    drop((consumer, pool));
    fs::remove_file(path).unwrap();
}

// -----------------------------------------------------------------
// K14: exclusive runtime role leases
// -----------------------------------------------------------------

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let id = ID.fetch_add(1, TestOrdering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "darkbot-fastlane-v2-{}-{}-{name}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn role_leases_are_exclusive_per_role_and_coexist_across_roles() {
    let dir = tmp_dir("lease");
    let geth = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::GethOwner, 7).unwrap();
    let rust = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, 7).unwrap();
    assert_ne!(geth.path(), rust.path());
    assert!(matches!(
        RoleLeaseV2::acquire(&dir, RuntimeRoleV2::GethOwner, 7),
        Err(V2Error::RoleLeaseHeld { role: "geth-owner" })
    ));
    assert!(matches!(
        RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, 8),
        Err(V2Error::RoleLeaseHeld { role: "rust-owner" })
    ));
    assert!(matches!(
        RoleLeaseV2::acquire(&dir, RuntimeRoleV2::GethOwner, 0),
        Err(V2Error::ZeroGeneration)
    ));
    let content = fs::read_to_string(geth.path()).unwrap();
    assert_eq!(
        content,
        format!(
            "version=1\nrole=geth-owner\ngeneration=7\npid={}\n",
            std::process::id()
        )
    );

    // Closing the owner releases the lock; the file stays.
    drop(geth);
    let again = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::GethOwner, 9).unwrap();
    assert_eq!(again.role(), RuntimeRoleV2::GethOwner);
    drop((again, rust));
    fs::remove_dir_all(dir).unwrap();
}

fn victim(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("victim");
    fs::write(&path, b"keep").unwrap();
    path
}

#[test]
fn e75_linked_lock_files_are_refused_and_never_truncated() {
    for hard in [false, true] {
        let dir = tmp_dir("e75-lock");
        let other = tmp_dir("e75-victim");
        let victim = victim(&other);
        let lock = dir.join("geth-owner.lock");
        if hard {
            fs::hard_link(&victim, &lock).unwrap();
        } else {
            std::os::unix::fs::symlink(&victim, &lock).unwrap();
        }
        assert!(RoleLeaseV2::acquire(&dir, RuntimeRoleV2::GethOwner, 1).is_err());
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(other).unwrap();
    }
}

#[test]
fn e75_untrusted_directories_and_arena_links_fail_closed() {
    use std::os::unix::fs::PermissionsExt;
    let lanes = [txpool_lane(1)];
    let dir = tmp_dir("e75-open");
    for mode in [0o770, 0o702, 0o777] {
        fs::set_permissions(&dir, fs::Permissions::from_mode(mode)).unwrap();
        let error = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, 1)
            .err()
            .unwrap();
        assert!(
            matches!(&error, V2Error::Io(e) if crate::trust::is_untrusted(e)),
            "{error}"
        );
        let error = ArenaV2::create(&arena_path_v2(&dir, 1), 1, &lanes)
            .err()
            .unwrap();
        assert!(
            matches!(&error, V2Error::Io(e) if crate::trust::is_untrusted(e)),
            "{error}"
        );
        assert!(stale_arenas_v2(&dir, 1).is_err());
    }
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();

    let other = tmp_dir("e75-real");
    let real = arena_path_v2(&other, 1);
    let arena = ArenaV2::create(&real, 1, &lanes).unwrap();
    let link = arena_path_v2(&dir, 1);
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(ArenaV2::open(&link, 1).is_err());
    assert!(probe_arena_v2(&link, 1).is_err());
    assert!(LaneSetV2::open(&link, 1, &lanes).is_err());
    assert!(ConsumerV2::open(&link, 1, lanes[0]).is_err());
    let victim = victim(&other);
    let victim_link = arena_path_v2(&dir, 2);
    std::os::unix::fs::symlink(&victim, &victim_link).unwrap();
    assert!(ArenaV2::create(&victim_link, 2, &lanes).is_err());
    assert_eq!(fs::read(&victim).unwrap(), b"keep");

    fs::set_permissions(&real, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(ArenaV2::open(&real, 1).is_err());
    fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(ArenaV2::open(&real, 1).is_ok());
    assert_eq!(
        probe_arena_v2(&arena_path_v2(&other, 9), 9).unwrap(),
        ArenaProbeV2::Absent
    );
    drop(arena);
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(other).unwrap();
}

const LEASE_CHILD_ENV: &str = "DARKBOT_FASTLANE_LEASE_CHILD_DIR";

#[test]
#[ignore = "helper process of role_lease_is_released_when_the_holder_process_dies"]
fn role_lease_child_holder() {
    let Some(dir) = std::env::var_os(LEASE_CHILD_ENV) else {
        return;
    };
    let _lease =
        RoleLeaseV2::acquire(std::path::Path::new(&dir), RuntimeRoleV2::RustOwner, 1).unwrap();
    // libtest may prefix the line with the test name; the parent searches.
    println!("\nDARKBOT_LEASE_LOCKED");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    thread::sleep(Duration::from_secs(60));
}

#[test]
fn role_lease_is_released_when_the_holder_process_dies() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let dir = tmp_dir("lease-child");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "protocol_v2::tests::role_lease_child_holder",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(LEASE_CHILD_ENV, &dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut locked = false;
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap() > 0 {
        if line.contains("DARKBOT_LEASE_LOCKED") {
            locked = true;
            break;
        }
        line.clear();
    }
    assert!(locked, "the child process did not take the lease");
    assert!(matches!(
        RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, 1),
        Err(V2Error::RoleLeaseHeld { .. })
    ));
    child.kill().unwrap();
    child.wait().unwrap();
    let lease = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, 1).unwrap();
    drop(lease);
    fs::remove_dir_all(dir).unwrap();
}

// -----------------------------------------------------------------
// K15: generation, restart, O_EXCL primitives
// -----------------------------------------------------------------

#[test]
fn existing_arenas_are_detected_never_attached_and_never_overwritten() {
    let dir = tmp_dir("probe");
    let lane = peer_lane(140);
    let current = arena_path_v2(&dir, 41);
    assert_eq!(
        current.file_name().unwrap().to_str().unwrap(),
        "fastlane-v2-g00000000000000000041.mmap"
    );
    assert_eq!(probe_arena_v2(&current, 41).unwrap(), ArenaProbeV2::Absent);
    assert!(matches!(
        probe_arena_v2(&current, 0),
        Err(V2Error::ZeroGeneration)
    ));

    drop(ArenaV2::create(&current, 41, &[lane]).unwrap());
    assert_eq!(probe_arena_v2(&current, 41).unwrap(), ArenaProbeV2::Current);
    assert_eq!(
        probe_arena_v2(&current, 42).unwrap(),
        ArenaProbeV2::StaleGeneration { observed: 41 }
    );

    // create_new on an existing arena: explicit result, bytes untouched.
    let before = fs::read(&current).unwrap();
    assert!(matches!(
        ArenaV2::create(&current, 41, &[lane]),
        Err(V2Error::ArenaExists {
            probe: ArenaProbeV2::Current
        })
    ));
    assert!(matches!(
        ArenaV2::create(&current, 42, &[lane]),
        Err(V2Error::ArenaExists {
            probe: ArenaProbeV2::StaleGeneration { observed: 41 }
        })
    ));
    assert_eq!(fs::read(&current).unwrap(), before);
    assert!(matches!(
        ArenaV2::open(&current, 42),
        Err(V2Error::GenerationMismatch {
            expected: 42,
            observed: 41
        })
    ));

    // A file that is not an arena.
    let junk = dir.join("fastlane-v2-g00000000000000000007.mmap");
    fs::write(&junk, b"not an arena").unwrap();
    // A file with group/other bits is outside the trust contract (E7-5).
    assert!(
        matches!(probe_arena_v2(&junk, 7), Err(V2Error::Io(e)) if crate::trust::is_untrusted(&e))
    );
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&junk, fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert_eq!(probe_arena_v2(&junk, 7).unwrap(), ArenaProbeV2::NotAnArena);
    assert!(matches!(
        ArenaV2::create(&junk, 7, &[lane]),
        Err(V2Error::ArenaExists {
            probe: ArenaProbeV2::NotAnArena
        })
    ));
    assert_eq!(fs::read(&junk).unwrap(), b"not an arena");

    // A new generation gets its own file beside the old ones.
    let next = arena_path_v2(&dir, 42);
    drop(ArenaV2::create(&next, 42, &[lane]).unwrap());
    fs::write(dir.join("unrelated.mmap"), b"x").unwrap();
    let stale = stale_arenas_v2(&dir, 42).unwrap();
    assert_eq!(stale, vec![(7, junk.clone()), (41, current.clone())]);
    assert!(current.exists() && junk.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_undefined_lifecycle_state_quarantines_even_an_empty_lane() {
    let path = tmp("c13-invalid-state");
    // 160: empty, polled; 161: one record waiting, polled; 162: empty, acked.
    let lanes = [peer_lane(160), peer_lane(161), peer_lane(162)];
    let mut arena = ArenaV2::create(&path, 160, &lanes).unwrap();
    arena.bind_peer_lane(161, owner(1)).unwrap();
    let mut producer = ProducerV2::open(&path, 160, lanes[1]).unwrap();
    producer.publish(&record(b"r")).unwrap();
    for lane in lanes {
        let (mapping, lane_offset) = open_lane(&path, 160, lane).unwrap();
        mapping
            .atomic_u32(lane_offset + LH_OFF_STATE)
            .store(99, Ordering::Release);
    }
    for (index, lane) in lanes.iter().enumerate() {
        let mut consumer = ConsumerV2::open(&path, 160, *lane).unwrap();
        let first = if index == 2 {
            consumer.acknowledge_drained_if_closing().map(|_| ())
        } else {
            consumer.try_consume().map(|_| ())
        };
        assert!(
            matches!(&first, Err(V2Error::InvalidLifecycleState { observed: 99 })),
            "lane {index}: {first:?}"
        );
        for _ in 0..3 {
            assert!(matches!(
                consumer.try_consume(),
                Err(V2Error::LaneQuarantined {
                    reason: QUARANTINE_INVALID_STATE
                })
            ));
        }
        let s = consumer.stats();
        assert_eq!(
            (s.state, s.corrupt_records, s.quarantine_reason, s.shutdown),
            (LANE_STATE_QUARANTINED, 1, QUARANTINE_INVALID_STATE, true),
            "lane {index}"
        );
    }
    // The waiting record was never consumed.
    assert_eq!(stats_of(&arena, 161).read_sequence, 0);
    drop((producer, arena));
    fs::remove_file(path).unwrap();
}

// -----------------------------------------------------------------
// C1.3 Go<->Rust lifecycle interop. Four alternating processes on one
// arena, run in order by bsc/tools/c13_lifecycle_interop.sh:
// geth phase 1 -> rust phase 1 -> geth phase 2 -> rust phase 2.
// -----------------------------------------------------------------

const INTEROP_ENV: &str = "DARKBOT_C13_INTEROP_DIR";
const INTEROP_GENERATION: u64 = 1303;

/// Non-uniform, so a word-order mismatch between Go and Rust cannot hide.
fn interop_owner(base: u8) -> [u8; 32] {
    std::array::from_fn(|i| base + i as u8)
}

fn interop_arena() -> Option<std::path::PathBuf> {
    std::env::var_os(INTEROP_ENV)
        .map(|dir| arena_path_v2(std::path::Path::new(&dir), INTEROP_GENERATION))
}

/// Phase 1: read what Go wrote, consume lane 1 to its final write, ack
/// the drain; find Go's broken record on lane 2 and quarantine it.
#[test]
#[ignore = "C1.3 Go<->Rust interop phase; run by bsc/tools/c13_lifecycle_interop.sh"]
fn interop_rust_phase_1() {
    let Some(path) = interop_arena() else {
        return;
    };
    assert_eq!(
        probe_arena_v2(&path, INTEROP_GENERATION).unwrap(),
        ArenaProbeV2::Current
    );
    let arena = ArenaV2::open(&path, INTEROP_GENERATION).unwrap();
    let s = arena.lane_stats(1).unwrap();
    assert_eq!(
        (s.state, s.lane_epoch, s.owner, s.final_write_sequence),
        (LANE_STATE_CLOSING, 2, interop_owner(0x10), 3)
    );
    assert_eq!(
        (s.write_sequence, s.read_sequence, s.generation),
        (3, 0, 1303)
    );
    let s = arena.lane_stats(2).unwrap();
    assert_eq!(
        (s.state, s.lane_epoch, s.owner, s.write_sequence),
        (LANE_STATE_BOUND, 2, interop_owner(0x40), 1)
    );

    let mut consumer = ConsumerV2::open(&path, INTEROP_GENERATION, peer_lane(1)).unwrap();
    for n in 0..3u64 {
        let out = consumer.try_consume().unwrap().unwrap();
        assert_eq!(
            (
                out.sequence,
                out.lane_id,
                out.lane_epoch,
                out.kind,
                out.flags
            ),
            (n, 1, 2, RecordKind::EarlyTxSeen, 3)
        );
        assert_eq!(
            (out.timestamp_ns, out.clock_id),
            (123_456_789, CLOCK_ID_MONOTONIC)
        );
        assert_eq!(
            (out.source, out.peer, out.hash, out.aux),
            ([0x11; 16], [0x22; 32], [0x33; 32], [0x44; 32])
        );
        assert_eq!(&*out.payload, format!("interop-{n}").as_bytes());
    }
    assert!(consumer.try_consume().unwrap().is_none());
    assert!(consumer.acknowledge_drained_if_closing().unwrap());
    assert_eq!(arena.lane_stats(1).unwrap().drained_epoch, 2);

    let mut broken = ConsumerV2::open(&path, INTEROP_GENERATION, peer_lane(2)).unwrap();
    assert!(matches!(
        broken.try_consume(),
        Err(V2Error::ChecksumMismatch { .. })
    ));
    assert!(matches!(
        broken.try_consume(),
        Err(V2Error::LaneQuarantined {
            reason: QUARANTINE_CHECKSUM
        })
    ));
    println!("\nC13_INTEROP_RUST_PHASE1_OK");
}

/// Phase 2: Go released and rebound lane 1 with epoch + 1; consume the
/// new binding's record, sequence continuing at 3.
#[test]
#[ignore = "C1.3 Go<->Rust interop phase; run by bsc/tools/c13_lifecycle_interop.sh"]
fn interop_rust_phase_2() {
    let Some(path) = interop_arena() else {
        return;
    };
    let arena = ArenaV2::open(&path, INTEROP_GENERATION).unwrap();
    let s = arena.lane_stats(1).unwrap();
    assert_eq!(
        (s.state, s.lane_epoch, s.owner, s.final_write_sequence),
        (LANE_STATE_BOUND, 3, interop_owner(0x70), 0)
    );
    assert_eq!(
        (s.write_sequence, s.read_sequence, s.drained_epoch),
        (4, 3, 2)
    );
    let mut consumer = ConsumerV2::open(&path, INTEROP_GENERATION, peer_lane(1)).unwrap();
    let out = consumer.try_consume().unwrap().unwrap();
    assert_eq!(
        (out.sequence, out.lane_epoch, &*out.payload),
        (3, 3, &b"interop-3"[..])
    );
    assert!(consumer.try_consume().unwrap().is_none());
    let s = arena.lane_stats(2).unwrap();
    assert_eq!(
        (
            s.state,
            s.quarantine_reason,
            s.corrupt_records,
            s.read_sequence,
            s.shutdown
        ),
        (LANE_STATE_QUARANTINED, QUARANTINE_CHECKSUM, 1, 0, true)
    );
    println!("\nC13_INTEROP_RUST_PHASE2_OK");
}
