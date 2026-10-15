// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// The single producer of a V2 lane.

pub struct ProducerV2 {
    mapping: MappingV2,
    config: LaneConfigV2,
    lane_offset: usize,
}

impl ProducerV2 {
    pub fn open(path: &Path, generation: u64, expected: LaneConfigV2) -> Result<Self, V2Error> {
        let (mapping, lane_offset) = open_lane(path, generation, expected)?;
        Ok(Self {
            mapping,
            config: expected,
            lane_offset,
        })
    }

    pub fn publish(&mut self, record: &RecordV2<'_>) -> Result<u64, V2Error> {
        let state = self
            .mapping
            .atomic_u32(self.lane_offset + LH_OFF_STATE)
            .load(Ordering::Acquire);
        if state != LANE_STATE_BOUND {
            return Err(V2Error::LifecycleState {
                expected: LANE_STATE_BOUND,
                observed: state,
            });
        }
        if record.payload.len() > self.config.max_payload() {
            self.atomic(LH_OFF_OVERSIZE).fetch_add(1, Ordering::Relaxed);
            return Err(V2Error::PayloadTooLarge {
                len: record.payload.len(),
                max: self.config.max_payload(),
            });
        }
        if self.atomic(LH_OFF_SHUTDOWN).load(Ordering::Acquire) != 0 {
            return Err(V2Error::Shutdown);
        }
        let write = self.atomic(LH_OFF_WRITE_SEQUENCE).load(Ordering::Relaxed);
        let read = self.atomic(LH_OFF_READ_SEQUENCE).load(Ordering::Acquire);
        if write.wrapping_sub(read) >= self.config.capacity as u64 {
            self.atomic(LH_OFF_DROPPED_FULL)
                .fetch_add(1, Ordering::Relaxed);
            return Err(V2Error::QueueFull { sequence: write });
        }
        let lane_epoch = self.atomic(LH_OFF_LANE_EPOCH).load(Ordering::Acquire);
        let index = (write % self.config.capacity as u64) as usize;
        let slot_offset = self.lane_offset + LANE_HEADER_SIZE_V2 + index * self.config.slot_stride;

        self.mapping
            .atomic_u64(slot_offset + SLOT_OFF_COMMITTED)
            .store(EMPTY_SEQUENCE, Ordering::Relaxed);

        {
            let bytes = self.mapping.bytes_mut();
            let slot = &mut bytes[slot_offset..slot_offset + self.config.slot_stride];
            slot[8..SLOT_HEADER_SIZE_V2].fill(0);
            put_u16(slot, SLOT_OFF_KIND, record.kind as u16);
            put_u16(slot, SLOT_OFF_FLAGS, record.flags);
            put_u32(slot, SLOT_OFF_PAYLOAD_LEN, record.payload.len() as u32);
            put_u64(slot, SLOT_OFF_TIMESTAMP, record.timestamp_ns);
            put_u32(slot, SLOT_OFF_CLOCK_ID, record.clock_id);
            slot[SLOT_OFF_SOURCE..SLOT_OFF_SOURCE + SOURCE_LEN].copy_from_slice(&record.source);
            slot[SLOT_OFF_PEER..SLOT_OFF_PEER + PEER_LEN].copy_from_slice(&record.peer);
            slot[SLOT_OFF_HASH..SLOT_OFF_HASH + HASH_LEN].copy_from_slice(&record.hash);
            slot[SLOT_OFF_AUX..SLOT_OFF_AUX + AUX_LEN].copy_from_slice(&record.aux);
            put_u64(slot, SLOT_OFF_LANE_EPOCH, lane_epoch);
            slot[SLOT_HEADER_SIZE_V2..SLOT_HEADER_SIZE_V2 + record.payload.len()]
                .copy_from_slice(record.payload);
            let crc = record_crc32c(slot, record.payload.len());
            put_u32(slot, SLOT_OFF_CRC32C, crc);
        }

        self.mapping
            .atomic_u64(slot_offset + SLOT_OFF_COMMITTED)
            .store(write.wrapping_add(1), Ordering::Release);
        self.atomic(LH_OFF_WRITE_SEQUENCE)
            .store(write.wrapping_add(1), Ordering::Release);
        Ok(write)
    }

    pub fn stats(&self) -> LaneStatsV2 {
        lane_stats(&self.mapping, self.lane_offset)
    }

    fn atomic(&self, rel: usize) -> &AtomicU64 {
        self.mapping.atomic_u64(self.lane_offset + rel)
    }
}
