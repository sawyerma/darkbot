//! Phase D contract §8/§11: the same bytes reach the same Flap decoder on
//! every path. A real launch transaction goes through the FastLane wire
//! (an EARLY_TX_SEEN record on a bound peer lane of the mmap/SPSC arena,
//! read borrowed by the HOT0 poller) and through the runtime's journal
//! (first-sighting record, read back by the journal reader); the decoder's
//! result on the received bytes equals its result on the original bytes.
#![cfg(feature = "runtime")]

use alloy_primitives::{Address, hex};
use darkbot_bsc_execution::ingress::is_flap_launch;
use darkbot_bsc_execution::runtime_journal::{RuntimeJournal, first_sighting_payload};
use darkbot_bsc_execution::sources::SourceId;
use darkbot_bsc_geth_ipc::RecordKind;
use darkbot_bsc_geth_ipc::hot0::{Hot0Config, Hot0Decision, Hot0Poller};
use darkbot_bsc_geth_ipc::ingress::{SOURCE_ETH_BROADCAST, ingress_aux};
use darkbot_bsc_geth_ipc::protocol_v2::{
    ArenaV2, BorrowedRecordV2, CLOCK_ID_MONOTONIC, DirectionV2, LaneClassV2, LaneConfigV2,
    ProducerV2, RecordV2, arena_path_v2,
};
use darkbot_bsc_journal::{JournalReader, RecordKind as JournalKind};
use darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_0;
use darkbot_bsc_trading::venues::flap::recognize::recognize;
use darkbot_bsc_tx_encoding::rlp::{encode_bytes, encode_list, encode_u64};
use std::fs;

const GENERATION: u64 = 1_590;
const PEER: u16 = 16;
const CHAIN: u16 = 2;
/// A pending `newTokenV6` launch of the G0.4 coverage capture.
const LAUNCH: &str =
    include_str!("../../trading/fixtures/flap/launch_v6_native_split_tax_pending.hex");

fn lane(lane_id: u16, class: LaneClassV2) -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id,
        direction: DirectionV2::GethToRust,
        class,
        flags: 0,
        capacity: 8,
        slot_stride: 4096,
    }
}

fn launch_tx() -> (Vec<u8>, Vec<u8>) {
    let portal = bsc_mainnet_v5_27_0().portal;
    let calldata = hex::decode(LAUNCH.trim()).unwrap();
    let raw = encode_list(&[
        encode_u64(7),
        encode_u64(1_000_000_000),
        encode_u64(3_000_000),
        encode_bytes(portal.as_slice()),
        encode_u64(0),
        encode_bytes(&calldata),
        encode_u64(147),
        encode_bytes(&[1u8; 32]),
        encode_bytes(&[2u8; 32]),
    ]);
    (raw, calldata)
}

/// The decoder's result for the calldata inside `raw`, printed.
fn decision(raw: &[u8], calldata_of: impl Fn(&[u8]) -> Vec<u8>) -> String {
    let portal = bsc_mainnet_v5_27_0().portal;
    let calldata = calldata_of(raw);
    format!(
        "{} {:?}",
        is_flap_launch(raw, 16_384),
        recognize(portal, Some(portal), &calldata, raw.len())
    )
}

fn calldata_of(raw: &[u8]) -> Vec<u8> {
    let target = darkbot_bsc_geth_ipc::ingress::decode_tx_target(raw).unwrap();
    assert_eq!(
        target.to.map(Address::from),
        Some(bsc_mainnet_v5_27_0().portal)
    );
    target.data.to_vec()
}

#[test]
fn fastlane_and_journal_bytes_decode_like_the_original() {
    let (raw, calldata) = launch_tx();
    let original = decision(&raw, |_| calldata.clone());
    assert!(original.starts_with("true LaunchV6("), "{original}");

    // FastLane: producer → mmap arena → HOT0 poller (borrowed record).
    let dir = std::env::temp_dir().join(format!("bsc-flap-fastlane-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = arena_path_v2(&dir, GENERATION);
    let peer = lane(PEER, LaneClassV2::PeerIngress);
    let chain = lane(CHAIN, LaneClassV2::ChainEvidence);
    let mut arena = ArenaV2::create(&path, GENERATION, &[chain, peer]).unwrap();
    arena.bind_peer_lane(PEER, [7; 32]).unwrap();
    let mut producer = ProducerV2::open(&path, GENERATION, peer).unwrap();
    producer
        .publish(&RecordV2 {
            kind: RecordKind::EarlyTxSeen,
            flags: 0,
            timestamp_ns: 1_000,
            clock_id: CLOCK_ID_MONOTONIC,
            source: SOURCE_ETH_BROADCAST,
            peer: [0x22; 32],
            hash: [0x33; 32],
            aux: ingress_aux(999),
            payload: &raw,
        })
        .unwrap();
    let config = Hot0Config {
        intent_slots: 4,
        dedup_sets: 16,
    };
    let mut poller = Hot0Poller::open(&path, GENERATION, &[peer], Some(chain), config).unwrap();
    let mut received = None;
    poller.poll_once(
        |_: &BorrowedRecordV2<'_>| {},
        |record| {
            received = Some(decision(record.payload, calldata_of));
            Hot0Decision::Reject
        },
    );
    assert_eq!(received.as_deref(), Some(original.as_str()), "FastLane");
    drop(arena);

    // Capture → replay: the runtime journal's first-sighting record, read
    // back by the journal reader (as the census replays it).
    let journal_dir = dir.join("journal");
    let mut journal = RuntimeJournal::open(&journal_dir, 1, 16, raw.len() + 64).unwrap();
    let hash = [0x44; 32];
    assert!(journal.record(
        JournalKind::RawSignedTx,
        2_000,
        &first_sighting_payload(SourceId::PublicNode, &hash, &raw),
    ));
    let path = journal.path().to_path_buf();
    journal.finish().unwrap();
    let file = fs::File::open(&path).unwrap();
    let mut reader = JournalReader::new(std::io::BufReader::new(file), raw.len() + 64).unwrap();
    let mut replayed = None;
    while let Some(record) = reader.next_record().unwrap() {
        if record.kind == JournalKind::RawSignedTx {
            // Source byte, hash, then the signed bytes.
            assert_eq!(&record.payload[1..33], &hash);
            replayed = Some(decision(&record.payload[33..], calldata_of));
        }
    }
    assert_eq!(
        replayed.as_deref(),
        Some(original.as_str()),
        "journal replay"
    );
    fs::remove_dir_all(&dir).unwrap();
}
