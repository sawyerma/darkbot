//! C1.5 HOT0 receive infrastructure.
//!
//! Transport-only, like the rest of this crate: it receives records, keeps
//! them borrowed in their mmap slot while a caller-supplied decision looks at
//! them, and copies an admitted launch exactly once into a preallocated
//! [`IntentSlot`]. Venue decode, state, economics and admission are the
//! caller's (Core's and the runner's); they reach this module only as the
//! `decide` closure of [`Hot0Poller::poll_once`].
//!
//! # Receive path
//!
//! ```text
//! Acquire committed sequence            (LaneSetV2::try_acquire)
//! -> validate header + payload integrity (unchanged Wire V2 contract)
//! -> L2 = monotonic clock               (the Rust Acquire/receive boundary)
//! -> BorrowedRecordV2 on the mmap bytes
//! -> duplicate of a seen launch? release the slot
//! -> decide(&record) on the borrowed bytes
//! -> Admit: copy launch bytes once into an IntentSlot, confirm the slot
//! -> release the guard: read_sequence Release, slot reusable
//! ```
//!
//! The guard and the borrowed record never leave [`Hot0Poller::poll_once`].
//! What leaves it is an [`IntentHandle`] to bytes the poller owns.
//!
//! # Lanes
//!
//! The poller keeps its active-lane list locally. There is no producer-written
//! ready bitmap and the peer lanes are never merged into one queue. A poll
//! starts at the lane after the last one that yielded a record, so a noisy
//! peer gets one record per round. Idle polls examine one lane's lifecycle
//! each, so lanes bound after startup become active without a scan burst on
//! the receive path; [`Hot0Poller::refresh_all`] does a full scan for
//! startup and maintenance.
//!
//! Order across lanes is the records' own timestamps and provenance. The
//! order in which this poller reaches lanes is not an arrival order; a
//! duplicate that is polled later can carry an earlier `T_ingress`, and the
//! dedup window records that.
//!
//! Chain-lane evidence is serviced before any peer record in every poll and
//! handed to the `on_chain` closure. This module is not a reconciler and
//! decides nothing about what is canonical; C2 owns chain state. It only checks
//! that the evidence stays a contiguous head history ([`ChainContinuityV2`]).
//! If the Chain lane faults, geth reports lost chain evidence or a head reset
//! with a GAP record, or a head record does not follow from the previous one,
//! admission is refused until the poller is rebuilt.
//!
//! # Hot-path properties
//!
//! `poll_once` performs no heap allocation, no file, network or logger
//! access, no sleep and takes no lock. Every table is allocated in
//! [`Hot0Poller::open`]. `tests/c15_hot0_alloc.rs` measures the allocation
//! claim with a counting allocator.

use std::fmt;
use std::path::Path;

use crate::HASH_LEN;
use crate::PEER_LEN;
use crate::RecordKind;
use crate::SOURCE_LEN;
use crate::ingress::{IngressTimesV2, ingress_times_borrowed};
use crate::protocol_v2::{
    BorrowedRecordV2, LANE_STATE_BOUND, LANE_STATE_CLOSING, LANE_STATE_FREE, LaneConfigV2,
    LaneSetV2, RecordGuardV2, V2Error,
};

/// Largest launch transaction an [`IntentSlot`] holds: the payload bound of
/// the C1.4 peer lane (4096-byte stride, 256-byte slot header).
pub const INTENT_LAUNCH_MAX: usize = 3840;
/// Largest signed BUY transaction an [`IntentSlot`] holds.
pub const INTENT_BUY_MAX: usize = 1024;
/// Entries per dedup set.
pub const DEDUP_WAYS: usize = 4;

/// `CLOCK_MONOTONIC` in nanoseconds, the clock of geth's `MonotonicNS`
/// (`clock_id` 1). Returns 0 if the clock cannot be read; a record received
/// without a clock is never admitted.
pub fn monotonic_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    if rc != 0 || ts.tv_sec < 0 || ts.tv_nsec < 0 {
        return 0;
    }
    (ts.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(ts.tv_nsec as u64)
}

// ---------------------------------------------------------------------------
// Intent slots
// ---------------------------------------------------------------------------

/// The states of an [`IntentSlot`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentState {
    /// Unused.
    Free,
    /// Holds the admitted launch; the BUY is not set yet.
    Launch,
    /// Holds launch and BUY; immutable until released.
    Sealed,
}

/// The identity and BUY material Core finalizes for an admitted launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentBuy<'a> {
    pub intent_id: u64,
    pub generation: u64,
    pub policy_generation: u64,
    pub nonce: u64,
    pub adapter_permit_mask: u32,
    pub buy_hash: [u8; HASH_LEN],
    pub signed_buy: &'a [u8],
}

/// Why [`IntentSlot::seal`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentError {
    /// The slot does not hold a launch waiting for its BUY.
    NotAwaitingBuy(IntentState),
    BuyTooLarge {
        len: usize,
        max: usize,
    },
    EmptyBuy,
}

/// Preallocated, fixed-size storage for one admitted candidate (C1.5).
///
/// The launch fields are copied once from the borrowed record; the BUY
/// fields are set once by [`IntentSlot::seal`]. After that the slot is
/// immutable until released, and every adapter reads the same bytes: one
/// nonce, one signed BUY. It holds no reference into the mmap arena.
pub struct IntentSlot {
    state: IntentState,
    serial: u64,
    intent_id: u64,
    generation: u64,
    policy_generation: u64,
    nonce: u64,
    adapter_permit_mask: u32,
    launch_lane_id: u16,
    launch_lane_epoch: u64,
    launch_sequence: u64,
    clock_id: u32,
    packet_decoded_ns: u64,
    ingress_ns: u64,
    received_ns: u64,
    launch_hash: [u8; HASH_LEN],
    launch_peer: [u8; PEER_LEN],
    launch_source: [u8; SOURCE_LEN],
    buy_hash: [u8; HASH_LEN],
    launch_len: usize,
    buy_len: usize,
    launch: [u8; INTENT_LAUNCH_MAX],
    buy: [u8; INTENT_BUY_MAX],
}

impl IntentSlot {
    fn empty() -> Self {
        Self {
            state: IntentState::Free,
            serial: 0,
            intent_id: 0,
            generation: 0,
            policy_generation: 0,
            nonce: 0,
            adapter_permit_mask: 0,
            launch_lane_id: 0,
            launch_lane_epoch: 0,
            launch_sequence: 0,
            clock_id: 0,
            packet_decoded_ns: 0,
            ingress_ns: 0,
            received_ns: 0,
            launch_hash: [0; HASH_LEN],
            launch_peer: [0; PEER_LEN],
            launch_source: [0; SOURCE_LEN],
            buy_hash: [0; HASH_LEN],
            launch_len: 0,
            buy_len: 0,
            launch: [0; INTENT_LAUNCH_MAX],
            buy: [0; INTENT_BUY_MAX],
        }
    }

    pub fn state(&self) -> IntentState {
        self.state
    }

    /// The exact raw launch transaction bytes of the admitted record.
    pub fn launch_bytes(&self) -> &[u8] {
        &self.launch[..self.launch_len]
    }

    pub fn launch_hash(&self) -> &[u8; HASH_LEN] {
        &self.launch_hash
    }

    /// The peer whose lane delivered the admitted record.
    pub fn launch_peer(&self) -> &[u8; PEER_LEN] {
        &self.launch_peer
    }

    pub fn launch_source(&self) -> &[u8; SOURCE_LEN] {
        &self.launch_source
    }

    /// `(lane id, lane epoch, lane sequence)` of the admitted record. A lane
    /// sequence orders records of one lane only.
    pub fn launch_provenance(&self) -> (u16, u64, u64) {
        let lane = self.launch_lane_id;
        (lane, self.launch_lane_epoch, self.launch_sequence)
    }

    pub fn clock_id(&self) -> u32 {
        self.clock_id
    }

    /// `T_packet_decoded` of the admitted record.
    pub fn packet_decoded_ns(&self) -> u64 {
        self.packet_decoded_ns
    }

    /// `T_ingress` (L0) of the admitted record.
    pub fn ingress_ns(&self) -> u64 {
        self.ingress_ns
    }

    /// L2: the clock read immediately after the record was acquired.
    pub fn received_ns(&self) -> u64 {
        self.received_ns
    }

    /// Sets the BUY once. Afterwards the slot is immutable until released.
    pub fn seal(&mut self, buy: &IntentBuy<'_>) -> Result<(), IntentError> {
        if self.state != IntentState::Launch {
            return Err(IntentError::NotAwaitingBuy(self.state));
        }
        if buy.signed_buy.is_empty() {
            return Err(IntentError::EmptyBuy);
        }
        if buy.signed_buy.len() > INTENT_BUY_MAX {
            return Err(IntentError::BuyTooLarge {
                len: buy.signed_buy.len(),
                max: INTENT_BUY_MAX,
            });
        }
        self.intent_id = buy.intent_id;
        self.generation = buy.generation;
        self.policy_generation = buy.policy_generation;
        self.nonce = buy.nonce;
        self.adapter_permit_mask = buy.adapter_permit_mask;
        self.buy_hash = buy.buy_hash;
        self.buy_len = buy.signed_buy.len();
        self.buy[..self.buy_len].copy_from_slice(buy.signed_buy);
        self.state = IntentState::Sealed;
        Ok(())
    }

    /// The signed BUY bytes; empty until sealed.
    pub fn buy_bytes(&self) -> &[u8] {
        &self.buy[..self.buy_len]
    }

    pub fn buy_hash(&self) -> &[u8; HASH_LEN] {
        &self.buy_hash
    }

    /// `(intent id, generation, policy generation, nonce, permit mask)`.
    pub fn identity(&self) -> (u64, u64, u64, u64, u32) {
        let mask = self.adapter_permit_mask;
        (
            self.intent_id,
            self.generation,
            self.policy_generation,
            self.nonce,
            mask,
        )
    }

    fn clear(&mut self) {
        self.state = IntentState::Free;
        self.launch_len = 0;
        self.buy_len = 0;
        self.intent_id = 0;
        self.generation = 0;
        self.policy_generation = 0;
        self.nonce = 0;
        self.adapter_permit_mask = 0;
    }
}

/// A claim on one [`IntentSlot`]. The serial distinguishes it from handles
/// of earlier uses of the same slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentHandle {
    index: usize,
    serial: u64,
}

/// A fixed pool of [`IntentSlot`]s, allocated and written once at startup.
/// An unreleased slot is never reused; a full pool refuses the claim.
pub struct IntentPool {
    slots: Box<[IntentSlot]>,
    next_serial: u64,
    in_use: usize,
}

impl IntentPool {
    fn new(capacity: usize) -> Self {
        let slots: Vec<IntentSlot> = (0..capacity).map(|_| IntentSlot::empty()).collect();
        Self {
            slots: slots.into_boxed_slice(),
            next_serial: 1,
            in_use: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn in_use(&self) -> usize {
        self.in_use
    }

    fn claim(&mut self) -> Option<IntentHandle> {
        let index = self
            .slots
            .iter()
            .position(|slot| slot.state == IntentState::Free)?;
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1).max(1);
        self.slots[index].serial = serial;
        self.slots[index].state = IntentState::Launch;
        self.in_use += 1;
        Some(IntentHandle { index, serial })
    }

    /// The slot of a live handle; `None` for a released or foreign handle.
    pub fn get(&self, handle: IntentHandle) -> Option<&IntentSlot> {
        let slot = self.slots.get(handle.index)?;
        let live = slot.serial == handle.serial && slot.state != IntentState::Free;
        live.then_some(slot)
    }

    /// Mutable access for [`IntentSlot::seal`].
    pub fn get_mut(&mut self, handle: IntentHandle) -> Option<&mut IntentSlot> {
        let slot = self.slots.get_mut(handle.index)?;
        let live = slot.serial == handle.serial && slot.state != IntentState::Free;
        live.then_some(slot)
    }

    /// Returns the slot to the pool. `false` for a handle that is not live.
    pub fn release(&mut self, handle: IntentHandle) -> bool {
        let Some(slot) = self.get_mut(handle) else {
            return false;
        };
        slot.clear();
        self.in_use -= 1;
        true
    }
}

// ---------------------------------------------------------------------------
// Bounded duplicate window
// ---------------------------------------------------------------------------

/// What the dedup window knows about a hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DedupResult {
    New,
    /// Seen before. `earliest_ns` is the earliest `T_ingress` of the
    /// earlier observations.
    Duplicate {
        earliest_ns: u64,
    },
}

/// A bounded window of recently seen transaction hashes.
///
/// `sets` sets of [`DEDUP_WAYS`] entries; the set is chosen by the first
/// eight hash bytes, and a full set replaces its oldest entry. It is a
/// receive-side filter: it stops the same launch delivered by several peers
/// from being decided several times. It is not the economic dedup, which
/// Unit 4 owns for the whole session; an entry evicted here is counted, and a
/// later copy of that hash is decided again and stopped there.
pub struct DedupWindowV2 {
    mask: usize,
    hashes: Box<[[u8; HASH_LEN]]>,
    serials: Box<[u64]>,
    earliest: Box<[u64]>,
    next_serial: u64,
    evictions: u64,
}

impl DedupWindowV2 {
    /// `sets` must be a power of two.
    pub fn new(sets: usize) -> Option<Self> {
        if sets == 0 || !sets.is_power_of_two() {
            return None;
        }
        let entries = sets.checked_mul(DEDUP_WAYS)?;
        Some(Self {
            mask: sets - 1,
            hashes: vec![[0u8; HASH_LEN]; entries].into_boxed_slice(),
            serials: vec![0u64; entries].into_boxed_slice(),
            earliest: vec![0u64; entries].into_boxed_slice(),
            next_serial: 1,
            evictions: 0,
        })
    }

    /// Records `hash` (32 bytes) with its `T_ingress`.
    pub fn observe(&mut self, hash: &[u8], ingress_ns: u64) -> DedupResult {
        let mut prefix = [0u8; 8];
        let take = hash.len().min(8);
        prefix[..take].copy_from_slice(&hash[..take]);
        let set = (u64::from_le_bytes(prefix) as usize) & self.mask;
        let base = set * DEDUP_WAYS;
        let mut victim = base;
        for entry in base..base + DEDUP_WAYS {
            if self.serials[entry] != 0 && self.hashes[entry][..] == *hash {
                let earliest_ns = self.earliest[entry];
                if ingress_ns < earliest_ns {
                    self.earliest[entry] = ingress_ns;
                }
                return DedupResult::Duplicate { earliest_ns };
            }
            if self.serials[entry] < self.serials[victim] {
                victim = entry;
            }
        }
        if self.serials[victim] != 0 {
            self.evictions += 1;
        }
        let copy = hash.len().min(HASH_LEN);
        self.hashes[victim] = [0u8; HASH_LEN];
        self.hashes[victim][..copy].copy_from_slice(&hash[..copy]);
        self.serials[victim] = self.next_serial;
        self.earliest[victim] = ingress_ns;
        self.next_serial += 1;
        DedupResult::New
    }

    /// Entries replaced while still occupied.
    pub fn evictions(&self) -> u64 {
        self.evictions
    }
}

// ---------------------------------------------------------------------------
// Chain-lane evidence
// ---------------------------------------------------------------------------

/// A CANONICAL_HEAD record of the Chain lane: `hash` is the block hash, `aux`
/// the parent hash, the payload `number u64 LE | tx_count u32 LE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalHeadV2<'a> {
    pub observed_ns: u64,
    pub number: u64,
    pub tx_count: u32,
    pub hash: &'a [u8],
    pub parent_hash: &'a [u8],
}

/// A REORG record of the Chain lane: `hash` is the new head hash, `aux` the
/// common ancestor hash, the payload `old number u64 LE | old hash 32 | new
/// number u64 LE | new hash 32 | common number u64 LE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReorgV2<'a> {
    pub observed_ns: u64,
    pub old_number: u64,
    pub old_hash: &'a [u8],
    pub new_number: u64,
    pub new_hash: &'a [u8],
    pub common_number: u64,
    pub common_hash: &'a [u8],
}

fn le_u64(bytes: &[u8]) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(raw)
}

/// Decodes a CANONICAL_HEAD record; `None` if it is not one or is malformed.
pub fn decode_canonical_head<'a>(record: &BorrowedRecordV2<'a>) -> Option<CanonicalHeadV2<'a>> {
    if record.kind != RecordKind::CanonicalHead || record.payload.len() != 12 {
        return None;
    }
    let mut count = [0u8; 4];
    count.copy_from_slice(&record.payload[8..12]);
    Some(CanonicalHeadV2 {
        observed_ns: record.timestamp_ns,
        number: le_u64(&record.payload[..8]),
        tx_count: u32::from_le_bytes(count),
        hash: record.hash,
        parent_hash: record.aux,
    })
}

/// Decodes a REORG record; `None` if it is not one or is malformed.
pub fn decode_reorg<'a>(record: &BorrowedRecordV2<'a>) -> Option<ReorgV2<'a>> {
    if record.kind != RecordKind::Reorg || record.payload.len() != 88 {
        return None;
    }
    let payload = record.payload;
    Some(ReorgV2 {
        observed_ns: record.timestamp_ns,
        old_number: le_u64(&payload[..8]),
        old_hash: &payload[8..40],
        new_number: le_u64(&payload[40..48]),
        new_hash: &payload[48..80],
        common_number: le_u64(&payload[80..88]),
        common_hash: record.aux,
    })
}

/// Why geth published a GAP record on the Chain lane (`aux[8..16]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainGapReasonV2 {
    /// Chain records were lost (lane full, clock failure, second publisher).
    LostRecords,
    /// `SetHead` rewound the head.
    SetHead,
    /// A snap-sync head commit replaced the head.
    SnapSyncCommit,
    /// The chain was reset to genesis.
    GenesisReset,
}

/// A GAP record of the Chain lane: `aux[0..8]` the records lost before it,
/// `aux[8..16]` the reason. A head reset carries the payload `old number u64
/// LE | old hash 32 | new number u64 LE | new hash 32`; a loss has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainGapV2<'a> {
    pub observed_ns: u64,
    pub lost: u64,
    pub reason: ChainGapReasonV2,
    /// `(old number, old hash, new number, new hash)` of a head reset.
    pub reset: Option<(u64, &'a [u8], u64, &'a [u8])>,
}

/// Decodes a GAP record; `None` if it is not one or is malformed.
pub fn decode_chain_gap<'a>(record: &BorrowedRecordV2<'a>) -> Option<ChainGapV2<'a>> {
    if record.kind != RecordKind::Gap || record.aux[16..].iter().any(|b| *b != 0) {
        return None;
    }
    let reason = match le_u64(&record.aux[8..16]) {
        0 => ChainGapReasonV2::LostRecords,
        1 => ChainGapReasonV2::SetHead,
        2 => ChainGapReasonV2::SnapSyncCommit,
        3 => ChainGapReasonV2::GenesisReset,
        _ => return None,
    };
    let payload = record.payload;
    let reset = match (reason, payload.len()) {
        (ChainGapReasonV2::LostRecords, 0) => None,
        (ChainGapReasonV2::LostRecords, _) => return None,
        (_, 80) => Some((
            le_u64(&payload[..8]),
            &payload[8..40],
            le_u64(&payload[40..48]),
            &payload[48..80],
        )),
        _ => return None,
    };
    Some(ChainGapV2 {
        observed_ns: record.timestamp_ns,
        lost: le_u64(&record.aux[..8]),
        reason,
        reset,
    })
}

/// Why the Chain lane stopped being a contiguous head history (E7-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainBreakV2 {
    /// geth reported a GAP: lost records or a head reset.
    Gap(ChainGapReasonV2),
    /// A GAP record that does not decode.
    MalformedGap,
    /// A CANONICAL_HEAD or REORG record that does not decode.
    MalformedRecord,
    /// A CANONICAL_HEAD that neither repeats the head nor extends it by one
    /// block with the head as parent.
    CanonicalDiscontinuity,
    /// A REORG whose old head is not the current head, or whose common
    /// ancestor lies above one of its ends.
    ReorgDiscontinuity,
}

/// The result of one Chain-lane record for [`ChainContinuityV2`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainStepV2 {
    /// The first head record: the stream starts here, unverified against any
    /// earlier history.
    Anchored,
    /// The head advanced by one block onto its child.
    Extended,
    /// The current head again (e.g. the CANONICAL_HEAD after its REORG).
    Repeated,
    /// A REORG from the current head to a new one.
    Reorged,
    /// A record of another kind; continuity is unchanged.
    Ignored,
    /// Continuity is broken; sticky until the owner re-anchors after an
    /// independent reconciliation.
    Broken(ChainBreakV2),
}

/// Continuity check of the Chain-lane evidence (E7-2).
///
/// It decides nothing about what is canonical: that is C2's chain-state
/// owner. It only refuses to treat the evidence as a contiguous head history
/// once a record does not follow from the previous one, so a consumer can
/// fail closed (`CHAIN_STATE_GAP`). A break is sticky; [`Self::reanchor`] is
/// for the owner after an independent read-only reconciliation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChainContinuityV2 {
    head: Option<(u64, [u8; 32])>,
    broken: Option<ChainBreakV2>,
}

impl ChainContinuityV2 {
    /// The last head, `(number, hash)`.
    pub fn head(&self) -> Option<(u64, [u8; 32])> {
        self.head
    }

    pub fn broken(&self) -> Option<ChainBreakV2> {
        self.broken
    }

    /// Forgets the head and the break; the next head record anchors again.
    pub fn reanchor(&mut self) {
        *self = Self::default();
    }

    pub fn observe(&mut self, record: &BorrowedRecordV2<'_>) -> ChainStepV2 {
        if let Some(cause) = self.broken {
            return ChainStepV2::Broken(cause);
        }
        let step = match record.kind {
            RecordKind::CanonicalHead => match decode_canonical_head(record) {
                None => ChainStepV2::Broken(ChainBreakV2::MalformedRecord),
                Some(head) => self.canonical(&head),
            },
            RecordKind::Reorg => match decode_reorg(record) {
                None => ChainStepV2::Broken(ChainBreakV2::MalformedRecord),
                Some(reorg) => self.reorg(&reorg),
            },
            RecordKind::Gap => match decode_chain_gap(record) {
                None => ChainStepV2::Broken(ChainBreakV2::MalformedGap),
                Some(gap) => ChainStepV2::Broken(ChainBreakV2::Gap(gap.reason)),
            },
            _ => ChainStepV2::Ignored,
        };
        if let ChainStepV2::Broken(cause) = step {
            self.broken = Some(cause);
        }
        step
    }

    fn canonical(&mut self, head: &CanonicalHeadV2<'_>) -> ChainStepV2 {
        let Some(hash) = hash32(head.hash) else {
            return ChainStepV2::Broken(ChainBreakV2::MalformedRecord);
        };
        match self.head {
            None => {
                self.head = Some((head.number, hash));
                ChainStepV2::Anchored
            }
            Some((number, current)) if hash == current => {
                if head.number == number {
                    ChainStepV2::Repeated
                } else {
                    ChainStepV2::Broken(ChainBreakV2::CanonicalDiscontinuity)
                }
            }
            Some((number, current))
                if head.parent_hash == current && Some(head.number) == number.checked_add(1) =>
            {
                self.head = Some((head.number, hash));
                ChainStepV2::Extended
            }
            Some(_) => ChainStepV2::Broken(ChainBreakV2::CanonicalDiscontinuity),
        }
    }

    fn reorg(&mut self, reorg: &ReorgV2<'_>) -> ChainStepV2 {
        let Some(new_hash) = hash32(reorg.new_hash) else {
            return ChainStepV2::Broken(ChainBreakV2::MalformedRecord);
        };
        if reorg.common_number > reorg.old_number || reorg.common_number > reorg.new_number {
            return ChainStepV2::Broken(ChainBreakV2::ReorgDiscontinuity);
        }
        if let Some((number, current)) = self.head
            && (reorg.old_number != number || reorg.old_hash != current)
        {
            return ChainStepV2::Broken(ChainBreakV2::ReorgDiscontinuity);
        }
        self.head = Some((reorg.new_number, new_hash));
        ChainStepV2::Reorged
    }
}

fn hash32(bytes: &[u8]) -> Option<[u8; 32]> {
    bytes.try_into().ok()
}

// ---------------------------------------------------------------------------
// Poller
// ---------------------------------------------------------------------------

/// The caller's verdict on a borrowed record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hot0Decision {
    Reject,
    Admit,
}

/// Why an admitted record did not become an intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hot0Drop {
    /// Every [`IntentSlot`] is in use.
    IntentPoolExhausted,
    /// The launch is larger than [`INTENT_LAUNCH_MAX`].
    LaunchTooLarge,
    /// The slot changed while it was held; the lane is quarantined.
    UnstableSlot,
}

/// What one [`Hot0Poller::poll_once`] did with a peer record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hot0Outcome {
    /// No peer lane held a record.
    Idle,
    /// A launch this window already saw; its slot was released undecided.
    Duplicate,
    /// The decision rejected the record, or admission is refused because the
    /// Chain lane faulted.
    Rejected,
    /// The launch was copied into the intent slot of this handle.
    Admitted(IntentHandle),
    /// The decision admitted the record but it could not be kept.
    Dropped(Hot0Drop),
    /// A record outside the peer-lane contract (kind, clock, boundary
    /// fields); released, never decided.
    Invalid,
}

/// Counters of a [`Hot0Poller`]. Every received record is in exactly one of
/// `duplicates`, `rejected`, `rejected_chain_unavailable`, `admitted`,
/// `dropped_*` and `invalid_records`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hot0Stats {
    pub polls: u64,
    pub idle_polls: u64,
    pub chain_records: u64,
    /// GAP records of the Chain lane: geth reported lost chain evidence or a
    /// head reset.
    pub chain_gaps: u64,
    /// CANONICAL_HEAD / REORG records that broke continuity (E7-2).
    pub chain_discontinuities: u64,
    pub chain_foreign_records: u64,
    pub peer_records: u64,
    pub duplicates: u64,
    /// Duplicates whose `T_ingress` was earlier than every earlier
    /// observation: poll order was not arrival order.
    pub duplicates_with_earlier_ingress: u64,
    pub rejected: u64,
    pub rejected_chain_unavailable: u64,
    pub admitted: u64,
    pub dropped_pool_exhausted: u64,
    pub dropped_launch_too_large: u64,
    pub dropped_unstable_slot: u64,
    pub invalid_records: u64,
    pub clock_errors: u64,
    /// Lanes that returned a wire error and were dropped from the list.
    pub lane_faults: u64,
    /// Lanes that ended (sealed and drained).
    pub lanes_ended: u64,
    /// CLOSING lanes drained and acknowledged.
    pub lanes_drained: u64,
}

/// Sizes of the poller's preallocated tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hot0Config {
    pub intent_slots: usize,
    /// Dedup sets; a power of two.
    pub dedup_sets: usize,
}

#[derive(Debug)]
pub enum Hot0Error {
    Config(&'static str),
    Wire(V2Error),
}

impl fmt::Display for Hot0Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(what) => write!(f, "hot0 configuration: {what}"),
            Self::Wire(err) => write!(f, "hot0 wire: {err}"),
        }
    }
}

impl std::error::Error for Hot0Error {}

/// Everything of the poller except the lanes, so a held record guard (which
/// borrows the lanes) can be handled with mutable access to the rest.
struct Hot0Core {
    dedup: DedupWindowV2,
    intents: IntentPool,
    stats: Hot0Stats,
    clock: fn() -> u64,
    chain_fault: bool,
    continuity: ChainContinuityV2,
}

enum LanePoll {
    Empty,
    Ended,
    Fault,
    Handled(Hot0Outcome),
}

/// The single-owner multi-lane receive loop of HOT0 (C1.5).
pub struct Hot0Poller {
    lanes: LaneSetV2,
    chain: Option<usize>,
    /// Lane indices polled for records.
    active: Vec<usize>,
    listed: Vec<bool>,
    /// Lanes that ended or faulted; never listed again.
    retired: Vec<bool>,
    cursor: usize,
    scan: usize,
    core: Hot0Core,
}

impl Hot0Poller {
    /// Opens `peers` and the optional Chain lane behind one mapping and
    /// allocates every table. Nothing is allocated after this returns.
    pub fn open(
        path: &Path,
        generation: u64,
        peers: &[LaneConfigV2],
        chain: Option<LaneConfigV2>,
        config: Hot0Config,
    ) -> Result<Self, Hot0Error> {
        if config.intent_slots == 0 {
            return Err(Hot0Error::Config("intent_slots is zero"));
        }
        let Some(dedup) = DedupWindowV2::new(config.dedup_sets) else {
            return Err(Hot0Error::Config("dedup_sets is not a power of two"));
        };
        let mut all = Vec::with_capacity(peers.len() + 1);
        all.extend_from_slice(peers);
        let chain_index = chain.map(|lane| {
            all.push(lane);
            all.len() - 1
        });
        let lanes = LaneSetV2::open(path, generation, &all).map_err(Hot0Error::Wire)?;
        let count = lanes.len();
        let mut poller = Self {
            lanes,
            chain: chain_index,
            active: Vec::with_capacity(count),
            listed: vec![false; count],
            retired: vec![false; count],
            cursor: 0,
            scan: 0,
            core: Hot0Core {
                dedup,
                intents: IntentPool::new(config.intent_slots),
                stats: Hot0Stats::default(),
                clock: monotonic_ns,
                chain_fault: false,
                continuity: ChainContinuityV2::default(),
            },
        };
        poller.refresh_all();
        Ok(poller)
    }

    /// Replaces the clock; for tests.
    pub fn set_clock(&mut self, clock: fn() -> u64) {
        self.core.clock = clock;
    }

    pub fn stats(&self) -> Hot0Stats {
        self.core.stats
    }

    pub fn dedup_evictions(&self) -> u64 {
        self.core.dedup.evictions()
    }

    pub fn intents(&self) -> &IntentPool {
        &self.core.intents
    }

    pub fn intents_mut(&mut self) -> &mut IntentPool {
        &mut self.core.intents
    }

    /// Whether the Chain lane faulted. Admission is refused while it did.
    pub fn chain_faulted(&self) -> bool {
        self.core.chain_fault
    }

    /// Why the Chain-lane evidence stopped being contiguous, if it did.
    pub fn chain_break(&self) -> Option<ChainBreakV2> {
        self.core.continuity.broken()
    }

    /// The lane ids currently polled, in poll order from the cursor.
    pub fn active_lane_ids(&self) -> impl Iterator<Item = u16> + '_ {
        let count = self.active.len();
        (0..count).map(move |step| {
            let lane = self.active[(self.cursor + step) % count];
            self.lanes.config(lane).lane_id
        })
    }

    /// Examines the lifecycle of every peer lane. For startup and
    /// maintenance; idle polls do the same one lane at a time.
    pub fn refresh_all(&mut self) {
        for lane in 0..self.lanes.len() {
            self.refresh_lane(lane);
        }
    }

    /// Services all waiting Chain-lane evidence, then handles at most one
    /// peer record, starting at the lane after the last successful one.
    ///
    /// `on_chain` receives each CANONICAL_HEAD and REORG record;
    /// `decide` receives a peer record that is valid and not a duplicate.
    /// Both get the record borrowed from its slot, for the call only.
    pub fn poll_once<C, D>(&mut self, mut on_chain: C, mut decide: D) -> Hot0Outcome
    where
        C: FnMut(&BorrowedRecordV2<'_>),
        D: FnMut(&BorrowedRecordV2<'_>) -> Hot0Decision,
    {
        self.core.stats.polls += 1;
        self.service_chain(&mut on_chain);
        let count = self.active.len();
        for step in 0..count {
            let position = (self.cursor + step) % count;
            let lane = self.active[position];
            if self.retired[lane] {
                continue;
            }
            match poll_lane(&mut self.lanes, lane, &mut self.core, &mut decide) {
                LanePoll::Empty => {}
                LanePoll::Ended => {
                    self.retired[lane] = true;
                    self.core.stats.lanes_ended += 1;
                }
                LanePoll::Fault => {
                    self.retired[lane] = true;
                    self.core.stats.lane_faults += 1;
                }
                LanePoll::Handled(outcome) => {
                    self.cursor = (position + 1) % count;
                    return outcome;
                }
            }
        }
        self.core.stats.idle_polls += 1;
        self.scan_one();
        Hot0Outcome::Idle
    }

    fn service_chain<C>(&mut self, on_chain: &mut C)
    where
        C: FnMut(&BorrowedRecordV2<'_>),
    {
        let Some(chain) = self.chain else {
            return;
        };
        if self.core.chain_fault || self.retired[chain] {
            return;
        }
        loop {
            let guard = match self.lanes.try_acquire(chain) {
                Ok(Some(guard)) => guard,
                Ok(None) => return,
                Err(V2Error::Shutdown) => {
                    self.retired[chain] = true;
                    self.core.stats.lanes_ended += 1;
                    return;
                }
                Err(_) => {
                    self.core.chain_fault = true;
                    self.core.stats.lane_faults += 1;
                    return;
                }
            };
            let record = guard.record();
            let step = self.core.continuity.observe(&record);
            match record.kind {
                RecordKind::CanonicalHead | RecordKind::Reorg => {
                    self.core.stats.chain_records += 1;
                    if let ChainStepV2::Broken(_) = step {
                        // The evidence no longer follows from the previous
                        // head: an unreported head change (E7-2).
                        self.core.stats.chain_discontinuities += 1;
                        self.core.chain_fault = true;
                    } else {
                        on_chain(&record);
                    }
                }
                // geth lost chain evidence before this point (lane full, or
                // a second publisher detected) or reset the head outside
                // canonical/reorg evidence. What follows is incomplete.
                RecordKind::Gap => {
                    self.core.stats.chain_gaps += 1;
                    self.core.chain_fault = true;
                }
                _ => self.core.stats.chain_foreign_records += 1,
            }
            if guard.release().is_err() {
                self.core.chain_fault = true;
                self.core.stats.lane_faults += 1;
                return;
            }
        }
    }

    /// One step of the incremental lifecycle scan, taken on idle polls.
    fn scan_one(&mut self) {
        let total = self.lanes.len();
        if total == 0 {
            return;
        }
        let lane = self.scan % total;
        self.scan = (lane + 1) % total;
        self.refresh_lane(lane);
    }

    fn refresh_lane(&mut self, lane: usize) {
        if Some(lane) == self.chain || self.retired[lane] {
            return;
        }
        let pending = self.lanes.pending(lane);
        let wanted = match self.lanes.lifecycle_state(lane) {
            LANE_STATE_BOUND => true,
            // A record on a FREE lane is a late write; acquiring it
            // quarantines the lane, so it must be polled.
            LANE_STATE_FREE => pending,
            LANE_STATE_CLOSING => pending || !self.acknowledge(lane),
            _ => false,
        };
        self.set_listed(lane, wanted);
    }

    /// Acknowledges a drained CLOSING lane. `true` once it needs no polling.
    fn acknowledge(&mut self, lane: usize) -> bool {
        match self.lanes.acknowledge_drained_if_closing(lane) {
            Ok(true) => {
                if self.listed[lane] {
                    self.core.stats.lanes_drained += 1;
                }
                true
            }
            Ok(false) => false,
            Err(_) => {
                self.retired[lane] = true;
                self.core.stats.lane_faults += 1;
                true
            }
        }
    }

    fn set_listed(&mut self, lane: usize, wanted: bool) {
        if wanted == self.listed[lane] {
            return;
        }
        self.listed[lane] = wanted;
        if wanted {
            // Capacity was reserved for every lane in `open`.
            self.active.push(lane);
            return;
        }
        if let Some(position) = self.active.iter().position(|listed| *listed == lane) {
            self.active.remove(position);
            if position < self.cursor {
                self.cursor -= 1;
            }
        }
        if self.cursor >= self.active.len() {
            self.cursor = 0;
        }
    }
}

fn poll_lane<D>(
    lanes: &mut LaneSetV2,
    lane: usize,
    core: &mut Hot0Core,
    decision: &mut D,
) -> LanePoll
where
    D: FnMut(&BorrowedRecordV2<'_>) -> Hot0Decision,
{
    let guard = match lanes.try_acquire(lane) {
        Ok(Some(guard)) => guard,
        Ok(None) => return LanePoll::Empty,
        Err(V2Error::Shutdown) => return LanePoll::Ended,
        Err(_) => return LanePoll::Fault,
    };
    // L2: the Rust Acquire/receive boundary.
    let received_ns = (core.clock)();
    let outcome = core.handle(&guard, received_ns, decision);
    match guard.release() {
        Ok(_) => LanePoll::Handled(outcome),
        Err(_) => LanePoll::Fault,
    }
}

impl Hot0Core {
    fn handle<D>(
        &mut self,
        guard: &RecordGuardV2<'_>,
        received_ns: u64,
        decide: &mut D,
    ) -> Hot0Outcome
    where
        D: FnMut(&BorrowedRecordV2<'_>) -> Hot0Decision,
    {
        let record = guard.record();
        self.stats.peer_records += 1;
        let Ok(times) = ingress_times_borrowed(&record) else {
            self.stats.invalid_records += 1;
            return Hot0Outcome::Invalid;
        };
        if received_ns == 0 {
            self.stats.clock_errors += 1;
            self.stats.invalid_records += 1;
            return Hot0Outcome::Invalid;
        }
        let seen = self.dedup.observe(record.hash, times.ingress_ns);
        if let DedupResult::Duplicate { earliest_ns } = seen {
            self.stats.duplicates += 1;
            if times.ingress_ns < earliest_ns {
                self.stats.duplicates_with_earlier_ingress += 1;
            }
            return Hot0Outcome::Duplicate;
        }
        if self.chain_fault {
            self.stats.rejected_chain_unavailable += 1;
            return Hot0Outcome::Rejected;
        }
        match decide(&record) {
            Hot0Decision::Reject => {
                self.stats.rejected += 1;
                Hot0Outcome::Rejected
            }
            Hot0Decision::Admit => self.admit(guard, &record, times, received_ns),
        }
    }

    fn admit(
        &mut self,
        guard: &RecordGuardV2<'_>,
        record: &BorrowedRecordV2<'_>,
        times: IngressTimesV2,
        received_ns: u64,
    ) -> Hot0Outcome {
        if record.payload.len() > INTENT_LAUNCH_MAX {
            self.stats.dropped_launch_too_large += 1;
            return Hot0Outcome::Dropped(Hot0Drop::LaunchTooLarge);
        }
        let Some(handle) = self.intents.claim() else {
            self.stats.dropped_pool_exhausted += 1;
            return Hot0Outcome::Dropped(Hot0Drop::IntentPoolExhausted);
        };
        let slot = &mut self.intents.slots[handle.index];
        // The one copy of the launch bytes; it also confirms the slot.
        let Ok(len) = guard.copy_payload(&mut slot.launch) else {
            slot.clear();
            self.intents.in_use -= 1;
            self.stats.dropped_unstable_slot += 1;
            return Hot0Outcome::Dropped(Hot0Drop::UnstableSlot);
        };
        slot.launch_len = len;
        slot.launch_hash.copy_from_slice(record.hash);
        slot.launch_peer.copy_from_slice(record.peer);
        slot.launch_source.copy_from_slice(record.source);
        slot.launch_lane_id = record.lane_id;
        slot.launch_lane_epoch = record.lane_epoch;
        slot.launch_sequence = record.sequence;
        slot.clock_id = record.clock_id;
        slot.packet_decoded_ns = times.packet_decoded_ns;
        slot.ingress_ns = times.ingress_ns;
        slot.received_ns = received_ns;
        self.stats.admitted += 1;
        Hot0Outcome::Admitted(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingress::{C14_FLAP_SCOPE, SOURCE_ETH_BROADCAST, decode_tx_target, ingress_aux};
    use crate::protocol_v2::{
        ArenaV2, CLOCK_ID_MONOTONIC, ConsumerV2, DirectionV2, LaneClassV2, ProducerV2, RecordV2,
        arena_path_v2,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    const GENERATION: u64 = 1505;
    const CHAIN_LANE: u16 = 2;
    const FIRST_PEER: u16 = 16;
    const PORTAL: [u8; 20] = [
        0xe2, 0xce, 0x6a, 0xb8, 0x08, 0x74, 0xfa, 0x9f, 0xa2, 0xaa, 0xe6, 0x5d, 0x27, 0x7d, 0xd6,
        0xb8, 0xe6, 0x5c, 0x9d, 0xe0,
    ];

    static ID: AtomicU64 = AtomicU64::new(0);

    fn fixed_clock() -> u64 {
        9_000
    }

    fn peer_lane(lane_id: u16) -> LaneConfigV2 {
        LaneConfigV2 {
            lane_id,
            direction: DirectionV2::GethToRust,
            class: LaneClassV2::PeerIngress,
            flags: 0,
            capacity: 8,
            slot_stride: 1024,
        }
    }

    fn chain_lane() -> LaneConfigV2 {
        LaneConfigV2 {
            lane_id: CHAIN_LANE,
            direction: DirectionV2::GethToRust,
            class: LaneClassV2::ChainEvidence,
            flags: 0,
            capacity: 8,
            slot_stride: 512,
        }
    }

    /// A legacy transaction to the Portal whose calldata is the launch
    /// selector followed by `tag`; the smallest envelope the C1.4 scope
    /// matches. `to` set to another address makes it a non-match.
    fn envelope(to: [u8; 20], tag: u8) -> Vec<u8> {
        let mut body = vec![0x01, 0x01, 0x01, 0x94];
        body.extend_from_slice(&to);
        body.push(0x80);
        body.extend_from_slice(&[0x85, 0x8c, 0xb5, 0x77, 0x2c, tag]);
        body.extend_from_slice(&[0x01, 0x01, 0x01]);
        let mut raw = vec![0xc0 + body.len() as u8];
        raw.extend_from_slice(&body);
        raw
    }

    struct Fixture {
        dir: PathBuf,
        path: PathBuf,
        arena: ArenaV2,
        peers: Vec<LaneConfigV2>,
    }

    impl Fixture {
        /// An arena with `peers` peer lanes and the Chain lane; the first
        /// `bound` peer lanes are BOUND.
        fn new(name: &str, peers: u16, bound: u16) -> Self {
            let id = ID.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "darkbot-fastlane-hot0-{}-{}-{name}",
                std::process::id(),
                id
            ));
            fs::create_dir_all(&dir).unwrap();
            let path = arena_path_v2(&dir, GENERATION);
            let peers: Vec<LaneConfigV2> = (0..peers).map(|i| peer_lane(FIRST_PEER + i)).collect();
            let mut lanes = vec![chain_lane()];
            lanes.extend_from_slice(&peers);
            let mut arena = ArenaV2::create(&path, GENERATION, &lanes).unwrap();
            for lane in &peers[..usize::from(bound)] {
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

        fn producer(&self, lane_id: u16) -> ProducerV2 {
            ProducerV2::open(&self.path, GENERATION, peer_lane(lane_id)).unwrap()
        }

        fn chain_producer(&self) -> ProducerV2 {
            ProducerV2::open(&self.path, GENERATION, chain_lane()).unwrap()
        }

        fn poller(&self, intent_slots: usize) -> Hot0Poller {
            let config = Hot0Config {
                intent_slots,
                dedup_sets: 16,
            };
            let chain = Some(chain_lane());
            let mut poller =
                Hot0Poller::open(&self.path, GENERATION, &self.peers, chain, config).unwrap();
            poller.set_clock(fixed_clock);
            poller
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    /// Publishes an EARLY_TX_SEEN record with `T_ingress = ingress_ns`.
    fn publish(producer: &mut ProducerV2, hash: u8, ingress_ns: u64, payload: &[u8]) {
        let record = RecordV2 {
            kind: RecordKind::EarlyTxSeen,
            flags: 0,
            timestamp_ns: ingress_ns,
            clock_id: CLOCK_ID_MONOTONIC,
            source: SOURCE_ETH_BROADCAST,
            peer: [0x22; 32],
            hash: [hash; 32],
            aux: ingress_aux(ingress_ns - 1),
            payload,
        };
        producer.publish(&record).unwrap();
    }

    /// The factual scope decision on the borrowed bytes: the C1.4 envelope
    /// decoder and scope, nothing copied.
    fn scope_decision(record: &BorrowedRecordV2<'_>) -> Hot0Decision {
        match decode_tx_target(record.payload) {
            Ok(target) if C14_FLAP_SCOPE.matches(&target) => Hot0Decision::Admit,
            _ => Hot0Decision::Reject,
        }
    }

    fn no_chain(_: &BorrowedRecordV2<'_>) {}

    #[test]
    fn round_robin_starts_after_the_last_successful_lane() {
        let fixture = Fixture::new("round-robin", 3, 3);
        let launch = envelope(PORTAL, 1);
        let mut producers: Vec<ProducerV2> = (0..3).map(|i| fixture.producer(16 + i)).collect();
        // Lane 16: hashes 1, 4. Lane 17: 2, 5. Lane 18: 3, 6.
        for round in 0..2u8 {
            for (lane, producer) in producers.iter_mut().enumerate() {
                publish(producer, round * 3 + lane as u8 + 1, 100, &launch);
            }
        }
        let mut poller = fixture.poller(8);
        let mut order = Vec::new();
        for _ in 0..6 {
            poller.poll_once(no_chain, |record| {
                order.push((record.lane_id, record.hash[0]));
                Hot0Decision::Reject
            });
        }
        assert_eq!(
            order,
            [(16, 1), (17, 2), (18, 3), (16, 4), (17, 5), (18, 6)]
        );
        let idle = poller.poll_once(no_chain, scope_decision);
        assert_eq!(idle, Hot0Outcome::Idle);
    }

    #[test]
    fn a_noisy_lane_does_not_starve_another_lane() {
        let fixture = Fixture::new("starvation", 2, 2);
        let launch = envelope(PORTAL, 1);
        let mut noisy = fixture.producer(16);
        let mut quiet = fixture.producer(17);
        for hash in 1..=8 {
            publish(&mut noisy, hash, 100, &launch);
        }
        publish(&mut quiet, 50, 100, &launch);
        let mut poller = fixture.poller(8);
        let mut order = Vec::new();
        for _ in 0..3 {
            poller.poll_once(no_chain, |record| {
                order.push(record.lane_id);
                Hot0Decision::Reject
            });
        }
        // The quiet lane is served on the second poll, not after eight.
        assert_eq!(order, [16, 17, 16]);
        // And while the noisy lane refills, the quiet lane keeps its turn.
        publish(&mut quiet, 51, 100, &launch);
        publish(&mut noisy, 9, 100, &launch);
        poller.poll_once(no_chain, |record| {
            order.push(record.lane_id);
            Hot0Decision::Reject
        });
        assert_eq!(order.last(), Some(&17));
    }

    #[test]
    fn a_launch_seen_on_several_peer_lanes_is_decided_once() {
        let fixture = Fixture::new("duplicates", 3, 3);
        let launch = envelope(PORTAL, 7);
        // The lane polled last carries the earliest T_ingress.
        publish(&mut fixture.producer(16), 9, 300, &launch);
        publish(&mut fixture.producer(17), 9, 200, &launch);
        publish(&mut fixture.producer(18), 9, 100, &launch);
        let mut poller = fixture.poller(4);
        let mut decided = 0;
        let mut outcomes = Vec::new();
        for _ in 0..3 {
            outcomes.push(poller.poll_once(no_chain, |record| {
                decided += 1;
                scope_decision(record)
            }));
        }
        assert_eq!(decided, 1);
        assert!(matches!(outcomes[0], Hot0Outcome::Admitted(_)));
        assert_eq!(outcomes[1..], [Hot0Outcome::Duplicate; 2]);
        let stats = poller.stats();
        assert_eq!((stats.admitted, stats.duplicates), (1, 2));
        // Poll order 16, 17, 18 was the reverse of arrival order.
        assert_eq!(stats.duplicates_with_earlier_ingress, 2);
        // Every duplicate slot was released at once: all three lanes drained.
        for lane in 16..19 {
            let lane_stats = fixture.arena.lane_stats(lane).unwrap();
            assert_eq!(lane_stats.read_sequence, lane_stats.write_sequence);
        }
        // The intent keeps the provenance of the record that was admitted.
        let Hot0Outcome::Admitted(handle) = outcomes[0] else {
            unreachable!()
        };
        let slot = poller.intents().get(handle).unwrap();
        assert_eq!(slot.launch_provenance(), (16, 2, 0));
        assert_eq!((slot.ingress_ns(), slot.received_ns()), (300, 9_000));
    }

    #[test]
    fn an_admitted_launch_is_copied_once_and_survives_slot_reuse() {
        let fixture = Fixture::new("intent", 1, 1);
        let launch = envelope(PORTAL, 3);
        let other = envelope([0xaa; 20], 3);
        let mut producer = fixture.producer(16);
        publish(&mut producer, 1, 500, &other);
        publish(&mut producer, 2, 600, &launch);
        let mut poller = fixture.poller(2);
        // The decision is the C1.4 decode on the borrowed bytes.
        let first = poller.poll_once(no_chain, scope_decision);
        assert_eq!(first, Hot0Outcome::Rejected);
        let Hot0Outcome::Admitted(handle) = poller.poll_once(no_chain, scope_decision) else {
            panic!("launch not admitted");
        };
        // Reuse every slot of the lane, including the admitted record's.
        for hash in 10..18 {
            publish(&mut producer, hash, 700, &other);
        }
        let slot = poller.intents().get(handle).unwrap();
        assert_eq!(slot.state(), IntentState::Launch);
        assert_eq!(slot.launch_bytes(), &launch[..]);
        assert_eq!(slot.launch_hash(), &[2u8; 32]);
        assert_eq!(slot.launch_peer(), &[0x22u8; 32]);
        assert_eq!(slot.launch_source(), &SOURCE_ETH_BROADCAST);
        assert_eq!((slot.packet_decoded_ns(), slot.ingress_ns()), (599, 600));
        assert_eq!(slot.clock_id(), CLOCK_ID_MONOTONIC);
    }

    #[test]
    fn an_intent_is_sealed_once_and_released_explicitly() {
        let fixture = Fixture::new("seal", 1, 1);
        let launch = envelope(PORTAL, 4);
        let mut producer = fixture.producer(16);
        publish(&mut producer, 1, 500, &launch);
        publish(&mut producer, 2, 500, &launch);
        let mut poller = fixture.poller(1);
        let Hot0Outcome::Admitted(handle) = poller.poll_once(no_chain, scope_decision) else {
            panic!("launch not admitted");
        };
        // The pool is bounded: a second admission is dropped, with evidence.
        assert_eq!(
            poller.poll_once(no_chain, scope_decision),
            Hot0Outcome::Dropped(Hot0Drop::IntentPoolExhausted)
        );
        assert_eq!(poller.stats().dropped_pool_exhausted, 1);

        let signed = [0x5a; 200];
        let buy = IntentBuy {
            intent_id: 11,
            generation: GENERATION,
            policy_generation: 3,
            nonce: 42,
            adapter_permit_mask: 0b101,
            buy_hash: [0x77; 32],
            signed_buy: &signed,
        };
        let slot = poller.intents_mut().get_mut(handle).unwrap();
        slot.seal(&buy).unwrap();
        assert_eq!(slot.state(), IntentState::Sealed);
        assert_eq!(slot.buy_bytes(), &signed[..]);
        assert_eq!(slot.identity(), (11, GENERATION, 3, 42, 0b101));
        // No second nonce, no second BUY.
        assert_eq!(
            slot.seal(&buy),
            Err(IntentError::NotAwaitingBuy(IntentState::Sealed))
        );
        assert!(poller.intents_mut().release(handle));
        assert!(poller.intents().get(handle).is_none());
        assert!(!poller.intents_mut().release(handle));
        assert_eq!(poller.intents().in_use(), 0);
    }

    #[test]
    fn oversize_and_empty_buys_are_refused() {
        let mut pool = IntentPool::new(1);
        let handle = pool.claim().unwrap();
        let slot = pool.get_mut(handle).unwrap();
        let large = vec![0u8; INTENT_BUY_MAX + 1];
        let mut buy = IntentBuy {
            intent_id: 1,
            generation: 1,
            policy_generation: 1,
            nonce: 1,
            adapter_permit_mask: 1,
            buy_hash: [1; 32],
            signed_buy: &large,
        };
        assert!(matches!(
            slot.seal(&buy),
            Err(IntentError::BuyTooLarge { .. })
        ));
        buy.signed_buy = &[];
        assert_eq!(slot.seal(&buy), Err(IntentError::EmptyBuy));
        assert_eq!(slot.state(), IntentState::Launch);
    }

    #[test]
    fn chain_evidence_is_serviced_before_peer_records() {
        let fixture = Fixture::new("chain-first", 1, 1);
        let launch = envelope(PORTAL, 5);
        publish(&mut fixture.producer(16), 1, 500, &launch);
        let mut chain = fixture.chain_producer();
        for number in [77u8, 78] {
            let mut payload = [0u8; 12];
            payload[..8].copy_from_slice(&u64::from(number).to_le_bytes());
            payload[8..].copy_from_slice(&3u32.to_le_bytes());
            let record = RecordV2 {
                kind: RecordKind::CanonicalHead,
                flags: 0,
                timestamp_ns: 400,
                clock_id: CLOCK_ID_MONOTONIC,
                source: [0; 16],
                peer: [0; 32],
                hash: [number; 32],
                aux: [number - 1; 32],
                payload: &payload,
            };
            chain.publish(&record).unwrap();
        }
        let mut poller = fixture.poller(2);
        let mut events = Vec::new();
        let mut heads = Vec::new();
        let outcome = poller.poll_once(
            |record| {
                let head = decode_canonical_head(record).unwrap();
                let ends = (head.hash[0], head.parent_hash[0]);
                heads.push((head.number, head.tx_count, ends));
            },
            |record| {
                events.push(record.hash[0]);
                scope_decision(record)
            },
        );
        assert!(matches!(outcome, Hot0Outcome::Admitted(_)));
        // Both heads were delivered before the peer record was decided.
        assert_eq!(heads, [(77, 3, (77, 76)), (78, 3, (78, 77))]);
        assert_eq!(events, [1]);
        assert_eq!(poller.stats().chain_records, 2);
    }

    #[test]
    fn a_faulted_chain_lane_refuses_admission() {
        let fixture = Fixture::new("chain-fault", 1, 1);
        let launch = envelope(PORTAL, 6);
        publish(&mut fixture.producer(16), 1, 500, &launch);
        let chain = ConsumerV2::open(&fixture.path, GENERATION, chain_lane()).unwrap();
        chain.quarantine();
        // One consumer per lane: the quarantine stays in the arena.
        drop(chain);
        let mut poller = fixture.poller(2);
        let mut decided = 0;
        let outcome = poller.poll_once(no_chain, |record| {
            decided += 1;
            scope_decision(record)
        });
        assert_eq!(outcome, Hot0Outcome::Rejected);
        assert_eq!(decided, 0);
        assert!(poller.chain_faulted());
        assert_eq!(poller.stats().rejected_chain_unavailable, 1);
    }

    #[test]
    fn a_chain_gap_record_refuses_admission() {
        let fixture = Fixture::new("chain-gap", 1, 1);
        let launch = envelope(PORTAL, 6);
        publish(&mut fixture.producer(16), 1, 500, &launch);
        let mut aux = [0u8; 32];
        aux[..8].copy_from_slice(&3u64.to_le_bytes());
        let gap = RecordV2 {
            kind: RecordKind::Gap,
            flags: 0,
            timestamp_ns: 400,
            clock_id: CLOCK_ID_MONOTONIC,
            source: [0; 16],
            peer: [0; 32],
            hash: [0; 32],
            aux,
            payload: &[],
        };
        fixture.chain_producer().publish(&gap).unwrap();
        let mut poller = fixture.poller(2);
        let mut chain_calls = 0;
        let outcome = poller.poll_once(|_| chain_calls += 1, scope_decision);
        assert_eq!(outcome, Hot0Outcome::Rejected);
        assert_eq!(chain_calls, 0);
        assert!(poller.chain_faulted());
        let stats = poller.stats();
        assert_eq!((stats.chain_gaps, stats.rejected_chain_unavailable), (1, 1));
    }

    #[test]
    fn a_corrupt_peer_lane_is_retired_and_the_others_continue() {
        let mut fixture = Fixture::new("lane-fault", 2, 2);
        let launch = envelope(PORTAL, 8);
        publish(&mut fixture.producer(16), 1, 500, &launch);
        publish(&mut fixture.producer(17), 2, 500, &launch);
        // Quarantined after the poller listed it: the poll meets the fault.
        let mut poller = fixture.poller(2);
        fixture.arena.quarantine_lane(16).unwrap();
        let outcome = poller.poll_once(no_chain, scope_decision);
        assert!(matches!(outcome, Hot0Outcome::Admitted(_)));
        assert_eq!(poller.stats().lane_faults, 1);
        // The quarantined lane's record was not consumed.
        let faulted = fixture.arena.lane_stats(16).unwrap();
        assert_eq!((faulted.read_sequence, faulted.write_sequence), (0, 1));
        let idle = poller.poll_once(no_chain, scope_decision);
        assert_eq!(idle, Hot0Outcome::Idle);
    }

    #[test]
    fn records_outside_the_peer_contract_are_released_undecided() {
        let fixture = Fixture::new("invalid", 1, 1);
        let launch = envelope(PORTAL, 9);
        let mut producer = fixture.producer(16);
        let mut record = RecordV2 {
            kind: RecordKind::TxpoolAccepted,
            flags: 0,
            timestamp_ns: 500,
            clock_id: CLOCK_ID_MONOTONIC,
            source: SOURCE_ETH_BROADCAST,
            peer: [0x22; 32],
            hash: [1; 32],
            aux: ingress_aux(499),
            payload: &launch,
        };
        producer.publish(&record).unwrap();
        record.kind = RecordKind::EarlyTxSeen;
        record.clock_id = 2;
        producer.publish(&record).unwrap();
        record.clock_id = CLOCK_ID_MONOTONIC;
        record.aux = [0; 32];
        producer.publish(&record).unwrap();
        let mut poller = fixture.poller(2);
        let mut decided = 0;
        for _ in 0..3 {
            let outcome = poller.poll_once(no_chain, |_| {
                decided += 1;
                Hot0Decision::Admit
            });
            assert_eq!(outcome, Hot0Outcome::Invalid);
        }
        assert_eq!(decided, 0);
        assert_eq!(poller.stats().invalid_records, 3);
        let stats = fixture.arena.lane_stats(16).unwrap();
        assert_eq!(stats.read_sequence, 3);
    }

    #[test]
    fn idle_polls_discover_lanes_bound_later() {
        let mut fixture = Fixture::new("discover", 3, 0);
        let mut poller = fixture.poller(2);
        assert_eq!(poller.active_lane_ids().count(), 0);
        fixture.arena.bind_peer_lane(18, [0x18; 32]).unwrap();
        let launch = envelope(PORTAL, 2);
        publish(&mut fixture.producer(18), 1, 500, &launch);
        // One lane is examined per idle poll: chain lane and three peers.
        let mut polls = 0;
        let outcome = loop {
            polls += 1;
            let outcome = poller.poll_once(no_chain, scope_decision);
            if outcome != Hot0Outcome::Idle || polls == 16 {
                break outcome;
            }
        };
        assert!(matches!(outcome, Hot0Outcome::Admitted(_)));
        assert!(polls <= 5, "{polls} polls");
        assert_eq!(poller.active_lane_ids().collect::<Vec<_>>(), [18]);
    }

    #[test]
    fn a_closing_lane_is_drained_acknowledged_and_unlisted() {
        let mut fixture = Fixture::new("closing", 1, 1);
        let launch = envelope(PORTAL, 2);
        publish(&mut fixture.producer(16), 1, 500, &launch);
        let mut poller = fixture.poller(2);
        let owner = [16u8; 32];
        fixture.arena.begin_close_peer_lane(16, owner, 2).unwrap();
        // The unread record of the closing lane is still delivered.
        let outcome = poller.poll_once(no_chain, scope_decision);
        assert!(matches!(outcome, Hot0Outcome::Admitted(_)));
        poller.refresh_all();
        assert_eq!(poller.active_lane_ids().count(), 0);
        assert_eq!(poller.stats().lanes_drained, 1);
        // The drain acknowledgement lets geth release the lane.
        fixture.arena.release_peer_lane(16, owner, 2).unwrap();
        assert_eq!(fixture.arena.lane_stats(16).unwrap().state, LANE_STATE_FREE);
    }

    #[test]
    fn a_sealed_lane_ends_without_a_fault() {
        let mut fixture = Fixture::new("sealed", 1, 1);
        let launch = envelope(PORTAL, 2);
        publish(&mut fixture.producer(16), 1, 500, &launch);
        let mut poller = fixture.poller(2);
        fixture.arena.begin_shutdown().unwrap();
        let outcome = poller.poll_once(no_chain, scope_decision);
        assert!(matches!(outcome, Hot0Outcome::Admitted(_)));
        let idle = poller.poll_once(no_chain, scope_decision);
        assert_eq!(idle, Hot0Outcome::Idle);
        let stats = poller.stats();
        // Peer lane and Chain lane both ended; neither is a fault.
        assert_eq!((stats.lanes_ended, stats.lane_faults), (2, 0));
        assert!(!poller.chain_faulted());
    }

    #[test]
    fn the_dedup_window_is_bounded_and_counts_evictions() {
        let mut window = DedupWindowV2::new(1).unwrap();
        assert!(DedupWindowV2::new(3).is_none());
        assert!(DedupWindowV2::new(0).is_none());
        for tag in 1..=4u8 {
            assert_eq!(window.observe(&[tag; 32], 100), DedupResult::New);
        }
        assert_eq!(
            window.observe(&[2; 32], 50),
            DedupResult::Duplicate { earliest_ns: 100 }
        );
        // The earliest T_ingress is kept across duplicates.
        assert_eq!(
            window.observe(&[2; 32], 70),
            DedupResult::Duplicate { earliest_ns: 50 }
        );
        // A fifth hash replaces the oldest entry of the full set.
        assert_eq!(window.observe(&[5; 32], 100), DedupResult::New);
        assert_eq!(window.evictions(), 1);
        assert_eq!(window.observe(&[1; 32], 100), DedupResult::New);
        assert_eq!(window.evictions(), 2);
    }

    #[test]
    fn a_reorg_record_decodes_on_the_borrowed_bytes() {
        let fixture = Fixture::new("reorg", 1, 0);
        let mut chain = fixture.chain_producer();
        let mut payload = [0u8; 88];
        payload[..8].copy_from_slice(&10u64.to_le_bytes());
        payload[8..40].copy_from_slice(&[0xa1; 32]);
        payload[40..48].copy_from_slice(&11u64.to_le_bytes());
        payload[48..80].copy_from_slice(&[0xb2; 32]);
        payload[80..88].copy_from_slice(&8u64.to_le_bytes());
        let record = RecordV2 {
            kind: RecordKind::Reorg,
            flags: 0,
            timestamp_ns: 400,
            clock_id: CLOCK_ID_MONOTONIC,
            source: [0; 16],
            peer: [0; 32],
            hash: [0xb2; 32],
            aux: [0xc3; 32],
            payload: &payload,
        };
        chain.publish(&record).unwrap();
        let mut poller = fixture.poller(1);
        let mut seen = None;
        poller.poll_once(
            |record| {
                let reorg = decode_reorg(record).unwrap();
                let numbers = (reorg.old_number, reorg.new_number, reorg.common_number);
                seen = Some((numbers, reorg.old_hash[0], reorg.new_hash[0]));
                assert_eq!(reorg.common_hash, &[0xc3; 32][..]);
                assert!(decode_canonical_head(record).is_none());
            },
            scope_decision,
        );
        assert_eq!(seen, Some(((10, 11, 8), 0xa1, 0xb2)));
    }

    fn chain_record(
        kind: RecordKind,
        hash: [u8; 32],
        aux: [u8; 32],
        payload: &[u8],
    ) -> RecordV2<'_> {
        RecordV2 {
            kind,
            flags: 0,
            timestamp_ns: 400,
            clock_id: CLOCK_ID_MONOTONIC,
            source: [0; 16],
            peer: [0; 32],
            hash,
            aux,
            payload,
        }
    }

    fn canonical_payload(number: u64) -> [u8; 12] {
        let mut payload = [0u8; 12];
        payload[..8].copy_from_slice(&number.to_le_bytes());
        payload
    }

    fn reorg_payload(old: (u64, u8), new: (u64, u8), common: u64) -> [u8; 88] {
        let mut payload = [0u8; 88];
        payload[..8].copy_from_slice(&old.0.to_le_bytes());
        payload[8..40].copy_from_slice(&[old.1; 32]);
        payload[40..48].copy_from_slice(&new.0.to_le_bytes());
        payload[48..80].copy_from_slice(&[new.1; 32]);
        payload[80..88].copy_from_slice(&common.to_le_bytes());
        payload
    }

    /// Publishes the records and returns each continuity step plus the
    /// poller's verdict on one launch published after them.
    fn chain_run(
        name: &str,
        records: &[RecordV2<'_>],
    ) -> (Vec<ChainStepV2>, Hot0Outcome, Hot0Stats) {
        let fixture = Fixture::new(name, 1, 1);
        let mut chain = fixture.chain_producer();
        let mut continuity = ChainContinuityV2::default();
        let mut steps = Vec::new();
        for record in records {
            chain.publish(record).unwrap();
            let owned = record.payload.to_vec();
            let borrowed = BorrowedRecordV2 {
                sequence: 0,
                lane_id: 2,
                lane_epoch: 1,
                kind: record.kind,
                flags: 0,
                timestamp_ns: record.timestamp_ns,
                clock_id: record.clock_id,
                source: &record.source,
                peer: &record.peer,
                hash: &record.hash,
                aux: &record.aux,
                payload: &owned,
            };
            steps.push(continuity.observe(&borrowed));
        }
        publish(&mut fixture.producer(16), 1, 500, &envelope(PORTAL, 7));
        let mut poller = fixture.poller(2);
        let outcome = poller.poll_once(no_chain, scope_decision);
        assert_eq!(poller.chain_break(), continuity.broken());
        (steps, outcome, poller.stats())
    }

    #[test]
    fn e72_a_contiguous_head_history_with_a_reorg_keeps_admission() {
        let (p10, p11, reorg) = (
            canonical_payload(10),
            canonical_payload(11),
            reorg_payload((11, 11), (11, 0xb1), 10),
        );
        let p11b = canonical_payload(11);
        let p12 = canonical_payload(12);
        let records = [
            chain_record(RecordKind::CanonicalHead, [10; 32], [9; 32], &p10),
            chain_record(RecordKind::CanonicalHead, [11; 32], [10; 32], &p11),
            chain_record(RecordKind::Reorg, [0xb1; 32], [10; 32], &reorg),
            chain_record(RecordKind::CanonicalHead, [0xb1; 32], [10; 32], &p11b),
            chain_record(RecordKind::CanonicalHead, [12; 32], [0xb1; 32], &p12),
        ];
        let (steps, outcome, stats) = chain_run("e72-contiguous", &records);
        use ChainStepV2::*;
        assert_eq!(steps, [Anchored, Extended, Reorged, Repeated, Extended]);
        assert!(matches!(outcome, Hot0Outcome::Admitted(_)));
        assert_eq!((stats.chain_records, stats.chain_discontinuities), (5, 0));
    }

    #[test]
    fn e72_unreported_head_changes_refuse_admission() {
        let (p10, p12, p11) = (
            canonical_payload(10),
            canonical_payload(12),
            canonical_payload(11),
        );
        let skip = [
            chain_record(RecordKind::CanonicalHead, [10; 32], [9; 32], &p10),
            chain_record(RecordKind::CanonicalHead, [12; 32], [11; 32], &p12),
        ];
        let wrong_parent = [
            chain_record(RecordKind::CanonicalHead, [10; 32], [9; 32], &p10),
            chain_record(RecordKind::CanonicalHead, [11; 32], [0xee; 32], &p11),
        ];
        let wrong_number = [
            chain_record(RecordKind::CanonicalHead, [10; 32], [9; 32], &p10),
            chain_record(RecordKind::CanonicalHead, [10; 32], [9; 32], &p11),
        ];
        let stale_reorg = reorg_payload((9, 9), (11, 0xb1), 8);
        let foreign_old = [
            chain_record(RecordKind::CanonicalHead, [10; 32], [9; 32], &p10),
            chain_record(RecordKind::Reorg, [0xb1; 32], [8; 32], &stale_reorg),
        ];
        let high_common = reorg_payload((10, 10), (11, 0xb1), 12);
        let impossible = [
            chain_record(RecordKind::CanonicalHead, [10; 32], [9; 32], &p10),
            chain_record(RecordKind::Reorg, [0xb1; 32], [12; 32], &high_common),
        ];
        for (name, records, cause) in [
            ("e72-skip", &skip[..], ChainBreakV2::CanonicalDiscontinuity),
            (
                "e72-parent",
                &wrong_parent[..],
                ChainBreakV2::CanonicalDiscontinuity,
            ),
            (
                "e72-number",
                &wrong_number[..],
                ChainBreakV2::CanonicalDiscontinuity,
            ),
            (
                "e72-old",
                &foreign_old[..],
                ChainBreakV2::ReorgDiscontinuity,
            ),
            (
                "e72-common",
                &impossible[..],
                ChainBreakV2::ReorgDiscontinuity,
            ),
        ] {
            let (steps, outcome, stats) = chain_run(name, records);
            assert_eq!(steps.last(), Some(&ChainStepV2::Broken(cause)), "{name}");
            assert_eq!(outcome, Hot0Outcome::Rejected, "{name}");
            assert_eq!(
                (
                    stats.chain_discontinuities,
                    stats.rejected_chain_unavailable
                ),
                (1, 1),
                "{name}"
            );
        }
    }

    #[test]
    fn e72_head_reset_and_loss_gaps_decode_and_stay_broken() {
        let p10 = canonical_payload(10);
        let mut reset = [0u8; 80];
        reset[..8].copy_from_slice(&10u64.to_le_bytes());
        reset[8..40].copy_from_slice(&[10; 32]);
        reset[40..48].copy_from_slice(&4u64.to_le_bytes());
        reset[48..80].copy_from_slice(&[4; 32]);
        let mut reset_aux = [0u8; 32];
        reset_aux[8..16].copy_from_slice(&1u64.to_le_bytes());
        let mut lost_aux = [0u8; 32];
        lost_aux[..8].copy_from_slice(&5u64.to_le_bytes());
        let p5 = canonical_payload(5);
        let records = [
            chain_record(RecordKind::CanonicalHead, [10; 32], [9; 32], &p10),
            chain_record(RecordKind::Gap, [0; 32], reset_aux, &reset),
            chain_record(RecordKind::CanonicalHead, [5; 32], [4; 32], &p5),
        ];
        let (steps, outcome, stats) = chain_run("e72-reset", &records);
        let broken = ChainStepV2::Broken(ChainBreakV2::Gap(ChainGapReasonV2::SetHead));
        assert_eq!(steps, [ChainStepV2::Anchored, broken, broken]);
        assert_eq!(outcome, Hot0Outcome::Rejected);
        assert_eq!(stats.chain_gaps, 1);

        let view = |aux: &'static [u8; 32], payload: &'static [u8]| BorrowedRecordV2 {
            sequence: 0,
            lane_id: 2,
            lane_epoch: 1,
            kind: RecordKind::Gap,
            flags: 0,
            timestamp_ns: 1,
            clock_id: CLOCK_ID_MONOTONIC,
            source: &[0; 16],
            peer: &[0; 32],
            hash: &[0; 32],
            aux,
            payload,
        };
        static LOST: [u8; 32] = {
            let mut aux = [0u8; 32];
            aux[0] = 5;
            aux
        };
        static RESET: [u8; 32] = {
            let mut aux = [0u8; 32];
            aux[8] = 3;
            aux
        };
        static UNKNOWN: [u8; 32] = {
            let mut aux = [0u8; 32];
            aux[8] = 9;
            aux
        };
        static PAYLOAD: [u8; 80] = [7; 80];
        let lost = decode_chain_gap(&view(&LOST, &[])).unwrap();
        assert_eq!(
            (lost.lost, lost.reason, lost.reset),
            (5, ChainGapReasonV2::LostRecords, None)
        );
        let genesis = decode_chain_gap(&view(&RESET, &PAYLOAD)).unwrap();
        assert_eq!(genesis.reason, ChainGapReasonV2::GenesisReset);
        assert!(genesis.reset.is_some());
        assert!(decode_chain_gap(&view(&UNKNOWN, &PAYLOAD)).is_none());
        assert!(decode_chain_gap(&view(&LOST, &PAYLOAD)).is_none());
        assert!(decode_chain_gap(&view(&RESET, &[])).is_none());
        let mut continuity = ChainContinuityV2::default();
        assert_eq!(
            continuity.observe(&view(&UNKNOWN, &PAYLOAD)),
            ChainStepV2::Broken(ChainBreakV2::MalformedGap)
        );
        continuity.reanchor();
        assert_eq!(continuity, ChainContinuityV2::default());
    }
}
