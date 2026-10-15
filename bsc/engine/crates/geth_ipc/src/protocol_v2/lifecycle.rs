// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// Peer-lane lifecycle: state transitions, quarantine, drain acknowledgement,
// owner records and lease checks on the mapped lane headers.

fn lane_quarantine_reason(mapping: &MappingV2, lane_offset: usize) -> u64 {
    lane_u64(mapping, lane_offset, LH_OFF_QUARANTINE_REASON).load(Ordering::Acquire)
}

/// Refuses a QUARANTINED lane and quarantines a lane whose state word holds
/// no defined state. FREE, BOUND and CLOSING pass.
fn lane_admit_state(mapping: &MappingV2, lane_offset: usize, state: u32) -> Result<(), V2Error> {
    match state {
        LANE_STATE_FREE | LANE_STATE_BOUND | LANE_STATE_CLOSING => Ok(()),
        LANE_STATE_QUARANTINED => Err(V2Error::LaneQuarantined {
            reason: lane_quarantine_reason(mapping, lane_offset),
        }),
        observed => Err(lane_corrupt(
            mapping,
            lane_offset,
            QUARANTINE_INVALID_STATE,
            V2Error::InvalidLifecycleState { observed },
        )),
    }
}

/// Quarantines for a structural defect. Only the call that moves the lane
/// into QUARANTINED counts the incident and reports the defect.
fn lane_corrupt(mapping: &MappingV2, lane_offset: usize, reason: u64, defect: V2Error) -> V2Error {
    if quarantine_lane_at(mapping, lane_offset, reason).1 {
        lane_u64(mapping, lane_offset, LH_OFF_CORRUPT).fetch_add(1, Ordering::Relaxed);
        defect
    } else {
        V2Error::LaneQuarantined {
            reason: lane_quarantine_reason(mapping, lane_offset),
        }
    }
}

fn lane_late_write(
    mapping: &MappingV2,
    lane_offset: usize,
    final_write: u64,
    write: u64,
) -> V2Error {
    quarantine_lane_at(mapping, lane_offset, QUARANTINE_LATE_WRITE);
    V2Error::LateWriteAfterClose { final_write, write }
}

fn lane_acknowledge_drained(mapping: &MappingV2, lane_offset: usize) -> Result<bool, V2Error> {
    let state = lane_state(mapping, lane_offset);
    lane_admit_state(mapping, lane_offset, state)?;
    if state != LANE_STATE_CLOSING {
        return Ok(false);
    }
    let final_write = lane_u64(mapping, lane_offset, LH_OFF_FINAL_WRITE).load(Ordering::Acquire);
    let write = lane_u64(mapping, lane_offset, LH_OFF_WRITE_SEQUENCE).load(Ordering::Acquire);
    if write != final_write {
        return Err(lane_late_write(mapping, lane_offset, final_write, write));
    }
    let read = lane_u64(mapping, lane_offset, LH_OFF_READ_SEQUENCE).load(Ordering::Acquire);
    if read != final_write {
        return Ok(false);
    }
    let epoch = lane_u64(mapping, lane_offset, LH_OFF_LANE_EPOCH).load(Ordering::Acquire);
    lane_u64(mapping, lane_offset, LH_OFF_DRAINED_EPOCH).store(epoch, Ordering::Release);
    Ok(true)
}

fn lane_u64(mapping: &MappingV2, lane_offset: usize, relative: usize) -> &AtomicU64 {
    mapping.atomic_u64(lane_offset + relative)
}

fn lane_state(mapping: &MappingV2, lane_offset: usize) -> u32 {
    mapping
        .atomic_u32(lane_offset + LH_OFF_STATE)
        .load(Ordering::Acquire)
}

/// Compare-and-swap of the lifecycle state. Release on success publishes the
/// field writes that preceded it.
fn transition_state(
    mapping: &MappingV2,
    lane_offset: usize,
    from: u32,
    to: u32,
) -> Result<(), V2Error> {
    mapping
        .atomic_u32(lane_offset + LH_OFF_STATE)
        .compare_exchange(from, to, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| ())
        .map_err(|observed| V2Error::LifecycleState {
            expected: from,
            observed,
        })
}

/// `shutdown = 1`; the first quarantining party records `reason`; then any
/// state -> QUARANTINED by compare-and-swap. Returns the previous state
/// (QUARANTINED if it already was) and whether this call made the
/// transition, which happens exactly once per lane and generation.
fn quarantine_lane_at(mapping: &MappingV2, lane_offset: usize, reason: u64) -> (u32, bool) {
    lane_u64(mapping, lane_offset, LH_OFF_SHUTDOWN).store(1, Ordering::Release);
    let _ = lane_u64(mapping, lane_offset, LH_OFF_QUARANTINE_REASON).compare_exchange(
        QUARANTINE_REASON_NONE,
        reason,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
    let state = mapping.atomic_u32(lane_offset + LH_OFF_STATE);
    let mut current = state.load(Ordering::Acquire);
    loop {
        if current == LANE_STATE_QUARANTINED {
            return (current, false);
        }
        match state.compare_exchange(
            current,
            LANE_STATE_QUARANTINED,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(previous) => return (previous, true),
            Err(observed) => current = observed,
        }
    }
}

/// The owner tag as four atomic little-endian 64-bit words (offset 296 is
/// 8-byte aligned). Relaxed: ordered by the state transition.
fn store_owner(mapping: &MappingV2, lane_offset: usize, owner: &[u8; OWNER_LEN]) {
    for (word, chunk) in owner.chunks_exact(8).enumerate() {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(chunk);
        lane_u64(mapping, lane_offset, LH_OFF_OWNER + word * 8)
            .store(u64::from_le_bytes(bytes), Ordering::Relaxed);
    }
}

fn load_owner(mapping: &MappingV2, lane_offset: usize) -> [u8; OWNER_LEN] {
    let mut owner = [0u8; OWNER_LEN];
    for (word, chunk) in owner.chunks_exact_mut(8).enumerate() {
        let value = lane_u64(mapping, lane_offset, LH_OFF_OWNER + word * 8).load(Ordering::Relaxed);
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    owner
}

fn check_lease(
    mapping: &MappingV2,
    lane_offset: usize,
    owner: &[u8; OWNER_LEN],
    epoch: u64,
) -> Result<(), V2Error> {
    if load_owner(mapping, lane_offset) != *owner {
        return Err(V2Error::LaneMismatch("owner"));
    }
    let current = lane_u64(mapping, lane_offset, LH_OFF_LANE_EPOCH).load(Ordering::Acquire);
    if current != epoch {
        return Err(V2Error::LeaseEpochMismatch {
            expected: current,
            observed: epoch,
        });
    }
    Ok(())
}

fn lane_stats(mapping: &MappingV2, lane_offset: usize) -> LaneStatsV2 {
    LaneStatsV2 {
        write_sequence: mapping
            .atomic_u64(lane_offset + LH_OFF_WRITE_SEQUENCE)
            .load(Ordering::Acquire),
        read_sequence: mapping
            .atomic_u64(lane_offset + LH_OFF_READ_SEQUENCE)
            .load(Ordering::Acquire),
        dropped_full: mapping
            .atomic_u64(lane_offset + LH_OFF_DROPPED_FULL)
            .load(Ordering::Acquire),
        oversize: mapping
            .atomic_u64(lane_offset + LH_OFF_OVERSIZE)
            .load(Ordering::Acquire),
        corrupt_records: mapping
            .atomic_u64(lane_offset + LH_OFF_CORRUPT)
            .load(Ordering::Acquire),
        lane_epoch: mapping
            .atomic_u64(lane_offset + LH_OFF_LANE_EPOCH)
            .load(Ordering::Acquire),
        generation: mapping
            .atomic_u64(lane_offset + LH_OFF_GENERATION)
            .load(Ordering::Acquire),
        shutdown: mapping
            .atomic_u64(lane_offset + LH_OFF_SHUTDOWN)
            .load(Ordering::Acquire)
            != 0,
        state: mapping
            .atomic_u32(lane_offset + LH_OFF_STATE)
            .load(Ordering::Acquire),
        final_write_sequence: mapping
            .atomic_u64(lane_offset + LH_OFF_FINAL_WRITE)
            .load(Ordering::Acquire),
        drained_epoch: mapping
            .atomic_u64(lane_offset + LH_OFF_DRAINED_EPOCH)
            .load(Ordering::Acquire),
        owner: load_owner(mapping, lane_offset),
        quarantine_reason: mapping
            .atomic_u64(lane_offset + LH_OFF_QUARANTINE_REASON)
            .load(Ordering::Acquire),
    }
}
