// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// The V2 arena: creation/opening, directory lookup, restart probes and stale
// arena discovery.

pub struct ArenaV2 {
    mapping: MappingV2,
}

impl ArenaV2 {
    pub fn create(path: &Path, generation: u64, lanes: &[LaneConfigV2]) -> Result<Self, V2Error> {
        validate_arena_config(generation, lanes)?;
        let layout = layout_for(lanes)?;
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(V2Error::ArenaExists {
                    probe: probe_arena_v2(path, generation)?,
                });
            }
            Err(error) => return Err(V2Error::Io(error)),
        };
        file.set_len(u64::try_from(layout.total).map_err(|_| V2Error::SizeOverflow)?)
            .map_err(V2Error::Io)?;
        let mut mapping = MappingV2::map(file, layout.total)?;
        mapping.bytes_mut().fill(0);

        {
            let bytes = mapping.bytes_mut();
            bytes[SH_OFF_MAGIC..SH_OFF_MAGIC + 8].copy_from_slice(&MAGIC_V2);
            put_u16(bytes, SH_OFF_VERSION, VERSION_V2);
            put_u16(bytes, SH_OFF_ENDIAN, ENDIAN_MARKER_V2);
            put_u32(bytes, SH_OFF_HEADER_SIZE, SUPER_HEADER_SIZE_V2 as u32);
            put_u64(bytes, SH_OFF_GENERATION, generation);
            put_u32(bytes, SH_OFF_LANE_COUNT, lanes.len() as u32);
            put_u32(bytes, SH_OFF_DIR_SIZE, DIRECTORY_ENTRY_SIZE_V2 as u32);
            put_u32(bytes, SH_OFF_LANE_HEADER_SIZE, LANE_HEADER_SIZE_V2 as u32);
            put_u32(bytes, SH_OFF_SLOT_HEADER_SIZE, SLOT_HEADER_SIZE_V2 as u32);
            put_u32(bytes, SH_OFF_ALIGNMENT, ALIGN_V2 as u32);
            put_u32(bytes, SH_OFF_CRC_KIND, CRC_KIND_CRC32C);
            put_u64(bytes, SH_OFF_TOTAL_BYTES, layout.total as u64);

            for (index, lane) in lanes.iter().enumerate() {
                let dir = SUPER_HEADER_SIZE_V2 + index * DIRECTORY_ENTRY_SIZE_V2;
                let lane_offset = layout.lane_offsets[index];
                let lane_len = lane_bytes(*lane)?;
                put_u16(bytes, dir + DE_OFF_LANE_ID, lane.lane_id);
                put_u16(bytes, dir + DE_OFF_DIRECTION, lane.direction as u16);
                put_u16(bytes, dir + DE_OFF_CLASS, lane.class as u16);
                put_u16(bytes, dir + DE_OFF_FLAGS, lane.flags);
                put_u32(bytes, dir + DE_OFF_CAPACITY, lane.capacity as u32);
                put_u32(bytes, dir + DE_OFF_SLOT_STRIDE, lane.slot_stride as u32);
                put_u64(bytes, dir + DE_OFF_LANE_OFFSET, lane_offset as u64);
                put_u64(bytes, dir + DE_OFF_LANE_BYTES, lane_len as u64);

                put_u64(bytes, lane_offset + LH_OFF_WRITE_SEQUENCE, 0);
                put_u64(bytes, lane_offset + LH_OFF_DROPPED_FULL, 0);
                put_u64(bytes, lane_offset + LH_OFF_OVERSIZE, 0);
                put_u64(bytes, lane_offset + LH_OFF_PRODUCER_STATUS, 0);
                put_u64(bytes, lane_offset + LH_OFF_READ_SEQUENCE, 0);
                put_u64(bytes, lane_offset + LH_OFF_CORRUPT, 0);
                put_u64(bytes, lane_offset + LH_OFF_DRAINED_EPOCH, 0);
                put_u64(bytes, lane_offset + LH_OFF_GENERATION, generation);
                put_u64(bytes, lane_offset + LH_OFF_LANE_EPOCH, 1);
                let initial_state = if lane.class == LaneClassV2::PeerIngress {
                    LANE_STATE_FREE
                } else {
                    LANE_STATE_BOUND
                };
                put_u32(bytes, lane_offset + LH_OFF_STATE, initial_state);
                put_u64(bytes, lane_offset + LH_OFF_FINAL_WRITE, 0);
                put_u64(bytes, lane_offset + LH_OFF_SHUTDOWN, 0);
                bytes[lane_offset + LH_OFF_OWNER..lane_offset + LH_OFF_OWNER + 32].fill(0);

                for slot in 0..lane.capacity {
                    let slot_offset = lane_offset + LANE_HEADER_SIZE_V2 + slot * lane.slot_stride;
                    put_u64(bytes, slot_offset + SLOT_OFF_COMMITTED, EMPTY_SEQUENCE);
                }
            }
        }
        mapping.sync()?;
        Ok(Self { mapping })
    }

    pub fn mapped_len(&self) -> usize {
        self.mapping.len
    }

    /// Opens an existing arena for allocation or recovery. Fails closed on a
    /// different generation, a frozen-layout mismatch, a duplicate lane id or a
    /// lane whose range or generation does not match the directory.
    pub fn open(path: &Path, generation: u64) -> Result<Self, V2Error> {
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
        let mut seen = BTreeSet::new();
        for lane_id in directory_lane_ids(&mapping) {
            if !seen.insert(lane_id) {
                return Err(V2Error::DuplicateLaneId(lane_id));
            }
            locate_lane_by_id(&mapping, lane_id)?;
        }
        Ok(Self { mapping })
    }

    /// The arena generation.
    pub fn generation(&self) -> u64 {
        get_u64(self.mapping.bytes(), SH_OFF_GENERATION)
    }

    /// Ids of all peer-ingress lanes, in directory order.
    pub fn peer_lane_ids(&self) -> Result<Vec<u16>, V2Error> {
        let mut ids = Vec::new();
        for lane_id in directory_lane_ids(&self.mapping) {
            let (config, _) = locate_lane_by_id(&self.mapping, lane_id)?;
            if config.class == LaneClassV2::PeerIngress {
                ids.push(lane_id);
            }
        }
        Ok(ids)
    }

    /// Diagnostic snapshot of one lane. Each field is read atomically; the
    /// snapshot as a whole is not.
    pub fn lane_stats(&self, lane_id: u16) -> Result<LaneStatsV2, V2Error> {
        let (_, lane_offset) = locate_lane_by_id(&self.mapping, lane_id)?;
        Ok(lane_stats(&self.mapping, lane_offset))
    }

    /// FREE -> BOUND for a new peer. Refused after shutdown. Increments
    /// `lane_epoch` (fails closed on exhaustion), writes the owner, clears
    /// `final_write_sequence` and publishes BOUND with Release. Returns the
    /// new epoch, which with the owner forms the lease. The previous
    /// binding's `drained_epoch` needs no clearing: it names an older epoch,
    /// which never equals the new one.
    ///
    /// A FREE lane holding unread records breaks the release invariant: it is
    /// quarantined and the bind fails.
    pub fn bind_peer_lane(&mut self, lane_id: u16, owner: [u8; 32]) -> Result<u64, V2Error> {
        let lane_offset = self.peer_lane_offset(lane_id)?;
        let mapping = &self.mapping;
        let observed = lane_state(mapping, lane_offset);
        if observed != LANE_STATE_FREE {
            return Err(V2Error::LifecycleState {
                expected: LANE_STATE_FREE,
                observed,
            });
        }
        // Shutdown is terminal: set by a runtime shutdown or a quarantine,
        // never cleared, so no owner is bound after it began.
        if lane_u64(mapping, lane_offset, LH_OFF_SHUTDOWN).load(Ordering::Acquire) != 0 {
            return Err(V2Error::Shutdown);
        }
        let current_epoch =
            lane_u64(mapping, lane_offset, LH_OFF_LANE_EPOCH).load(Ordering::Acquire);
        let next_epoch = current_epoch
            .checked_add(1)
            .ok_or(V2Error::EpochExhausted)?;
        let write = lane_u64(mapping, lane_offset, LH_OFF_WRITE_SEQUENCE).load(Ordering::Acquire);
        let read = lane_u64(mapping, lane_offset, LH_OFF_READ_SEQUENCE).load(Ordering::Acquire);
        if write != read {
            quarantine_lane_at(mapping, lane_offset, QUARANTINE_LATE_WRITE);
            return Err(V2Error::LateWriteAfterClose {
                final_write: read,
                write,
            });
        }
        store_owner(mapping, lane_offset, &owner);
        lane_u64(mapping, lane_offset, LH_OFF_FINAL_WRITE).store(0, Ordering::Relaxed);
        lane_u64(mapping, lane_offset, LH_OFF_LANE_EPOCH).store(next_epoch, Ordering::Relaxed);
        transition_state(mapping, lane_offset, LANE_STATE_FREE, LANE_STATE_BOUND)?;
        Ok(next_epoch)
    }

    /// BOUND -> CLOSING. The caller is the lane's producer and has published
    /// its last record: `final_write_sequence` is frozen at the current
    /// `write_sequence` before CLOSING is published with Release. Requires the
    /// lease's owner and epoch. Returns `final_write_sequence`.
    pub fn begin_close_peer_lane(
        &mut self,
        lane_id: u16,
        owner: [u8; 32],
        epoch: u64,
    ) -> Result<u64, V2Error> {
        let lane_offset = self.peer_lane_offset(lane_id)?;
        let mapping = &self.mapping;
        let observed = lane_state(mapping, lane_offset);
        if observed != LANE_STATE_BOUND {
            return Err(V2Error::LifecycleState {
                expected: LANE_STATE_BOUND,
                observed,
            });
        }
        check_lease(mapping, lane_offset, &owner, epoch)?;
        let final_write =
            lane_u64(mapping, lane_offset, LH_OFF_WRITE_SEQUENCE).load(Ordering::Acquire);
        lane_u64(mapping, lane_offset, LH_OFF_FINAL_WRITE).store(final_write, Ordering::Relaxed);
        transition_state(mapping, lane_offset, LANE_STATE_BOUND, LANE_STATE_CLOSING)?;
        Ok(final_write)
    }

    /// CLOSING -> FREE, only after the consumer acknowledged the drain of this
    /// epoch: requires the lease's owner and epoch,
    /// `drained_epoch == lane_epoch` and
    /// `read_sequence == final_write_sequence == write_sequence`. A write past
    /// `final_write_sequence` quarantines the lane. Clears the owner and
    /// `final_write_sequence`, then publishes FREE with Release.
    pub fn release_peer_lane(
        &mut self,
        lane_id: u16,
        owner: [u8; 32],
        epoch: u64,
    ) -> Result<(), V2Error> {
        let lane_offset = self.peer_lane_offset(lane_id)?;
        let mapping = &self.mapping;
        let observed = lane_state(mapping, lane_offset);
        if observed != LANE_STATE_CLOSING {
            return Err(V2Error::LifecycleState {
                expected: LANE_STATE_CLOSING,
                observed,
            });
        }
        check_lease(mapping, lane_offset, &owner, epoch)?;
        let final_write =
            lane_u64(mapping, lane_offset, LH_OFF_FINAL_WRITE).load(Ordering::Acquire);
        let write = lane_u64(mapping, lane_offset, LH_OFF_WRITE_SEQUENCE).load(Ordering::Acquire);
        if write != final_write {
            quarantine_lane_at(mapping, lane_offset, QUARANTINE_LATE_WRITE);
            return Err(V2Error::LateWriteAfterClose { final_write, write });
        }
        let read = lane_u64(mapping, lane_offset, LH_OFF_READ_SEQUENCE).load(Ordering::Acquire);
        let drained = lane_u64(mapping, lane_offset, LH_OFF_DRAINED_EPOCH).load(Ordering::Acquire);
        if drained != epoch || read != final_write {
            return Err(V2Error::LaneNotDrained { read, final_write });
        }
        store_owner(mapping, lane_offset, &[0u8; OWNER_LEN]);
        lane_u64(mapping, lane_offset, LH_OFF_FINAL_WRITE).store(0, Ordering::Relaxed);
        transition_state(mapping, lane_offset, LANE_STATE_CLOSING, LANE_STATE_FREE)
    }

    /// Quarantines one lane of any class (reason OPERATOR): `shutdown = 1`,
    /// state QUARANTINED. Terminal for this generation; idempotent. Returns
    /// the previous state.
    pub fn quarantine_lane(&mut self, lane_id: u16) -> Result<u32, V2Error> {
        let (_, lane_offset) = locate_lane_by_id(&self.mapping, lane_id)?;
        Ok(quarantine_lane_at(&self.mapping, lane_offset, QUARANTINE_OPERATOR).0)
    }

    /// Quarantines every lane of this arena (reason RESTART), as a runtime
    /// restart does with the previous generation before abandoning it.
    /// Returns how many lanes were not quarantined before. Destroys no record.
    pub fn quarantine_all(&mut self) -> Result<usize, V2Error> {
        let mut newly = 0;
        for lane_id in directory_lane_ids(&self.mapping) {
            let (_, lane_offset) = locate_lane_by_id(&self.mapping, lane_id)?;
            if quarantine_lane_at(&self.mapping, lane_offset, QUARANTINE_RESTART).1 {
                newly += 1;
            }
        }
        Ok(newly)
    }

    /// Begins shutdown: sets `shutdown = 1` on every lane. From then on every
    /// bind and every publish of this arena is refused; records already
    /// written stay readable until the consumer drained them. Idempotent.
    /// Nothing is unmapped: each handle unmaps only its own mapping when it is
    /// dropped, which Rust ownership orders after its last use.
    pub fn begin_shutdown(&mut self) -> Result<(), V2Error> {
        for lane_id in directory_lane_ids(&self.mapping) {
            let (_, lane_offset) = locate_lane_by_id(&self.mapping, lane_id)?;
            lane_u64(&self.mapping, lane_offset, LH_OFF_SHUTDOWN).store(1, Ordering::Release);
        }
        Ok(())
    }

    fn peer_lane_offset(&self, lane_id: u16) -> Result<usize, V2Error> {
        let (config, lane_offset) = locate_lane_by_id(&self.mapping, lane_id)?;
        if config.class != LaneClassV2::PeerIngress {
            return Err(V2Error::LaneMismatch(
                "lifecycle requires peer-ingress lane",
            ));
        }
        Ok(lane_offset)
    }
}

/// What lies at an arena path, read from its SuperHeader only. Probing never
/// maps, attaches, modifies or deletes the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaProbeV2 {
    /// No file.
    Absent,
    /// A Wire-V2 arena of the expected generation.
    Current,
    /// A Wire-V2 arena of another generation.
    StaleGeneration { observed: u64 },
    /// A file that is not a valid Wire-V2 arena.
    NotAnArena,
}

/// The arena file of one generation: `<dir>/fastlane-v2-g<20 digits>.mmap`.
/// A new generation therefore never collides with the file of an earlier
/// one, and `create_new` stays the guard against silent reuse.
pub fn arena_path_v2(dir: &Path, generation: u64) -> PathBuf {
    dir.join(format!("fastlane-v2-g{generation:020}.mmap"))
}

/// Classifies the file at `path` against `generation`.
pub fn probe_arena_v2(path: &Path, generation: u64) -> Result<ArenaProbeV2, V2Error> {
    if generation == 0 {
        return Err(V2Error::ZeroGeneration);
    }
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ArenaProbeV2::Absent);
        }
        Err(error) => return Err(V2Error::Io(error)),
    };
    let len = file.metadata().map_err(V2Error::Io)?.len();
    let mut header = [0u8; SUPER_HEADER_SIZE_V2];
    if len < SUPER_HEADER_SIZE_V2 as u64 || file.read_exact(&mut header).is_err() {
        return Ok(ArenaProbeV2::NotAnArena);
    }
    let lane_count = get_u32(&header, SH_OFF_LANE_COUNT) as usize;
    let observed = get_u64(&header, SH_OFF_GENERATION);
    if header[SH_OFF_MAGIC..SH_OFF_MAGIC + 8] != MAGIC_V2
        || get_u16(&header, SH_OFF_VERSION) != VERSION_V2
        || get_u16(&header, SH_OFF_ENDIAN) != ENDIAN_MARKER_V2
        || get_u32(&header, SH_OFF_HEADER_SIZE) as usize != SUPER_HEADER_SIZE_V2
        || get_u32(&header, SH_OFF_DIR_SIZE) as usize != DIRECTORY_ENTRY_SIZE_V2
        || get_u32(&header, SH_OFF_LANE_HEADER_SIZE) as usize != LANE_HEADER_SIZE_V2
        || get_u32(&header, SH_OFF_SLOT_HEADER_SIZE) as usize != SLOT_HEADER_SIZE_V2
        || get_u32(&header, SH_OFF_ALIGNMENT) as usize != ALIGN_V2
        || get_u32(&header, SH_OFF_CRC_KIND) != CRC_KIND_CRC32C
        || get_u64(&header, SH_OFF_TOTAL_BYTES) != len
        || lane_count == 0
        || lane_count > MAX_LANES_V2
        || observed == 0
    {
        return Ok(ArenaProbeV2::NotAnArena);
    }
    if observed == generation {
        Ok(ArenaProbeV2::Current)
    } else {
        Ok(ArenaProbeV2::StaleGeneration { observed })
    }
}

/// Arena files in `dir` named for a generation other than `generation`,
/// sorted by generation. Read-only: nothing is opened for writing, attached
/// or deleted; disposal belongs to the startup orchestration.
pub fn stale_arenas_v2(dir: &Path, generation: u64) -> Result<Vec<(u64, PathBuf)>, V2Error> {
    let mut stale = Vec::new();
    for entry in fs::read_dir(dir).map_err(V2Error::Io)? {
        let entry = entry.map_err(V2Error::Io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(digits) = name
            .strip_prefix("fastlane-v2-g")
            .and_then(|rest| rest.strip_suffix(".mmap"))
        else {
            continue;
        };
        if digits.len() != 20 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(observed) = digits.parse::<u64>() else {
            continue;
        };
        if observed != generation {
            stale.push((observed, entry.path()));
        }
    }
    stale.sort();
    Ok(stale)
}

fn open_lane(
    path: &Path,
    generation: u64,
    expected: LaneConfigV2,
) -> Result<(MappingV2, usize), V2Error> {
    if generation == 0 {
        return Err(V2Error::ZeroGeneration);
    }
    expected.validate()?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(V2Error::Io)?;
    let len = usize::try_from(file.metadata().map_err(V2Error::Io)?.len())
        .map_err(|_| V2Error::SizeOverflow)?;
    let mapping = MappingV2::map(file, len)?;
    validate_super(&mapping, generation)?;
    let lane_offset = locate_lane(&mapping, expected)?;
    Ok((mapping, lane_offset))
}

fn validate_super(mapping: &MappingV2, generation: u64) -> Result<(), V2Error> {
    if mapping.len < SUPER_HEADER_SIZE_V2 {
        return Err(V2Error::BadLayout("short super header"));
    }
    let b = mapping.bytes();
    if b[SH_OFF_MAGIC..SH_OFF_MAGIC + 8] != MAGIC_V2 {
        return Err(V2Error::BadMagic);
    }
    let version = get_u16(b, SH_OFF_VERSION);
    if version != VERSION_V2 {
        return Err(V2Error::BadVersion(version));
    }
    let endian = get_u16(b, SH_OFF_ENDIAN);
    if endian != ENDIAN_MARKER_V2 {
        return Err(V2Error::BadEndian(endian));
    }
    if get_u32(b, SH_OFF_HEADER_SIZE) as usize != SUPER_HEADER_SIZE_V2
        || get_u32(b, SH_OFF_DIR_SIZE) as usize != DIRECTORY_ENTRY_SIZE_V2
        || get_u32(b, SH_OFF_LANE_HEADER_SIZE) as usize != LANE_HEADER_SIZE_V2
        || get_u32(b, SH_OFF_SLOT_HEADER_SIZE) as usize != SLOT_HEADER_SIZE_V2
        || get_u32(b, SH_OFF_ALIGNMENT) as usize != ALIGN_V2
        || get_u32(b, SH_OFF_CRC_KIND) != CRC_KIND_CRC32C
    {
        return Err(V2Error::BadLayout("frozen layout constants"));
    }
    let observed = get_u64(b, SH_OFF_GENERATION);
    if observed != generation {
        return Err(V2Error::GenerationMismatch {
            expected: generation,
            observed,
        });
    }
    if get_u64(b, SH_OFF_TOTAL_BYTES) as usize != mapping.len {
        return Err(V2Error::BadLayout("total bytes"));
    }
    let lane_count = get_u32(b, SH_OFF_LANE_COUNT) as usize;
    if lane_count == 0 || lane_count > MAX_LANES_V2 {
        return Err(V2Error::InvalidLaneCount(lane_count));
    }
    let dir_end = SUPER_HEADER_SIZE_V2
        .checked_add(
            lane_count
                .checked_mul(DIRECTORY_ENTRY_SIZE_V2)
                .ok_or(V2Error::SizeOverflow)?,
        )
        .ok_or(V2Error::SizeOverflow)?;
    if dir_end > mapping.len {
        return Err(V2Error::BadLayout("directory out of bounds"));
    }
    Ok(())
}

fn locate_lane(mapping: &MappingV2, expected: LaneConfigV2) -> Result<usize, V2Error> {
    let b = mapping.bytes();
    let lane_count = get_u32(b, SH_OFF_LANE_COUNT) as usize;
    for index in 0..lane_count {
        let off = SUPER_HEADER_SIZE_V2 + index * DIRECTORY_ENTRY_SIZE_V2;
        if get_u16(b, off + DE_OFF_LANE_ID) != expected.lane_id {
            continue;
        }
        if get_u16(b, off + DE_OFF_DIRECTION) != expected.direction as u16 {
            return Err(V2Error::LaneMismatch("direction"));
        }
        if get_u16(b, off + DE_OFF_CLASS) != expected.class as u16 {
            return Err(V2Error::LaneMismatch("class"));
        }
        if get_u16(b, off + DE_OFF_FLAGS) != expected.flags {
            return Err(V2Error::LaneMismatch("flags"));
        }
        if get_u32(b, off + DE_OFF_CAPACITY) as usize != expected.capacity {
            return Err(V2Error::LaneMismatch("capacity"));
        }
        if get_u32(b, off + DE_OFF_SLOT_STRIDE) as usize != expected.slot_stride {
            return Err(V2Error::LaneMismatch("slot stride"));
        }
        let lane_offset = usize::try_from(get_u64(b, off + DE_OFF_LANE_OFFSET))
            .map_err(|_| V2Error::SizeOverflow)?;
        let lane_len = usize::try_from(get_u64(b, off + DE_OFF_LANE_BYTES))
            .map_err(|_| V2Error::SizeOverflow)?;
        if lane_offset % ALIGN_V2 != 0
            || lane_len != lane_bytes(expected)?
            || lane_offset
                .checked_add(lane_len)
                .ok_or(V2Error::SizeOverflow)?
                > mapping.len
        {
            return Err(V2Error::BadLayout("lane range"));
        }
        let observed_generation = get_u64(b, lane_offset + LH_OFF_GENERATION);
        let super_generation = get_u64(b, SH_OFF_GENERATION);
        if observed_generation != super_generation {
            return Err(V2Error::GenerationMismatch {
                expected: super_generation,
                observed: observed_generation,
            });
        }
        return Ok(lane_offset);
    }
    Err(V2Error::LaneNotFound(expected.lane_id))
}

fn locate_lane_by_id(mapping: &MappingV2, lane_id: u16) -> Result<(LaneConfigV2, usize), V2Error> {
    let b = mapping.bytes();
    let lane_count = get_u32(b, SH_OFF_LANE_COUNT) as usize;
    for index in 0..lane_count {
        let off = SUPER_HEADER_SIZE_V2 + index * DIRECTORY_ENTRY_SIZE_V2;
        if get_u16(b, off + DE_OFF_LANE_ID) != lane_id {
            continue;
        }
        let direction = match get_u16(b, off + DE_OFF_DIRECTION) {
            1 => DirectionV2::GethToRust,
            2 => DirectionV2::RustToGeth,
            _ => return Err(V2Error::BadLayout("direction")),
        };
        let class = match get_u16(b, off + DE_OFF_CLASS) {
            1 => LaneClassV2::PeerIngress,
            2 => LaneClassV2::TxpoolEvidence,
            3 => LaneClassV2::ChainEvidence,
            4 => LaneClassV2::OutboundHandoffEvidence,
            5 => LaneClassV2::SignedTransactionEgress,
            _ => return Err(V2Error::BadLayout("lane class")),
        };
        let config = LaneConfigV2 {
            lane_id,
            direction,
            class,
            flags: get_u16(b, off + DE_OFF_FLAGS),
            capacity: get_u32(b, off + DE_OFF_CAPACITY) as usize,
            slot_stride: get_u32(b, off + DE_OFF_SLOT_STRIDE) as usize,
        };
        config.validate()?;
        let lane_offset = usize::try_from(get_u64(b, off + DE_OFF_LANE_OFFSET))
            .map_err(|_| V2Error::SizeOverflow)?;
        let lane_len = usize::try_from(get_u64(b, off + DE_OFF_LANE_BYTES))
            .map_err(|_| V2Error::SizeOverflow)?;
        if !lane_offset.is_multiple_of(ALIGN_V2)
            || lane_len != lane_bytes(config)?
            || lane_offset
                .checked_add(lane_len)
                .ok_or(V2Error::SizeOverflow)?
                > mapping.len
        {
            return Err(V2Error::BadLayout("lane range"));
        }
        let observed_generation = get_u64(b, lane_offset + LH_OFF_GENERATION);
        let super_generation = get_u64(b, SH_OFF_GENERATION);
        if observed_generation != super_generation {
            return Err(V2Error::GenerationMismatch {
                expected: super_generation,
                observed: observed_generation,
            });
        }
        return Ok((config, lane_offset));
    }
    Err(V2Error::LaneNotFound(lane_id))
}

fn directory_lane_ids(mapping: &MappingV2) -> Vec<u16> {
    let b = mapping.bytes();
    let lane_count = get_u32(b, SH_OFF_LANE_COUNT) as usize;
    (0..lane_count)
        .map(|index| {
            get_u16(
                b,
                SUPER_HEADER_SIZE_V2 + index * DIRECTORY_ENTRY_SIZE_V2 + DE_OFF_LANE_ID,
            )
        })
        .collect()
}
