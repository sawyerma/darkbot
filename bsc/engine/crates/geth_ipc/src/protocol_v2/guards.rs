// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// C1.5 borrowed receive: the slot guard and the borrowed record that cannot
// outlive it.

/// A record borrowed from its mmap slot (C1.5). Every slice points into the
/// lane's slot and lives only as long as the [`RecordGuardV2`] it came from.
/// `source`, `peer`, `hash` and `aux` have their fixed wire lengths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowedRecordV2<'a> {
    pub sequence: u64,
    pub lane_id: u16,
    pub lane_epoch: u64,
    pub kind: RecordKind,
    pub flags: u16,
    pub timestamp_ns: u64,
    pub clock_id: u32,
    pub source: &'a [u8],
    pub peer: &'a [u8],
    pub hash: &'a [u8],
    pub aux: &'a [u8],
    pub payload: &'a [u8],
}

impl BorrowedRecordV2<'_> {
    /// An owned copy with a heap payload. For audit, capture and tests; the
    /// HOT0 receive path never calls it.
    pub fn to_owned_record(&self) -> OwnedRecordV2 {
        let mut source = [0u8; SOURCE_LEN];
        let mut peer = [0u8; PEER_LEN];
        let mut hash = [0u8; HASH_LEN];
        let mut aux = [0u8; AUX_LEN];
        source.copy_from_slice(self.source);
        peer.copy_from_slice(self.peer);
        hash.copy_from_slice(self.hash);
        aux.copy_from_slice(self.aux);
        OwnedRecordV2 {
            sequence: self.sequence,
            lane_id: self.lane_id,
            lane_epoch: self.lane_epoch,
            kind: self.kind,
            flags: self.flags,
            timestamp_ns: self.timestamp_ns,
            clock_id: self.clock_id,
            source,
            peer,
            hash,
            aux,
            payload: self.payload.to_vec().into_boxed_slice(),
        }
    }
}

/// A committed, validated record located in its slot.
struct AcquiredSlotV2 {
    read: u64,
    slot_offset: usize,
    payload_len: usize,
    kind: RecordKind,
    lane_epoch: u64,
}

/// Exclusive hold on one validated record in its slot (C1.5).
///
/// While the guard lives `read_sequence` still names this record, so the
/// producer cannot write the slot. Releasing or dropping the guard advances
/// `read_sequence` with Release; the slot is reusable from then on. Before
/// advancing, and after [`RecordGuardV2::copy_payload`], the guard confirms
/// that the slot still carries the commit it validated: the producer clears
/// `committed_sequence` before it writes a slot, so a slot rewritten against
/// the protocol is detected, the lane is quarantined and `read_sequence` does
/// not advance.
///
/// The guard cannot outlive its consumer and a borrowed record cannot
/// outlive its guard:
///
/// ```compile_fail
/// use darkbot_bsc_geth_ipc::protocol_v2::{BorrowedRecordV2, ConsumerV2};
/// fn escape(consumer: &mut ConsumerV2) -> BorrowedRecordV2<'_> {
///     let guard = consumer.try_acquire().unwrap().unwrap();
///     guard.record()
/// }
/// ```
///
/// ```compile_fail
/// use darkbot_bsc_geth_ipc::protocol_v2::ConsumerV2;
/// fn second_acquire_while_held(consumer: &mut ConsumerV2) {
///     let guard = consumer.try_acquire().unwrap().unwrap();
///     let _other = consumer.try_acquire();
///     drop(guard);
/// }
/// ```
///
/// The same code with the record used inside the guard's life compiles:
///
/// ```no_run
/// use darkbot_bsc_geth_ipc::protocol_v2::ConsumerV2;
/// fn inside(consumer: &mut ConsumerV2) -> usize {
///     let guard = consumer.try_acquire().unwrap().unwrap();
///     let len = guard.record().payload.len();
///     guard.release().unwrap();
///     len
/// }
/// ```
pub struct RecordGuardV2<'lane> {
    mapping: &'lane MappingV2,
    config: LaneConfigV2,
    lane_offset: usize,
    slot: AcquiredSlotV2,
    released: bool,
    _exclusive: PhantomData<&'lane mut ()>,
}

impl<'lane> RecordGuardV2<'lane> {
    fn new(
        mapping: &'lane MappingV2,
        config: LaneConfigV2,
        lane_offset: usize,
        slot: AcquiredSlotV2,
    ) -> Self {
        Self {
            mapping,
            config,
            lane_offset,
            slot,
            released: false,
            _exclusive: PhantomData,
        }
    }

    /// The record, borrowed from the slot for the life of this borrow.
    pub fn record(&self) -> BorrowedRecordV2<'_> {
        let start = self.slot.slot_offset;
        let slot = &self.mapping.bytes()[start..start + self.config.slot_stride];
        let payload_end = SLOT_HEADER_SIZE_V2 + self.slot.payload_len;
        BorrowedRecordV2 {
            sequence: self.slot.read,
            lane_id: self.config.lane_id,
            lane_epoch: self.slot.lane_epoch,
            kind: self.slot.kind,
            flags: get_u16(slot, SLOT_OFF_FLAGS),
            timestamp_ns: get_u64(slot, SLOT_OFF_TIMESTAMP),
            clock_id: get_u32(slot, SLOT_OFF_CLOCK_ID),
            source: &slot[SLOT_OFF_SOURCE..SLOT_OFF_SOURCE + SOURCE_LEN],
            peer: &slot[SLOT_OFF_PEER..SLOT_OFF_PEER + PEER_LEN],
            hash: &slot[SLOT_OFF_HASH..SLOT_OFF_HASH + HASH_LEN],
            aux: &slot[SLOT_OFF_AUX..SLOT_OFF_AUX + AUX_LEN],
            payload: &slot[SLOT_HEADER_SIZE_V2..payload_end],
        }
    }

    /// The lane sequence of the held record.
    pub fn sequence(&self) -> u64 {
        self.slot.read
    }

    /// Whether the slot still carries the validated commit.
    pub fn is_stable(&self) -> bool {
        self.committed() == self.slot.read.wrapping_add(1)
    }

    /// Copies the payload into `out` and then confirms the slot is unchanged,
    /// so the copy is exactly the validated record. Returns the payload
    /// length. Fails without advancing if `out` is too small or the slot was
    /// rewritten; the latter quarantines the lane.
    pub fn copy_payload(&self, out: &mut [u8]) -> Result<usize, V2Error> {
        let len = self.slot.payload_len;
        if out.len() < len {
            return Err(V2Error::PayloadTooLarge {
                len,
                max: out.len(),
            });
        }
        out[..len].copy_from_slice(self.record().payload);
        self.check_stable()?;
        Ok(len)
    }

    /// Releases the record: confirms the slot is unchanged, then advances
    /// `read_sequence` with Release. Returns the released sequence.
    pub fn release(mut self) -> Result<u64, V2Error> {
        self.finish()
    }

    fn committed(&self) -> u64 {
        let offset = self.slot.slot_offset + SLOT_OFF_COMMITTED;
        self.mapping.atomic_u64(offset).load(Ordering::Acquire)
    }

    fn check_stable(&self) -> Result<(), V2Error> {
        let expected = self.slot.read.wrapping_add(1);
        let observed = self.committed();
        if observed == expected {
            return Ok(());
        }
        Err(lane_corrupt(
            self.mapping,
            self.lane_offset,
            QUARANTINE_CORRUPT_SEQUENCE,
            V2Error::CorruptSequence { expected, observed },
        ))
    }

    fn finish(&mut self) -> Result<u64, V2Error> {
        self.released = true;
        self.check_stable()?;
        let next = self.slot.read.wrapping_add(1);
        lane_u64(self.mapping, self.lane_offset, LH_OFF_READ_SEQUENCE)
            .store(next, Ordering::Release);
        Ok(self.slot.read)
    }
}

impl Drop for RecordGuardV2<'_> {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.finish();
        }
    }
}
