//! C1.7 local-geth outbound: the Rust side.
//!
//! ```text
//! HOT0 --egress lane (Rust -> geth, SPSC)--> geth outbound coordinator
//!      <--coordinator evidence lane---------  DISPATCHED / per-slot refusals
//!      <--worker evidence lane 0..N---------  one record per peer send
//! ```
//!
//! [`EgressProducerV2`] is the egress lane's only producer (E6-2). It
//! publishes nothing but the signed bytes of an [`EgressWrite`], which only
//! the one economic owner, `IntentLane::start_write`, creates after it
//! consumed this adapter's permit: the same signed bytes and nonce every
//! other adapter uses, one egress per permit. The generic [`ProducerV2`]
//! refuses the egress lane, and an HOT0 `IntentSlot` is storage only, never
//! an egress authority. Committing to the egress lane is `T_queue`. It is
//! not a send.
//!
//! geth answers with evidence on lanes of class `OutboundHandoffEvidence`,
//! one lane per producer: the coordinator and each permanent peer worker.
//! A worker's record carries that worker's own clock reading taken when its
//! peer send returned. A returned send is a local transport handoff: not
//! remote receipt, not inclusion, not execution and not a purchase.
//!
//! This module mirrors the lane plan of the geth overlay
//! (`darkbot/fastlane/outbound.go`, `chain_v2.go`) and decodes its evidence.
//! It decides nothing.

use crate::HASH_LEN;
use crate::PEER_LEN;
use crate::RecordKind;
use crate::protocol_v2::{
    BorrowedRecordV2, CLOCK_ID_MONOTONIC, DirectionV2, LaneClassV2, LaneConfigV2, LaneStatsV2,
    ProducerV2, RecordV2, V2Error,
};
use darkbot_bsc_trading::intent::EgressWrite;
use std::path::Path;

pub const C15_CHAIN_LANE_ID: u16 = 2;
pub const C15_CHAIN_LANE_CAPACITY: usize = 1024;
pub const C15_CHAIN_LANE_SLOT_STRIDE: usize = 512;
pub const C17_EGRESS_LANE_ID: u16 = 3;
pub const C17_EGRESS_LANE_CAPACITY: usize = 256;
pub const C17_EGRESS_LANE_SLOT_STRIDE: usize = 4096;
/// The coordinator's evidence lane; worker `i` uses `C17_COORDINATOR_LANE_ID
/// + 1 + i`.
pub const C17_COORDINATOR_LANE_ID: u16 = 512;
pub const C17_EVIDENCE_LANE_CAPACITY: usize = 1024;
pub const C17_EVIDENCE_LANE_SLOT_STRIDE: usize = 512;
pub const C17_MAX_OUTBOUND_WORKERS: usize = 16;
/// `aux[1]` of a record that belongs to no worker slot.
pub const C17_NO_WORKER: u8 = 0xff;

/// `source` of an egress record.
pub const SOURCE_HOT0_EGRESS: [u8; 16] = *b"hot0-egress\0\0\0\0\0";
/// `source` of a worker's evidence record.
pub const SOURCE_P2P_WORKER: [u8; 16] = *b"p2p-worker\0\0\0\0\0\0";
/// `source` of a coordinator evidence record.
pub const SOURCE_P2P_COORDINATOR: [u8; 16] = *b"p2p-coord\0\0\0\0\0\0\0";

/// The dedicated Chain lane (CANONICAL_HEAD, REORG, GAP).
pub fn chain_lane() -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id: C15_CHAIN_LANE_ID,
        direction: DirectionV2::GethToRust,
        class: LaneClassV2::ChainEvidence,
        flags: 0,
        capacity: C15_CHAIN_LANE_CAPACITY,
        slot_stride: C15_CHAIN_LANE_SLOT_STRIDE,
    }
}

/// The Rust -> geth egress lane. HOT0 is its only producer.
pub fn egress_lane() -> LaneConfigV2 {
    LaneConfigV2 {
        lane_id: C17_EGRESS_LANE_ID,
        direction: DirectionV2::RustToGeth,
        class: LaneClassV2::SignedTransactionEgress,
        flags: 0,
        capacity: C17_EGRESS_LANE_CAPACITY,
        slot_stride: C17_EGRESS_LANE_SLOT_STRIDE,
    }
}

/// Evidence lane `index`: 0 is the coordinator's, `i + 1` worker `i`'s.
pub fn evidence_lane(index: usize) -> LaneConfigV2 {
    let offset = u16::try_from(index).unwrap_or(u16::MAX);
    LaneConfigV2 {
        lane_id: C17_COORDINATOR_LANE_ID.saturating_add(offset),
        direction: DirectionV2::GethToRust,
        class: LaneClassV2::OutboundHandoffEvidence,
        flags: 0,
        capacity: C17_EVIDENCE_LANE_CAPACITY,
        slot_stride: C17_EVIDENCE_LANE_SLOT_STRIDE,
    }
}

/// The outbound lanes of `workers` workers, in geth's order: egress, then
/// the coordinator's evidence lane, then one evidence lane per worker.
pub fn outbound_lanes(workers: usize) -> Vec<LaneConfigV2> {
    let mut lanes = Vec::with_capacity(workers + 2);
    lanes.push(egress_lane());
    for index in 0..=workers {
        lanes.push(evidence_lane(index));
    }
    lanes
}

/// Why a BUY was not published on the egress lane. Nothing was written to
/// the lane in every case, so geth saw nothing of it.
#[derive(Debug)]
pub enum EgressError {
    /// The clock could not be read; an egress record needs its commit time.
    Clock,
    Wire(V2Error),
}

/// The only producer of the signed-transaction egress lane (E6-2).
pub struct EgressProducerV2 {
    producer: ProducerV2,
}

impl EgressProducerV2 {
    /// Opens the egress lane of the arena at `path`.
    pub fn open(path: &Path, generation: u64) -> Result<Self, V2Error> {
        Ok(Self {
            producer: ProducerV2::open_any_class(path, generation, egress_lane())?,
        })
    }

    /// Publishes the BUY that `write` releases, once: `write` is consumed.
    ///
    /// `commit_ns` is HOT0's clock reading taken immediately before the call
    /// (`hot0::monotonic_ns`); geth's coordinator echoes it in its evidence,
    /// so the egress commit and every handoff share one clock domain.
    /// Returns the egress lane sequence, which the evidence also carries.
    ///
    /// Disclosure boundary: `Ok` means the record is committed and geth may
    /// broadcast it; that is a local queue, not a send, an inclusion or a
    /// purchase. An error means nothing reached the lane. The intent lane has
    /// no state for "permit consumed but nothing disclosed", so the caller
    /// reports every error as `IntentLane::write_failed` (UNKNOWN, lane
    /// halted) and never retries; `Ok` as `write_completed`.
    pub fn publish_buy(
        &mut self,
        write: EgressWrite<'_>,
        commit_ns: u64,
    ) -> Result<u64, EgressError> {
        if commit_ns == 0 {
            return Err(EgressError::Clock);
        }
        let record = RecordV2 {
            kind: RecordKind::OutboundSignedTx,
            flags: 0,
            timestamp_ns: commit_ns,
            clock_id: CLOCK_ID_MONOTONIC,
            source: SOURCE_HOT0_EGRESS,
            peer: [0; PEER_LEN],
            hash: write.tx_hash().0,
            aux: [0; 32],
            payload: write.bytes(),
        };
        self.producer.publish(&record).map_err(EgressError::Wire)
    }

    pub fn stats(&self) -> LaneStatsV2 {
        self.producer.stats()
    }
}

/// The outcome of one worker slot, or of the dispatch (`aux[0]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutboundStatus {
    /// The peer send returned without error.
    HandedOff = 1,
    /// The peer send returned an error. Bytes may have left: unknown, never
    /// "not sent".
    SendErrorUnknown = 2,
    /// The worker slot had no peer. Nothing was sent.
    NoTarget = 3,
    /// The worker was busy and its queue full. Nothing was sent through it.
    QueueFull = 4,
    /// The peer already knew the transaction. Nothing was sent to it.
    AlreadyKnown = 5,
    /// The egress record was not a signed transaction with the stated hash.
    InvalidEgress = 6,
    /// The coordinator's summary of one BUY.
    Dispatched = 7,
}

impl OutboundStatus {
    pub fn from_raw(raw: u8) -> Option<Self> {
        Some(match raw {
            1 => Self::HandedOff,
            2 => Self::SendErrorUnknown,
            3 => Self::NoTarget,
            4 => Self::QueueFull,
            5 => Self::AlreadyKnown,
            6 => Self::InvalidEgress,
            7 => Self::Dispatched,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::HandedOff => "HANDED_OFF",
            Self::SendErrorUnknown => "SEND_ERROR_UNKNOWN",
            Self::NoTarget => "NO_TARGET",
            Self::QueueFull => "QUEUE_FULL",
            Self::AlreadyKnown => "ALREADY_KNOWN",
            Self::InvalidEgress => "INVALID_EGRESS",
            Self::Dispatched => "DISPATCHED",
        }
    }
}

/// Who published an evidence record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceOrigin {
    Coordinator,
    Worker,
}

/// One decoded outbound evidence record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandoffEvidence {
    pub origin: EvidenceOrigin,
    pub status: OutboundStatus,
    /// The worker slot, `None` on a DISPATCHED or INVALID_EGRESS record.
    pub worker: Option<u8>,
    /// Workers that had a peer when the BUY was dispatched.
    pub selected: u8,
    /// Workers the BUY was queued to (coordinator records).
    pub queued: u8,
    pub lane_id: u16,
    pub lane_sequence: u64,
    pub egress_sequence: u64,
    /// The event time on the publisher's clock: a worker's send return, the
    /// coordinator's observation of the egress record.
    pub event_ns: u64,
    /// When the coordinator observed the egress record.
    pub observed_ns: u64,
    /// Worker: the time just before the peer send. Coordinator: HOT0's
    /// egress commit time.
    pub third_ns: u64,
    pub hash: [u8; HASH_LEN],
    pub peer: [u8; PEER_LEN],
}

/// Why an evidence record was not decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffError {
    NotHandoffEvidence(RecordKind),
    ClockDomain(u32),
    UnknownSource,
    UnknownStatus(u8),
    /// The record's fields contradict its origin or its own timestamps.
    Inconsistent(&'static str),
}

fn aux_u64(aux: &[u8], offset: usize) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&aux[offset..offset + 8]);
    u64::from_le_bytes(raw)
}

/// Decodes and checks one outbound evidence record.
pub fn decode_handoff(record: &BorrowedRecordV2<'_>) -> Result<HandoffEvidence, HandoffError> {
    if record.kind != RecordKind::OutboundPeerHandoff {
        return Err(HandoffError::NotHandoffEvidence(record.kind));
    }
    if record.clock_id != CLOCK_ID_MONOTONIC {
        return Err(HandoffError::ClockDomain(record.clock_id));
    }
    let origin = if *record.source == SOURCE_P2P_WORKER {
        EvidenceOrigin::Worker
    } else if *record.source == SOURCE_P2P_COORDINATOR {
        EvidenceOrigin::Coordinator
    } else {
        return Err(HandoffError::UnknownSource);
    };
    let aux = record.aux;
    let Some(status) = OutboundStatus::from_raw(aux[0]) else {
        return Err(HandoffError::UnknownStatus(aux[0]));
    };
    let worker = (aux[1] != C17_NO_WORKER).then_some(aux[1]);
    let observed_ns = aux_u64(aux, 16);
    let third_ns = aux_u64(aux, 24);
    let summary = matches!(
        status,
        OutboundStatus::Dispatched | OutboundStatus::InvalidEgress
    );
    if summary != worker.is_none() {
        return Err(HandoffError::Inconsistent("worker index"));
    }
    match origin {
        EvidenceOrigin::Worker => {
            let sent = matches!(
                status,
                OutboundStatus::HandedOff
                    | OutboundStatus::SendErrorUnknown
                    | OutboundStatus::NoTarget
                    | OutboundStatus::AlreadyKnown
            );
            if !sent {
                return Err(HandoffError::Inconsistent("status of a worker record"));
            }
            if observed_ns > third_ns || third_ns > record.timestamp_ns {
                return Err(HandoffError::Inconsistent("worker timestamps"));
            }
        }
        EvidenceOrigin::Coordinator => {
            let coordinated = matches!(
                status,
                OutboundStatus::Dispatched
                    | OutboundStatus::InvalidEgress
                    | OutboundStatus::NoTarget
                    | OutboundStatus::QueueFull
            );
            if !coordinated {
                return Err(HandoffError::Inconsistent("status of a coordinator record"));
            }
            if observed_ns != record.timestamp_ns {
                return Err(HandoffError::Inconsistent("coordinator timestamps"));
            }
        }
    }
    let mut hash = [0u8; HASH_LEN];
    let mut peer = [0u8; PEER_LEN];
    hash.copy_from_slice(record.hash);
    peer.copy_from_slice(record.peer);
    Ok(HandoffEvidence {
        origin,
        status,
        worker,
        selected: aux[2],
        queued: aux[3],
        lane_id: record.lane_id,
        lane_sequence: record.sequence,
        egress_sequence: aux_u64(aux, 8),
        event_ns: record.timestamp_ns,
        observed_ns,
        third_ns,
        hash,
        peer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingress::SOURCE_ETH_BROADCAST;
    use crate::protocol_v2::{ArenaV2, ConsumerV2, arena_path_v2};
    use std::fs;
    use std::path::PathBuf;

    const GENERATION: u64 = 1707;

    fn tmp(name: &str) -> PathBuf {
        let name = format!("darkbot-fastlane-c17-{}-{name}", std::process::id());
        let dir = std::env::temp_dir().join(name);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn evidence_aux(status: u8, worker: u8, times: (u64, u64)) -> [u8; 32] {
        let mut aux = [0u8; 32];
        aux[0] = status;
        aux[1] = worker;
        aux[2] = 2;
        aux[3] = 1;
        aux[8..16].copy_from_slice(&5u64.to_le_bytes());
        aux[16..24].copy_from_slice(&times.0.to_le_bytes());
        aux[24..32].copy_from_slice(&times.1.to_le_bytes());
        aux
    }

    fn evidence(source: [u8; 16], event_ns: u64, aux: [u8; 32]) -> RecordV2<'static> {
        RecordV2 {
            kind: RecordKind::OutboundPeerHandoff,
            flags: 0,
            timestamp_ns: event_ns,
            clock_id: CLOCK_ID_MONOTONIC,
            source,
            peer: [0x44; 32],
            hash: [0x55; 32],
            aux,
            payload: &[],
        }
    }

    #[test]
    fn the_lane_plan_matches_the_geth_overlay() {
        let lanes = outbound_lanes(3);
        assert_eq!(lanes.len(), 5);
        assert_eq!(lanes[0], egress_lane());
        assert_eq!(lanes[0].direction, DirectionV2::RustToGeth);
        assert_eq!(lanes[0].class, LaneClassV2::SignedTransactionEgress);
        let ids: Vec<u16> = lanes[1..].iter().map(|lane| lane.lane_id).collect();
        assert_eq!(ids, [512, 513, 514, 515]);
        for lane in lanes.iter().chain([chain_lane()].iter()) {
            lane.validate().unwrap();
        }
        assert_eq!((chain_lane().lane_id, chain_lane().capacity), (2, 1024));
        assert_eq!(chain_lane().class, LaneClassV2::ChainEvidence);
        // The source tags are geth's, zero padded to 16 bytes.
        assert_eq!(&SOURCE_P2P_WORKER[..10], b"p2p-worker");
        assert_eq!(&SOURCE_P2P_COORDINATOR[..9], b"p2p-coord");
        assert_eq!(SOURCE_P2P_COORDINATOR[9..], [0u8; 7]);
    }

    #[test]
    fn the_egress_lane_is_written_only_against_a_consumed_permit() {
        use alloy_primitives::{Address, B256};
        use darkbot_bsc_trading::intent::{
            AdapterId, AdapterSet, ArmedFence, IntentLane, LaneError,
        };
        let dir = tmp("egress");
        let path = arena_path_v2(&dir, GENERATION);
        let arena = ArenaV2::create(&path, GENERATION, &[egress_lane()]).unwrap();
        // The generic producer cannot write the egress lane.
        assert!(matches!(
            ProducerV2::open(&path, GENERATION, egress_lane()),
            Err(V2Error::EgressRequiresPermit)
        ));
        let (first, second) = (AdapterId::new(0).unwrap(), AdapterId::new(1).unwrap());
        let permits = AdapterSet::new(&[first, second]).unwrap();
        let signed = [0x5au8; 300];
        let hash = B256::repeat_byte(0x77);
        let fence = || ArmedFence::for_tests(56, Address::repeat_byte(0x11), 9);
        let token = Address::repeat_byte(0x22);
        let (mut lane, dispatch) =
            IntentLane::dispatched_for_tests(fence(), token, &signed, hash, permits).unwrap();
        let mut egress = EgressProducerV2::open(&path, GENERATION).unwrap();

        let write = lane.start_write(dispatch.ticket, first, hash).unwrap();
        assert_eq!(egress.publish_buy(write, 700).unwrap(), 0);
        lane.write_completed(dispatch.ticket, first).unwrap();
        // One permit, one egress: the consumed permit releases nothing again.
        assert_eq!(
            lane.start_write(dispatch.ticket, first, hash).err(),
            Some(LaneError::PermitConsumed)
        );
        // A write for another transaction than the intent's is refused and
        // stops the lane; nothing reaches the egress lane.
        assert_eq!(
            lane.start_write(dispatch.ticket, second, B256::repeat_byte(0x78))
                .err(),
            Some(LaneError::WrongHash)
        );
        assert_eq!(egress.stats().write_sequence, 1);

        // Without a clock nothing is written.
        let (mut other, other_dispatch) =
            IntentLane::dispatched_for_tests(fence(), token, &signed, hash, permits).unwrap();
        let write = other
            .start_write(other_dispatch.ticket, first, hash)
            .unwrap();
        assert!(matches!(
            egress.publish_buy(write, 0),
            Err(EgressError::Clock)
        ));
        assert_eq!(egress.stats().write_sequence, 1);

        // geth's coordinator reads exactly the lane's bytes and hash, once.
        let mut coordinator = ConsumerV2::open(&path, GENERATION, egress_lane()).unwrap();
        let received = coordinator.try_consume().unwrap().unwrap();
        assert_eq!(received.kind, RecordKind::OutboundSignedTx);
        assert_eq!(received.timestamp_ns, 700);
        assert_eq!(received.source, SOURCE_HOT0_EGRESS);
        assert_eq!(received.hash, [0x77; 32]);
        assert_eq!(&received.payload[..], &signed[..]);
        assert!(coordinator.try_consume().unwrap().is_none());
        drop((egress, coordinator, arena));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn handoff_evidence_decodes_and_keeps_its_provenance() {
        let dir = tmp("evidence");
        let path = arena_path_v2(&dir, GENERATION);
        let lanes = [evidence_lane(0), evidence_lane(1)];
        let arena = ArenaV2::create(&path, GENERATION, &lanes).unwrap();
        let mut coordinator = ProducerV2::open(&path, GENERATION, lanes[0]).unwrap();
        let mut worker = ProducerV2::open(&path, GENERATION, lanes[1]).unwrap();
        // DISPATCHED: observed at 1000, HOT0 committed at 900.
        let dispatched = evidence_aux(7, C17_NO_WORKER, (1_000, 900));
        let summary = evidence(SOURCE_P2P_COORDINATOR, 1_000, dispatched);
        coordinator.publish(&summary).unwrap();
        // Worker 0: observed 1000, send started 1010, returned 1040.
        let handed = evidence(SOURCE_P2P_WORKER, 1_040, evidence_aux(1, 0, (1_000, 1_010)));
        worker.publish(&handed).unwrap();

        let mut reader = ConsumerV2::open(&path, GENERATION, lanes[0]).unwrap();
        let guard = reader.try_acquire().unwrap().unwrap();
        let decoded = decode_handoff(&guard.record()).unwrap();
        guard.release().unwrap();
        assert_eq!(decoded.origin, EvidenceOrigin::Coordinator);
        assert_eq!(decoded.status, OutboundStatus::Dispatched);
        assert_eq!(decoded.worker, None);
        assert_eq!((decoded.selected, decoded.queued), (2, 1));
        assert_eq!((decoded.observed_ns, decoded.third_ns), (1_000, 900));
        assert_eq!((decoded.lane_id, decoded.egress_sequence), (512, 5));

        let mut reader = ConsumerV2::open(&path, GENERATION, lanes[1]).unwrap();
        let guard = reader.try_acquire().unwrap().unwrap();
        let decoded = decode_handoff(&guard.record()).unwrap();
        guard.release().unwrap();
        assert_eq!(decoded.origin, EvidenceOrigin::Worker);
        assert_eq!(decoded.status, OutboundStatus::HandedOff);
        assert_eq!((decoded.worker, decoded.lane_id), (Some(0), 513));
        assert_eq!((decoded.third_ns, decoded.event_ns), (1_010, 1_040));
        assert_eq!((decoded.hash, decoded.peer), ([0x55; 32], [0x44; 32]));
        assert_eq!(decoded.status.name(), "HANDED_OFF");
        drop((reader, coordinator, worker, arena));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn evidence_that_contradicts_itself_is_refused() {
        let dir = tmp("refused");
        let path = arena_path_v2(&dir, GENERATION);
        let lane = evidence_lane(0);
        let arena = ArenaV2::create(&path, GENERATION, &[lane]).unwrap();
        let mut producer = ProducerV2::open(&path, GENERATION, lane).unwrap();
        let ok = evidence_aux(1, 0, (1_000, 1_010));
        let mut wrong_kind = evidence(SOURCE_P2P_WORKER, 1_040, ok);
        wrong_kind.kind = RecordKind::EarlyTxSeen;
        let mut wrong_clock = evidence(SOURCE_P2P_WORKER, 1_040, ok);
        wrong_clock.clock_id = 2;
        let (coord, work) = (SOURCE_P2P_COORDINATOR, SOURCE_P2P_WORKER);
        let cases = [
            wrong_kind,
            wrong_clock,
            evidence(SOURCE_ETH_BROADCAST, 1_040, ok),
            evidence(work, 1_040, evidence_aux(9, 0, (1_000, 1_010))),
            // A worker cannot report a coordinator outcome, and vice versa.
            evidence(work, 1_040, evidence_aux(4, 0, (1_000, 1_010))),
            evidence(coord, 1_000, evidence_aux(1, 0, (1_000, 900))),
            // DISPATCHED names no worker; a worker outcome names one.
            evidence(coord, 1_000, evidence_aux(7, 0, (1_000, 900))),
            evidence(work, 1_040, evidence_aux(1, 0xff, (1_000, 1_010))),
            // A send cannot return before it started, or start before the
            // coordinator observed the BUY.
            evidence(work, 1_005, evidence_aux(1, 0, (1_000, 1_010))),
            evidence(work, 1_040, evidence_aux(1, 0, (1_020, 1_010))),
            evidence(coord, 1_001, evidence_aux(7, 0xff, (1_000, 900))),
        ];
        for record in &cases {
            producer.publish(record).unwrap();
        }
        let mut reader = ConsumerV2::open(&path, GENERATION, lane).unwrap();
        let mut refused = 0;
        while let Some(guard) = reader.try_acquire().unwrap() {
            assert!(decode_handoff(&guard.record()).is_err());
            refused += 1;
        }
        assert_eq!(refused, cases.len());
        drop((reader, producer, arena));
        fs::remove_dir_all(dir).unwrap();
    }
}
