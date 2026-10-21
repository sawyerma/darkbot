// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// The single consumer of a V2 lane and slot acquisition.

pub struct ConsumerV2 {
    mapping: MappingV2,
    config: LaneConfigV2,
    lane_offset: usize,
    _claim: LaneClaimV2,
}

impl ConsumerV2 {
    pub fn open(path: &Path, generation: u64, expected: LaneConfigV2) -> Result<Self, V2Error> {
        let (mapping, lane_offset, claim) =
            open_lane(path, generation, expected, LaneRoleV2::Consumer)?;
        Ok(Self {
            mapping,
            config: expected,
            lane_offset,
            _claim: claim,
        })
    }

    /// Validates the next committed record and holds it in its slot (C1.5).
    /// Returns `Ok(None)` if the lane is empty.
    ///
    /// A QUARANTINED lane is not consumed. The first structural defect of a
    /// record (sequence, payload length, lane epoch, CRC32C, record kind, or
    /// an unknown lifecycle state) quarantines the lane, counts exactly one
    /// corruption incident and returns that defect; every later call returns
    /// [`V2Error::LaneQuarantined`] without counting again. A record on a FREE
    /// lane, or at or past `final_write_sequence` on a CLOSING lane, was
    /// written outside its binding: the lane is quarantined as a late write.
    /// `read_sequence` never advances past a refused record, and it advances
    /// past an accepted one only when the returned guard is released.
    pub fn try_acquire(&mut self) -> Result<Option<RecordGuardV2<'_>>, V2Error> {
        let Some(slot) = acquire_slot(&self.mapping, self.config, self.lane_offset)? else {
            return Ok(None);
        };
        let guard = RecordGuardV2::new(&self.mapping, self.config, self.lane_offset, slot);
        Ok(Some(guard))
    }

    /// Consumes the next record into an owned copy, or returns `Ok(None)` if
    /// the lane is empty. The validation is [`ConsumerV2::try_acquire`]'s; the
    /// record is released before it is returned. For audit, capture and
    /// tests: the payload is copied to the heap.
    pub fn try_consume(&mut self) -> Result<Option<OwnedRecordV2>, V2Error> {
        let Some(guard) = self.try_acquire()? else {
            return Ok(None);
        };
        let record = guard.record().to_owned_record();
        guard.release()?;
        Ok(Some(record))
    }

    /// The consumer's half of the release protocol. On a CLOSING lane whose
    /// records up to `final_write_sequence` have all been consumed, stores
    /// `drained_epoch = lane_epoch` with Release and returns `true`. Returns
    /// `false` while the lane is not CLOSING or records remain. A record past
    /// `final_write_sequence` quarantines the lane and fails; a QUARANTINED
    /// lane fails.
    pub fn acknowledge_drained_if_closing(&self) -> Result<bool, V2Error> {
        lane_acknowledge_drained(&self.mapping, self.lane_offset)
    }

    /// Quarantines this lane (reason OPERATOR): terminal for this generation,
    /// idempotent. Returns the previous state.
    pub fn quarantine(&self) -> u32 {
        quarantine_lane_at(&self.mapping, self.lane_offset, QUARANTINE_OPERATOR).0
    }

    /// The lane's lifecycle state.
    pub fn lifecycle_state(&self) -> u32 {
        lane_state(&self.mapping, self.lane_offset)
    }

    /// The recorded quarantine reason, `QUARANTINE_REASON_NONE` if none.
    pub fn quarantine_reason(&self) -> u64 {
        lane_quarantine_reason(&self.mapping, self.lane_offset)
    }

    pub fn stats(&self) -> LaneStatsV2 {
        lane_stats(&self.mapping, self.lane_offset)
    }
}

/// The receive validation shared by every consumer of a lane. It reads
/// `read_sequence`, never writes it.
fn acquire_slot(
    mapping: &MappingV2,
    config: LaneConfigV2,
    lane_offset: usize,
) -> Result<Option<AcquiredSlotV2>, V2Error> {
    let read = lane_u64(mapping, lane_offset, LH_OFF_READ_SEQUENCE).load(Ordering::Relaxed);
    let write = lane_u64(mapping, lane_offset, LH_OFF_WRITE_SEQUENCE).load(Ordering::Acquire);
    // Loaded after write_sequence: a record observed above was published
    // by a producer that saw BOUND, so this load sees BOUND or later.
    let state = lane_state(mapping, lane_offset);
    // The state is validated before the empty check: an undefined state
    // quarantines the lane even when no record is waiting.
    lane_admit_state(mapping, lane_offset, state)?;
    if read == write {
        if lane_u64(mapping, lane_offset, LH_OFF_SHUTDOWN).load(Ordering::Acquire) != 0 {
            return Err(V2Error::Shutdown);
        }
        return Ok(None);
    }
    if state == LANE_STATE_FREE {
        return Err(lane_late_write(mapping, lane_offset, read, write));
    }
    if state == LANE_STATE_CLOSING {
        let final_write =
            lane_u64(mapping, lane_offset, LH_OFF_FINAL_WRITE).load(Ordering::Acquire);
        if read >= final_write {
            return Err(lane_late_write(mapping, lane_offset, final_write, write));
        }
    }

    let index = (read % config.capacity as u64) as usize;
    let slot_offset = lane_offset + LANE_HEADER_SIZE_V2 + index * config.slot_stride;
    let observed = mapping
        .atomic_u64(slot_offset + SLOT_OFF_COMMITTED)
        .load(Ordering::Acquire);
    let expected_sequence = read.wrapping_add(1);
    if observed != expected_sequence {
        return Err(lane_corrupt(
            mapping,
            lane_offset,
            QUARANTINE_CORRUPT_SEQUENCE,
            V2Error::CorruptSequence {
                expected: expected_sequence,
                observed,
            },
        ));
    }

    let bytes = mapping.bytes();
    let slot = &bytes[slot_offset..slot_offset + config.slot_stride];
    let payload_len = get_u32(slot, SLOT_OFF_PAYLOAD_LEN) as usize;
    if payload_len > config.max_payload() {
        return Err(lane_corrupt(
            mapping,
            lane_offset,
            QUARANTINE_PAYLOAD_LENGTH,
            V2Error::PayloadTooLarge {
                len: payload_len,
                max: config.max_payload(),
            },
        ));
    }
    let lane_epoch = get_u64(slot, SLOT_OFF_LANE_EPOCH);
    let expected_epoch = lane_u64(mapping, lane_offset, LH_OFF_LANE_EPOCH).load(Ordering::Acquire);
    if lane_epoch != expected_epoch {
        return Err(lane_corrupt(
            mapping,
            lane_offset,
            QUARANTINE_EPOCH_MISMATCH,
            V2Error::LaneEpochMismatch {
                expected: expected_epoch,
                observed: lane_epoch,
            },
        ));
    }
    let expected_crc = get_u32(slot, SLOT_OFF_CRC32C);
    let actual_crc = record_crc32c(slot, payload_len);
    if actual_crc != expected_crc {
        return Err(lane_corrupt(
            mapping,
            lane_offset,
            QUARANTINE_CHECKSUM,
            V2Error::ChecksumMismatch {
                expected: expected_crc,
                actual: actual_crc,
            },
        ));
    }

    let kind_raw = get_u16(slot, SLOT_OFF_KIND);
    let kind = match RecordKind::try_from(kind_raw) {
        Ok(kind) => kind,
        Err(_) => {
            return Err(lane_corrupt(
                mapping,
                lane_offset,
                QUARANTINE_RECORD_KIND,
                V2Error::UnknownRecordKind(kind_raw),
            ));
        }
    };
    Ok(Some(AcquiredSlotV2 {
        read,
        slot_offset,
        payload_len,
        kind,
        lane_epoch,
    }))
}
