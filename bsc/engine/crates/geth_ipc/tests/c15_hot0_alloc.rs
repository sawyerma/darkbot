//! C1.5 allocation measurement.
//!
//! A counting global allocator measures the heap allocations of the HOT0
//! receive path on this thread. The qualified common path (chain evidence,
//! reject, duplicate, admit with the one launch copy, seal, release) must
//! allocate nothing. The owned audit path (`ConsumerV2::try_consume`) is
//! measured as the control: it must show the allocations the borrowed path
//! removed, which also shows that the counter observes them.
//!
//! This file holds one test so that no other test thread runs in the binary.

use darkbot_bsc_geth_ipc::RecordKind;
use darkbot_bsc_geth_ipc::hot0::{
    Hot0Config, Hot0Decision, Hot0Outcome, Hot0Poller, IntentBuy, decode_canonical_head,
};
use darkbot_bsc_geth_ipc::ingress::{
    C14_FLAP_SCOPE, SOURCE_ETH_BROADCAST, decode_tx_target, ingress_aux,
};
use darkbot_bsc_geth_ipc::v2::{
    ArenaV2, BorrowedRecordV2, CLOCK_ID_MONOTONIC, ConsumerV2, DirectionV2, LaneClassV2,
    LaneConfigV2, ProducerV2, RecordV2, arena_path_v2,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fs;

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

fn count() {
    let _ = ALLOCATIONS.try_with(|cell| cell.set(cell.get() + 1));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

const GENERATION: u64 = 1515;
const CHAIN_LANE: u16 = 2;
const RECORDS: u64 = 48;
const PORTAL: [u8; 20] = [
    0xe2, 0xce, 0x6a, 0xb8, 0x08, 0x74, 0xfa, 0x9f, 0xa2, 0xaa, 0xe6, 0x5d, 0x27, 0x7d, 0xd6, 0xb8,
    0xe6, 0x5c, 0x9d, 0xe0,
];

/// The production peer-lane shape of C1.4: 64 slots of 4096 bytes.
fn peer_lane(lane_id: u16) -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id,
        direction: DirectionV2::GethToRust,
        class: LaneClassV2::PeerIngress,
        flags: 0,
        capacity: 64,
        slot_stride: 4096,
    }
}

fn chain_lane() -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id: CHAIN_LANE,
        direction: DirectionV2::GethToRust,
        class: LaneClassV2::ChainEvidence,
        flags: 0,
        capacity: 64,
        slot_stride: 512,
    }
}

/// A legacy transaction with 1,200 bytes of calldata that starts with the
/// launch selector: the size class of a real `newTokenV6` launch.
fn launch_envelope(to: [u8; 20]) -> Vec<u8> {
    let mut data = vec![0x8c, 0xb5, 0x77, 0x2c];
    data.resize(1_200, 0x11);
    let mut body = vec![0x01, 0x01, 0x01, 0x94];
    body.extend_from_slice(&to);
    body.push(0x80);
    body.extend_from_slice(&[0xb9, 0x04, 0xb0]);
    body.extend_from_slice(&data);
    body.extend_from_slice(&[0x01, 0x01, 0x01]);
    let len = body.len() as u16;
    let mut raw = vec![0xf9];
    raw.extend_from_slice(&len.to_be_bytes());
    raw.extend_from_slice(&body);
    raw
}

fn early(hash: [u8; 32], ingress_ns: u64, payload: &[u8]) -> RecordV2<'_> {
    RecordV2 {
        kind: RecordKind::EarlyTxSeen,
        flags: 0,
        timestamp_ns: ingress_ns,
        clock_id: CLOCK_ID_MONOTONIC,
        source: SOURCE_ETH_BROADCAST,
        peer: [0x22; 32],
        hash,
        aux: ingress_aux(ingress_ns - 1),
        payload,
    }
}

fn hash_of(sequence: u64, salt: u8) -> [u8; 32] {
    let mut hash = [salt; 32];
    hash[..8].copy_from_slice(&sequence.to_le_bytes());
    hash
}

fn scope_decision(record: &BorrowedRecordV2<'_>) -> Hot0Decision {
    match decode_tx_target(record.payload) {
        Ok(target) if C14_FLAP_SCOPE.matches(&target) => Hot0Decision::Admit,
        _ => Hot0Decision::Reject,
    }
}

#[test]
fn the_hot0_receive_path_allocates_nothing() {
    let name = format!("darkbot-fastlane-c15-alloc-{}", std::process::id());
    let dir = std::env::temp_dir().join(name);
    fs::create_dir_all(&dir).unwrap();
    let path = arena_path_v2(&dir, GENERATION);
    let peers = [peer_lane(16), peer_lane(17)];
    let lanes = [chain_lane(), peers[0], peers[1]];
    let mut arena = ArenaV2::create(&path, GENERATION, &lanes).unwrap();
    arena.bind_peer_lane(16, [0x16; 32]).unwrap();
    arena.bind_peer_lane(17, [0x17; 32]).unwrap();
    let mut peer16 = ProducerV2::open(&path, GENERATION, peers[0]).unwrap();
    let mut peer17 = ProducerV2::open(&path, GENERATION, peers[1]).unwrap();
    let mut chain = ProducerV2::open(&path, GENERATION, chain_lane()).unwrap();

    let launch = launch_envelope(PORTAL);
    let other = launch_envelope([0xaa; 20]);
    let head_payload = [0u8; 12];
    // Lane 16: launches (admitted) alternating with non-matches (rejected).
    // Lane 17: the same launches again (duplicates) and its own non-matches.
    for sequence in 0..RECORDS {
        let launch_hash = hash_of(sequence, 0xa0);
        let payload = if sequence.is_multiple_of(2) {
            &launch
        } else {
            &other
        };
        let salt = if sequence.is_multiple_of(2) {
            0xa0
        } else {
            0xb0
        };
        peer16
            .publish(&early(hash_of(sequence, salt), 500, payload))
            .unwrap();
        let payload17 = if sequence.is_multiple_of(2) {
            &launch
        } else {
            &other
        };
        let hash17 = if sequence.is_multiple_of(2) {
            launch_hash
        } else {
            hash_of(sequence, 0xc0)
        };
        peer17.publish(&early(hash17, 400, payload17)).unwrap();
        let head = RecordV2 {
            kind: RecordKind::CanonicalHead,
            flags: 0,
            timestamp_ns: 300,
            clock_id: CLOCK_ID_MONOTONIC,
            source: [0; 16],
            peer: [0; 32],
            hash: hash_of(sequence, 0xd0),
            aux: [0; 32],
            payload: &head_payload,
        };
        chain.publish(&head).unwrap();
    }

    let config = Hot0Config {
        intent_slots: 4,
        dedup_sets: 64,
    };
    let chain_config = Some(chain_lane());
    let mut poller = Hot0Poller::open(&path, GENERATION, &peers, chain_config, config).unwrap();
    let signed_buy = [0x5a_u8; 300];
    let mut heads = 0u64;
    let mut admitted = 0u64;
    let mut rejected = 0u64;
    let mut duplicates = 0u64;
    let mut other_outcomes = 0u64;

    // The measured region: every record of both lanes plus idle polls.
    let before = allocations();
    for _ in 0..(RECORDS * 2 + 8) {
        let outcome = poller.poll_once(
            |record| {
                if decode_canonical_head(record).is_some() {
                    heads += 1;
                }
            },
            scope_decision,
        );
        match outcome {
            Hot0Outcome::Admitted(handle) => {
                admitted += 1;
                let buy = IntentBuy {
                    intent_id: admitted,
                    generation: GENERATION,
                    policy_generation: 1,
                    nonce: admitted,
                    adapter_permit_mask: 1,
                    buy_hash: [0x77; 32],
                    signed_buy: &signed_buy,
                };
                let pool = poller.intents_mut();
                let sealed = pool.get_mut(handle).map(|slot| slot.seal(&buy).is_ok());
                if sealed != Some(true) || !pool.release(handle) {
                    other_outcomes += 1;
                }
            }
            Hot0Outcome::Rejected => rejected += 1,
            Hot0Outcome::Duplicate => duplicates += 1,
            Hot0Outcome::Idle => {}
            _ => other_outcomes += 1,
        }
    }
    let hot_allocations = allocations() - before;

    assert_eq!(hot_allocations, 0, "HOT0 receive path allocated");
    assert_eq!(heads, RECORDS);
    assert_eq!((admitted, duplicates), (RECORDS / 2, RECORDS / 2));
    assert_eq!(rejected, RECORDS);
    assert_eq!(other_outcomes, 0);
    let stats = poller.stats();
    assert_eq!(stats.peer_records, RECORDS * 2);
    assert_eq!(stats.duplicates_with_earlier_ingress, RECORDS / 2);

    // Control: the owned audit path allocates once per record.
    for sequence in 0..8u64 {
        peer16
            .publish(&early(hash_of(sequence, 0xe0), 500, &other))
            .unwrap();
    }
    let mut audit = ConsumerV2::open(&path, GENERATION, peers[0]).unwrap();
    let before = allocations();
    let mut consumed = 0u64;
    while let Ok(Some(record)) = audit.try_consume() {
        consumed += u64::from(!record.payload.is_empty());
    }
    let owned_allocations = allocations() - before;
    assert_eq!(consumed, 8);
    assert!(owned_allocations >= 8, "{owned_allocations} allocations");

    drop((poller, audit, peer16, peer17, chain, arena));
    fs::remove_dir_all(dir).unwrap();
}
