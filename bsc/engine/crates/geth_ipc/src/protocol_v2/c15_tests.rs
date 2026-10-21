use super::*;
use std::sync::atomic::AtomicU64 as TestAtomicU64;

const GENERATION: u64 = 1500;
const CAPACITY: usize = 4;

static ID: TestAtomicU64 = TestAtomicU64::new(0);

fn tmp(name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let id = ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("darkbot-fastlane-c15-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    dir.join(format!("{id}-{name}.mmap"))
}

fn lane(lane_id: u16) -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id,
        direction: DirectionV2::GethToRust,
        class: LaneClassV2::PeerIngress,
        flags: 0,
        capacity: CAPACITY,
        slot_stride: 512,
    }
}

fn record(tag: u8, payload: &[u8]) -> RecordV2<'_> {
    RecordV2 {
        kind: RecordKind::EarlyTxSeen,
        flags: 3,
        timestamp_ns: 1_000 + u64::from(tag),
        clock_id: CLOCK_ID_MONOTONIC,
        source: [tag; SOURCE_LEN],
        peer: [tag.wrapping_add(1); PEER_LEN],
        hash: [tag.wrapping_add(2); HASH_LEN],
        aux: [tag.wrapping_add(3); AUX_LEN],
        payload,
    }
}

/// An arena with lanes 16 and 17, both BOUND, and handles for lane 16.
fn setup(name: &str) -> (PathBuf, ArenaV2, ProducerV2, ConsumerV2) {
    let path = tmp(name);
    let mut arena = ArenaV2::create(&path, GENERATION, &[lane(16), lane(17)]).unwrap();
    arena.bind_peer_lane(16, [0x16; 32]).unwrap();
    arena.bind_peer_lane(17, [0x17; 32]).unwrap();
    let producer = ProducerV2::open(&path, GENERATION, lane(16)).unwrap();
    let consumer = ConsumerV2::open(&path, GENERATION, lane(16)).unwrap();
    (path, arena, producer, consumer)
}

fn slot_offset(arena: &ArenaV2, lane_id: u16, index: usize) -> usize {
    let lane_offset = locate_lane_by_id(&arena.mapping, lane_id).unwrap().1;
    lane_offset + LANE_HEADER_SIZE_V2 + index * lane(lane_id).slot_stride
}

#[test]
fn the_borrowed_record_is_exactly_the_published_record() {
    let (path, arena, mut producer, mut consumer) = setup("parity");
    let payload = [0xabu8; 200];
    let published = record(7, &payload);
    producer.publish(&published).unwrap();
    producer.publish(&published).unwrap();

    let guard = consumer.try_acquire().unwrap().unwrap();
    let borrowed = guard.record();
    assert_eq!((borrowed.sequence, borrowed.lane_id), (0, 16));
    assert_eq!(borrowed.lane_epoch, 2);
    assert_eq!((borrowed.kind, borrowed.flags), (published.kind, 3));
    assert_eq!(borrowed.timestamp_ns, published.timestamp_ns);
    assert_eq!(borrowed.clock_id, published.clock_id);
    assert_eq!(borrowed.source, &published.source[..]);
    assert_eq!(borrowed.peer, &published.peer[..]);
    assert_eq!(borrowed.hash, &published.hash[..]);
    assert_eq!(borrowed.aux, &published.aux[..]);
    assert_eq!(borrowed.payload, &payload[..]);
    // The bytes are the mmap slot's, not a copy: the payload pointer lies
    // in the consumer's mapping at the slot's payload offset.
    let payload_at = borrowed.payload.as_ptr() as usize;
    let consumer_base = guard.mapping.bytes().as_ptr() as usize;
    let expected_offset = slot_offset(&arena, 16, 0) + SLOT_HEADER_SIZE_V2;
    assert_eq!(payload_at - consumer_base, expected_offset);
    let owned_from_borrow = borrowed.to_owned_record();
    assert_eq!(guard.release().unwrap(), 0);

    // The owned path yields the same content for the same record.
    let owned = consumer.try_consume().unwrap().unwrap();
    assert_eq!(owned.sequence, 1);
    assert_eq!(
        OwnedRecordV2 {
            sequence: 1,
            ..owned_from_borrow
        },
        owned
    );
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn a_held_slot_cannot_be_reused_and_a_released_slot_can() {
    let (path, arena, mut producer, mut consumer) = setup("reuse");
    for tag in 0..CAPACITY as u8 {
        producer.publish(&record(tag, &[tag; 64])).unwrap();
    }
    let guard = consumer.try_acquire().unwrap().unwrap();
    // The lane is full and slot 0 is held: no publish, no overwrite.
    let refused = producer.publish(&record(9, &[9; 64]));
    assert!(matches!(refused, Err(V2Error::QueueFull { sequence: 4 })));
    assert_eq!(producer.stats().dropped_full, 1);
    assert!(guard.is_stable());
    assert_eq!(guard.record().payload, &[0u8; 64][..]);
    assert_eq!(arena.lane_stats(16).unwrap().read_sequence, 0);

    // Release makes the slot reusable at once.
    assert_eq!(guard.release().unwrap(), 0);
    assert_eq!(producer.publish(&record(9, &[9; 64])).unwrap(), 4);
    let mut seen = Vec::new();
    while let Some(guard) = consumer.try_acquire().unwrap() {
        let borrowed = guard.record();
        seen.push((borrowed.sequence, borrowed.payload[0]));
        guard.release().unwrap();
    }
    assert_eq!(seen, [(1, 1), (2, 2), (3, 3), (4, 9)]);
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn dropping_the_guard_releases_the_record() {
    let (path, arena, mut producer, mut consumer) = setup("drop");
    producer.publish(&record(1, b"one")).unwrap();
    producer.publish(&record(2, b"two")).unwrap();
    {
        let guard = consumer.try_acquire().unwrap().unwrap();
        assert_eq!(guard.sequence(), 0);
        assert_eq!(arena.lane_stats(16).unwrap().read_sequence, 0);
    }
    assert_eq!(arena.lane_stats(16).unwrap().read_sequence, 1);
    let guard = consumer.try_acquire().unwrap().unwrap();
    assert_eq!(guard.record().payload, b"two");
    drop(guard);
    assert!(consumer.try_acquire().unwrap().is_none());
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn the_guard_stays_correct_across_wraparound() {
    let (path, arena, mut producer, mut consumer) = setup("wrap");
    let rounds = CAPACITY as u64 * 3;
    for sequence in 0..rounds {
        let tag = sequence as u8;
        let published = producer.publish(&record(tag, &[tag; 32])).unwrap();
        assert_eq!(published, sequence);
        let guard = consumer.try_acquire().unwrap().unwrap();
        let borrowed = guard.record();
        assert_eq!((borrowed.sequence, borrowed.payload[0]), (sequence, tag));
        assert_eq!(guard.release().unwrap(), sequence);
    }
    let stats = arena.lane_stats(16).unwrap();
    assert_eq!((stats.write_sequence, stats.read_sequence), (12, 12));
    assert_eq!((stats.dropped_full, stats.corrupt_records), (0, 0));
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn a_corrupt_record_is_refused_and_never_released() {
    let (path, mut arena, mut producer, mut consumer) = setup("corrupt");
    producer.publish(&record(1, &[1; 64])).unwrap();
    let offset = slot_offset(&arena, 16, 0) + SLOT_HEADER_SIZE_V2 + 10;
    arena.mapping.bytes_mut()[offset] ^= 0x40;
    let refused = consumer.try_acquire().map(|guard| guard.is_some());
    assert!(matches!(refused, Err(V2Error::ChecksumMismatch { .. })));
    let stats = arena.lane_stats(16).unwrap();
    assert_eq!(stats.state, LANE_STATE_QUARANTINED);
    assert_eq!(stats.read_sequence, 0);
    assert_eq!(stats.corrupt_records, 1);
    assert_eq!(stats.quarantine_reason, QUARANTINE_CHECKSUM);
    let again = consumer.try_acquire().map(|guard| guard.is_some());
    assert!(matches!(again, Err(V2Error::LaneQuarantined { .. })));
    assert_eq!(arena.lane_stats(16).unwrap().corrupt_records, 1);
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn a_visible_commit_change_under_the_guard_quarantines_and_is_not_released() {
    let (path, arena, mut producer, mut consumer) = setup("rewrite");
    producer.publish(&record(1, &[1; 64])).unwrap();
    let guard = consumer.try_acquire().unwrap().unwrap();
    // A writer outside the contract clears committed_sequence, sequentially.
    // This proves only that a visible commit-word change is caught; a
    // concurrent plain-byte rewrite is excluded structurally, not detected
    // (see claims.rs).
    let committed = slot_offset(&arena, 16, 0) + SLOT_OFF_COMMITTED;
    arena
        .mapping
        .atomic_u64(committed)
        .store(EMPTY_SEQUENCE, Ordering::Release);
    assert!(!guard.is_stable());
    let mut out = [0u8; 64];
    let copied = guard.copy_payload(&mut out);
    assert!(matches!(copied, Err(V2Error::CorruptSequence { .. })));
    assert!(guard.release().is_err());
    let stats = arena.lane_stats(16).unwrap();
    assert_eq!(stats.state, LANE_STATE_QUARANTINED);
    assert_eq!(stats.read_sequence, 0);
    assert_eq!(stats.corrupt_records, 1);
    assert_eq!(stats.quarantine_reason, QUARANTINE_CORRUPT_SEQUENCE);
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn copy_payload_copies_once_and_refuses_a_small_buffer() {
    let (path, arena, mut producer, mut consumer) = setup("copy");
    producer.publish(&record(1, &[5; 100])).unwrap();
    let guard = consumer.try_acquire().unwrap().unwrap();
    let mut small = [0u8; 99];
    let refused = guard.copy_payload(&mut small);
    assert!(matches!(refused, Err(V2Error::PayloadTooLarge { .. })));
    let mut out = [0u8; 128];
    assert_eq!(guard.copy_payload(&mut out).unwrap(), 100);
    assert_eq!(out[..100], [5u8; 100]);
    assert_eq!(out[100..], [0u8; 28]);
    // A refused copy is not a wire defect.
    assert_eq!(guard.release().unwrap(), 0);
    assert_eq!(arena.lane_stats(16).unwrap().state, LANE_STATE_BOUND);
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn a_lane_set_reads_several_lanes_through_one_mapping() {
    let (path, mut arena, mut producer16, consumer) = setup("lane-set");
    drop(consumer);
    let mut producer17 = ProducerV2::open(&path, GENERATION, lane(17)).unwrap();
    producer16.publish(&record(1, b"sixteen")).unwrap();
    producer17.publish(&record(2, b"seventeen")).unwrap();
    let mut set = LaneSetV2::open(&path, GENERATION, &[lane(16), lane(17)]).unwrap();
    assert_eq!((set.len(), set.is_empty()), (2, false));
    assert_eq!(set.config(1).lane_id, 17);
    assert!(set.pending(0) && set.pending(1));

    let guard = set.try_acquire(1).unwrap().unwrap();
    assert_eq!(guard.record().lane_id, 17);
    assert_eq!(guard.record().payload, b"seventeen");
    guard.release().unwrap();
    assert!(!set.pending(1));
    assert!(set.pending(0));
    assert_eq!(set.stats(1).read_sequence, 1);
    assert_eq!(set.lifecycle_state(0), LANE_STATE_BOUND);

    // A quarantined lane fails through the set like through a consumer.
    arena.quarantine_lane(16).unwrap();
    let refused = set.try_acquire(0).map(|guard| guard.is_some());
    assert!(matches!(refused, Err(V2Error::LaneQuarantined { .. })));
    assert_eq!(set.stats(0).read_sequence, 0);
    drop((producer16, producer17, set, arena));
    fs::remove_file(path).unwrap();
}

#[test]
fn a_sealed_lane_delivers_its_records_and_then_ends() {
    let (path, mut arena, mut producer, mut consumer) = setup("sealed");
    producer.publish(&record(1, b"last")).unwrap();
    arena.begin_shutdown().unwrap();
    let guard = consumer.try_acquire().unwrap().unwrap();
    assert_eq!(guard.record().payload, b"last");
    guard.release().unwrap();
    let ended = consumer.try_acquire().map(|guard| guard.is_some());
    assert!(matches!(ended, Err(V2Error::Shutdown)));
    drop((producer, consumer, arena));
    fs::remove_file(path).unwrap();
}
