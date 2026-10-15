// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// Wire V2 byte layout: format constants, header/directory/lane/slot offsets,
// lane states and quarantine reasons, the arena layout and little-endian
// field access. Offsets and sizes are the ABI shared with the Go overlay.

pub const MAGIC_V2: [u8; 8] = *b"DBFLN002";
pub const VERSION_V2: u16 = 2;
pub const ENDIAN_MARKER_V2: u16 = 0x0102;
pub const ALIGN_V2: usize = 128;
pub const SUPER_HEADER_SIZE_V2: usize = 256;
pub const DIRECTORY_ENTRY_SIZE_V2: usize = 128;
pub const LANE_HEADER_SIZE_V2: usize = 384;
pub const SLOT_HEADER_SIZE_V2: usize = 256;
pub const MIN_SLOT_STRIDE_V2: usize = 512;
pub const MAX_SLOT_STRIDE_V2: usize = 256 * 1024;
pub const MAX_CAPACITY_V2: usize = 4096;
pub const MAX_LANES_V2: usize = 1024;
pub const CRC_KIND_CRC32C: u32 = 1;
pub const CLOCK_ID_MONOTONIC: u32 = 1;
pub const LANE_STATE_FREE: u32 = 0;
pub const LANE_STATE_BOUND: u32 = 1;
pub const LANE_STATE_CLOSING: u32 = 2;
pub const LANE_STATE_QUARANTINED: u32 = 3;
const EMPTY_SEQUENCE: u64 = u64::MAX;
const OWNER_LEN: usize = 32;

/// Why a lane was quarantined, stored once in the lifecycle word at offset
/// 328 (reserved in C1.2) by the first quarantining party. Zero: none.
pub const QUARANTINE_REASON_NONE: u64 = 0;
pub const QUARANTINE_CORRUPT_SEQUENCE: u64 = 1;
pub const QUARANTINE_PAYLOAD_LENGTH: u64 = 2;
pub const QUARANTINE_EPOCH_MISMATCH: u64 = 3;
pub const QUARANTINE_CHECKSUM: u64 = 4;
pub const QUARANTINE_RECORD_KIND: u64 = 5;
pub const QUARANTINE_LATE_WRITE: u64 = 6;
pub const QUARANTINE_INVALID_STATE: u64 = 7;
pub const QUARANTINE_RESTART: u64 = 8;
pub const QUARANTINE_OPERATOR: u64 = 9;

/// Version of the diagnostic content of a role-lease file.
pub const ROLE_LEASE_VERSION_V2: u32 = 1;

const SH_OFF_MAGIC: usize = 0;
const SH_OFF_VERSION: usize = 8;
const SH_OFF_ENDIAN: usize = 10;
const SH_OFF_HEADER_SIZE: usize = 12;
const SH_OFF_GENERATION: usize = 16;
const SH_OFF_LANE_COUNT: usize = 24;
const SH_OFF_DIR_SIZE: usize = 28;
const SH_OFF_LANE_HEADER_SIZE: usize = 32;
const SH_OFF_SLOT_HEADER_SIZE: usize = 36;
const SH_OFF_ALIGNMENT: usize = 40;
const SH_OFF_CRC_KIND: usize = 44;
const SH_OFF_TOTAL_BYTES: usize = 48;

const DE_OFF_LANE_ID: usize = 0;
const DE_OFF_DIRECTION: usize = 2;
const DE_OFF_CLASS: usize = 4;
const DE_OFF_FLAGS: usize = 6;
const DE_OFF_CAPACITY: usize = 8;
const DE_OFF_SLOT_STRIDE: usize = 12;
const DE_OFF_LANE_OFFSET: usize = 16;
const DE_OFF_LANE_BYTES: usize = 24;

const LH_OFF_WRITE_SEQUENCE: usize = 0;
const LH_OFF_DROPPED_FULL: usize = 8;
const LH_OFF_OVERSIZE: usize = 16;
const LH_OFF_PRODUCER_STATUS: usize = 24;
const LH_OFF_READ_SEQUENCE: usize = 128;
const LH_OFF_CORRUPT: usize = 136;
const LH_OFF_DRAINED_EPOCH: usize = 144;
const LH_OFF_GENERATION: usize = 256;
const LH_OFF_LANE_EPOCH: usize = 264;
const LH_OFF_STATE: usize = 272;
const LH_OFF_FINAL_WRITE: usize = 280;
const LH_OFF_SHUTDOWN: usize = 288;
const LH_OFF_OWNER: usize = 296;
const LH_OFF_QUARANTINE_REASON: usize = 328;

const SLOT_OFF_COMMITTED: usize = 0;
const SLOT_OFF_KIND: usize = 8;
const SLOT_OFF_FLAGS: usize = 10;
const SLOT_OFF_PAYLOAD_LEN: usize = 12;
const SLOT_OFF_TIMESTAMP: usize = 16;
const SLOT_OFF_CLOCK_ID: usize = 24;
const SLOT_OFF_SOURCE: usize = 32;
const SLOT_OFF_PEER: usize = 48;
const SLOT_OFF_HASH: usize = 80;
const SLOT_OFF_AUX: usize = 112;
const SLOT_OFF_LANE_EPOCH: usize = 144;
const SLOT_OFF_CRC32C: usize = 152;

struct ArenaLayout {
    total: usize,
    lane_offsets: Vec<usize>,
}

fn layout_for(lanes: &[LaneConfigV2]) -> Result<ArenaLayout, V2Error> {
    let directory_end = SUPER_HEADER_SIZE_V2
        .checked_add(
            lanes
                .len()
                .checked_mul(DIRECTORY_ENTRY_SIZE_V2)
                .ok_or(V2Error::SizeOverflow)?,
        )
        .ok_or(V2Error::SizeOverflow)?;
    let mut cursor = align_up(directory_end, ALIGN_V2)?;
    let mut lane_offsets = Vec::with_capacity(lanes.len());
    for lane in lanes {
        lane_offsets.push(cursor);
        cursor = cursor
            .checked_add(lane_bytes(*lane)?)
            .ok_or(V2Error::SizeOverflow)?;
        cursor = align_up(cursor, ALIGN_V2)?;
    }
    Ok(ArenaLayout {
        total: cursor,
        lane_offsets,
    })
}

fn validate_arena_config(generation: u64, lanes: &[LaneConfigV2]) -> Result<(), V2Error> {
    if generation == 0 {
        return Err(V2Error::ZeroGeneration);
    }
    if lanes.is_empty() || lanes.len() > MAX_LANES_V2 {
        return Err(V2Error::InvalidLaneCount(lanes.len()));
    }
    let mut ids = BTreeSet::new();
    for lane in lanes {
        lane.validate()?;
        if !ids.insert(lane.lane_id) {
            return Err(V2Error::DuplicateLaneId(lane.lane_id));
        }
    }
    Ok(())
}

fn lane_bytes(config: LaneConfigV2) -> Result<usize, V2Error> {
    config
        .capacity
        .checked_mul(config.slot_stride)
        .and_then(|slots| LANE_HEADER_SIZE_V2.checked_add(slots))
        .ok_or(V2Error::SizeOverflow)
}

fn align_up(value: usize, align: usize) -> Result<usize, V2Error> {
    value
        .checked_add(align - 1)
        .map(|v| v & !(align - 1))
        .ok_or(V2Error::SizeOverflow)
}

fn put_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(buf: &mut [u8], offset: usize, value: u64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(buf[offset..offset + 2].try_into().expect("u16 slice"))
}

fn get_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(buf[offset..offset + 4].try_into().expect("u32 slice"))
}

fn get_u64(buf: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(buf[offset..offset + 8].try_into().expect("u64 slice"))
}
