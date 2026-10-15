// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// Lane description (direction, class, configuration), lane statistics and
// the lane set used for multi-lane receive (C1.5).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum DirectionV2 {
    GethToRust = 1,
    RustToGeth = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum LaneClassV2 {
    PeerIngress = 1,
    TxpoolEvidence = 2,
    ChainEvidence = 3,
    OutboundHandoffEvidence = 4,
    SignedTransactionEgress = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneConfigV2 {
    pub lane_id: u16,
    pub direction: DirectionV2,
    pub class: LaneClassV2,
    pub flags: u16,
    pub capacity: usize,
    pub slot_stride: usize,
}

impl LaneConfigV2 {
    pub fn validate(self) -> Result<(), V2Error> {
        if self.capacity == 0 || self.capacity > MAX_CAPACITY_V2 {
            return Err(V2Error::InvalidCapacity(self.capacity));
        }
        if self.slot_stride < MIN_SLOT_STRIDE_V2
            || self.slot_stride > MAX_SLOT_STRIDE_V2
            || !self.slot_stride.is_multiple_of(ALIGN_V2)
            || self.slot_stride <= SLOT_HEADER_SIZE_V2
        {
            return Err(V2Error::InvalidSlotStride(self.slot_stride));
        }
        let _ = lane_bytes(self)?;
        Ok(())
    }

    pub fn max_payload(self) -> usize {
        self.slot_stride - SLOT_HEADER_SIZE_V2
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneStatsV2 {
    pub write_sequence: u64,
    pub read_sequence: u64,
    pub dropped_full: u64,
    pub oversize: u64,
    pub corrupt_records: u64,
    pub lane_epoch: u64,
    pub generation: u64,
    pub shutdown: bool,
    pub state: u32,
    pub final_write_sequence: u64,
    pub drained_epoch: u64,
    pub owner: [u8; 32],
    pub quarantine_reason: u64,
}

/// Several lanes of one arena behind a single mapping (C1.5).
///
/// HOT0 polls every peer lane; one mapping per lane would map the arena
/// hundreds of times. A lane set maps it once. `try_acquire` borrows the
/// whole set exclusively, so at most one record is held at a time, which is
/// HOT0's single-owner rule. Lane indices are positions in the slice given to
/// [`LaneSetV2::open`]; an index out of range panics like a slice index.
pub struct LaneSetV2 {
    mapping: MappingV2,
    lanes: Vec<(LaneConfigV2, usize)>,
}

impl LaneSetV2 {
    pub fn open(path: &Path, generation: u64, lanes: &[LaneConfigV2]) -> Result<Self, V2Error> {
        if generation == 0 {
            return Err(V2Error::ZeroGeneration);
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(V2Error::Io)?;
        let len = usize::try_from(file.metadata().map_err(V2Error::Io)?.len())
            .map_err(|_| V2Error::SizeOverflow)?;
        let mapping = MappingV2::map(file, len)?;
        validate_super(&mapping, generation)?;
        let mut located = Vec::with_capacity(lanes.len());
        for lane in lanes {
            lane.validate()?;
            located.push((*lane, locate_lane(&mapping, *lane)?));
        }
        Ok(Self {
            mapping,
            lanes: located,
        })
    }

    pub fn len(&self) -> usize {
        self.lanes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lanes.is_empty()
    }

    pub fn config(&self, index: usize) -> LaneConfigV2 {
        self.lanes[index].0
    }

    /// Validates and holds the next record of lane `index`; see
    /// [`ConsumerV2::try_acquire`].
    pub fn try_acquire(&mut self, index: usize) -> Result<Option<RecordGuardV2<'_>>, V2Error> {
        let (config, lane_offset) = self.lanes[index];
        let Some(slot) = acquire_slot(&self.mapping, config, lane_offset)? else {
            return Ok(None);
        };
        let guard = RecordGuardV2::new(&self.mapping, config, lane_offset, slot);
        Ok(Some(guard))
    }

    /// Whether lane `index` holds an unread record. Two atomic loads; no
    /// validation.
    pub fn pending(&self, index: usize) -> bool {
        let lane_offset = self.lanes[index].1;
        let read = lane_u64(&self.mapping, lane_offset, LH_OFF_READ_SEQUENCE);
        let write = lane_u64(&self.mapping, lane_offset, LH_OFF_WRITE_SEQUENCE);
        read.load(Ordering::Relaxed) != write.load(Ordering::Acquire)
    }

    pub fn lifecycle_state(&self, index: usize) -> u32 {
        lane_state(&self.mapping, self.lanes[index].1)
    }

    /// See [`ConsumerV2::acknowledge_drained_if_closing`].
    pub fn acknowledge_drained_if_closing(&self, index: usize) -> Result<bool, V2Error> {
        lane_acknowledge_drained(&self.mapping, self.lanes[index].1)
    }

    pub fn stats(&self, index: usize) -> LaneStatsV2 {
        lane_stats(&self.mapping, self.lanes[index].1)
    }
}
