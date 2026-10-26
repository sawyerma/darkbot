//! Phase E contract §8/§11: the same bytes reach the same Four.meme decoder
//! on every path. The nine signed transactions of the G0.1 Four freeze
//! (Classic and OpenFour launches and BUYs, legacy and EIP-1559 envelopes)
//! go through the FastLane wire (EARLY_TX_SEEN records on a bound peer lane
//! of the mmap/SPSC arena, read borrowed by the HOT0 poller) and through
//! the runtime's journal (first-sighting records, read back by the journal
//! reader); the decoder's result on the received bytes equals its result on
//! the original bytes.
#![cfg(feature = "runtime")]

use alloy_primitives::{Address, hex, keccak256};
use darkbot_bsc_execution::ingress::four_launch_token;
use darkbot_bsc_execution::runtime_journal::{RuntimeJournal, first_sighting_payload};
use darkbot_bsc_execution::sources::SourceId;
use darkbot_bsc_geth_ipc::RecordKind;
use darkbot_bsc_geth_ipc::hot0::{Hot0Config, Hot0Decision, Hot0Poller};
use darkbot_bsc_geth_ipc::ingress::{SOURCE_ETH_BROADCAST, decode_tx_target, ingress_aux};
use darkbot_bsc_geth_ipc::protocol_v2::{
    ArenaV2, BorrowedRecordV2, CLOCK_ID_MONOTONIC, DirectionV2, LaneClassV2, LaneConfigV2,
    ProducerV2, RecordV2, arena_path_v2,
};
use darkbot_bsc_journal::{JournalReader, RecordKind as JournalKind};
use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_g01_129842711;
use darkbot_bsc_trading::venues::fourmeme::recognize::recognize;
use std::fs;

const GENERATION: u64 = 1_592;
const PEER: u16 = 16;
const CHAIN: u16 = 2;
const MAX_CALLDATA: usize = 16_384;
/// `<family> <kind> <block> <block-hash> <tx-hash> <status> <from> <to>
/// <value> <input> <raw-signed-tx>`.
const CALLS: &str = include_str!("../../trading/fixtures/fourmeme/freeze_20261026_calls.txt");

struct Fixture {
    label: String,
    hash: [u8; 32],
    to: Address,
    input: Vec<u8>,
    raw: Vec<u8>,
}

fn fixtures() -> Vec<Fixture> {
    CALLS
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let r: Vec<&str> = l.split_whitespace().collect();
            Fixture {
                label: format!("{} {} {}", r[0], r[1], r[4]),
                hash: hex::decode_to_array(r[4]).unwrap(),
                to: Address::from_slice(&hex::decode(r[7]).unwrap()),
                input: hex::decode(r[9]).unwrap(),
                raw: hex::decode(r[10]).unwrap(),
            }
        })
        .collect()
}

/// The decoder's result for the signed bytes `raw`, printed: the pending
/// launch token and the recognized call.
fn decision(raw: &[u8]) -> String {
    let deployment = bsc_mainnet_g01_129842711();
    let target = decode_tx_target(raw).unwrap();
    format!(
        "{:?} {:?}",
        four_launch_token(&deployment, raw, MAX_CALLDATA),
        recognize(
            &deployment,
            target.to.map(Address::from),
            target.data,
            MAX_CALLDATA
        )
    )
}

fn lane(lane_id: u16, class: LaneClassV2) -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id,
        direction: DirectionV2::GethToRust,
        class,
        flags: 0,
        capacity: 16,
        slot_stride: 4096,
    }
}

#[test]
fn fastlane_and_journal_bytes_decode_like_the_original() {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), 9);
    let deployment = bsc_mainnet_g01_129842711();
    let mut originals = Vec::new();
    for f in &fixtures {
        // The signed bytes are the transaction (hash) and carry the input.
        assert_eq!(keccak256(&f.raw).0, f.hash, "{}", f.label);
        let target = decode_tx_target(&f.raw).unwrap();
        assert_eq!(target.to.map(Address::from), Some(f.to), "{}", f.label);
        assert_eq!(target.data, f.input.as_slice(), "{}", f.label);
        let original = decision(&f.raw);
        assert!(
            !original.contains("NotFour") && !original.contains("Malformed"),
            "{original}"
        );
        // Launches name a pending token; BUYs do not.
        assert_eq!(
            four_launch_token(&deployment, &f.raw, MAX_CALLDATA).is_some(),
            f.label.contains("LAUNCH"),
            "{}",
            f.label
        );
        originals.push(original);
    }

    // FastLane: producer → mmap arena → HOT0 poller (borrowed records).
    let dir = std::env::temp_dir().join(format!("bsc-four-fastlane-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = arena_path_v2(&dir, GENERATION);
    let peer = lane(PEER, LaneClassV2::PeerIngress);
    let chain = lane(CHAIN, LaneClassV2::ChainEvidence);
    let mut arena = ArenaV2::create(&path, GENERATION, &[chain, peer]).unwrap();
    arena.bind_peer_lane(PEER, [7; 32]).unwrap();
    let mut producer = ProducerV2::open(&path, GENERATION, peer).unwrap();
    let config = Hot0Config {
        intent_slots: 4,
        dedup_sets: 16,
    };
    let mut poller = Hot0Poller::open(&path, GENERATION, &[peer], Some(chain), config).unwrap();
    for (i, f) in fixtures.iter().enumerate() {
        let published = 1_000 + 10 * i as u64;
        producer
            .publish(&RecordV2 {
                kind: RecordKind::EarlyTxSeen,
                flags: 0,
                timestamp_ns: published,
                clock_id: CLOCK_ID_MONOTONIC,
                source: SOURCE_ETH_BROADCAST,
                peer: [0x22; 32],
                hash: f.hash,
                aux: ingress_aux(published - 1),
                payload: &f.raw,
            })
            .unwrap();
        let mut received = None;
        poller.poll_once(
            |_: &BorrowedRecordV2<'_>| {},
            |record| {
                assert_eq!(record.hash, f.hash.as_slice());
                received = Some(decision(record.payload));
                Hot0Decision::Reject
            },
        );
        assert_eq!(
            received.as_deref(),
            Some(originals[i].as_str()),
            "FastLane {}",
            f.label
        );
    }
    drop(arena);

    // Capture → replay: first-sighting records of the runtime journal, read
    // back by the journal reader (as the census replays them).
    let max = fixtures.iter().map(|f| f.raw.len()).max().unwrap() + 64;
    let journal_dir = dir.join("journal");
    let mut journal = RuntimeJournal::open(&journal_dir, 1, 32, max).unwrap();
    for (i, f) in fixtures.iter().enumerate() {
        assert!(journal.record(
            JournalKind::RawSignedTx,
            2_000 + i as u64,
            &first_sighting_payload(SourceId::PublicNode, &f.hash, &f.raw),
        ));
    }
    let path = journal.path().to_path_buf();
    journal.finish().unwrap();
    let file = fs::File::open(&path).unwrap();
    let mut reader = JournalReader::new(std::io::BufReader::new(file), max).unwrap();
    let mut replayed = Vec::new();
    while let Some(record) = reader.next_record().unwrap() {
        if record.kind == JournalKind::RawSignedTx {
            // Source byte, hash, then the signed bytes.
            let i = replayed.len();
            assert_eq!(&record.payload[1..33], &fixtures[i].hash);
            replayed.push(decision(&record.payload[33..]));
        }
    }
    assert_eq!(replayed, originals, "journal replay");
    fs::remove_dir_all(&dir).unwrap();
}
