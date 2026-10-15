//! C1.4 geth ingress records on Wire V2, Rust side.
//!
//! # EARLY_TX_SEEN on a peer lane
//!
//! - `timestamp_ns` is `T_ingress`: taken once per packet after geth's
//!   required basic validation succeeded, before prefilter, serialization
//!   and publication.
//! - `aux[0..8]` is `T_packet_decoded`, u64 little-endian: taken once per
//!   packet after the packet was decoded into transactions and before the
//!   validation. It is diagnostic and never actionable.
//! - `aux[8..32]` is zero, reserved in C1.4.
//! - Both times use `clock_id` [`CLOCK_ID_MONOTONIC`].
//! - `source` is `eth-broadcast` (Transactions) or `eth-pooled`
//!   (PooledTransactions), `peer` the sending peer's node ID, the payload the
//!   canonical signed transaction.
//!
//! # TXPOOL_ACCEPTED on the dedicated Txpool lane
//!
//! Every transaction the local txpool accepted, independent of the peer
//! prefilter, published by geth's `txBroadcastLoop` as the lane's sole
//! producer: source `txpool`, zero peer and aux.
//!
//! # False-negative audit
//!
//! [`IngressAuditV2`] decodes each TXPOOL_ACCEPTED payload itself, sharing no
//! parsing with geth, and judges it against the scope. An in-scope
//! transaction counts as observed only if its earliest valid peer-lane
//! `T_ingress` is not later than its TXPOOL_ACCEPTED timestamp, both on
//! [`CLOCK_ID_MONOTONIC`]. Any other in-scope transaction is C1.4
//! false-negative evidence and fails the qualification; a record outside the
//! contract makes the evidence invalid. [`IngressAuditV2::qualification`] is
//! fail-closed: only closed, drained, contract-conforming evidence passes.
//! The scope is the Rust mirror of the
//! geth prefilter, the supported launch-selector superset; its canonical bytes
//! are a golden value that darkbot-core recomputes from the committed venue
//! constants.

use std::collections::{HashMap, VecDeque};

use crate::RecordKind;
use crate::protocol_v2::{
    BorrowedRecordV2, CLOCK_ID_MONOTONIC, ConsumerV2, DirectionV2, LaneClassV2, LaneConfigV2,
    LaneStatsV2, OwnedRecordV2, V2Error, crc32c,
};

/// Source of a record from an ETH `Transactions` packet.
pub const SOURCE_ETH_BROADCAST: [u8; 16] = source_tag(b"eth-broadcast");
/// Source of a record from an ETH `PooledTransactions` packet.
pub const SOURCE_ETH_POOLED: [u8; 16] = source_tag(b"eth-pooled");
/// Source of a TXPOOL_ACCEPTED record.
pub const SOURCE_TXPOOL: [u8; 16] = source_tag(b"txpool");

const fn source_tag(name: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    let mut i = 0;
    while i < name.len() && i < 16 {
        out[i] = name[i];
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Lane plan
// ---------------------------------------------------------------------------

pub const C14_TXPOOL_LANE_ID: u16 = 1;
pub const C14_FIRST_PEER_LANE_ID: u16 = 16;
/// Qualified upper bound of max_peers + churn_reserve.
pub const C14_PEER_LANE_POOL_BOUND: usize = 256;
pub const C14_PEER_LANE_CAPACITY: usize = 64;
pub const C14_PEER_LANE_SLOT_STRIDE: usize = 4096;
pub const C14_TXPOOL_LANE_CAPACITY: usize = 4096;
pub const C14_TXPOOL_LANE_SLOT_STRIDE: usize = 4096;

/// The C1.4 arena layout: the Txpool lane, then one peer lane per peer slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C14LanePlan {
    pub peer_lanes: usize,
    pub peer_lane_capacity: usize,
    pub peer_lane_slot_stride: usize,
    pub txpool_lane_capacity: usize,
    pub txpool_lane_slot_stride: usize,
}

impl C14LanePlan {
    /// The production plan for `peer_lanes` = max_peers + churn_reserve.
    pub fn production(peer_lanes: usize) -> Self {
        Self {
            peer_lanes,
            peer_lane_capacity: C14_PEER_LANE_CAPACITY,
            peer_lane_slot_stride: C14_PEER_LANE_SLOT_STRIDE,
            txpool_lane_capacity: C14_TXPOOL_LANE_CAPACITY,
            txpool_lane_slot_stride: C14_TXPOOL_LANE_SLOT_STRIDE,
        }
    }

    pub fn txpool_lane(&self) -> LaneConfigV2 {
        LaneConfigV2 {
            lane_id: C14_TXPOOL_LANE_ID,
            direction: DirectionV2::GethToRust,
            class: LaneClassV2::TxpoolEvidence,
            flags: 0,
            capacity: self.txpool_lane_capacity,
            slot_stride: self.txpool_lane_slot_stride,
        }
    }

    pub fn peer_lane(&self, lane_id: u16) -> LaneConfigV2 {
        LaneConfigV2 {
            lane_id,
            direction: DirectionV2::GethToRust,
            class: LaneClassV2::PeerIngress,
            flags: 0,
            capacity: self.peer_lane_capacity,
            slot_stride: self.peer_lane_slot_stride,
        }
    }

    pub fn lanes(&self) -> Vec<LaneConfigV2> {
        let mut lanes = Vec::with_capacity(self.peer_lanes + 1);
        lanes.push(self.txpool_lane());
        for i in 0..self.peer_lanes {
            let offset = u16::try_from(i).unwrap_or(u16::MAX);
            let lane_id = C14_FIRST_PEER_LANE_ID.saturating_add(offset);
            lanes.push(self.peer_lane(lane_id));
        }
        lanes
    }
}

// ---------------------------------------------------------------------------
// Boundary timestamps
// ---------------------------------------------------------------------------

/// The two C1.4 boundaries of an EARLY_TX_SEEN record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngressTimesV2 {
    pub packet_decoded_ns: u64,
    pub ingress_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngressRecordError {
    NotEarlyTxSeen(RecordKind),
    ClockDomain(u32),
    ReservedAux,
    MissingPacketDecoded,
    DecodedAfterIngress {
        packet_decoded_ns: u64,
        ingress_ns: u64,
    },
}

/// Encodes the aux field of an EARLY_TX_SEEN record for `T_packet_decoded`.
pub fn ingress_aux(packet_decoded_ns: u64) -> [u8; 32] {
    let mut aux = [0u8; 32];
    aux[..8].copy_from_slice(&packet_decoded_ns.to_le_bytes());
    aux
}

/// Reads and checks the boundaries of an EARLY_TX_SEEN record.
pub fn ingress_times(record: &OwnedRecordV2) -> Result<IngressTimesV2, IngressRecordError> {
    ingress_times_parts(
        record.kind,
        record.clock_id,
        record.timestamp_ns,
        &record.aux,
    )
}

/// [`ingress_times`] for a record borrowed from its slot (C1.5).
pub fn ingress_times_borrowed(
    record: &BorrowedRecordV2<'_>,
) -> Result<IngressTimesV2, IngressRecordError> {
    ingress_times_parts(
        record.kind,
        record.clock_id,
        record.timestamp_ns,
        record.aux,
    )
}

/// The boundary check on the record fields themselves. `aux` is the 32-byte
/// aux field.
pub fn ingress_times_parts(
    kind: RecordKind,
    clock_id: u32,
    timestamp_ns: u64,
    aux: &[u8],
) -> Result<IngressTimesV2, IngressRecordError> {
    if kind != RecordKind::EarlyTxSeen {
        return Err(IngressRecordError::NotEarlyTxSeen(kind));
    }
    if clock_id != CLOCK_ID_MONOTONIC {
        return Err(IngressRecordError::ClockDomain(clock_id));
    }
    if aux.len() != 32 || aux[8..].iter().any(|byte| *byte != 0) {
        return Err(IngressRecordError::ReservedAux);
    }
    let mut decoded = [0u8; 8];
    decoded.copy_from_slice(&aux[..8]);
    let packet_decoded_ns = u64::from_le_bytes(decoded);
    if packet_decoded_ns == 0 {
        return Err(IngressRecordError::MissingPacketDecoded);
    }
    if packet_decoded_ns > timestamp_ns {
        return Err(IngressRecordError::DecodedAfterIngress {
            packet_decoded_ns,
            ingress_ns: timestamp_ns,
        });
    }
    Ok(IngressTimesV2 {
        packet_decoded_ns,
        ingress_ns: timestamp_ns,
    })
}

// ---------------------------------------------------------------------------
// Prefilter scope mirror
// ---------------------------------------------------------------------------

/// The only prefilter role: a launch selector.
pub const ROLE_LAUNCH: u8 = 1;

/// Canonical bytes of [`C14_FLAP_SCOPE`]; identical in Go, here and in the
/// darkbot-core recomputation from the committed venue constants.
pub const C14_SCOPE_CANONICAL_HEX: &str =
    "444243313450463138000000000000000100e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de001008cb5772c01";
/// CRC32C of the canonical scope bytes.
pub const C14_SCOPE_FINGERPRINT: u32 = 0x1692_c814;

/// A conservative address+selector superset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefilterScopeV2 {
    pub chain_id: u64,
    pub addresses: &'static [[u8; 20]],
    pub selectors: &'static [([u8; 4], u8)],
}

/// Flap Portal v5.24.0 on BSC mainnet with the launch selector
/// (`newTokenV6`) of every supported mode. BUY traffic is not launch
/// detection and is not in the peer-ingress scope.
pub const C14_FLAP_SCOPE: PrefilterScopeV2 = PrefilterScopeV2 {
    chain_id: 56,
    addresses: &[[
        0xe2, 0xce, 0x6a, 0xb8, 0x08, 0x74, 0xfa, 0x9f, 0xa2, 0xaa, 0xe6, 0x5d, 0x27, 0x7d, 0xd6,
        0xb8, 0xe6, 0x5c, 0x9d, 0xe0,
    ]],
    selectors: &[([0x8c, 0xb5, 0x77, 0x2c], ROLE_LAUNCH)],
};

impl PrefilterScopeV2 {
    /// `"DBC14PF1" | chain_id u64 LE | address count u16 LE | addresses
    /// ascending | selector count u16 LE | (selector, role) ascending`.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut addresses = self.addresses.to_vec();
        addresses.sort_unstable();
        let mut selectors = self.selectors.to_vec();
        selectors.sort_unstable();
        let address_count = u16::try_from(addresses.len()).unwrap_or(u16::MAX);
        let selector_count = u16::try_from(selectors.len()).unwrap_or(u16::MAX);
        let mut out = Vec::with_capacity(20 + 20 * addresses.len() + 5 * selectors.len());
        out.extend_from_slice(b"DBC14PF1");
        out.extend_from_slice(&self.chain_id.to_le_bytes());
        out.extend_from_slice(&address_count.to_le_bytes());
        for address in &addresses {
            out.extend_from_slice(address);
        }
        out.extend_from_slice(&selector_count.to_le_bytes());
        for (selector, role) in &selectors {
            out.extend_from_slice(selector);
            out.push(*role);
        }
        out
    }

    pub fn fingerprint(&self) -> u32 {
        crc32c(&self.canonical_bytes())
    }

    /// Destination first, then the 4-byte selector, as the geth prefilter.
    pub fn matches(&self, target: &TxTargetV2<'_>) -> bool {
        let Some(to) = target.to else {
            return false;
        };
        if !self.addresses.contains(&to) || target.data.len() < 4 {
            return false;
        }
        self.selectors
            .iter()
            .any(|(selector, _)| target.data[..4] == selector[..])
    }
}

// ---------------------------------------------------------------------------
// Transaction envelope: destination and calldata
// ---------------------------------------------------------------------------

/// Destination and calldata of a canonical signed transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxTargetV2<'a> {
    /// `None` for a legacy transaction, the type byte otherwise.
    pub tx_type: Option<u8>,
    /// `None` for a contract creation.
    pub to: Option<[u8; 20]>,
    pub data: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeErrorV2 {
    Empty,
    Malformed(&'static str),
    UnsupportedType(u8),
}

#[derive(Clone, Copy)]
struct RlpItem<'a> {
    list: bool,
    payload: &'a [u8],
}

fn rlp_length(bytes: &[u8], width: usize) -> Result<usize, EnvelopeErrorV2> {
    if width == 0 || width > 8 || bytes.len() < width {
        return Err(EnvelopeErrorV2::Malformed("rlp: truncated length"));
    }
    if bytes[0] == 0 {
        return Err(EnvelopeErrorV2::Malformed("rlp: length with leading zero"));
    }
    let mut length = 0u64;
    for byte in &bytes[..width] {
        length = (length << 8) | u64::from(*byte);
    }
    usize::try_from(length).map_err(|_| EnvelopeErrorV2::Malformed("rlp: length overflow"))
}

/// Splits the first RLP item off `bytes`.
fn rlp_next(bytes: &[u8]) -> Result<(RlpItem<'_>, &[u8]), EnvelopeErrorV2> {
    let (&prefix, rest) = bytes
        .split_first()
        .ok_or(EnvelopeErrorV2::Malformed("rlp: missing item"))?;
    let (list, width, length) = match prefix {
        0x00..=0x7f => {
            return Ok((
                RlpItem {
                    list: false,
                    payload: &bytes[..1],
                },
                rest,
            ));
        }
        0x80..=0xb7 => (false, 0, usize::from(prefix - 0x80)),
        0xb8..=0xbf => {
            let width = usize::from(prefix - 0xb7);
            (false, width, rlp_length(rest, width)?)
        }
        0xc0..=0xf7 => (true, 0, usize::from(prefix - 0xc0)),
        _ => {
            let width = usize::from(prefix - 0xf7);
            (true, width, rlp_length(rest, width)?)
        }
    };
    let body = &rest[width..];
    if body.len() < length {
        return Err(EnvelopeErrorV2::Malformed("rlp: truncated item"));
    }
    Ok((
        RlpItem {
            list,
            payload: &body[..length],
        },
        &body[length..],
    ))
}

/// Field `index` of an RLP list payload.
fn rlp_field(list_payload: &[u8], index: usize) -> Result<RlpItem<'_>, EnvelopeErrorV2> {
    let mut rest = list_payload;
    for _ in 0..index {
        rest = rlp_next(rest)?.1;
    }
    Ok(rlp_next(rest)?.0)
}

/// Decodes destination and calldata of a canonical signed transaction:
/// legacy, EIP-2930 (1), EIP-1559 (2), EIP-4844 (3, plain or with the
/// network sidecar wrapper) and EIP-7702 (4). Other types are unsupported.
pub fn decode_tx_target(raw: &[u8]) -> Result<TxTargetV2<'_>, EnvelopeErrorV2> {
    let first = *raw.first().ok_or(EnvelopeErrorV2::Empty)?;
    let (tx_type, envelope) = match first {
        0xc0..=0xff => (None, raw),
        0x01..=0x04 => (Some(first), &raw[1..]),
        0x00 | 0x05..=0x7f => return Err(EnvelopeErrorV2::UnsupportedType(first)),
        0x80..=0xbf => {
            return Err(EnvelopeErrorV2::Malformed(
                "envelope: neither a list nor a type byte",
            ));
        }
    };
    let (outer, trailing) = rlp_next(envelope)?;
    if !outer.list || !trailing.is_empty() {
        return Err(EnvelopeErrorV2::Malformed(
            "envelope: not exactly one rlp list",
        ));
    }
    let (fields, to_index, data_index) = match tx_type {
        None => (outer.payload, 3, 5),
        Some(1) => (outer.payload, 4, 6),
        Some(3) => {
            let head = rlp_field(outer.payload, 0)?;
            let fields = if head.list {
                head.payload
            } else {
                outer.payload
            };
            (fields, 5, 7)
        }
        Some(_) => (outer.payload, 5, 7),
    };
    let to = rlp_field(fields, to_index)?;
    let data = rlp_field(fields, data_index)?;
    if to.list || data.list {
        return Err(EnvelopeErrorV2::Malformed(
            "envelope: list where destination or calldata was expected",
        ));
    }
    let to = match to.payload.len() {
        0 => None,
        20 => {
            let mut address = [0u8; 20];
            address.copy_from_slice(to.payload);
            Some(address)
        }
        _ => {
            return Err(EnvelopeErrorV2::Malformed(
                "envelope: destination is not 20 bytes",
            ));
        }
    };
    Ok(TxTargetV2 {
        tx_type,
        to,
        data: data.payload,
    })
}

// ---------------------------------------------------------------------------
// False-negative audit
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditVerdictV2 {
    /// Outside the scope.
    NotRelevant,
    /// In scope, and the earliest valid peer-lane `T_ingress` is not later
    /// than the TXPOOL_ACCEPTED timestamp.
    ObservedOnPeerLane,
    /// In scope without a peer-lane `T_ingress` at or before the
    /// TXPOOL_ACCEPTED timestamp: C1.4 false-negative evidence, a
    /// qualification failure. A peer observation after the acceptance does
    /// not count.
    FalseNegative,
    /// A TXPOOL_ACCEPTED record outside the C1.4 contract (clock domain,
    /// source, nonzero peer or aux).
    InvalidRecord,
    /// A transaction type the audit cannot decode.
    UnsupportedEnvelope,
    /// A payload that is not a valid transaction envelope.
    MalformedEnvelope,
    /// Not a TXPOOL_ACCEPTED record.
    NotTxpoolRecord,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IngressAuditStatsV2 {
    pub peer_records: u64,
    pub invalid_peer_records: u64,
    pub peer_hashes_evicted: u64,
    pub txpool_records: u64,
    /// Every record passed to [`IngressAuditV2::judge_txpool_record`].
    pub txpool_judged: u64,
    pub invalid_txpool_records: u64,
    pub relevant: u64,
    pub observed: u64,
    pub false_negatives: u64,
    pub late_peer_observations: u64,
    pub unsupported_envelopes: u64,
    pub malformed_envelopes: u64,
    pub foreign_records: u64,
    /// Lane consumer errors seen by [`IngressAuditV2::poll`].
    pub consumer_errors: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FalseNegativeEvidenceV2 {
    pub hash: [u8; 32],
    pub txpool_timestamp_ns: u64,
    /// Earliest valid peer-lane `T_ingress`; `Some` only if a peer lane
    /// carried the transaction after the txpool accepted it.
    pub earliest_peer_ingress_ns: Option<u64>,
    pub selector: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditQualificationV2 {
    /// Closed, drained, contract-conforming evidence without a false
    /// negative.
    Pass,
    /// Evidence outside the C1.4 contract (record contract, record kind,
    /// consumer or wire errors): nothing qualifies.
    InvalidEvidence(&'static str),
    FalseNegatives(u64),
    /// Evidence that may lack records: never a pass.
    Incomplete(&'static str),
}

/// Txpool producer counters geth reports at the qualification point
/// (`fastlane.IngressV2Snapshot()`: `txpool_published`, `txpool_refused`,
/// `txpool_serialize_errors`, `txpool_clock_errors`). They are not on the
/// wire: a transaction the producer could not stamp, serialize or publish
/// leaves no TXPOOL_ACCEPTED record, so only these counters reveal it. Each
/// counts transactions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TxpoolProducerCountersV2 {
    pub published: u64,
    pub refused: u64,
    pub serialize_errors: u64,
    pub clock_errors: u64,
}

const FALSE_NEGATIVE_EVIDENCE_LIMIT: usize = 4096;

fn is_peer_source(source: &[u8; 16]) -> bool {
    *source == SOURCE_ETH_BROADCAST || *source == SOURCE_ETH_POOLED
}

/// Correlates TXPOOL_ACCEPTED with EARLY_TX_SEEN outside the hot path.
///
/// Per hash it keeps the earliest valid peer-lane `T_ingress`. A
/// transaction's peer-lane publish happens before geth hands it to the
/// fetcher and the txpool, so it happens before its TXPOOL_ACCEPTED publish;
/// [`IngressAuditV2::poll`] therefore drains the Txpool lane first, then every
/// peer lane, and only then judges. The time comparison decides: a peer
/// record drained before judging but stamped after the acceptance does not
/// make the transaction observed. [`IngressAuditV2::qualification`] passes
/// only closed, drained, contract-conforming evidence.
pub struct IngressAuditV2 {
    scope: PrefilterScopeV2,
    capacity: usize,
    earliest: HashMap<[u8; 32], u64>,
    order: VecDeque<[u8; 32]>,
    stats: IngressAuditStatsV2,
    evidence: Vec<FalseNegativeEvidenceV2>,
}

impl IngressAuditV2 {
    /// `capacity` bounds the window of remembered peer-lane hashes.
    pub fn new(scope: PrefilterScopeV2, capacity: usize) -> Self {
        Self {
            scope,
            capacity: capacity.max(1),
            earliest: HashMap::new(),
            order: VecDeque::new(),
            stats: IngressAuditStatsV2::default(),
            evidence: Vec::new(),
        }
    }

    /// Records the `T_ingress` of an EARLY_TX_SEEN record, keeping the
    /// earliest per hash. A record outside the C1.4 contract (peer source,
    /// clock domain, boundary fields) is counted invalid and not used.
    /// Returns whether the record was used.
    pub fn observe_peer_record(&mut self, record: &OwnedRecordV2) -> bool {
        if record.kind != RecordKind::EarlyTxSeen {
            self.stats.foreign_records += 1;
            return false;
        }
        let times = match ingress_times(record) {
            Ok(times) if is_peer_source(&record.source) => times,
            _ => {
                self.stats.invalid_peer_records += 1;
                return false;
            }
        };
        self.stats.peer_records += 1;
        if let Some(earliest) = self.earliest.get_mut(&record.hash) {
            *earliest = (*earliest).min(times.ingress_ns);
            return true;
        }
        self.earliest.insert(record.hash, times.ingress_ns);
        self.order.push_back(record.hash);
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.earliest.remove(&old);
                self.stats.peer_hashes_evicted += 1;
            }
        }
        true
    }

    /// Judges one TXPOOL_ACCEPTED record against the peer observations.
    pub fn judge_txpool_record(&mut self, record: &OwnedRecordV2) -> AuditVerdictV2 {
        self.stats.txpool_judged += 1;
        if record.kind != RecordKind::TxpoolAccepted {
            self.stats.foreign_records += 1;
            return AuditVerdictV2::NotTxpoolRecord;
        }
        if record.clock_id != CLOCK_ID_MONOTONIC
            || record.source != SOURCE_TXPOOL
            || record.peer != [0; 32]
            || record.aux != [0; 32]
        {
            self.stats.invalid_txpool_records += 1;
            return AuditVerdictV2::InvalidRecord;
        }
        self.stats.txpool_records += 1;
        let target = match decode_tx_target(&record.payload) {
            Ok(target) => target,
            Err(EnvelopeErrorV2::UnsupportedType(_)) => {
                self.stats.unsupported_envelopes += 1;
                return AuditVerdictV2::UnsupportedEnvelope;
            }
            Err(_) => {
                self.stats.malformed_envelopes += 1;
                return AuditVerdictV2::MalformedEnvelope;
            }
        };
        if !self.scope.matches(&target) {
            return AuditVerdictV2::NotRelevant;
        }
        self.stats.relevant += 1;
        let earliest = self.earliest.get(&record.hash).copied();
        if let Some(peer_ingress_ns) = earliest {
            if peer_ingress_ns <= record.timestamp_ns {
                self.stats.observed += 1;
                return AuditVerdictV2::ObservedOnPeerLane;
            }
            self.stats.late_peer_observations += 1;
        }
        self.stats.false_negatives += 1;
        if self.evidence.len() < FALSE_NEGATIVE_EVIDENCE_LIMIT {
            let mut selector = [0u8; 4];
            selector.copy_from_slice(&target.data[..4]);
            self.evidence.push(FalseNegativeEvidenceV2 {
                hash: record.hash,
                txpool_timestamp_ns: record.timestamp_ns,
                earliest_peer_ingress_ns: earliest,
                selector,
            });
        }
        AuditVerdictV2::FalseNegative
    }

    /// Drains up to `max_txpool_records` from the Txpool lane, then every
    /// peer lane, then judges the drained txpool records in order.
    pub fn poll(
        &mut self,
        peers: &mut [ConsumerV2],
        txpool: &mut ConsumerV2,
        max_txpool_records: usize,
    ) -> Result<Vec<AuditVerdictV2>, V2Error> {
        let mut batch = Vec::new();
        while batch.len() < max_txpool_records {
            match self.consume(txpool)? {
                Some(record) => batch.push(record),
                None => break,
            }
        }
        for consumer in peers.iter_mut() {
            while let Some(record) = self.consume(consumer)? {
                self.observe_peer_record(&record);
            }
        }
        Ok(batch
            .iter()
            .map(|record| self.judge_txpool_record(record))
            .collect())
    }

    /// One consume. `V2Error::Shutdown` is the normal end of a sealed lane:
    /// `ConsumerV2::try_consume` returns it only after its state check and
    /// only when `read == write`, so no record is left behind; a quarantined
    /// lane, which also has shutdown set, reports `LaneQuarantined` instead.
    /// Every other error is counted, so it can never pass.
    fn consume(&mut self, consumer: &mut ConsumerV2) -> Result<Option<OwnedRecordV2>, V2Error> {
        match consumer.try_consume() {
            Ok(record) => Ok(record),
            Err(V2Error::Shutdown) => Ok(None),
            Err(err) => {
                self.stats.consumer_errors += 1;
                Err(err)
            }
        }
    }

    pub fn stats(&self) -> IngressAuditStatsV2 {
        self.stats
    }

    pub fn false_negatives(&self) -> &[FalseNegativeEvidenceV2] {
        &self.evidence
    }

    /// Fail-closed C1.4 qualification at a quiescent point: the Txpool lane
    /// is sealed (`shutdown` set, so every later publish is refused), the
    /// final `producer` counters were read after the last producer call and
    /// before this call, and the audit has polled until the lane is drained.
    ///
    /// `Pass` requires: no invalid, foreign or consumer-error evidence and an
    /// intact Txpool lane (no corrupt record, no quarantine); no false
    /// negative; producer counters available with no refusal, serialize or
    /// clock error; no lane drop or oversize refusal; no undecodable record;
    /// a sealed lane in state BOUND; `read == write`, `published == write`
    /// and every read record judged. Anything else is invalid or incomplete.
    pub fn qualification(
        &self,
        txpool_lane: &LaneStatsV2,
        producer: Option<&TxpoolProducerCountersV2>,
    ) -> AuditQualificationV2 {
        use AuditQualificationV2::{FalseNegatives, Incomplete, InvalidEvidence, Pass};
        let s = self.stats;
        let lane = txpool_lane;
        let invalid = [
            (s.invalid_peer_records, "invalid peer records"),
            (s.invalid_txpool_records, "invalid txpool records"),
            (s.foreign_records, "foreign record kinds"),
            (s.consumer_errors, "lane consumer errors"),
            (lane.corrupt_records, "txpool lane corrupt records"),
            (lane.quarantine_reason, "txpool lane quarantine reason"),
        ];
        if let Some(&(_, reason)) = invalid.iter().find(|(count, _)| *count > 0) {
            return InvalidEvidence(reason);
        }
        if lane.state == crate::protocol_v2::LANE_STATE_QUARANTINED {
            return InvalidEvidence("txpool lane quarantined");
        }
        if s.false_negatives > 0 {
            return FalseNegatives(s.false_negatives);
        }
        let Some(p) = producer else {
            return Incomplete("txpool producer counters unavailable");
        };
        let incomplete = [
            (p.refused, "txpool producer refused records"),
            (p.serialize_errors, "txpool producer serialize errors"),
            (p.clock_errors, "txpool producer clock errors"),
            (lane.dropped_full, "txpool lane dropped records"),
            (lane.oversize, "txpool lane refused oversize records"),
            (s.unsupported_envelopes, "undecodable txpool records"),
            (s.malformed_envelopes, "undecodable txpool records"),
        ];
        if let Some(&(_, reason)) = incomplete.iter().find(|(count, _)| *count > 0) {
            return Incomplete(reason);
        }
        // Drained is not closed: without shutdown the producer can still
        // publish the next record.
        if !lane.shutdown {
            return Incomplete("txpool producer window still open");
        }
        // The Txpool lane is a dedicated lane: BOUND for its whole life.
        if lane.state != crate::protocol_v2::LANE_STATE_BOUND {
            return InvalidEvidence("txpool lane not bound");
        }
        if lane.read_sequence != lane.write_sequence {
            return Incomplete("txpool lane not drained");
        }
        if p.published != lane.write_sequence {
            return Incomplete("txpool producer and lane disagree");
        }
        if s.txpool_judged != lane.read_sequence {
            return Incomplete("txpool records read but not judged");
        }
        Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol_v2::{
        ArenaV2, LANE_STATE_BOUND, LANE_STATE_CLOSING, ProducerV2, RecordV2, arena_path_v2,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const DOGE: &str =
        include_str!("../../trading/fixtures/flap/launch_v6_doge_block_124415357.hex");
    const CREATOR_BUY: &str =
        include_str!("../../trading/fixtures/flap/launch_v6_native_creator_buy_pending.hex");
    const SPLIT_TAX: &str =
        include_str!("../../trading/fixtures/flap/launch_v6_native_split_tax_pending.hex");
    const PORTAL: [u8; 20] = [
        0xe2, 0xce, 0x6a, 0xb8, 0x08, 0x74, 0xfa, 0x9f, 0xa2, 0xaa, 0xe6, 0x5d, 0x27, 0x7d, 0xd6,
        0xb8, 0xe6, 0x5c, 0x9d, 0xe0,
    ];
    const OTHER: [u8; 20] = [0xaa; 20];
    const LAUNCH: [u8; 4] = [0x8c, 0xb5, 0x77, 0x2c];
    const BUY: [u8; 4] = [0xef, 0x7e, 0xc2, 0xe7];

    static ID: AtomicU64 = AtomicU64::new(0);

    fn unhex(text: &str) -> Vec<u8> {
        let text = text.trim();
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    fn hex(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(char::from(DIGITS[usize::from(byte >> 4)]));
            out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
        }
        out
    }

    fn rlp_header(base: u8, len: usize) -> Vec<u8> {
        if len < 56 {
            return vec![base + len as u8];
        }
        let be = len.to_be_bytes();
        let skip = be.iter().take_while(|b| **b == 0).count();
        let mut out = vec![base + 55 + (8 - skip) as u8];
        out.extend_from_slice(&be[skip..]);
        out
    }

    fn rlp_bytes(data: &[u8]) -> Vec<u8> {
        if data.len() == 1 && data[0] < 0x80 {
            return data.to_vec();
        }
        let mut out = rlp_header(0x80, data.len());
        out.extend_from_slice(data);
        out
    }

    fn rlp_uint(value: u64) -> Vec<u8> {
        let be = value.to_be_bytes();
        let skip = be.iter().take_while(|b| **b == 0).count();
        rlp_bytes(&be[skip..])
    }

    fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
        let body = items.concat();
        let mut out = rlp_header(0xc0, body.len());
        out.extend_from_slice(&body);
        out
    }

    /// A signed-looking envelope; the audit never checks signatures.
    fn envelope(
        tx_type: Option<u8>,
        to: Option<[u8; 20]>,
        data: &[u8],
        blob_wrapper: bool,
    ) -> Vec<u8> {
        let mut fields = Vec::new();
        if tx_type.is_some() {
            fields.push(rlp_uint(56)); // chain id
        }
        fields.push(rlp_uint(7)); // nonce
        match tx_type {
            None | Some(1) => fields.push(rlp_uint(1_000_000_000)), // gas price
            Some(_) => {
                fields.push(rlp_uint(1)); // max priority fee
                fields.push(rlp_uint(3)); // max fee
            }
        }
        fields.push(rlp_uint(21_000)); // gas
        fields.push(rlp_bytes(to.as_ref().map_or(&[][..], |a| &a[..])));
        fields.push(rlp_uint(0)); // value
        fields.push(rlp_bytes(data));
        if tx_type.is_some() {
            fields.push(rlp_list(&[])); // access list
        }
        match tx_type {
            Some(3) => {
                fields.push(rlp_uint(1)); // max fee per blob gas
                fields.push(rlp_list(&[rlp_bytes(&[0x01; 32])]));
            }
            Some(4) => fields.push(rlp_list(&[])), // authorization list
            _ => {}
        }
        fields.push(rlp_uint(1));
        fields.push(rlp_bytes(&[0x11; 32]));
        fields.push(rlp_bytes(&[0x22; 32]));
        let mut body = rlp_list(&fields);
        if blob_wrapper {
            body = rlp_list(&[
                body,
                rlp_list(&[rlp_bytes(&[0x33; 64])]),
                rlp_list(&[rlp_bytes(&[0x44; 48])]),
                rlp_list(&[rlp_bytes(&[0x55; 48])]),
            ]);
        }
        let mut out = Vec::new();
        if let Some(tx_type) = tx_type {
            out.push(tx_type);
        }
        out.extend_from_slice(&body);
        out
    }

    fn launch_tx(tx_type: Option<u8>) -> Vec<u8> {
        envelope(tx_type, Some(PORTAL), &unhex(DOGE), false)
    }

    /// A valid EARLY_TX_SEEN with `T_ingress = ingress_ns` (at least 2).
    fn peer_record(hash: u8, ingress_ns: u64, payload: &[u8]) -> OwnedRecordV2 {
        OwnedRecordV2 {
            sequence: 0,
            lane_id: C14_FIRST_PEER_LANE_ID,
            lane_epoch: 2,
            kind: RecordKind::EarlyTxSeen,
            flags: 0,
            timestamp_ns: ingress_ns,
            clock_id: CLOCK_ID_MONOTONIC,
            source: SOURCE_ETH_BROADCAST,
            peer: [0x22; 32],
            hash: [hash; 32],
            aux: ingress_aux(ingress_ns - 1),
            payload: payload.to_vec().into_boxed_slice(),
        }
    }

    /// A valid TXPOOL_ACCEPTED observed at `timestamp_ns`.
    fn txpool_record(hash: u8, timestamp_ns: u64, payload: &[u8]) -> OwnedRecordV2 {
        OwnedRecordV2 {
            lane_id: C14_TXPOOL_LANE_ID,
            lane_epoch: 1,
            kind: RecordKind::TxpoolAccepted,
            timestamp_ns,
            source: SOURCE_TXPOOL,
            peer: [0; 32],
            aux: [0; 32],
            ..peer_record(hash, 2, payload)
        }
    }

    /// A sealed (shutdown) BOUND Txpool lane and producer counters with `n`
    /// records published, read and judged, and nothing refused.
    fn drained(n: u64) -> (LaneStatsV2, TxpoolProducerCountersV2) {
        let lane = LaneStatsV2 {
            write_sequence: n,
            read_sequence: n,
            shutdown: true,
            state: LANE_STATE_BOUND,
            ..LaneStatsV2::default()
        };
        let counters = TxpoolProducerCountersV2 {
            published: n,
            ..TxpoolProducerCountersV2::default()
        };
        (lane, counters)
    }

    fn tmp(name: &str) -> PathBuf {
        let id = ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "darkbot-fastlane-c14-{}-{}-{name}",
            std::process::id(),
            id
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn producer(path: &Path, generation: u64, lane: LaneConfigV2) -> ProducerV2 {
        ProducerV2::open(path, generation, lane).unwrap()
    }

    fn consumer(path: &Path, generation: u64, lane: LaneConfigV2) -> ConsumerV2 {
        ConsumerV2::open(path, generation, lane).unwrap()
    }

    fn try_publish(producer: &mut ProducerV2, record: &OwnedRecordV2) -> Result<u64, V2Error> {
        producer.publish(&RecordV2 {
            kind: record.kind,
            flags: 0,
            timestamp_ns: record.timestamp_ns,
            clock_id: record.clock_id,
            source: record.source,
            peer: record.peer,
            hash: record.hash,
            aux: record.aux,
            payload: &record.payload,
        })
    }

    fn publish(producer: &mut ProducerV2, record: &OwnedRecordV2) {
        try_publish(producer, record).unwrap();
    }

    fn drain(consumer: &mut ConsumerV2) -> Vec<OwnedRecordV2> {
        let mut out = Vec::new();
        while let Some(record) = consumer.try_consume().unwrap() {
            out.push(record);
        }
        out
    }

    // -----------------------------------------------------------------
    // Scope, boundaries, envelopes
    // -----------------------------------------------------------------

    #[test]
    fn the_scope_mirror_is_the_golden_committed_launch_scope() {
        assert_eq!(
            hex(&C14_FLAP_SCOPE.canonical_bytes()),
            C14_SCOPE_CANONICAL_HEX
        );
        assert_eq!(C14_FLAP_SCOPE.fingerprint(), C14_SCOPE_FINGERPRINT);
        assert_eq!(C14_FLAP_SCOPE.chain_id, 56);
        assert_eq!(C14_FLAP_SCOPE.addresses, &[PORTAL]);
        assert_eq!(C14_FLAP_SCOPE.selectors, &[(LAUNCH, ROLE_LAUNCH)]);
    }

    #[test]
    fn ingress_aux_and_times_follow_the_c14_wire_semantics() {
        assert_eq!(
            hex(&ingress_aux(0x0102_0304_0506_0708)),
            "0807060504030201000000000000000000000000000000000000000000000000"
        );
        let mut rec = peer_record(1, 2_000, b"x");
        rec.aux = ingress_aux(1_500);
        assert_eq!(
            ingress_times(&rec),
            Ok(IngressTimesV2 {
                packet_decoded_ns: 1_500,
                ingress_ns: 2_000
            })
        );
        rec.aux = ingress_aux(2_000);
        assert!(ingress_times(&rec).is_ok());

        let mut later = rec.clone();
        later.aux = ingress_aux(2_001);
        assert!(matches!(
            ingress_times(&later),
            Err(IngressRecordError::DecodedAfterIngress { .. })
        ));
        let mut missing = rec.clone();
        missing.aux = [0; 32];
        assert_eq!(
            ingress_times(&missing),
            Err(IngressRecordError::MissingPacketDecoded)
        );
        let mut reserved = rec.clone();
        reserved.aux[31] = 1;
        assert_eq!(
            ingress_times(&reserved),
            Err(IngressRecordError::ReservedAux)
        );
        let mut clock = rec.clone();
        clock.clock_id = 2;
        assert_eq!(
            ingress_times(&clock),
            Err(IngressRecordError::ClockDomain(2))
        );
        let txpool = txpool_record(1, 2_000, b"x");
        let expected = IngressRecordError::NotEarlyTxSeen(RecordKind::TxpoolAccepted);
        assert_eq!(ingress_times(&txpool), Err(expected));
    }

    #[test]
    fn every_supported_envelope_yields_destination_and_calldata() {
        let launch = unhex(DOGE);
        for (tx_type, wrapper) in [
            (None, false),
            (Some(1), false),
            (Some(2), false),
            (Some(3), false),
            (Some(3), true),
            (Some(4), false),
        ] {
            let raw = envelope(tx_type, Some(PORTAL), &launch, wrapper);
            let target = decode_tx_target(&raw).unwrap();
            assert_eq!(
                (target.tx_type, target.to, target.data),
                (tx_type, Some(PORTAL), &launch[..]),
                "{tx_type:?} wrapper={wrapper}"
            );
        }
        let creation = envelope(Some(2), None, &launch, false);
        assert_eq!(decode_tx_target(&creation).unwrap().to, None);
        let one_byte = envelope(None, Some(PORTAL), &[0x05], false);
        assert_eq!(decode_tx_target(&one_byte).unwrap().data, &[0x05]);
    }

    #[test]
    fn malformed_and_unsupported_envelopes_are_explicit() {
        let launch = unhex(DOGE);
        assert_eq!(decode_tx_target(&[]), Err(EnvelopeErrorV2::Empty));
        assert_eq!(
            decode_tx_target(&[0x05, 0xc0]),
            Err(EnvelopeErrorV2::UnsupportedType(0x05))
        );
        assert_eq!(
            decode_tx_target(&[0x00, 0xc0]),
            Err(EnvelopeErrorV2::UnsupportedType(0x00))
        );
        assert!(matches!(
            decode_tx_target(&[0x80]),
            Err(EnvelopeErrorV2::Malformed(_))
        ));
        let full = envelope(Some(2), Some(PORTAL), &launch, false);
        assert!(matches!(
            decode_tx_target(&full[..full.len() - 1]),
            Err(EnvelopeErrorV2::Malformed(_))
        ));
        let mut trailing = full;
        trailing.push(0x00);
        assert!(matches!(
            decode_tx_target(&trailing),
            Err(EnvelopeErrorV2::Malformed(_))
        ));
        let short_to = rlp_list(&[
            rlp_uint(0),
            rlp_uint(1),
            rlp_uint(2),
            rlp_bytes(&[0xaa; 19]),
            rlp_uint(0),
            rlp_bytes(&launch),
        ]);
        assert!(matches!(
            decode_tx_target(&short_to),
            Err(EnvelopeErrorV2::Malformed(_))
        ));
    }

    #[test]
    fn the_scope_passes_every_committed_launch_and_filters_buy() {
        for data in [unhex(DOGE), unhex(CREATOR_BUY), unhex(SPLIT_TAX)] {
            for tx_type in [None, Some(2)] {
                let raw = envelope(tx_type, Some(PORTAL), &data, false);
                assert!(C14_FLAP_SCOPE.matches(&decode_tx_target(&raw).unwrap()));
            }
        }
        let mut buy = BUY.to_vec();
        buy.extend_from_slice(&[0u8; 192]);
        let launch = unhex(DOGE);
        for (to, data) in [
            (Some(PORTAL), buy),
            (Some(OTHER), launch.clone()),
            (Some(PORTAL), vec![0xde, 0xad, 0xbe, 0xef]),
            (Some(PORTAL), launch[..3].to_vec()),
            (None, launch.clone()),
        ] {
            for tx_type in [None, Some(2)] {
                let raw = envelope(tx_type, to, &data, false);
                assert!(!C14_FLAP_SCOPE.matches(&decode_tx_target(&raw).unwrap()));
            }
        }
    }

    // -----------------------------------------------------------------
    // False-negative audit
    // -----------------------------------------------------------------

    #[test]
    fn the_audit_judges_every_kind_of_txpool_record() {
        let launch = launch_tx(None);
        let other = envelope(None, Some(OTHER), &unhex(DOGE), false);
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 16);
        assert!(audit.observe_peer_record(&peer_record(1, 90, &launch)));
        assert!(!audit.observe_peer_record(&txpool_record(9, 90, &launch)));
        let verdicts = [
            audit.judge_txpool_record(&txpool_record(1, 100, &launch)),
            audit.judge_txpool_record(&txpool_record(2, 100, &other)),
            audit.judge_txpool_record(&txpool_record(3, 100, &launch)),
            audit.judge_txpool_record(&txpool_record(4, 100, &[0x05, 0xc0])),
            audit.judge_txpool_record(&txpool_record(5, 100, &[0x80])),
            audit.judge_txpool_record(&peer_record(6, 100, &launch)),
        ];
        assert_eq!(
            verdicts,
            [
                AuditVerdictV2::ObservedOnPeerLane,
                AuditVerdictV2::NotRelevant,
                AuditVerdictV2::FalseNegative,
                AuditVerdictV2::UnsupportedEnvelope,
                AuditVerdictV2::MalformedEnvelope,
                AuditVerdictV2::NotTxpoolRecord
            ]
        );
        let stats = audit.stats();
        assert_eq!(
            (stats.relevant, stats.observed, stats.false_negatives),
            (2, 1, 1)
        );
        assert_eq!(
            (stats.unsupported_envelopes, stats.malformed_envelopes),
            (1, 1)
        );
        assert_eq!(stats.foreign_records, 2);
        let evidence = audit.false_negatives();
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            (evidence[0].hash, evidence[0].earliest_peer_ingress_ns),
            ([3; 32], None)
        );
        assert_eq!(evidence[0].selector, LAUNCH);
        assert_eq!(stats.txpool_judged, 6);
        assert_eq!(
            audit.qualification(&LaneStatsV2::default(), None),
            AuditQualificationV2::InvalidEvidence("foreign record kinds")
        );
    }

    #[test]
    fn a_peer_observation_after_the_txpool_acceptance_is_a_false_negative() {
        let launch = launch_tx(Some(2));
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 16);
        // TXPOOL_ACCEPTED(X, t=100), EARLY_TX_SEEN(X, t=101).
        audit.observe_peer_record(&peer_record(1, 101, &launch));
        let late = audit.judge_txpool_record(&txpool_record(1, 100, &launch));
        assert_eq!(late, AuditVerdictV2::FalseNegative);
        // No peer observation at all.
        let none = audit.judge_txpool_record(&txpool_record(2, 100, &launch));
        assert_eq!(none, AuditVerdictV2::FalseNegative);
        let evidence = audit.false_negatives();
        assert_eq!(evidence[0].earliest_peer_ingress_ns, Some(101));
        assert_eq!(evidence[0].txpool_timestamp_ns, 100);
        assert_eq!(evidence[1].earliest_peer_ingress_ns, None);
        assert_eq!(audit.stats().late_peer_observations, 1);
        assert_eq!(
            audit.qualification(&LaneStatsV2::default(), None),
            AuditQualificationV2::FalseNegatives(2)
        );
    }

    #[test]
    fn a_peer_observation_before_or_at_the_txpool_acceptance_is_observed() {
        let launch = launch_tx(None);
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 16);
        // EARLY_TX_SEEN(X, t=99), TXPOOL_ACCEPTED(X, t=100).
        audit.observe_peer_record(&peer_record(1, 99, &launch));
        // EARLY_TX_SEEN(Y, t=100), TXPOOL_ACCEPTED(Y, t=100).
        audit.observe_peer_record(&peer_record(2, 100, &launch));
        let before = audit.judge_txpool_record(&txpool_record(1, 100, &launch));
        let same = audit.judge_txpool_record(&txpool_record(2, 100, &launch));
        assert_eq!(before, AuditVerdictV2::ObservedOnPeerLane);
        assert_eq!(same, AuditVerdictV2::ObservedOnPeerLane);
        let (lane, counters) = drained(2);
        assert_eq!(
            audit.qualification(&lane, Some(&counters)),
            AuditQualificationV2::Pass
        );
    }

    #[test]
    fn the_earliest_peer_observation_wins() {
        let launch = launch_tx(Some(2));
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 16);
        // Later first, earlier second: the earlier one counts.
        audit.observe_peer_record(&peer_record(1, 150, &launch));
        audit.observe_peer_record(&peer_record(1, 95, &launch));
        // Earlier first, later second: the earlier one stays.
        audit.observe_peer_record(&peer_record(2, 95, &launch));
        audit.observe_peer_record(&peer_record(2, 150, &launch));
        // Only observations after the acceptance.
        audit.observe_peer_record(&peer_record(3, 120, &launch));
        audit.observe_peer_record(&peer_record(3, 130, &launch));
        let mut judge = |hash| audit.judge_txpool_record(&txpool_record(hash, 100, &launch));
        let verdicts = [judge(1), judge(2), judge(3)];
        assert_eq!(
            verdicts,
            [
                AuditVerdictV2::ObservedOnPeerLane,
                AuditVerdictV2::ObservedOnPeerLane,
                AuditVerdictV2::FalseNegative
            ]
        );
        let evidence = audit.false_negatives();
        assert_eq!(evidence[0].earliest_peer_ingress_ns, Some(120));
        assert_eq!(audit.stats().peer_records, 6);
    }

    #[test]
    fn records_outside_the_c14_contract_are_invalid_evidence() {
        let launch = launch_tx(None);
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 16);
        let mut wrong_clock = peer_record(1, 90, &launch);
        wrong_clock.clock_id = 2;
        let mut wrong_source = peer_record(1, 90, &launch);
        wrong_source.source = SOURCE_TXPOOL;
        let mut reserved_aux = peer_record(1, 90, &launch);
        reserved_aux.aux[31] = 1;
        for record in [&wrong_clock, &wrong_source, &reserved_aux] {
            assert!(!audit.observe_peer_record(record));
        }
        // None of them makes the transaction observed.
        let unseen = audit.judge_txpool_record(&txpool_record(1, 100, &launch));
        assert_eq!(unseen, AuditVerdictV2::FalseNegative);
        let mut txpool_clock = txpool_record(2, 100, &launch);
        txpool_clock.clock_id = 0;
        let mut txpool_source = txpool_record(3, 100, &launch);
        txpool_source.source = SOURCE_ETH_POOLED;
        let mut txpool_peer = txpool_record(4, 100, &launch);
        txpool_peer.peer[0] = 1;
        let mut txpool_aux = txpool_record(5, 100, &launch);
        txpool_aux.aux[31] = 1;
        for record in [&txpool_clock, &txpool_source, &txpool_peer, &txpool_aux] {
            let verdict = audit.judge_txpool_record(record);
            assert_eq!(verdict, AuditVerdictV2::InvalidRecord);
        }
        let stats = audit.stats();
        assert_eq!(
            (stats.invalid_peer_records, stats.invalid_txpool_records),
            (3, 4)
        );
        assert_eq!(
            audit.qualification(&LaneStatsV2::default(), None),
            AuditQualificationV2::InvalidEvidence("invalid peer records")
        );
    }

    #[test]
    fn the_observation_window_is_bounded() {
        let launch = launch_tx(Some(2));
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 4);
        for tag in 0..11 {
            audit.observe_peer_record(&peer_record(tag, 50, &launch));
        }
        assert_eq!(audit.stats().peer_hashes_evicted, 7);
    }

    // -----------------------------------------------------------------
    // Fail-closed qualification
    // -----------------------------------------------------------------

    /// Completely closed and drained evidence: one launch observed on a peer
    /// lane before the txpool accepted it; a sealed BOUND Txpool lane; every
    /// txpool record published, read and judged; no producer, lane or record
    /// defect.
    struct Closed {
        audit: IngressAuditV2,
        lane: LaneStatsV2,
        counters: TxpoolProducerCountersV2,
    }

    impl Closed {
        fn new() -> Self {
            let launch = launch_tx(Some(2));
            let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 16);
            audit.observe_peer_record(&peer_record(1, 50, &launch));
            audit.judge_txpool_record(&txpool_record(1, 60, &launch));
            let (lane, counters) = drained(1);
            Self {
                audit,
                lane,
                counters,
            }
        }

        /// Judges one more txpool record and keeps lane and producer in
        /// step with it, so only the record itself can be the defect.
        fn judge(&mut self, record: &OwnedRecordV2) {
            self.audit.judge_txpool_record(record);
            let (lane, counters) = drained(self.audit.stats().txpool_judged);
            self.lane = lane;
            self.counters = counters;
        }

        fn qualification(&self) -> AuditQualificationV2 {
            self.audit.qualification(&self.lane, Some(&self.counters))
        }
    }

    /// `defect` changes one condition of closed evidence; the result must be
    /// exactly `expected`, never a pass.
    fn assert_defect(expected: AuditQualificationV2, defect: impl FnOnce(&mut Closed)) {
        let mut closed = Closed::new();
        assert_eq!(closed.qualification(), AuditQualificationV2::Pass);
        defect(&mut closed);
        assert_eq!(closed.qualification(), expected);
    }

    #[test]
    fn only_completely_closed_and_drained_evidence_can_yield_pass() {
        let closed = Closed::new();
        let lane = &closed.lane;
        assert!(lane.shutdown);
        assert_eq!(lane.state, LANE_STATE_BOUND);
        assert_eq!(
            (lane.read_sequence, closed.counters.published),
            (lane.write_sequence, lane.write_sequence)
        );
        assert_eq!(closed.audit.stats().txpool_judged, lane.read_sequence);
        assert_eq!(closed.qualification(), AuditQualificationV2::Pass);
        assert_eq!(
            closed.audit.qualification(&closed.lane, None),
            AuditQualificationV2::Incomplete("txpool producer counters unavailable")
        );
    }

    #[test]
    fn an_invalid_peer_record_never_passes() {
        let expected = AuditQualificationV2::InvalidEvidence("invalid peer records");
        assert_defect(expected, |closed| {
            let mut record = peer_record(2, 50, &launch_tx(None));
            record.clock_id = 2;
            closed.audit.observe_peer_record(&record);
        });
    }

    #[test]
    fn a_txpool_record_with_a_nonzero_peer_never_passes() {
        let expected = AuditQualificationV2::InvalidEvidence("invalid txpool records");
        assert_defect(expected, |closed| {
            let mut record = txpool_record(2, 60, &launch_tx(None));
            record.peer[0] = 1;
            closed.judge(&record);
        });
    }

    #[test]
    fn a_txpool_record_with_nonzero_aux_never_passes() {
        let expected = AuditQualificationV2::InvalidEvidence("invalid txpool records");
        assert_defect(expected, |closed| {
            let mut record = txpool_record(2, 60, &launch_tx(None));
            record.aux[0] = 1;
            closed.judge(&record);
        });
    }

    #[test]
    fn a_foreign_record_on_a_peer_lane_never_passes() {
        let expected = AuditQualificationV2::InvalidEvidence("foreign record kinds");
        assert_defect(expected, |closed| {
            let record = txpool_record(2, 60, &launch_tx(None));
            closed.audit.observe_peer_record(&record);
        });
    }

    #[test]
    fn a_foreign_record_on_the_txpool_lane_never_passes() {
        let expected = AuditQualificationV2::InvalidEvidence("foreign record kinds");
        assert_defect(expected, |closed| {
            closed.judge(&peer_record(2, 50, &launch_tx(None)));
        });
    }

    #[test]
    fn a_consumer_error_never_passes() {
        let expected = AuditQualificationV2::InvalidEvidence("lane consumer errors");
        assert_defect(expected, |closed| closed.audit.stats.consumer_errors = 1);
    }

    #[test]
    fn a_corrupt_or_quarantined_txpool_lane_never_passes() {
        let corrupt = AuditQualificationV2::InvalidEvidence("txpool lane corrupt records");
        assert_defect(corrupt, |closed| closed.lane.corrupt_records = 1);
        let quarantined = AuditQualificationV2::InvalidEvidence("txpool lane quarantined");
        assert_defect(quarantined, |closed| {
            closed.lane.state = crate::protocol_v2::LANE_STATE_QUARANTINED;
        });
    }

    #[test]
    fn a_recorded_txpool_quarantine_reason_never_passes() {
        let expected = AuditQualificationV2::InvalidEvidence("txpool lane quarantine reason");
        assert_defect(expected, |closed| {
            closed.lane.quarantine_reason = crate::protocol_v2::QUARANTINE_OPERATOR;
        });
    }

    #[test]
    fn an_open_txpool_producer_window_never_passes() {
        let expected = AuditQualificationV2::Incomplete("txpool producer window still open");
        assert_defect(expected, |closed| closed.lane.shutdown = false);
    }

    #[test]
    fn a_sealed_txpool_lane_that_is_not_bound_never_passes() {
        let expected = AuditQualificationV2::InvalidEvidence("txpool lane not bound");
        assert_defect(expected, |closed| {
            closed.lane.state = crate::protocol_v2::LANE_STATE_FREE;
        });
        assert_defect(expected, |closed| closed.lane.state = LANE_STATE_CLOSING);
    }

    #[test]
    fn a_false_negative_never_passes() {
        let expected = AuditQualificationV2::FalseNegatives(1);
        assert_defect(expected, |closed| {
            closed.judge(&txpool_record(2, 60, &launch_tx(None)));
        });
    }

    #[test]
    fn missing_txpool_producer_counters_never_pass() {
        let closed = Closed::new();
        assert_eq!(
            closed.audit.qualification(&closed.lane, None),
            AuditQualificationV2::Incomplete("txpool producer counters unavailable")
        );
    }

    #[test]
    fn txpool_producer_refusals_never_pass() {
        let expected = AuditQualificationV2::Incomplete("txpool producer refused records");
        assert_defect(expected, |closed| closed.counters.refused = 1);
    }

    #[test]
    fn txpool_producer_serialize_errors_never_pass() {
        let expected = AuditQualificationV2::Incomplete("txpool producer serialize errors");
        assert_defect(expected, |closed| closed.counters.serialize_errors = 1);
    }

    #[test]
    fn txpool_producer_clock_errors_never_pass() {
        let expected = AuditQualificationV2::Incomplete("txpool producer clock errors");
        assert_defect(expected, |closed| closed.counters.clock_errors = 1);
    }

    #[test]
    fn txpool_lane_drops_and_oversize_refusals_never_pass() {
        let dropped = AuditQualificationV2::Incomplete("txpool lane dropped records");
        assert_defect(dropped, |closed| closed.lane.dropped_full = 1);
        let oversize = AuditQualificationV2::Incomplete("txpool lane refused oversize records");
        assert_defect(oversize, |closed| closed.lane.oversize = 1);
    }

    #[test]
    fn an_unread_txpool_record_never_passes() {
        let expected = AuditQualificationV2::Incomplete("txpool lane not drained");
        assert_defect(expected, |closed| {
            closed.lane.write_sequence += 1;
            closed.counters.published += 1;
        });
    }

    #[test]
    fn producer_counters_that_disagree_with_the_lane_never_pass() {
        let expected = AuditQualificationV2::Incomplete("txpool producer and lane disagree");
        assert_defect(expected, |closed| closed.counters.published += 1);
    }

    #[test]
    fn txpool_records_read_but_not_judged_never_pass() {
        let expected = AuditQualificationV2::Incomplete("txpool records read but not judged");
        assert_defect(expected, |closed| {
            closed.lane.write_sequence += 1;
            closed.lane.read_sequence += 1;
            closed.counters.published += 1;
        });
    }

    #[test]
    fn undecodable_txpool_records_never_pass() {
        let expected = AuditQualificationV2::Incomplete("undecodable txpool records");
        assert_defect(expected, |closed| {
            closed.judge(&txpool_record(2, 60, &[0x05, 0xc0]));
        });
        assert_defect(expected, |closed| {
            closed.judge(&txpool_record(2, 60, &[0x80]));
        });
    }

    const POLL_GENERATION: u64 = 1405;

    /// An arena with the C1.4 lane plan for two peer lanes.
    fn poll_arena(name: &str) -> (PathBuf, PathBuf, C14LanePlan, ArenaV2) {
        let dir = tmp(name);
        let plan = C14LanePlan {
            peer_lanes: 2,
            peer_lane_capacity: 8,
            peer_lane_slot_stride: 4096,
            txpool_lane_capacity: 8,
            txpool_lane_slot_stride: 4096,
        };
        let path = arena_path_v2(&dir, POLL_GENERATION);
        let arena = ArenaV2::create(&path, POLL_GENERATION, &plan.lanes()).unwrap();
        (dir, path, plan, arena)
    }

    #[test]
    fn a_txpool_backlog_left_by_poll_is_incomplete_until_drained() {
        let (dir, path, plan, mut arena) = poll_arena("audit-backlog");
        let generation = POLL_GENERATION;
        arena.bind_peer_lane(16, [0x16; 32]).unwrap();
        let launch = launch_tx(None);
        let mut peer = producer(&path, generation, plan.peer_lane(16));
        let mut pool = producer(&path, generation, plan.txpool_lane());
        publish(&mut peer, &peer_record(1, 9, &launch));
        publish(&mut peer, &peer_record(2, 9, &launch));
        publish(&mut pool, &txpool_record(1, 10, &launch));
        publish(&mut pool, &txpool_record(2, 10, &launch));

        let mut peers = [consumer(&path, generation, plan.peer_lane(16))];
        let mut txpool = consumer(&path, generation, plan.txpool_lane());
        let (_, counters) = drained(2);
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 64);
        // The producer window is sealed; the unread record is the only defect.
        arena.begin_shutdown().unwrap();
        // One valid record judged, one TXPOOL_ACCEPTED still unread.
        let first = audit.poll(&mut peers, &mut txpool, 1).unwrap();
        assert_eq!(first, [AuditVerdictV2::ObservedOnPeerLane]);
        assert_eq!(
            audit.qualification(&txpool.stats(), Some(&counters)),
            AuditQualificationV2::Incomplete("txpool lane not drained")
        );
        let second = audit.poll(&mut peers, &mut txpool, 1).unwrap();
        assert_eq!(second, [AuditVerdictV2::ObservedOnPeerLane]);
        assert_eq!(
            audit.qualification(&txpool.stats(), Some(&counters)),
            AuditQualificationV2::Pass
        );
        drop((peers, txpool, peer, pool, arena));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_shutdown_closes_the_txpool_observation_window() {
        let (dir, path, plan, mut arena) = poll_arena("audit-window");
        let generation = POLL_GENERATION;
        arena.bind_peer_lane(16, [0x16; 32]).unwrap();
        let launch = launch_tx(None);
        let mut peer = producer(&path, generation, plan.peer_lane(16));
        let mut pool = producer(&path, generation, plan.txpool_lane());
        publish(&mut peer, &peer_record(1, 9, &launch));
        publish(&mut pool, &txpool_record(1, 10, &launch));

        let mut peers = [consumer(&path, generation, plan.peer_lane(16))];
        let mut txpool = consumer(&path, generation, plan.txpool_lane());
        let (_, counters) = drained(1);
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 64);
        audit.poll(&mut peers, &mut txpool, 16).unwrap();
        // Drained and consistent, but the producer could still publish.
        let open = txpool.stats();
        assert!(!open.shutdown);
        assert_eq!(open.read_sequence, open.write_sequence);
        assert_eq!(
            audit.qualification(&open, Some(&counters)),
            AuditQualificationV2::Incomplete("txpool producer window still open")
        );
        // Shutdown seals the lane: a later publish is refused.
        arena.begin_shutdown().unwrap();
        let refused = try_publish(&mut pool, &txpool_record(2, 11, &launch));
        assert!(matches!(refused, Err(V2Error::Shutdown)));
        let sealed = txpool.stats();
        assert!(sealed.shutdown);
        assert_eq!(sealed.state, LANE_STATE_BOUND);
        assert_eq!(sealed.write_sequence, 1);
        assert_eq!(
            audit.qualification(&sealed, Some(&counters)),
            AuditQualificationV2::Pass
        );
        drop((peers, txpool, peer, pool, arena));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn poll_reads_a_sealed_lane_to_its_end_without_error() {
        let (dir, path, plan, mut arena) = poll_arena("audit-sealed-records");
        let generation = POLL_GENERATION;
        arena.bind_peer_lane(16, [0x16; 32]).unwrap();
        let launch = launch_tx(None);
        let mut peer = producer(&path, generation, plan.peer_lane(16));
        let mut pool = producer(&path, generation, plan.txpool_lane());
        for hash in 1..=3 {
            publish(&mut peer, &peer_record(hash, 9, &launch));
            publish(&mut pool, &txpool_record(hash, 10, &launch));
        }
        let mut peers = [
            consumer(&path, generation, plan.peer_lane(16)),
            consumer(&path, generation, plan.peer_lane(17)),
        ];
        let mut txpool = consumer(&path, generation, plan.txpool_lane());
        arena.begin_shutdown().unwrap();

        // Records written before shutdown are all read; each drained sealed
        // lane, and the empty sealed lane 17, then ends the poll normally.
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 64);
        let verdicts = audit.poll(&mut peers, &mut txpool, 16).unwrap();
        assert_eq!(verdicts, [AuditVerdictV2::ObservedOnPeerLane; 3]);
        assert_eq!(audit.stats().peer_records, 3);
        assert_eq!(audit.stats().consumer_errors, 0);
        assert!(audit.poll(&mut peers, &mut txpool, 16).unwrap().is_empty());
        assert_eq!(audit.stats().consumer_errors, 0);
        let (_, counters) = drained(3);
        assert_eq!(
            audit.qualification(&txpool.stats(), Some(&counters)),
            AuditQualificationV2::Pass
        );
        drop((peers, txpool, peer, pool, arena));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn poll_on_empty_sealed_lanes_returns_nothing_without_error() {
        let (dir, path, plan, mut arena) = poll_arena("audit-sealed-empty");
        let generation = POLL_GENERATION;
        let mut peers = [consumer(&path, generation, plan.peer_lane(16))];
        let mut txpool = consumer(&path, generation, plan.txpool_lane());
        arena.begin_shutdown().unwrap();
        let lane = txpool.stats();
        assert!(lane.shutdown);
        assert_eq!((lane.read_sequence, lane.write_sequence), (0, 0));
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 64);
        let verdicts = audit.poll(&mut peers, &mut txpool, 16).unwrap();
        assert!(verdicts.is_empty());
        assert_eq!(audit.stats().consumer_errors, 0);
        drop((peers, txpool, arena));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_consumer_error_during_poll_is_invalid_evidence() {
        let (dir, path, plan, mut arena) = poll_arena("audit-consumer-error");
        let generation = POLL_GENERATION;
        let mut peers = [consumer(&path, generation, plan.peer_lane(17))];
        let mut txpool = consumer(&path, generation, plan.txpool_lane());
        arena.quarantine_lane(17).unwrap();
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 64);
        // Quarantine also sets shutdown; it stays an error, not an end.
        let polled = audit.poll(&mut peers, &mut txpool, 16);
        assert!(matches!(polled, Err(V2Error::LaneQuarantined { .. })));
        assert_eq!(audit.stats().consumer_errors, 1);
        let counters = TxpoolProducerCountersV2::default();
        assert_eq!(
            audit.qualification(&txpool.stats(), Some(&counters)),
            AuditQualificationV2::InvalidEvidence("lane consumer errors")
        );
        drop((peers, txpool, arena));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn poll_judges_a_peer_record_drained_late_by_its_timestamp() {
        let dir = tmp("audit-poll");
        let generation = 1404;
        let plan = C14LanePlan {
            peer_lanes: 2,
            peer_lane_capacity: 8,
            peer_lane_slot_stride: 4096,
            txpool_lane_capacity: 8,
            txpool_lane_slot_stride: 4096,
        };
        let path = arena_path_v2(&dir, generation);
        let mut arena = ArenaV2::create(&path, generation, &plan.lanes()).unwrap();
        arena.bind_peer_lane(16, [0x16; 32]).unwrap();
        arena.bind_peer_lane(17, [0x17; 32]).unwrap();
        let launch = launch_tx(None);
        let mut peer16 = producer(&path, generation, plan.peer_lane(16));
        let mut peer17 = producer(&path, generation, plan.peer_lane(17));
        let mut txpool_producer = producer(&path, generation, plan.txpool_lane());
        // X1: peer at 9, txpool at 10. X2: txpool only. X3: txpool at 10,
        // peer only at 11. Every record is in its lane before the poll.
        publish(&mut peer17, &peer_record(1, 9, &launch));
        publish(&mut txpool_producer, &txpool_record(1, 10, &launch));
        publish(&mut txpool_producer, &txpool_record(2, 10, &launch));
        publish(&mut txpool_producer, &txpool_record(3, 10, &launch));
        publish(&mut peer16, &peer_record(3, 11, &launch));

        let mut peers = [
            consumer(&path, generation, plan.peer_lane(16)),
            consumer(&path, generation, plan.peer_lane(17)),
        ];
        let mut txpool = consumer(&path, generation, plan.txpool_lane());
        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 64);
        let verdicts = audit.poll(&mut peers, &mut txpool, 16).unwrap();
        assert_eq!(
            verdicts,
            [
                AuditVerdictV2::ObservedOnPeerLane,
                AuditVerdictV2::FalseNegative,
                AuditVerdictV2::FalseNegative
            ]
        );
        assert_eq!(
            audit.false_negatives()[1].earliest_peer_ingress_ns,
            Some(11)
        );
        assert_eq!(
            audit.qualification(&txpool.stats(), Some(&drained(3).1)),
            AuditQualificationV2::FalseNegatives(2)
        );
        drop((peers, txpool, peer16, peer17, txpool_producer, arena));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_lane_plan_matches_the_geth_plan() {
        let plan = C14LanePlan::production(3);
        let lanes = plan.lanes();
        assert_eq!(lanes.len(), 4);
        assert_eq!(lanes[0].lane_id, C14_TXPOOL_LANE_ID);
        assert_eq!(lanes[0].class, LaneClassV2::TxpoolEvidence);
        assert_eq!(
            (lanes[0].capacity, lanes[0].slot_stride),
            (C14_TXPOOL_LANE_CAPACITY, C14_TXPOOL_LANE_SLOT_STRIDE)
        );
        for (i, lane) in lanes[1..].iter().enumerate() {
            assert_eq!(lane.lane_id, C14_FIRST_PEER_LANE_ID + i as u16);
            assert_eq!(lane.class, LaneClassV2::PeerIngress);
            assert_eq!(
                (lane.capacity, lane.slot_stride),
                (C14_PEER_LANE_CAPACITY, C14_PEER_LANE_SLOT_STRIDE)
            );
            lane.validate().unwrap();
        }
        assert_eq!(SOURCE_ETH_BROADCAST[..13], *b"eth-broadcast");
        assert_eq!(SOURCE_ETH_POOLED[..10], *b"eth-pooled");
        assert_eq!(SOURCE_TXPOOL[..6], *b"txpool");
    }

    // -----------------------------------------------------------------
    // C1.4 Go<->Rust ingress interop, run by
    // bsc/native/tools/c14_ingress_interop.sh
    // -----------------------------------------------------------------

    const INTEROP_ENV: &str = "DARKBOT_C14_INTEROP_DIR";

    struct Interop {
        dir: PathBuf,
        manifest: HashMap<String, String>,
    }

    impl Interop {
        fn load() -> Option<Self> {
            let dir = PathBuf::from(std::env::var_os(INTEROP_ENV)?);
            let manifest_path = dir.join("c14-interop.manifest");
            let text = fs::read_to_string(manifest_path).unwrap();
            let manifest = text
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            Some(Self { dir, manifest })
        }

        fn get(&self, key: &str) -> &str {
            self.manifest
                .get(key)
                .unwrap_or_else(|| panic!("manifest has no {key}"))
        }

        fn num<T: std::str::FromStr>(&self, key: &str) -> T
        where
            T::Err: std::fmt::Debug,
        {
            self.get(key).parse().unwrap()
        }

        fn bytes32(&self, key: &str) -> [u8; 32] {
            let mut out = [0u8; 32];
            out.copy_from_slice(&unhex(self.get(key)));
            out
        }

        fn plan(&self) -> C14LanePlan {
            C14LanePlan {
                peer_lanes: self.num("peer_lanes"),
                peer_lane_capacity: self.num("peer_lane_capacity"),
                peer_lane_slot_stride: self.num("peer_lane_slot_stride"),
                txpool_lane_capacity: self.num("txpool_lane_capacity"),
                txpool_lane_slot_stride: self.num("txpool_lane_slot_stride"),
            }
        }

        fn path(&self) -> PathBuf {
            arena_path_v2(&self.dir, self.num("generation"))
        }

        fn producer_counters(&self) -> TxpoolProducerCountersV2 {
            TxpoolProducerCountersV2 {
                published: self.num("txpool_published"),
                refused: self.num("txpool_refused"),
                serialize_errors: self.num("txpool_serialize_errors"),
                clock_errors: self.num("txpool_clock_errors"),
            }
        }
    }

    fn check_peer_record(record: &OwnedRecordV2, source: [u8; 16], peer: [u8; 32], hash: [u8; 32]) {
        assert_eq!(record.kind, RecordKind::EarlyTxSeen);
        assert_eq!(
            (record.source, record.peer, record.hash),
            (source, peer, hash)
        );
        let times = ingress_times(record).unwrap();
        assert!(times.packet_decoded_ns <= times.ingress_ns);
        let target = decode_tx_target(&record.payload).unwrap();
        assert!(C14_FLAP_SCOPE.matches(&target));
        assert_eq!(target.data[..4], LAUNCH);
    }

    /// Phase 1: read what geth published, run the audit with its two
    /// planted false negatives, drain and acknowledge peer A's closed lane.
    #[test]
    #[ignore = "C1.4 Go<->Rust ingress interop phase; run by bsc/native/tools/c14_ingress_interop.sh"]
    fn interop_c14_rust_phase_1() {
        let Some(io) = Interop::load() else {
            return;
        };
        let generation: u64 = io.num("generation");
        let plan = io.plan();
        let path = io.path();
        let arena = ArenaV2::open(&path, generation).unwrap();
        let lane_a: u16 = io.num("lane_a");
        let lane_b: u16 = io.num("lane_b");
        let node_a = io.bytes32("node_a");
        let node_b = io.bytes32("node_b");
        let epoch_a: u64 = io.num("epoch_a");
        let s = arena.lane_stats(lane_a).unwrap();
        assert_eq!(
            (s.state, s.owner, s.lane_epoch, s.final_write_sequence),
            (LANE_STATE_CLOSING, node_a, epoch_a, 1)
        );
        assert_eq!(s.write_sequence, 1);
        let s = arena.lane_stats(lane_b).unwrap();
        assert_eq!(
            (s.state, s.owner, s.write_sequence),
            (LANE_STATE_BOUND, node_b, 2)
        );

        let mut consumer_a = consumer(&path, generation, plan.peer_lane(lane_a));
        let mut consumer_b = consumer(&path, generation, plan.peer_lane(lane_b));
        let mut txpool = consumer(&path, generation, plan.txpool_lane());
        let a = drain(&mut consumer_a);
        let b = drain(&mut consumer_b);
        assert_eq!((a.len(), b.len()), (1, 2));
        let broadcast = SOURCE_ETH_BROADCAST;
        let pooled = SOURCE_ETH_POOLED;
        check_peer_record(&a[0], broadcast, node_a, io.bytes32("hash_launch1"));
        check_peer_record(&b[0], pooled, node_b, io.bytes32("hash_launch2"));
        check_peer_record(&b[1], pooled, node_b, io.bytes32("hash_late"));

        let mut audit = IngressAuditV2::new(C14_FLAP_SCOPE, 1024);
        for record in a.iter().chain(b.iter()) {
            assert!(audit.observe_peer_record(record));
        }
        let pool = drain(&mut txpool);
        assert_eq!(pool.len(), 6);
        for record in &pool {
            assert_eq!(record.kind, RecordKind::TxpoolAccepted);
            assert_eq!(
                (record.source, record.peer, record.clock_id),
                (SOURCE_TXPOOL, [0; 32], CLOCK_ID_MONOTONIC)
            );
        }
        // Peer lanes and the txpool lane carry the same canonical bytes.
        assert_eq!(a[0].payload, pool[0].payload);
        assert_eq!(b[0].payload, pool[3].payload);
        assert_eq!(b[1].payload, pool[5].payload);
        let verdicts: Vec<_> = pool.iter().map(|r| audit.judge_txpool_record(r)).collect();
        assert_eq!(
            verdicts,
            [
                AuditVerdictV2::ObservedOnPeerLane,
                AuditVerdictV2::NotRelevant,
                AuditVerdictV2::NotRelevant,
                AuditVerdictV2::ObservedOnPeerLane,
                AuditVerdictV2::FalseNegative,
                AuditVerdictV2::FalseNegative
            ]
        );
        let evidence = audit.false_negatives();
        assert_eq!(evidence.len(), 2);
        assert_eq!(evidence[0].hash, io.bytes32("hash_launch3"));
        assert_eq!(evidence[0].earliest_peer_ingress_ns, None);
        assert_eq!(evidence[1].hash, io.bytes32("hash_late"));
        let late = evidence[1].earliest_peer_ingress_ns.unwrap();
        assert!(late > evidence[1].txpool_timestamp_ns);
        // geth's txpool producer counters, read after its last publish.
        let counters = io.producer_counters();
        assert_eq!(counters, drained(6).1);
        let lane = txpool.stats();
        assert_eq!((lane.write_sequence, lane.read_sequence), (6, 6));
        assert_eq!(
            audit.qualification(&lane, Some(&counters)),
            AuditQualificationV2::FalseNegatives(2)
        );
        assert!(consumer_a.acknowledge_drained_if_closing().unwrap());
        println!("\nC14_INTEROP_RUST_PHASE1_OK");
    }

    /// Phase 2: geth released A's lane and bound it to C with epoch + 1;
    /// only C's record follows, under the new epoch.
    #[test]
    #[ignore = "C1.4 Go<->Rust ingress interop phase; run by bsc/native/tools/c14_ingress_interop.sh"]
    fn interop_c14_rust_phase_2() {
        let Some(io) = Interop::load() else {
            return;
        };
        let generation: u64 = io.num("generation");
        let plan = io.plan();
        let path = io.path();
        let arena = ArenaV2::open(&path, generation).unwrap();
        let lane_a: u16 = io.num("lane_a");
        let node_c = io.bytes32("node_c");
        let epoch_c: u64 = io.num("epoch_c");
        let s = arena.lane_stats(lane_a).unwrap();
        assert_eq!(
            (s.state, s.owner, s.lane_epoch),
            (LANE_STATE_BOUND, node_c, epoch_c)
        );
        assert_eq!((s.write_sequence, s.read_sequence), (2, 1));
        let mut lane = consumer(&path, generation, plan.peer_lane(lane_a));
        let records = drain(&mut lane);
        assert_eq!(records.len(), 1);
        assert_eq!((records[0].sequence, records[0].lane_epoch), (1, epoch_c));
        let pooled = SOURCE_ETH_POOLED;
        let hash = io.bytes32("hash_launch3");
        check_peer_record(&records[0], pooled, node_c, hash);
        println!("\nC14_INTEROP_RUST_PHASE2_OK");
    }
}
