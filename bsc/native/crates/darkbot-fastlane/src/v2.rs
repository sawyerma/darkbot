//! C1.2 Fast-Lane Wire V2 with the C1.3 peer-lane lifecycle.
//!
//! Wire V2 is intentionally implemented alongside V1 until C1.4 switches the
//! geth ingress hooks. The arena contains independently owned SPSC lanes.
//! C1.5 replaces the owned receive copy with a borrowed slot guard.
//!
//! # Peer-lane lifecycle (C1.3)
//!
//! ```text
//! FREE --bind--> BOUND --close--> CLOSING --drain ack--> (drained) --release--> FREE
//!   any state --quarantine--> QUARANTINED (terminal for this generation)
//! ```
//!
//! - Every lifecycle field (`lane_epoch`, state, `final_write_sequence`,
//!   `shutdown`, the four owner words, `drained_epoch`) is accessed atomically
//!   only; the byte layout is the frozen C1.2 one.
//! - State changes are compare-and-swap transitions. Field writes precede the
//!   transition and are published by its Release; readers Acquire the state
//!   first.
//! - A lease is `(lane id, owner, lane_epoch)`. Close and release require the
//!   exact owner and epoch, so a lease of an earlier binding never acts on a
//!   reused lane, even with the same owner bytes.
//! - Every bind increments `lane_epoch`; exhaustion fails closed. Sequence
//!   numbers stay monotonic across reuse: a slot of an earlier binding carries
//!   a `committed_sequence` that can never equal a future expected one, and
//!   its record epoch is checked as well.
//! - A record written after `final_write_sequence` was frozen quarantines the
//!   lane instead of releasing it.
//! - Peer connect/disconnect and allocation are outside the transaction hot
//!   path; `publish` keeps exactly its C1.2 work, including the state check.

#[cfg(not(target_endian = "little"))]
compile_error!("Fast-Lane Wire V2 requires a little-endian target");

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::{AUX_LEN, HASH_LEN, PEER_LEN, RecordKind, SOURCE_LEN};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordV2<'a> {
    pub kind: RecordKind,
    pub flags: u16,
    pub timestamp_ns: u64,
    pub clock_id: u32,
    pub source: [u8; SOURCE_LEN],
    pub peer: [u8; PEER_LEN],
    pub hash: [u8; HASH_LEN],
    pub aux: [u8; AUX_LEN],
    pub payload: &'a [u8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedRecordV2 {
    pub sequence: u64,
    pub lane_id: u16,
    pub lane_epoch: u64,
    pub kind: RecordKind,
    pub flags: u16,
    pub timestamp_ns: u64,
    pub clock_id: u32,
    pub source: [u8; SOURCE_LEN],
    pub peer: [u8; PEER_LEN],
    pub hash: [u8; HASH_LEN],
    pub aux: [u8; AUX_LEN],
    pub payload: Box<[u8]>,
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

#[derive(Debug)]
pub enum V2Error {
    ZeroGeneration,
    InvalidLaneCount(usize),
    DuplicateLaneId(u16),
    InvalidCapacity(usize),
    InvalidSlotStride(usize),
    SizeOverflow,
    Io(io::Error),
    MapFailed(io::Error),
    BadMagic,
    BadVersion(u16),
    BadEndian(u16),
    BadLayout(&'static str),
    GenerationMismatch {
        expected: u64,
        observed: u64,
    },
    LaneNotFound(u16),
    LaneMismatch(&'static str),
    PayloadTooLarge {
        len: usize,
        max: usize,
    },
    QueueFull {
        sequence: u64,
    },
    Shutdown,
    CorruptSequence {
        expected: u64,
        observed: u64,
    },
    LaneEpochMismatch {
        expected: u64,
        observed: u64,
    },
    ChecksumMismatch {
        expected: u32,
        actual: u32,
    },
    UnknownRecordKind(u16),
    LifecycleState {
        expected: u32,
        observed: u32,
    },
    LaneNotDrained {
        read: u64,
        final_write: u64,
    },
    EpochExhausted,
    /// A lifecycle call named another binding of the lane than the current
    /// one: `expected` is the lane's epoch, `observed` the caller's.
    LeaseEpochMismatch {
        expected: u64,
        observed: u64,
    },
    /// A record was written after the close froze `final_write_sequence`.
    /// The lane has been quarantined.
    LateWriteAfterClose {
        final_write: u64,
        write: u64,
    },
    /// No FREE peer lane is available. Nothing was waited for.
    PoolExhausted {
        peer_lanes: usize,
    },
    /// `max_peers` lanes are already bound.
    PeerLimitReached {
        max_peers: usize,
    },
    /// The pool bounds do not describe the arena's peer lanes.
    InvalidPoolBounds {
        peer_lanes: usize,
        max_peers: usize,
        churn_reserve: usize,
    },
    /// The lane is QUARANTINED; it is not consumed, bound or reused in this
    /// generation.
    LaneQuarantined {
        reason: u64,
    },
    /// The lifecycle state word holds no defined state. The lane has been
    /// quarantined.
    InvalidLifecycleState {
        observed: u32,
    },
    /// Another live process holds this runtime role.
    RoleLeaseHeld {
        role: &'static str,
    },
    /// The arena file already exists; it was neither attached nor
    /// overwritten.
    ArenaExists {
        probe: ArenaProbeV2,
    },
}

impl fmt::Display for V2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroGeneration => f.write_str("fastlane v2: generation must be nonzero"),
            Self::InvalidLaneCount(v) => write!(f, "fastlane v2: invalid lane count {v}"),
            Self::DuplicateLaneId(v) => write!(f, "fastlane v2: duplicate lane id {v}"),
            Self::InvalidCapacity(v) => write!(f, "fastlane v2: invalid capacity {v}"),
            Self::InvalidSlotStride(v) => write!(f, "fastlane v2: invalid slot stride {v}"),
            Self::SizeOverflow => f.write_str("fastlane v2: mapping size overflow"),
            Self::Io(e) => write!(f, "fastlane v2: I/O {e}"),
            Self::MapFailed(e) => write!(f, "fastlane v2: mmap {e}"),
            Self::BadMagic => f.write_str("fastlane v2: bad magic"),
            Self::BadVersion(v) => write!(f, "fastlane v2: unsupported version {v}"),
            Self::BadEndian(v) => write!(f, "fastlane v2: bad endian marker {v:#x}"),
            Self::BadLayout(v) => write!(f, "fastlane v2: bad layout {v}"),
            Self::GenerationMismatch { expected, observed } => {
                write!(
                    f,
                    "fastlane v2: generation mismatch {observed} != {expected}"
                )
            }
            Self::LaneNotFound(v) => write!(f, "fastlane v2: lane {v} not found"),
            Self::LaneMismatch(v) => write!(f, "fastlane v2: lane mismatch {v}"),
            Self::PayloadTooLarge { len, max } => {
                write!(f, "fastlane v2: payload {len} exceeds {max}")
            }
            Self::QueueFull { sequence } => write!(f, "fastlane v2: queue full at {sequence}"),
            Self::Shutdown => f.write_str("fastlane v2: lane shutdown"),
            Self::CorruptSequence { expected, observed } => {
                write!(f, "fastlane v2: corrupt sequence {observed} != {expected}")
            }
            Self::LaneEpochMismatch { expected, observed } => {
                write!(f, "fastlane v2: lane epoch {observed} != {expected}")
            }
            Self::ChecksumMismatch { expected, actual } => {
                write!(f, "fastlane v2: checksum {actual:#x} != {expected:#x}")
            }
            Self::UnknownRecordKind(v) => write!(f, "fastlane v2: unknown record kind {v}"),
            Self::LifecycleState { expected, observed } => {
                write!(f, "fastlane v2: lifecycle state {observed} != {expected}")
            }
            Self::LaneNotDrained { read, final_write } => {
                write!(
                    f,
                    "fastlane v2: lane not drained read={read} final_write={final_write}"
                )
            }
            Self::EpochExhausted => f.write_str("fastlane v2: lane epoch exhausted"),
            Self::LeaseEpochMismatch { expected, observed } => {
                write!(
                    f,
                    "fastlane v2: lease epoch {observed} != lane epoch {expected}"
                )
            }
            Self::LateWriteAfterClose { final_write, write } => {
                write!(
                    f,
                    "fastlane v2: write {write} after close at {final_write}; lane quarantined"
                )
            }
            Self::PoolExhausted { peer_lanes } => {
                write!(f, "fastlane v2: no free peer lane among {peer_lanes}")
            }
            Self::PeerLimitReached { max_peers } => {
                write!(f, "fastlane v2: {max_peers} peer lanes already bound")
            }
            Self::InvalidPoolBounds {
                peer_lanes,
                max_peers,
                churn_reserve,
            } => write!(
                f,
                "fastlane v2: pool bounds max_peers={max_peers} + churn_reserve={churn_reserve} \
                 do not match {peer_lanes} peer lanes"
            ),
            Self::LaneQuarantined { reason } => {
                write!(f, "fastlane v2: lane quarantined (reason {reason})")
            }
            Self::InvalidLifecycleState { observed } => {
                write!(f, "fastlane v2: invalid lifecycle state {observed}")
            }
            Self::RoleLeaseHeld { role } => {
                write!(f, "fastlane v2: role {role} is held by another process")
            }
            Self::ArenaExists { probe } => {
                write!(f, "fastlane v2: arena already exists ({probe:?})")
            }
        }
    }
}

impl std::error::Error for V2Error {}

struct MappingV2 {
    _file: File,
    ptr: NonNull<u8>,
    len: usize,
}

unsafe impl Send for MappingV2 {}
unsafe impl Sync for MappingV2 {}

impl MappingV2 {
    fn map(file: File, len: usize) -> Result<Self, V2Error> {
        let raw = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if raw == libc::MAP_FAILED {
            return Err(V2Error::MapFailed(io::Error::last_os_error()));
        }
        let ptr = NonNull::new(raw.cast::<u8>())
            .ok_or_else(|| V2Error::MapFailed(io::Error::other("mmap returned null")))?;
        Ok(Self {
            _file: file,
            ptr,
            len,
        })
    }

    fn bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    fn atomic_u64(&self, offset: usize) -> &AtomicU64 {
        debug_assert_eq!(offset % 8, 0);
        unsafe { &*self.ptr.as_ptr().add(offset).cast::<AtomicU64>() }
    }

    fn atomic_u32(&self, offset: usize) -> &AtomicU32 {
        debug_assert_eq!(offset % 4, 0);
        unsafe { &*self.ptr.as_ptr().add(offset).cast::<AtomicU32>() }
    }

    fn sync(&self) -> Result<(), V2Error> {
        let rc = unsafe { libc::msync(self.ptr.as_ptr().cast(), self.len, libc::MS_SYNC) };
        if rc != 0 {
            return Err(V2Error::Io(io::Error::last_os_error()));
        }
        Ok(())
    }
}

impl Drop for MappingV2 {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr().cast(), self.len);
        }
    }
}

pub struct ArenaV2 {
    mapping: MappingV2,
}

pub struct ProducerV2 {
    mapping: MappingV2,
    config: LaneConfigV2,
    lane_offset: usize,
}

pub struct ConsumerV2 {
    mapping: MappingV2,
    config: LaneConfigV2,
    lane_offset: usize,
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

impl ConsumerV2 {
    pub fn open(path: &Path, generation: u64, expected: LaneConfigV2) -> Result<Self, V2Error> {
        let (mapping, lane_offset) = open_lane(path, generation, expected)?;
        Ok(Self {
            mapping,
            config: expected,
            lane_offset,
        })
    }

    /// Consumes the next record, or returns `Ok(None)` if the lane is empty.
    ///
    /// A QUARANTINED lane is not consumed. The first structural defect of a
    /// record (sequence, payload length, lane epoch, CRC32C, record kind, or
    /// an unknown lifecycle state) quarantines the lane, counts exactly one
    /// corruption incident and returns that defect; every later call returns
    /// [`V2Error::LaneQuarantined`] without counting again. A record on a FREE
    /// lane, or at or past `final_write_sequence` on a CLOSING lane, was
    /// written outside its binding: the lane is quarantined as a late write.
    /// `read_sequence` never advances past a refused record.
    pub fn try_consume(&mut self) -> Result<Option<OwnedRecordV2>, V2Error> {
        let read = self.atomic(LH_OFF_READ_SEQUENCE).load(Ordering::Relaxed);
        let write = self.atomic(LH_OFF_WRITE_SEQUENCE).load(Ordering::Acquire);
        // Loaded after write_sequence: a record observed above was published
        // by a producer that saw BOUND, so this load sees BOUND or later.
        let state = lane_state(&self.mapping, self.lane_offset);
        // The state is validated before the empty check: an undefined state
        // quarantines the lane even when no record is waiting.
        self.admit_state(state)?;
        if read == write {
            if self.atomic(LH_OFF_SHUTDOWN).load(Ordering::Acquire) != 0 {
                return Err(V2Error::Shutdown);
            }
            return Ok(None);
        }
        if state == LANE_STATE_FREE {
            return Err(self.quarantine_late_write(read, write));
        }
        if state == LANE_STATE_CLOSING {
            let final_write = self.atomic(LH_OFF_FINAL_WRITE).load(Ordering::Acquire);
            if read >= final_write {
                return Err(self.quarantine_late_write(final_write, write));
            }
        }

        let index = (read % self.config.capacity as u64) as usize;
        let slot_offset = self.lane_offset + LANE_HEADER_SIZE_V2 + index * self.config.slot_stride;
        let observed = self
            .mapping
            .atomic_u64(slot_offset + SLOT_OFF_COMMITTED)
            .load(Ordering::Acquire);
        let expected_sequence = read.wrapping_add(1);
        if observed != expected_sequence {
            return Err(self.corrupt(
                QUARANTINE_CORRUPT_SEQUENCE,
                V2Error::CorruptSequence {
                    expected: expected_sequence,
                    observed,
                },
            ));
        }

        let bytes = self.mapping.bytes();
        let slot = &bytes[slot_offset..slot_offset + self.config.slot_stride];
        let payload_len = get_u32(slot, SLOT_OFF_PAYLOAD_LEN) as usize;
        if payload_len > self.config.max_payload() {
            return Err(self.corrupt(
                QUARANTINE_PAYLOAD_LENGTH,
                V2Error::PayloadTooLarge {
                    len: payload_len,
                    max: self.config.max_payload(),
                },
            ));
        }
        let lane_epoch = get_u64(slot, SLOT_OFF_LANE_EPOCH);
        let expected_epoch = self.atomic(LH_OFF_LANE_EPOCH).load(Ordering::Acquire);
        if lane_epoch != expected_epoch {
            return Err(self.corrupt(
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
            return Err(self.corrupt(
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
                return Err(
                    self.corrupt(QUARANTINE_RECORD_KIND, V2Error::UnknownRecordKind(kind_raw))
                );
            }
        };
        let mut source = [0u8; SOURCE_LEN];
        let mut peer = [0u8; PEER_LEN];
        let mut hash = [0u8; HASH_LEN];
        let mut aux = [0u8; AUX_LEN];
        source.copy_from_slice(&slot[SLOT_OFF_SOURCE..SLOT_OFF_SOURCE + SOURCE_LEN]);
        peer.copy_from_slice(&slot[SLOT_OFF_PEER..SLOT_OFF_PEER + PEER_LEN]);
        hash.copy_from_slice(&slot[SLOT_OFF_HASH..SLOT_OFF_HASH + HASH_LEN]);
        aux.copy_from_slice(&slot[SLOT_OFF_AUX..SLOT_OFF_AUX + AUX_LEN]);
        let payload = slot[SLOT_HEADER_SIZE_V2..SLOT_HEADER_SIZE_V2 + payload_len]
            .to_vec()
            .into_boxed_slice();

        let record = OwnedRecordV2 {
            sequence: read,
            lane_id: self.config.lane_id,
            lane_epoch,
            kind,
            flags: get_u16(slot, SLOT_OFF_FLAGS),
            timestamp_ns: get_u64(slot, SLOT_OFF_TIMESTAMP),
            clock_id: get_u32(slot, SLOT_OFF_CLOCK_ID),
            source,
            peer,
            hash,
            aux,
            payload,
        };
        self.atomic(LH_OFF_READ_SEQUENCE)
            .store(read.wrapping_add(1), Ordering::Release);
        Ok(Some(record))
    }

    /// The consumer's half of the release protocol. On a CLOSING lane whose
    /// records up to `final_write_sequence` have all been consumed, stores
    /// `drained_epoch = lane_epoch` with Release and returns `true`. Returns
    /// `false` while the lane is not CLOSING or records remain. A record past
    /// `final_write_sequence` quarantines the lane and fails; a QUARANTINED
    /// lane fails.
    pub fn acknowledge_drained_if_closing(&self) -> Result<bool, V2Error> {
        let mapping = &self.mapping;
        let lane_offset = self.lane_offset;
        let state = lane_state(mapping, lane_offset);
        self.admit_state(state)?;
        if state != LANE_STATE_CLOSING {
            return Ok(false);
        }
        let final_write =
            lane_u64(mapping, lane_offset, LH_OFF_FINAL_WRITE).load(Ordering::Acquire);
        let write = lane_u64(mapping, lane_offset, LH_OFF_WRITE_SEQUENCE).load(Ordering::Acquire);
        if write != final_write {
            return Err(self.quarantine_late_write(final_write, write));
        }
        let read = lane_u64(mapping, lane_offset, LH_OFF_READ_SEQUENCE).load(Ordering::Acquire);
        if read != final_write {
            return Ok(false);
        }
        let epoch = lane_u64(mapping, lane_offset, LH_OFF_LANE_EPOCH).load(Ordering::Acquire);
        lane_u64(mapping, lane_offset, LH_OFF_DRAINED_EPOCH).store(epoch, Ordering::Release);
        Ok(true)
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
        self.atomic(LH_OFF_QUARANTINE_REASON)
            .load(Ordering::Acquire)
    }

    /// Refuses a QUARANTINED lane and quarantines a lane whose state word
    /// holds no defined state. FREE, BOUND and CLOSING pass.
    fn admit_state(&self, state: u32) -> Result<(), V2Error> {
        match state {
            LANE_STATE_FREE | LANE_STATE_BOUND | LANE_STATE_CLOSING => Ok(()),
            LANE_STATE_QUARANTINED => Err(V2Error::LaneQuarantined {
                reason: self.quarantine_reason(),
            }),
            observed => Err(self.corrupt(
                QUARANTINE_INVALID_STATE,
                V2Error::InvalidLifecycleState { observed },
            )),
        }
    }

    /// Quarantines for a structural defect. Only the call that moves the lane
    /// into QUARANTINED counts the incident and reports the defect.
    fn corrupt(&self, reason: u64, defect: V2Error) -> V2Error {
        if quarantine_lane_at(&self.mapping, self.lane_offset, reason).1 {
            self.atomic(LH_OFF_CORRUPT).fetch_add(1, Ordering::Relaxed);
            defect
        } else {
            V2Error::LaneQuarantined {
                reason: self.quarantine_reason(),
            }
        }
    }

    fn quarantine_late_write(&self, final_write: u64, write: u64) -> V2Error {
        quarantine_lane_at(&self.mapping, self.lane_offset, QUARANTINE_LATE_WRITE);
        V2Error::LateWriteAfterClose { final_write, write }
    }

    pub fn stats(&self) -> LaneStatsV2 {
        lane_stats(&self.mapping, self.lane_offset)
    }

    fn atomic(&self, rel: usize) -> &AtomicU64 {
        self.mapping.atomic_u64(self.lane_offset + rel)
    }
}

/// Size of the peer-lane pool: at most `max_peers` lanes are bound at once;
/// `churn_reserve` further lanes take peers that connect while earlier lanes
/// still drain. Their sum must equal the arena's peer-lane count, so a
/// configuration asking for more peers than the arena qualifies fails at
/// startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolBoundsV2 {
    pub max_peers: usize,
    pub churn_reserve: usize,
}

/// One binding of one peer lane: lane id, owner and the epoch the bind
/// produced. Close and release need all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerLaneLeaseV2 {
    lane_id: u16,
    epoch: u64,
    owner: [u8; OWNER_LEN],
}

impl PeerLaneLeaseV2 {
    pub fn lane_id(&self) -> u16 {
        self.lane_id
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn owner(&self) -> [u8; OWNER_LEN] {
        self.owner
    }
}

/// Pool counters and the current state of every peer lane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoolStatsV2 {
    pub peer_lanes: usize,
    pub free: usize,
    pub bound: usize,
    pub closing: usize,
    pub quarantined: usize,
    pub allocations: u64,
    pub exhausted: u64,
    pub peer_limit_refusals: u64,
    pub releases: u64,
}

/// Bounded allocator of the arena's peer-ingress lanes. Never waits and
/// never spins for a lane: no FREE lane and the peer limit are explicit
/// results, counted in [`PoolStatsV2`]. Only FREE lanes are bound; BOUND,
/// CLOSING and QUARANTINED lanes are never taken.
///
/// Peer connect/disconnect is outside the transaction hot path; a pool shared
/// by several connection handlers belongs behind one mutex.
pub struct PeerLanePoolV2 {
    arena: ArenaV2,
    lanes: Vec<(u16, usize)>,
    bounds: PoolBoundsV2,
    shutting_down: bool,
    cursor: usize,
    allocations: u64,
    exhausted: u64,
    peer_limit_refusals: u64,
    releases: u64,
}

impl PeerLanePoolV2 {
    pub fn new(arena: ArenaV2, bounds: PoolBoundsV2) -> Result<Self, V2Error> {
        let mut lanes = Vec::new();
        for lane_id in arena.peer_lane_ids()? {
            let (_, lane_offset) = locate_lane_by_id(&arena.mapping, lane_id)?;
            lanes.push((lane_id, lane_offset));
        }
        let wanted = bounds.max_peers.checked_add(bounds.churn_reserve);
        if bounds.max_peers == 0 || wanted != Some(lanes.len()) {
            return Err(V2Error::InvalidPoolBounds {
                peer_lanes: lanes.len(),
                max_peers: bounds.max_peers,
                churn_reserve: bounds.churn_reserve,
            });
        }
        Ok(Self {
            arena,
            lanes,
            bounds,
            shutting_down: false,
            cursor: 0,
            allocations: 0,
            exhausted: 0,
            peer_limit_refusals: 0,
            releases: 0,
        })
    }

    /// Binds the next FREE peer lane to `owner`, round-robin from the lane
    /// after the previous allocation. Each lane is examined at most once.
    pub fn allocate(&mut self, owner: [u8; OWNER_LEN]) -> Result<PeerLaneLeaseV2, V2Error> {
        if self.shutting_down {
            return Err(V2Error::Shutdown);
        }
        if self.count(LANE_STATE_BOUND) >= self.bounds.max_peers {
            self.peer_limit_refusals = self.peer_limit_refusals.saturating_add(1);
            return Err(V2Error::PeerLimitReached {
                max_peers: self.bounds.max_peers,
            });
        }
        let count = self.lanes.len();
        for step in 0..count {
            let index = (self.cursor + step) % count;
            let (lane_id, lane_offset) = self.lanes[index];
            if lane_state(&self.arena.mapping, lane_offset) != LANE_STATE_FREE {
                continue;
            }
            match self.arena.bind_peer_lane(lane_id, owner) {
                Ok(epoch) => {
                    self.cursor = (index + 1) % count;
                    self.allocations = self.allocations.saturating_add(1);
                    return Ok(PeerLaneLeaseV2 {
                        lane_id,
                        epoch,
                        owner,
                    });
                }
                // The state left FREE between the check and the
                // compare-and-swap (a concurrent quarantine): not this lane.
                Err(V2Error::LifecycleState { .. }) => continue,
                Err(error) => return Err(error),
            }
        }
        self.exhausted = self.exhausted.saturating_add(1);
        Err(V2Error::PoolExhausted { peer_lanes: count })
    }

    /// BOUND -> CLOSING for the lease. Returns `final_write_sequence`.
    pub fn begin_close(&mut self, lease: &PeerLaneLeaseV2) -> Result<u64, V2Error> {
        self.arena
            .begin_close_peer_lane(lease.lane_id, lease.owner, lease.epoch)
    }

    /// CLOSING -> FREE for the lease, once its drain is acknowledged.
    pub fn release(&mut self, lease: &PeerLaneLeaseV2) -> Result<(), V2Error> {
        self.arena
            .release_peer_lane(lease.lane_id, lease.owner, lease.epoch)?;
        self.releases = self.releases.saturating_add(1);
        Ok(())
    }

    /// Begins shutdown: no further allocation, and `shutdown = 1` on every
    /// lane of the arena, so no publish and no bind succeeds afterwards.
    /// Close and release of existing leases stay possible. Idempotent.
    pub fn begin_shutdown(&mut self) -> Result<(), V2Error> {
        self.shutting_down = true;
        self.arena.begin_shutdown()
    }

    pub fn bounds(&self) -> PoolBoundsV2 {
        self.bounds
    }

    pub fn arena(&self) -> &ArenaV2 {
        &self.arena
    }

    pub fn arena_mut(&mut self) -> &mut ArenaV2 {
        &mut self.arena
    }

    pub fn stats(&self) -> PoolStatsV2 {
        PoolStatsV2 {
            peer_lanes: self.lanes.len(),
            free: self.count(LANE_STATE_FREE),
            bound: self.count(LANE_STATE_BOUND),
            closing: self.count(LANE_STATE_CLOSING),
            quarantined: self.count(LANE_STATE_QUARANTINED),
            allocations: self.allocations,
            exhausted: self.exhausted,
            peer_limit_refusals: self.peer_limit_refusals,
            releases: self.releases,
        }
    }

    fn count(&self, state: u32) -> usize {
        self.lanes
            .iter()
            .filter(|(_, lane_offset)| lane_state(&self.arena.mapping, *lane_offset) == state)
            .count()
    }
}

/// Exclusive runtime roles of one Fast-Lane directory. geth and Rust each
/// hold their own role at the same time; a second process of the same role
/// is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeRoleV2 {
    GethOwner,
    RustOwner,
}

impl RuntimeRoleV2 {
    pub fn name(self) -> &'static str {
        match self {
            Self::GethOwner => "geth-owner",
            Self::RustOwner => "rust-owner",
        }
    }
}

/// An exclusive role lease: `flock(LOCK_EX | LOCK_NB)` on
/// `<dir>/<role>.lock`, held through the open descriptor for the lease's
/// lifetime. The kernel lock is the only source of exclusivity; the file's
/// content (version, role, generation, pid) is diagnostic. Dropping the lease
/// or the death of the process closes the descriptor and releases the lock.
/// The lock file is never deleted, so no second process can lock a fresh
/// inode while the first still holds the old one.
pub struct RoleLeaseV2 {
    _file: File,
    path: PathBuf,
    role: RuntimeRoleV2,
}

impl RoleLeaseV2 {
    pub fn acquire(dir: &Path, role: RuntimeRoleV2, generation: u64) -> Result<Self, V2Error> {
        if generation == 0 {
            return Err(V2Error::ZeroGeneration);
        }
        let path = dir.join(format!("{}.lock", role.name()));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .map_err(V2Error::Io)?;
        // SAFETY: `file` owns a valid descriptor for the duration of the call.
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
                return Err(V2Error::RoleLeaseHeld { role: role.name() });
            }
            return Err(V2Error::Io(error));
        }
        let content = format!(
            "version={ROLE_LEASE_VERSION_V2}\nrole={}\ngeneration={generation}\npid={}\n",
            role.name(),
            std::process::id()
        );
        file.set_len(0).map_err(V2Error::Io)?;
        file.write_all_at(content.as_bytes(), 0)
            .map_err(V2Error::Io)?;
        Ok(Self {
            _file: file,
            path,
            role,
        })
    }

    pub fn role(&self) -> RuntimeRoleV2 {
        self.role
    }

    pub fn path(&self) -> &Path {
        &self.path
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

fn record_crc32c(slot: &[u8], payload_len: usize) -> u32 {
    let mut crc = 0xffff_ffffu32;
    crc = crc32c_update(crc, &slot[8..SLOT_OFF_CRC32C]);
    crc = crc32c_update(
        crc,
        &slot[SLOT_HEADER_SIZE_V2..SLOT_HEADER_SIZE_V2 + payload_len],
    );
    !crc
}

fn crc32c_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0x82f6_3b78 & mask);
        }
    }
    crc
}

pub fn crc32c(bytes: &[u8]) -> u32 {
    !crc32c_update(0xffff_ffff, bytes)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64 as TestAtomicU64, Ordering as TestOrdering};
    use std::thread;
    use std::time::{Duration, Instant};

    static ID: TestAtomicU64 = TestAtomicU64::new(0);

    fn tmp(name: &str) -> std::path::PathBuf {
        let id = ID.fetch_add(1, TestOrdering::Relaxed);
        std::env::temp_dir().join(format!(
            "darkbot-fastlane-v2-{}-{}-{name}.mmap",
            std::process::id(),
            id
        ))
    }

    fn peer_lane(id: u16) -> LaneConfigV2 {
        LaneConfigV2 {
            lane_id: id,
            direction: DirectionV2::GethToRust,
            class: LaneClassV2::PeerIngress,
            flags: 0,
            capacity: 4,
            slot_stride: 512,
        }
    }

    fn txpool_lane(id: u16) -> LaneConfigV2 {
        LaneConfigV2 {
            lane_id: id,
            direction: DirectionV2::GethToRust,
            class: LaneClassV2::TxpoolEvidence,
            flags: 0,
            capacity: 4,
            slot_stride: 512,
        }
    }

    fn owner(tag: u8) -> [u8; 32] {
        [tag; 32]
    }

    fn stats_of(arena: &ArenaV2, lane_id: u16) -> LaneStatsV2 {
        arena.lane_stats(lane_id).unwrap()
    }

    fn offset_of(arena: &ArenaV2, lane_id: u16) -> usize {
        locate_lane_by_id(&arena.mapping, lane_id).unwrap().1
    }

    fn drain_all(consumer: &mut ConsumerV2) -> Vec<OwnedRecordV2> {
        let mut out = Vec::new();
        while let Some(record) = consumer.try_consume().unwrap() {
            out.push(record);
        }
        out
    }

    fn record<'a>(payload: &'a [u8]) -> RecordV2<'a> {
        RecordV2 {
            kind: RecordKind::EarlyTxSeen,
            flags: 3,
            timestamp_ns: 123_456_789,
            clock_id: CLOCK_ID_MONOTONIC,
            source: [0x11; SOURCE_LEN],
            peer: [0x22; PEER_LEN],
            hash: [0x33; HASH_LEN],
            aux: [0x44; AUX_LEN],
            payload,
        }
    }

    #[test]
    fn crc32c_standard_and_cross_language_vectors() {
        assert_eq!(crc32c(b"123456789"), 0xe306_9283);

        let mut slot = vec![0u8; 512];
        let r = record(b"signed-transaction");
        put_u16(&mut slot, SLOT_OFF_KIND, r.kind as u16);
        put_u16(&mut slot, SLOT_OFF_FLAGS, r.flags);
        put_u32(&mut slot, SLOT_OFF_PAYLOAD_LEN, r.payload.len() as u32);
        put_u64(&mut slot, SLOT_OFF_TIMESTAMP, r.timestamp_ns);
        put_u32(&mut slot, SLOT_OFF_CLOCK_ID, r.clock_id);
        slot[SLOT_OFF_SOURCE..SLOT_OFF_SOURCE + SOURCE_LEN].copy_from_slice(&r.source);
        slot[SLOT_OFF_PEER..SLOT_OFF_PEER + PEER_LEN].copy_from_slice(&r.peer);
        slot[SLOT_OFF_HASH..SLOT_OFF_HASH + HASH_LEN].copy_from_slice(&r.hash);
        slot[SLOT_OFF_AUX..SLOT_OFF_AUX + AUX_LEN].copy_from_slice(&r.aux);
        put_u64(&mut slot, SLOT_OFF_LANE_EPOCH, 7);
        slot[SLOT_HEADER_SIZE_V2..SLOT_HEADER_SIZE_V2 + r.payload.len()].copy_from_slice(r.payload);
        assert_eq!(record_crc32c(&slot, r.payload.len()), 0xde71_4cce);
    }

    #[test]
    fn arena_roundtrip_wrap_and_directory_validation() {
        let path = tmp("roundtrip");
        let lanes = [peer_lane(10), peer_lane(11)];
        let mut arena = ArenaV2::create(&path, 7, &lanes).unwrap();
        assert_eq!(arena.mapped_len() % ALIGN_V2, 0);
        arena.bind_peer_lane(10, [0x10; 32]).unwrap();
        drop(arena);

        let mut producer = ProducerV2::open(&path, 7, lanes[0]).unwrap();
        let mut consumer = ConsumerV2::open(&path, 7, lanes[0]).unwrap();

        for n in 0..32u8 {
            producer.publish(&record(&[n])).unwrap();
            let out = consumer.try_consume().unwrap().unwrap();
            assert_eq!(out.sequence, u64::from(n));
            assert_eq!(out.lane_id, 10);
            assert_eq!(out.lane_epoch, 2);
            assert_eq!(&*out.payload, &[n]);
        }

        let mut wrong = lanes[0];
        wrong.capacity = 8;
        assert!(matches!(
            ProducerV2::open(&path, 7, wrong),
            Err(V2Error::LaneMismatch("capacity"))
        ));

        drop(producer);
        drop(consumer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn full_lane_never_overwrites_unread_records() {
        let path = tmp("full");
        let lane = peer_lane(1);
        let mut arena = ArenaV2::create(&path, 9, &[lane]).unwrap();
        arena.bind_peer_lane(1, [0x01; 32]).unwrap();
        drop(arena);
        let mut producer = ProducerV2::open(&path, 9, lane).unwrap();
        let mut consumer = ConsumerV2::open(&path, 9, lane).unwrap();

        for n in 0..lane.capacity {
            producer.publish(&record(&[n as u8])).unwrap();
        }
        assert!(matches!(
            producer.publish(&record(b"x")),
            Err(V2Error::QueueFull { .. })
        ));
        assert_eq!(producer.stats().dropped_full, 1);

        for n in 0..lane.capacity {
            let out = consumer.try_consume().unwrap().unwrap();
            assert_eq!(&*out.payload, &[n as u8]);
        }

        drop(producer);
        drop(consumer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn header_and_payload_corruption_are_both_detected() {
        for target in ["header", "payload"] {
            let path = tmp(target);
            let lane = peer_lane(2);
            let mut arena = ArenaV2::create(&path, 11, &[lane]).unwrap();
            arena.bind_peer_lane(2, [0x02; 32]).unwrap();
            drop(arena);
            let mut producer = ProducerV2::open(&path, 11, lane).unwrap();
            producer.publish(&record(b"abc")).unwrap();

            let (mut mapping, lane_offset) = open_lane(&path, 11, lane).unwrap();
            let slot_offset = lane_offset + LANE_HEADER_SIZE_V2;
            if target == "header" {
                mapping.bytes_mut()[slot_offset + SLOT_OFF_HASH] ^= 0x01;
            } else {
                mapping.bytes_mut()[slot_offset + SLOT_HEADER_SIZE_V2] ^= 0x01;
            }
            drop(mapping);

            let mut consumer = ConsumerV2::open(&path, 11, lane).unwrap();
            assert!(matches!(
                consumer.try_consume(),
                Err(V2Error::ChecksumMismatch { .. })
            ));
            assert_eq!(consumer.stats().corrupt_records, 1);
            assert!(matches!(
                consumer.try_consume(),
                Err(V2Error::LaneQuarantined {
                    reason: QUARANTINE_CHECKSUM
                })
            ));
            let stats = consumer.stats();
            assert_eq!(
                (stats.corrupt_records, stats.read_sequence, stats.state),
                (1, 0, LANE_STATE_QUARANTINED)
            );

            drop(producer);
            drop(consumer);
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn independent_lanes_support_parallel_single_producers() {
        let path = tmp("parallel");
        let lanes = [peer_lane(20), peer_lane(21)];
        let mut arena = ArenaV2::create(&path, 13, &lanes).unwrap();
        arena.bind_peer_lane(20, [0x20; 32]).unwrap();
        arena.bind_peer_lane(21, [0x21; 32]).unwrap();
        drop(arena);

        let p0_path = path.clone();
        let p1_path = path.clone();
        let l0 = lanes[0];
        let l1 = lanes[1];

        let t0 = thread::spawn(move || {
            let mut p = ProducerV2::open(&p0_path, 13, l0).unwrap();
            for n in 0..100u8 {
                loop {
                    match p.publish(&record(&[n])) {
                        Ok(_) => break,
                        Err(V2Error::QueueFull { .. }) => thread::yield_now(),
                        Err(e) => panic!("{e}"),
                    }
                }
            }
        });
        let t1 = thread::spawn(move || {
            let mut p = ProducerV2::open(&p1_path, 13, l1).unwrap();
            for n in 0..100u8 {
                loop {
                    match p.publish(&record(&[n])) {
                        Ok(_) => break,
                        Err(V2Error::QueueFull { .. }) => thread::yield_now(),
                        Err(e) => panic!("{e}"),
                    }
                }
            }
        });

        let mut c0 = ConsumerV2::open(&path, 13, lanes[0]).unwrap();
        let mut c1 = ConsumerV2::open(&path, 13, lanes[1]).unwrap();
        let mut got0 = 0;
        let mut got1 = 0;
        while got0 < 100 || got1 < 100 {
            if got0 < 100 && c0.try_consume().unwrap().is_some() {
                got0 += 1;
            }
            if got1 < 100 && c1.try_consume().unwrap().is_some() {
                got1 += 1;
            }
            thread::yield_now();
        }
        t0.join().unwrap();
        t1.join().unwrap();
        assert_eq!(got0, 100);
        assert_eq!(got1, 100);

        drop(c0);
        drop(c1);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn peer_lane_lifecycle_requires_drain_before_reuse() {
        let path = tmp("lifecycle");
        let lane = peer_lane(30);
        let mut arena = ArenaV2::create(&path, 21, &[lane]).unwrap();
        let owner_a = [0xa1; 32];
        let owner_b = [0xb2; 32];

        let (_, lane_offset) = locate_lane_by_id(&arena.mapping, 30).unwrap();
        assert_eq!(
            lane_stats(&arena.mapping, lane_offset).state,
            LANE_STATE_FREE
        );

        let epoch_a = arena.bind_peer_lane(30, owner_a).unwrap();
        assert_eq!(epoch_a, 2);

        let mut producer = ProducerV2::open(&path, 21, lane).unwrap();
        let mut consumer = ConsumerV2::open(&path, 21, lane).unwrap();
        producer.publish(&record(b"one")).unwrap();

        let final_write = arena.begin_close_peer_lane(30, owner_a, epoch_a).unwrap();
        assert_eq!(final_write, 1);
        assert!(matches!(
            arena.release_peer_lane(30, owner_a, epoch_a),
            Err(V2Error::LaneNotDrained { .. })
        ));
        assert!(!consumer.acknowledge_drained_if_closing().unwrap());

        assert!(consumer.try_consume().unwrap().is_some());
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        arena.release_peer_lane(30, owner_a, epoch_a).unwrap();

        let stats = lane_stats(&arena.mapping, lane_offset);
        assert_eq!(stats.state, LANE_STATE_FREE);
        assert_eq!(stats.owner, [0; 32]);
        assert_eq!(stats.write_sequence, stats.read_sequence);

        let epoch_b = arena.bind_peer_lane(30, owner_b).unwrap();
        assert_eq!(epoch_b, 3);

        drop(producer);
        drop(consumer);
        drop(arena);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn peer_lane_lifecycle_rejects_wrong_owner_and_wrong_state() {
        let path = tmp("owner");
        let lane = peer_lane(31);
        let mut arena = ArenaV2::create(&path, 22, &[lane]).unwrap();
        let owner = [0x11; 32];
        let wrong = [0x22; 32];

        let epoch = arena.bind_peer_lane(31, owner).unwrap();
        assert!(matches!(
            arena.bind_peer_lane(31, owner),
            Err(V2Error::LifecycleState { .. })
        ));
        assert!(matches!(
            arena.begin_close_peer_lane(31, wrong, epoch),
            Err(V2Error::LaneMismatch("owner"))
        ));
        arena.begin_close_peer_lane(31, owner, epoch).unwrap();

        let consumer = ConsumerV2::open(&path, 22, lane).unwrap();
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        assert!(matches!(
            arena.release_peer_lane(31, wrong, epoch),
            Err(V2Error::LaneMismatch("owner"))
        ));
        arena.release_peer_lane(31, owner, epoch).unwrap();

        drop(consumer);
        drop(arena);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn free_peer_lane_refuses_publish_and_wrong_generation_is_quarantined() {
        let path = tmp("generation");
        let lane = peer_lane(32);
        let arena = ArenaV2::create(&path, 23, &[lane]).unwrap();
        drop(arena);

        let mut producer = ProducerV2::open(&path, 23, lane).unwrap();
        assert!(matches!(
            producer.publish(&record(b"x")),
            Err(V2Error::LifecycleState {
                expected: LANE_STATE_BOUND,
                observed: LANE_STATE_FREE
            })
        ));
        assert!(matches!(
            ProducerV2::open(&path, 24, lane),
            Err(V2Error::GenerationMismatch { .. })
        ));

        drop(producer);
        fs::remove_file(path).unwrap();
    }

    // -----------------------------------------------------------------
    // C1.3 lifecycle
    // -----------------------------------------------------------------

    #[test]
    fn lifecycle_free_bound_closing_drained_free_and_second_bind() {
        let path = tmp("c13-cycle");
        let lane = peer_lane(40);
        let mut arena = ArenaV2::create(&path, 41, &[lane]).unwrap();
        let s = stats_of(&arena, 40);
        assert_eq!(
            (s.state, s.lane_epoch, s.owner),
            (LANE_STATE_FREE, 1, [0; 32])
        );

        // FREE -> BOUND: new epoch, owner written.
        let epoch_a = arena.bind_peer_lane(40, owner(0xa1)).unwrap();
        let s = stats_of(&arena, 40);
        assert_eq!(
            (epoch_a, s.state, s.lane_epoch, s.owner),
            (2, LANE_STATE_BOUND, 2, owner(0xa1))
        );

        let mut producer = ProducerV2::open(&path, 41, lane).unwrap();
        let mut consumer = ConsumerV2::open(&path, 41, lane).unwrap();
        producer.publish(&record(b"a0")).unwrap();
        producer.publish(&record(b"a1")).unwrap();

        // BOUND -> CLOSING: final_write frozen at write_sequence.
        let final_write = arena
            .begin_close_peer_lane(40, owner(0xa1), epoch_a)
            .unwrap();
        let s = stats_of(&arena, 40);
        assert_eq!(
            (
                final_write,
                s.state,
                s.final_write_sequence,
                s.write_sequence
            ),
            (2, LANE_STATE_CLOSING, 2, 2)
        );

        // CLOSING -> drained: every record of the old owner, then the ack.
        let drained = drain_all(&mut consumer);
        assert_eq!(drained.len(), 2);
        assert!(drained.iter().all(|r| r.lane_epoch == epoch_a));
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        assert_eq!(stats_of(&arena, 40).drained_epoch, epoch_a);

        // drained -> FREE: owner and final_write cleared, sequences kept.
        arena.release_peer_lane(40, owner(0xa1), epoch_a).unwrap();
        let s = stats_of(&arena, 40);
        assert_eq!(
            (s.state, s.owner, s.final_write_sequence),
            (LANE_STATE_FREE, [0; 32], 0)
        );
        assert_eq!((s.write_sequence, s.read_sequence), (2, 2));

        // FREE -> second BOUND: the epoch advances, sequences continue.
        let epoch_b = arena.bind_peer_lane(40, owner(0xb2)).unwrap();
        assert_eq!(epoch_b, epoch_a + 1);
        assert_eq!(stats_of(&arena, 40).owner, owner(0xb2));
        producer.publish(&record(b"b0")).unwrap();
        let out = consumer.try_consume().unwrap().unwrap();
        assert_eq!(
            (out.sequence, out.lane_epoch, &*out.payload),
            (2, epoch_b, &b"b0"[..])
        );

        drop((producer, consumer, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn publish_while_free_is_rejected_and_writes_nothing() {
        let path = tmp("c13-free-publish");
        let lane = peer_lane(41);
        let arena = ArenaV2::create(&path, 42, &[lane]).unwrap();
        let mut producer = ProducerV2::open(&path, 42, lane).unwrap();
        assert!(matches!(
            producer.publish(&record(b"x")),
            Err(V2Error::LifecycleState {
                expected: LANE_STATE_BOUND,
                observed: LANE_STATE_FREE
            })
        ));
        let s = producer.stats();
        assert_eq!((s.write_sequence, s.dropped_full, s.oversize), (0, 0, 0));
        let slot = offset_of(&arena, 41) + LANE_HEADER_SIZE_V2;
        assert_eq!(
            arena
                .mapping
                .atomic_u64(slot + SLOT_OFF_COMMITTED)
                .load(Ordering::Acquire),
            EMPTY_SEQUENCE
        );
        drop((producer, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn publish_after_close_is_rejected() {
        let path = tmp("c13-closing-publish");
        let lane = peer_lane(42);
        let mut arena = ArenaV2::create(&path, 43, &[lane]).unwrap();
        let epoch = arena.bind_peer_lane(42, owner(1)).unwrap();
        let mut producer = ProducerV2::open(&path, 43, lane).unwrap();
        producer.publish(&record(b"last")).unwrap();
        let final_write = arena.begin_close_peer_lane(42, owner(1), epoch).unwrap();
        assert!(matches!(
            producer.publish(&record(b"late")),
            Err(V2Error::LifecycleState {
                expected: LANE_STATE_BOUND,
                observed: LANE_STATE_CLOSING
            })
        ));
        assert_eq!(producer.stats().write_sequence, final_write);
        drop((producer, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn double_bind_is_rejected_without_side_effects() {
        let path = tmp("c13-double-bind");
        let lane = peer_lane(43);
        let mut arena = ArenaV2::create(&path, 44, &[lane]).unwrap();
        let epoch = arena.bind_peer_lane(43, owner(1)).unwrap();
        assert!(matches!(
            arena.bind_peer_lane(43, owner(2)),
            Err(V2Error::LifecycleState {
                expected: LANE_STATE_FREE,
                observed: LANE_STATE_BOUND
            })
        ));
        let s = stats_of(&arena, 43);
        assert_eq!(
            (s.state, s.lane_epoch, s.owner),
            (LANE_STATE_BOUND, epoch, owner(1))
        );
        drop(arena);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn close_requires_the_owner_and_the_epoch_of_the_binding() {
        let path = tmp("c13-close-owner");
        let lane = peer_lane(44);
        let mut arena = ArenaV2::create(&path, 45, &[lane]).unwrap();
        let epoch = arena.bind_peer_lane(44, owner(1)).unwrap();
        assert!(matches!(
            arena.begin_close_peer_lane(44, owner(2), epoch),
            Err(V2Error::LaneMismatch("owner"))
        ));
        assert!(matches!(
            arena.begin_close_peer_lane(44, owner(1), epoch + 1),
            Err(V2Error::LeaseEpochMismatch { expected, observed })
                if expected == epoch && observed == epoch + 1
        ));
        let s = stats_of(&arena, 44);
        assert_eq!((s.state, s.final_write_sequence), (LANE_STATE_BOUND, 0));
        drop(arena);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn release_requires_the_owner_and_the_epoch_of_the_binding() {
        let path = tmp("c13-release-owner");
        let lane = peer_lane(45);
        let mut arena = ArenaV2::create(&path, 46, &[lane]).unwrap();
        let epoch = arena.bind_peer_lane(45, owner(1)).unwrap();
        arena.begin_close_peer_lane(45, owner(1), epoch).unwrap();
        let consumer = ConsumerV2::open(&path, 46, lane).unwrap();
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        assert!(matches!(
            arena.release_peer_lane(45, owner(2), epoch),
            Err(V2Error::LaneMismatch("owner"))
        ));
        assert!(matches!(
            arena.release_peer_lane(45, owner(1), epoch - 1),
            Err(V2Error::LeaseEpochMismatch { expected, observed })
                if expected == epoch && observed == epoch - 1
        ));
        let s = stats_of(&arena, 45);
        assert_eq!((s.state, s.owner), (LANE_STATE_CLOSING, owner(1)));
        arena.release_peer_lane(45, owner(1), epoch).unwrap();
        drop((consumer, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn release_before_the_drain_is_rejected() {
        let path = tmp("c13-undrained");
        let lane = peer_lane(46);
        let mut arena = ArenaV2::create(&path, 47, &[lane]).unwrap();
        let epoch = arena.bind_peer_lane(46, owner(1)).unwrap();
        let mut producer = ProducerV2::open(&path, 47, lane).unwrap();
        let consumer = ConsumerV2::open(&path, 47, lane).unwrap();
        producer.publish(&record(b"unread")).unwrap();
        arena.begin_close_peer_lane(46, owner(1), epoch).unwrap();
        assert!(!consumer.acknowledge_drained_if_closing().unwrap());
        assert!(matches!(
            arena.release_peer_lane(46, owner(1), epoch),
            Err(V2Error::LaneNotDrained {
                read: 0,
                final_write: 1
            })
        ));
        let s = stats_of(&arena, 46);
        assert_eq!((s.state, s.drained_epoch), (LANE_STATE_CLOSING, 0));
        drop((producer, consumer, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn an_unread_record_prevents_reuse_until_it_is_consumed() {
        let path = tmp("c13-unread");
        let lane = peer_lane(47);
        let mut arena = ArenaV2::create(&path, 48, &[lane]).unwrap();
        let epoch = arena.bind_peer_lane(47, owner(1)).unwrap();
        let mut producer = ProducerV2::open(&path, 48, lane).unwrap();
        let mut consumer = ConsumerV2::open(&path, 48, lane).unwrap();
        for n in 0..3u8 {
            producer.publish(&record(&[n])).unwrap();
        }
        arena.begin_close_peer_lane(47, owner(1), epoch).unwrap();
        consumer.try_consume().unwrap().unwrap();
        consumer.try_consume().unwrap().unwrap();
        assert!(!consumer.acknowledge_drained_if_closing().unwrap());
        assert!(matches!(
            arena.release_peer_lane(47, owner(1), epoch),
            Err(V2Error::LaneNotDrained {
                read: 2,
                final_write: 3
            })
        ));
        assert!(matches!(
            arena.bind_peer_lane(47, owner(2)),
            Err(V2Error::LifecycleState {
                expected: LANE_STATE_FREE,
                observed: LANE_STATE_CLOSING
            })
        ));

        assert_eq!(&*consumer.try_consume().unwrap().unwrap().payload, &[2]);
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        arena.release_peer_lane(47, owner(1), epoch).unwrap();
        assert_eq!(arena.bind_peer_lane(47, owner(2)).unwrap(), epoch + 1);
        drop((producer, consumer, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn an_empty_closing_lane_drains_at_once() {
        let path = tmp("c13-empty-closing");
        let lane = peer_lane(48);
        let mut arena = ArenaV2::create(&path, 49, &[lane]).unwrap();
        let epoch = arena.bind_peer_lane(48, owner(1)).unwrap();
        assert_eq!(arena.begin_close_peer_lane(48, owner(1), epoch).unwrap(), 0);
        let consumer = ConsumerV2::open(&path, 49, lane).unwrap();
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        assert_eq!(stats_of(&arena, 48).drained_epoch, epoch);
        arena.release_peer_lane(48, owner(1), epoch).unwrap();
        assert_eq!(stats_of(&arena, 48).state, LANE_STATE_FREE);
        drop((consumer, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_wrong_arena_or_lane_generation_is_rejected() {
        let path = tmp("c13-generation");
        let lane = peer_lane(50);
        drop(ArenaV2::create(&path, 50, &[lane]).unwrap());
        assert!(matches!(
            ArenaV2::open(&path, 51),
            Err(V2Error::GenerationMismatch {
                expected: 51,
                observed: 50
            })
        ));
        assert!(matches!(
            ProducerV2::open(&path, 51, lane),
            Err(V2Error::GenerationMismatch { .. })
        ));
        assert!(matches!(
            ConsumerV2::open(&path, 51, lane),
            Err(V2Error::GenerationMismatch { .. })
        ));
        assert!(matches!(
            ArenaV2::open(&path, 0),
            Err(V2Error::ZeroGeneration)
        ));

        // A lane header that disagrees with the arena generation.
        let (mapping, lane_offset) = open_lane(&path, 50, lane).unwrap();
        mapping
            .atomic_u64(lane_offset + LH_OFF_GENERATION)
            .store(49, Ordering::Release);
        drop(mapping);
        assert!(matches!(
            ArenaV2::open(&path, 50),
            Err(V2Error::GenerationMismatch {
                expected: 50,
                observed: 49
            })
        ));
        assert!(matches!(
            ConsumerV2::open(&path, 50, lane),
            Err(V2Error::GenerationMismatch { .. })
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_record_of_another_epoch_quarantines_the_lane_once() {
        let path = tmp("c13-record-epoch");
        let lane = peer_lane(51);
        let mut arena = ArenaV2::create(&path, 52, &[lane]).unwrap();
        let epoch = arena.bind_peer_lane(51, owner(1)).unwrap();
        let mut producer = ProducerV2::open(&path, 52, lane).unwrap();
        producer.publish(&record(b"r")).unwrap();

        // Rewrite the record's epoch and re-seal its CRC, so that only the
        // epoch check can refuse it.
        let (mut mapping, lane_offset) = open_lane(&path, 52, lane).unwrap();
        let slot_offset = lane_offset + LANE_HEADER_SIZE_V2;
        {
            let slot = &mut mapping.bytes_mut()[slot_offset..slot_offset + lane.slot_stride];
            put_u64(slot, SLOT_OFF_LANE_EPOCH, epoch + 7);
            let payload_len = get_u32(slot, SLOT_OFF_PAYLOAD_LEN) as usize;
            let crc = record_crc32c(slot, payload_len);
            put_u32(slot, SLOT_OFF_CRC32C, crc);
        }
        drop(mapping);

        let mut consumer = ConsumerV2::open(&path, 52, lane).unwrap();
        assert!(matches!(
            consumer.try_consume(),
            Err(V2Error::LaneEpochMismatch { expected, observed })
                if expected == epoch && observed == epoch + 7
        ));
        for _ in 0..4 {
            assert!(matches!(
                consumer.try_consume(),
                Err(V2Error::LaneQuarantined {
                    reason: QUARANTINE_EPOCH_MISMATCH
                })
            ));
        }
        let s = consumer.stats();
        assert_eq!(
            (
                s.state,
                s.corrupt_records,
                s.read_sequence,
                s.quarantine_reason
            ),
            (LANE_STATE_QUARANTINED, 1, 0, QUARANTINE_EPOCH_MISMATCH)
        );
        drop((producer, consumer, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn lane_epoch_exhaustion_fails_closed() {
        let path = tmp("c13-epoch-exhaustion");
        let lane = peer_lane(52);
        let mut arena = ArenaV2::create(&path, 53, &[lane]).unwrap();
        let lane_offset = offset_of(&arena, 52);

        arena
            .mapping
            .atomic_u64(lane_offset + LH_OFF_LANE_EPOCH)
            .store(u64::MAX, Ordering::Release);
        assert!(matches!(
            arena.bind_peer_lane(52, owner(1)),
            Err(V2Error::EpochExhausted)
        ));
        let s = stats_of(&arena, 52);
        assert_eq!(
            (s.state, s.lane_epoch, s.owner),
            (LANE_STATE_FREE, u64::MAX, [0; 32])
        );

        // The last epoch is usable once; the bind after it is refused.
        arena
            .mapping
            .atomic_u64(lane_offset + LH_OFF_LANE_EPOCH)
            .store(u64::MAX - 1, Ordering::Release);
        let last = arena.bind_peer_lane(52, owner(1)).unwrap();
        assert_eq!(last, u64::MAX);
        arena.begin_close_peer_lane(52, owner(1), last).unwrap();
        let consumer = ConsumerV2::open(&path, 53, lane).unwrap();
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        arena.release_peer_lane(52, owner(1), last).unwrap();
        assert!(matches!(
            arena.bind_peer_lane(52, owner(2)),
            Err(V2Error::EpochExhausted)
        ));
        assert_eq!(stats_of(&arena, 52).state, LANE_STATE_FREE);
        drop((consumer, arena));
        fs::remove_file(path).unwrap();
    }

    fn peer_snapshot(pool: &PeerLanePoolV2, ids: &[u16]) -> Vec<LaneStatsV2> {
        ids.iter()
            .map(|id| pool.arena().lane_stats(*id).unwrap())
            .collect()
    }

    #[test]
    fn the_pool_binds_only_free_lanes_and_reports_exhaustion_without_waiting() {
        let path = tmp("c13-pool");
        let ids = [60u16, 61, 62];
        let lanes = [
            peer_lane(60),
            peer_lane(61),
            peer_lane(62),
            txpool_lane(100),
        ];
        let arena = ArenaV2::create(&path, 60, &lanes).unwrap();
        let mut pool = PeerLanePoolV2::new(
            arena,
            PoolBoundsV2 {
                max_peers: 2,
                churn_reserve: 1,
            },
        )
        .unwrap();
        assert_eq!(pool.stats().peer_lanes, 3);

        let a = pool.allocate(owner(0xa)).unwrap();
        let b = pool.allocate(owner(0xb)).unwrap();
        assert_ne!(a.lane_id(), b.lane_id());
        assert!(ids.contains(&a.lane_id()) && ids.contains(&b.lane_id()));

        // The peer limit is explicit and changes nothing.
        let started = Instant::now();
        let before = peer_snapshot(&pool, &ids);
        assert!(matches!(
            pool.allocate(owner(0xc)),
            Err(V2Error::PeerLimitReached { max_peers: 2 })
        ));
        assert_eq!(peer_snapshot(&pool, &ids), before);

        // A closing (still draining) lane frees a peer slot; the reserve lane
        // takes the new peer.
        pool.begin_close(&a).unwrap();
        let c = pool.allocate(owner(0xc)).unwrap();
        assert!(c.lane_id() != a.lane_id() && c.lane_id() != b.lane_id());

        // No FREE lane left: explicit, nothing stolen.
        pool.begin_close(&b).unwrap();
        let before = peer_snapshot(&pool, &ids);
        assert!(matches!(
            pool.allocate(owner(0xd)),
            Err(V2Error::PoolExhausted { peer_lanes: 3 })
        ));
        assert_eq!(peer_snapshot(&pool, &ids), before);
        assert!(started.elapsed() < Duration::from_secs(1));

        let stats = pool.stats();
        assert_eq!(
            (stats.free, stats.bound, stats.closing, stats.quarantined),
            (0, 1, 2, 0)
        );
        assert_eq!(
            (
                stats.allocations,
                stats.exhausted,
                stats.peer_limit_refusals
            ),
            (3, 1, 1)
        );

        // Once a's lane is drained and released, it is the one reused.
        let consumer_a = ConsumerV2::open(&path, 60, peer_lane(a.lane_id())).unwrap();
        assert!(consumer_a.acknowledge_drained_if_closing().unwrap());
        pool.release(&a).unwrap();
        let d = pool.allocate(owner(0xd)).unwrap();
        assert_eq!((d.lane_id(), d.epoch()), (a.lane_id(), a.epoch() + 1));

        drop((consumer_a, pool));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn the_pool_bounds_must_match_the_arena() {
        let path = tmp("c13-pool-bounds");
        let lanes = [peer_lane(63), peer_lane(64), txpool_lane(101)];
        drop(ArenaV2::create(&path, 61, &lanes).unwrap());
        for (max_peers, churn_reserve) in [(0, 2), (3, 0), (2, 1), (1, 0)] {
            let arena = ArenaV2::open(&path, 61).unwrap();
            assert!(matches!(
                PeerLanePoolV2::new(
                    arena,
                    PoolBoundsV2 {
                        max_peers,
                        churn_reserve
                    }
                ),
                Err(V2Error::InvalidPoolBounds { peer_lanes: 2, .. })
            ));
        }
        let arena = ArenaV2::open(&path, 61).unwrap();
        let pool = PeerLanePoolV2::new(
            arena,
            PoolBoundsV2 {
                max_peers: 1,
                churn_reserve: 1,
            },
        )
        .unwrap();
        assert_eq!(pool.stats().peer_lanes, 2);
        drop(pool);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn churn_keeps_epochs_monotonic_owners_apart_and_sequences_continuous() {
        let path = tmp("c13-churn");
        let lane = peer_lane(70);
        let arena = ArenaV2::create(&path, 70, &[lane]).unwrap();
        let mut pool = PeerLanePoolV2::new(
            arena,
            PoolBoundsV2 {
                max_peers: 1,
                churn_reserve: 0,
            },
        )
        .unwrap();
        let mut producer = ProducerV2::open(&path, 70, lane).unwrap();
        let mut consumer = ConsumerV2::open(&path, 70, lane).unwrap();
        let mut previous_epoch = 1u64;
        let mut next_sequence = 0u64;

        for cycle in 0..200u32 {
            let tag = cycle.to_le_bytes();
            let mut owner_bytes = [0u8; 32];
            owner_bytes[..4].copy_from_slice(&tag);
            let lease = pool.allocate(owner_bytes).unwrap();
            assert_eq!(lease.epoch(), previous_epoch + 1);
            previous_epoch = lease.epoch();

            let records = (cycle % 5) as usize;
            for index in 0..records {
                let mut payload = [0u8; 8];
                payload[..4].copy_from_slice(&tag);
                payload[4..].copy_from_slice(&(index as u32).to_le_bytes());
                producer.publish(&record(&payload)).unwrap();
            }
            let final_write = pool.begin_close(&lease).unwrap();
            assert_eq!(final_write, next_sequence + records as u64);

            // Only this binding's records, in order, under this epoch.
            let drained = drain_all(&mut consumer);
            assert_eq!(drained.len(), records);
            for (index, out) in drained.iter().enumerate() {
                assert_eq!(out.sequence, next_sequence + index as u64);
                assert_eq!(out.lane_epoch, lease.epoch());
                assert_eq!(&out.payload[..4], &tag);
                assert_eq!(&out.payload[4..], &(index as u32).to_le_bytes());
            }
            next_sequence += records as u64;

            assert!(consumer.acknowledge_drained_if_closing().unwrap());
            pool.release(&lease).unwrap();
            let s = pool.arena().lane_stats(70).unwrap();
            assert_eq!(
                (s.state, s.owner, s.write_sequence, s.read_sequence),
                (LANE_STATE_FREE, [0; 32], next_sequence, next_sequence)
            );
        }
        let stats = pool.stats();
        assert_eq!((stats.allocations, stats.releases), (200, 200));
        drop((producer, consumer, pool));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_stale_lease_never_acts_on_a_reused_lane_even_with_the_same_owner() {
        let path = tmp("c13-stale-lease");
        let lane = peer_lane(71);
        let arena = ArenaV2::create(&path, 71, &[lane]).unwrap();
        let mut pool = PeerLanePoolV2::new(
            arena,
            PoolBoundsV2 {
                max_peers: 1,
                churn_reserve: 0,
            },
        )
        .unwrap();
        let consumer = ConsumerV2::open(&path, 71, lane).unwrap();

        let first = pool.allocate(owner(7)).unwrap();
        pool.begin_close(&first).unwrap();
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        pool.release(&first).unwrap();
        let second = pool.allocate(owner(7)).unwrap();
        assert_eq!(second.epoch(), first.epoch() + 1);

        assert!(matches!(
            pool.begin_close(&first),
            Err(V2Error::LeaseEpochMismatch { expected, observed })
                if expected == second.epoch() && observed == first.epoch()
        ));
        assert!(matches!(
            pool.release(&first),
            Err(V2Error::LifecycleState {
                expected: LANE_STATE_CLOSING,
                observed: LANE_STATE_BOUND
            })
        ));
        let s = pool.arena().lane_stats(71).unwrap();
        assert_eq!(
            (s.state, s.lane_epoch, s.owner),
            (LANE_STATE_BOUND, second.epoch(), owner(7))
        );
        pool.begin_close(&second).unwrap();
        drop((consumer, pool));
        fs::remove_file(path).unwrap();
    }

    /// Binds `lane_id` once, fills all four slots (committed 1..=4, epoch
    /// 2), drains, releases and binds again. Returns (first, second) epoch.
    fn reuse_after_full_ring(
        arena: &mut ArenaV2,
        producer: &mut ProducerV2,
        consumer: &mut ConsumerV2,
        lane_id: u16,
    ) -> (u64, u64) {
        let first = arena.bind_peer_lane(lane_id, owner(1)).unwrap();
        for n in 0..4u8 {
            producer.publish(&record(&[n])).unwrap();
        }
        assert_eq!(drain_all(consumer).len(), 4);
        arena
            .begin_close_peer_lane(lane_id, owner(1), first)
            .unwrap();
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        arena.release_peer_lane(lane_id, owner(1), first).unwrap();
        let second = arena.bind_peer_lane(lane_id, owner(2)).unwrap();
        (first, second)
    }

    #[test]
    fn slots_of_an_earlier_binding_are_never_accepted_after_reuse() {
        let path = tmp("c13-old-slots");
        let lanes = [peer_lane(72), peer_lane(75)];
        let mut arena = ArenaV2::create(&path, 72, &lanes).unwrap();

        // A write_sequence that runs ahead of its slot: slot 0 still holds
        // committed_sequence 1 from the first binding, never 5.
        let mut producer = ProducerV2::open(&path, 72, lanes[0]).unwrap();
        let mut consumer = ConsumerV2::open(&path, 72, lanes[0]).unwrap();
        reuse_after_full_ring(&mut arena, &mut producer, &mut consumer, 72);
        let (mapping, lane_offset) = open_lane(&path, 72, lanes[0]).unwrap();
        mapping
            .atomic_u64(lane_offset + LH_OFF_WRITE_SEQUENCE)
            .store(5, Ordering::Release);
        assert!(matches!(
            consumer.try_consume(),
            Err(V2Error::CorruptSequence {
                expected: 5,
                observed: 1
            })
        ));
        assert_eq!(consumer.stats().read_sequence, 4);
        assert_eq!(consumer.lifecycle_state(), LANE_STATE_QUARANTINED);

        // Even with a matching committed_sequence, the old record's epoch is
        // refused.
        let mut producer_b = ProducerV2::open(&path, 72, lanes[1]).unwrap();
        let mut consumer_b = ConsumerV2::open(&path, 72, lanes[1]).unwrap();
        let (first, second) =
            reuse_after_full_ring(&mut arena, &mut producer_b, &mut consumer_b, 75);
        let (mapping_b, lane_offset_b) = open_lane(&path, 72, lanes[1]).unwrap();
        mapping_b
            .atomic_u64(lane_offset_b + LH_OFF_WRITE_SEQUENCE)
            .store(5, Ordering::Release);
        mapping_b
            .atomic_u64(lane_offset_b + LANE_HEADER_SIZE_V2 + SLOT_OFF_COMMITTED)
            .store(5, Ordering::Release);
        assert!(matches!(
            consumer_b.try_consume(),
            Err(V2Error::LaneEpochMismatch { expected, observed })
                if expected == second && observed == first
        ));
        assert_eq!(consumer_b.stats().read_sequence, 4);
        drop((
            mapping, mapping_b, producer, consumer, producer_b, consumer_b, arena,
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_write_after_close_quarantines_the_lane() {
        let path = tmp("c13-late-write");
        let lanes = [peer_lane(73), peer_lane(74)];
        let mut arena = ArenaV2::create(&path, 73, &lanes).unwrap();

        // Detected by the consumer's drain acknowledgement.
        let epoch = arena.bind_peer_lane(73, owner(1)).unwrap();
        let mut producer = ProducerV2::open(&path, 73, lanes[0]).unwrap();
        let mut consumer = ConsumerV2::open(&path, 73, lanes[0]).unwrap();
        producer.publish(&record(b"0")).unwrap();
        assert_eq!(arena.begin_close_peer_lane(73, owner(1), epoch).unwrap(), 1);
        // A producer that passed its state check before the close lands one
        // more record.
        let (mapping, lane_offset) = open_lane(&path, 73, lanes[0]).unwrap();
        mapping
            .atomic_u64(lane_offset + LH_OFF_WRITE_SEQUENCE)
            .store(2, Ordering::Release);
        consumer.try_consume().unwrap().unwrap();
        assert!(matches!(
            consumer.acknowledge_drained_if_closing(),
            Err(V2Error::LateWriteAfterClose {
                final_write: 1,
                write: 2
            })
        ));
        let s = stats_of(&arena, 73);
        assert_eq!(
            (s.state, s.shutdown, s.quarantine_reason, s.corrupt_records),
            (LANE_STATE_QUARANTINED, true, QUARANTINE_LATE_WRITE, 0)
        );
        assert!(matches!(
            arena.release_peer_lane(73, owner(1), epoch),
            Err(V2Error::LifecycleState {
                expected: LANE_STATE_CLOSING,
                observed: LANE_STATE_QUARANTINED
            })
        ));
        assert!(matches!(
            producer.publish(&record(b"x")),
            Err(V2Error::LifecycleState {
                observed: LANE_STATE_QUARANTINED,
                ..
            })
        ));

        // Detected by the release after an acknowledged drain.
        let epoch = arena.bind_peer_lane(74, owner(2)).unwrap();
        arena.begin_close_peer_lane(74, owner(2), epoch).unwrap();
        let consumer_b = ConsumerV2::open(&path, 73, lanes[1]).unwrap();
        assert!(consumer_b.acknowledge_drained_if_closing().unwrap());
        let (mapping_b, lane_offset_b) = open_lane(&path, 73, lanes[1]).unwrap();
        mapping_b
            .atomic_u64(lane_offset_b + LH_OFF_WRITE_SEQUENCE)
            .store(1, Ordering::Release);
        assert!(matches!(
            arena.release_peer_lane(74, owner(2), epoch),
            Err(V2Error::LateWriteAfterClose {
                final_write: 0,
                write: 1
            })
        ));
        assert_eq!(stats_of(&arena, 74).state, LANE_STATE_QUARANTINED);
        assert!(matches!(
            arena.bind_peer_lane(74, owner(3)),
            Err(V2Error::LifecycleState {
                expected: LANE_STATE_FREE,
                observed: LANE_STATE_QUARANTINED
            })
        ));
        drop((mapping, mapping_b, producer, consumer, consumer_b, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn quarantine_is_terminal_and_a_restart_quarantines_the_old_generation() {
        let path = tmp("c13-quarantine");
        let lanes = [peer_lane(80), peer_lane(81), txpool_lane(102)];
        let mut arena = ArenaV2::create(&path, 80, &lanes).unwrap();
        let epoch = arena.bind_peer_lane(80, owner(1)).unwrap();
        let consumer = ConsumerV2::open(&path, 80, lanes[0]).unwrap();
        let mut producer = ProducerV2::open(&path, 80, lanes[0]).unwrap();

        assert_eq!(consumer.quarantine(), LANE_STATE_BOUND);
        assert_eq!(consumer.quarantine(), LANE_STATE_QUARANTINED);
        assert_eq!(consumer.lifecycle_state(), LANE_STATE_QUARANTINED);
        assert!(matches!(
            producer.publish(&record(b"x")),
            Err(V2Error::LifecycleState {
                observed: LANE_STATE_QUARANTINED,
                ..
            })
        ));
        assert!(matches!(
            arena.begin_close_peer_lane(80, owner(1), epoch),
            Err(V2Error::LifecycleState {
                observed: LANE_STATE_QUARANTINED,
                ..
            })
        ));
        assert!(matches!(
            arena.bind_peer_lane(80, owner(2)),
            Err(V2Error::LifecycleState {
                observed: LANE_STATE_QUARANTINED,
                ..
            })
        ));
        drop(arena);

        // Restart: the new runtime quarantines every lane of the old
        // generation before abandoning it. No record is destroyed.
        let mut old = ArenaV2::open(&path, 80).unwrap();
        assert_eq!(old.generation(), 80);
        assert_eq!(old.quarantine_lane(81).unwrap(), LANE_STATE_FREE);
        assert_eq!(old.quarantine_all().unwrap(), 1);
        assert_eq!(old.quarantine_all().unwrap(), 0);
        for (id, reason) in [
            (80u16, QUARANTINE_OPERATOR),
            (81, QUARANTINE_OPERATOR),
            (102, QUARANTINE_RESTART),
        ] {
            let s = old.lane_stats(id).unwrap();
            assert_eq!(
                (s.state, s.shutdown, s.quarantine_reason),
                (LANE_STATE_QUARANTINED, true, reason)
            );
        }
        let mut txpool = ProducerV2::open(&path, 80, lanes[2]).unwrap();
        assert!(matches!(
            txpool.publish(&record(b"t")),
            Err(V2Error::LifecycleState {
                observed: LANE_STATE_QUARANTINED,
                ..
            })
        ));
        let mut pool = PeerLanePoolV2::new(
            old,
            PoolBoundsV2 {
                max_peers: 1,
                churn_reserve: 1,
            },
        )
        .unwrap();
        assert!(matches!(
            pool.allocate(owner(9)),
            Err(V2Error::PoolExhausted { peer_lanes: 2 })
        ));
        assert_eq!(pool.stats().quarantined, 2);
        assert!(matches!(
            ArenaV2::open(&path, 81),
            Err(V2Error::GenerationMismatch { .. })
        ));
        drop((consumer, producer, txpool, pool));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn independent_peer_lanes_cycle_concurrently() {
        const LANES: u16 = 4;
        const ROUNDS: u32 = 25;
        const PER_ROUND: u32 = 6;
        let path = tmp("c13-concurrent");
        let configs: Vec<LaneConfigV2> = (0..LANES).map(|i| peer_lane(90 + i)).collect();
        let mut arena = ArenaV2::create(&path, 90, &configs).unwrap();
        let bound_round: Arc<Vec<TestAtomicU64>> =
            Arc::new((0..LANES).map(|_| TestAtomicU64::new(0)).collect());
        let published_round: Arc<Vec<TestAtomicU64>> =
            Arc::new((0..LANES).map(|_| TestAtomicU64::new(0)).collect());

        let lane_owner = |lane: usize, round: u32| {
            let mut bytes = [0u8; 32];
            bytes[0] = lane as u8;
            bytes[1..5].copy_from_slice(&round.to_le_bytes());
            bytes
        };
        let mut owners = Vec::new();
        let mut epochs = Vec::new();
        for (lane, config) in configs.iter().enumerate() {
            owners.push(lane_owner(lane, 0));
            epochs.push(arena.bind_peer_lane(config.lane_id, owners[lane]).unwrap());
            bound_round[lane].store(1, TestOrdering::Release);
        }

        let mut producers = Vec::new();
        for (lane, config) in configs.iter().copied().enumerate() {
            let path = path.clone();
            let bound_round = Arc::clone(&bound_round);
            let published_round = Arc::clone(&published_round);
            producers.push(thread::spawn(move || {
                let mut producer = ProducerV2::open(&path, 90, config).unwrap();
                let deadline = Instant::now() + Duration::from_secs(30);
                for round in 0..ROUNDS {
                    while bound_round[lane].load(TestOrdering::Acquire) != u64::from(round) + 1 {
                        assert!(Instant::now() < deadline, "producer {lane} waited too long");
                        thread::yield_now();
                    }
                    for index in 0..PER_ROUND {
                        let mut payload = [0u8; 12];
                        payload[..4].copy_from_slice(&(lane as u32).to_le_bytes());
                        payload[4..8].copy_from_slice(&round.to_le_bytes());
                        payload[8..].copy_from_slice(&index.to_le_bytes());
                        loop {
                            match producer.publish(&record(&payload)) {
                                Ok(_) => break,
                                Err(V2Error::QueueFull { .. }) => {
                                    assert!(Instant::now() < deadline, "lane {lane} stayed full");
                                    thread::yield_now();
                                }
                                Err(error) => panic!("lane {lane}: {error}"),
                            }
                        }
                    }
                    published_round[lane].store(u64::from(round) + 1, TestOrdering::Release);
                }
            }));
        }

        let mut consumers: Vec<ConsumerV2> = configs
            .iter()
            .map(|config| ConsumerV2::open(&path, 90, *config).unwrap())
            .collect();
        let mut rounds = vec![0u32; LANES as usize];
        let mut consumed = vec![0u32; LANES as usize];
        let mut finished = 0;
        let deadline = Instant::now() + Duration::from_secs(30);
        while finished < LANES as usize {
            for (lane, config) in configs.iter().enumerate() {
                if rounds[lane] == ROUNDS {
                    continue;
                }
                while let Some(out) = consumers[lane].try_consume().unwrap() {
                    assert_eq!(out.lane_epoch, epochs[lane]);
                    let field =
                        |at: usize| u32::from_le_bytes(out.payload[at..at + 4].try_into().unwrap());
                    assert_eq!(
                        (field(0), field(4), field(8)),
                        (lane as u32, rounds[lane], consumed[lane])
                    );
                    consumed[lane] += 1;
                }
                let producer_done = published_round[lane].load(TestOrdering::Acquire)
                    == u64::from(rounds[lane]) + 1;
                if consumed[lane] == PER_ROUND && producer_done {
                    let final_write = arena
                        .begin_close_peer_lane(config.lane_id, owners[lane], epochs[lane])
                        .unwrap();
                    assert_eq!(final_write, u64::from((rounds[lane] + 1) * PER_ROUND));
                    assert!(consumers[lane].acknowledge_drained_if_closing().unwrap());
                    arena
                        .release_peer_lane(config.lane_id, owners[lane], epochs[lane])
                        .unwrap();
                    rounds[lane] += 1;
                    consumed[lane] = 0;
                    if rounds[lane] == ROUNDS {
                        finished += 1;
                        continue;
                    }
                    owners[lane] = lane_owner(lane, rounds[lane]);
                    let next = arena.bind_peer_lane(config.lane_id, owners[lane]).unwrap();
                    assert_eq!(next, epochs[lane] + 1);
                    epochs[lane] = next;
                    bound_round[lane].store(u64::from(rounds[lane]) + 1, TestOrdering::Release);
                }
            }
            assert!(
                Instant::now() < deadline,
                "lifecycle stalled: rounds={rounds:?}"
            );
            thread::yield_now();
        }
        for producer in producers {
            producer.join().unwrap();
        }
        for (lane, config) in configs.iter().enumerate() {
            let s = stats_of(&arena, config.lane_id);
            assert_eq!(s.state, LANE_STATE_FREE);
            assert_eq!(s.lane_epoch, 1 + u64::from(ROUNDS));
            assert_eq!(s.read_sequence, u64::from(ROUNDS * PER_ROUND));
            assert_eq!(epochs[lane], 1 + u64::from(ROUNDS));
        }
        drop((consumers, arena));
        fs::remove_file(path).unwrap();
    }

    // -----------------------------------------------------------------
    // K12: structural corruption quarantines once, never skips
    // -----------------------------------------------------------------

    #[test]
    fn every_structural_defect_quarantines_once_and_is_never_skipped() {
        for (defect, reason) in [
            ("sequence", QUARANTINE_CORRUPT_SEQUENCE),
            ("payload-length", QUARANTINE_PAYLOAD_LENGTH),
            ("epoch", QUARANTINE_EPOCH_MISMATCH),
            ("crc", QUARANTINE_CHECKSUM),
            ("kind", QUARANTINE_RECORD_KIND),
        ] {
            let path = tmp(defect);
            let lane = peer_lane(110);
            let mut arena = ArenaV2::create(&path, 110, &[lane]).unwrap();
            let epoch = arena.bind_peer_lane(110, owner(1)).unwrap();
            let mut producer = ProducerV2::open(&path, 110, lane).unwrap();
            producer.publish(&record(b"bad")).unwrap();
            producer.publish(&record(b"good")).unwrap();

            let (mut mapping, lane_offset) = open_lane(&path, 110, lane).unwrap();
            let slot_offset = lane_offset + LANE_HEADER_SIZE_V2;
            match defect {
                "sequence" => mapping
                    .atomic_u64(slot_offset + SLOT_OFF_COMMITTED)
                    .store(7, Ordering::Release),
                _ => {
                    let slot =
                        &mut mapping.bytes_mut()[slot_offset..slot_offset + lane.slot_stride];
                    match defect {
                        "payload-length" => {
                            put_u32(slot, SLOT_OFF_PAYLOAD_LEN, (lane.max_payload() + 1) as u32)
                        }
                        "epoch" => put_u64(slot, SLOT_OFF_LANE_EPOCH, epoch + 1),
                        "crc" => slot[SLOT_HEADER_SIZE_V2] ^= 0x01,
                        _ => put_u16(slot, SLOT_OFF_KIND, 99),
                    }
                    if defect == "epoch" || defect == "kind" {
                        // Re-seal so that only the targeted check can refuse it.
                        let len = get_u32(slot, SLOT_OFF_PAYLOAD_LEN) as usize;
                        let crc = record_crc32c(slot, len);
                        put_u32(slot, SLOT_OFF_CRC32C, crc);
                    }
                }
            }
            drop(mapping);

            let mut consumer = ConsumerV2::open(&path, 110, lane).unwrap();
            let first = consumer.try_consume();
            let reported = matches!(
                (defect, &first),
                ("sequence", Err(V2Error::CorruptSequence { .. }))
                    | ("payload-length", Err(V2Error::PayloadTooLarge { .. }))
                    | ("epoch", Err(V2Error::LaneEpochMismatch { .. }))
                    | ("crc", Err(V2Error::ChecksumMismatch { .. }))
                    | ("kind", Err(V2Error::UnknownRecordKind(99)))
            );
            assert!(reported, "{defect}: first poll returned {first:?}");
            for _ in 0..5 {
                assert!(
                    matches!(
                        consumer.try_consume(),
                        Err(V2Error::LaneQuarantined { reason: r }) if r == reason
                    ),
                    "{defect}"
                );
            }
            let s = consumer.stats();
            assert_eq!(
                (
                    s.state,
                    s.corrupt_records,
                    s.read_sequence,
                    s.quarantine_reason,
                    s.shutdown
                ),
                (LANE_STATE_QUARANTINED, 1, 0, reason, true),
                "{defect}"
            );
            // Not consumed, not reusable, no publish.
            assert!(matches!(
                producer.publish(&record(b"x")),
                Err(V2Error::LifecycleState {
                    observed: LANE_STATE_QUARANTINED,
                    ..
                })
            ));
            assert!(matches!(
                arena.begin_close_peer_lane(110, owner(1), epoch),
                Err(V2Error::LifecycleState {
                    observed: LANE_STATE_QUARANTINED,
                    ..
                })
            ));
            assert!(matches!(
                arena.bind_peer_lane(110, owner(2)),
                Err(V2Error::LifecycleState {
                    observed: LANE_STATE_QUARANTINED,
                    ..
                })
            ));
            assert!(matches!(
                consumer.acknowledge_drained_if_closing(),
                Err(V2Error::LaneQuarantined { .. })
            ));
            drop((producer, consumer, arena));
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn records_outside_a_binding_quarantine_as_late_writes_without_counting_corruption() {
        let path = tmp("c13-outside-binding");
        let lanes = [peer_lane(111), peer_lane(112)];
        let mut arena = ArenaV2::create(&path, 111, &lanes).unwrap();

        // A record on a FREE lane.
        let (mapping, lane_offset) = open_lane(&path, 111, lanes[0]).unwrap();
        mapping
            .atomic_u64(lane_offset + LH_OFF_WRITE_SEQUENCE)
            .store(1, Ordering::Release);
        let mut consumer = ConsumerV2::open(&path, 111, lanes[0]).unwrap();
        assert!(matches!(
            consumer.try_consume(),
            Err(V2Error::LateWriteAfterClose {
                final_write: 0,
                write: 1
            })
        ));
        let s = consumer.stats();
        assert_eq!(
            (
                s.state,
                s.quarantine_reason,
                s.corrupt_records,
                s.read_sequence
            ),
            (LANE_STATE_QUARANTINED, QUARANTINE_LATE_WRITE, 0, 0)
        );

        // A record at final_write_sequence on a CLOSING lane.
        let epoch = arena.bind_peer_lane(112, owner(1)).unwrap();
        let mut producer = ProducerV2::open(&path, 111, lanes[1]).unwrap();
        producer.publish(&record(b"in")).unwrap();
        arena.begin_close_peer_lane(112, owner(1), epoch).unwrap();
        let (mapping_b, lane_offset_b) = open_lane(&path, 111, lanes[1]).unwrap();
        mapping_b
            .atomic_u64(lane_offset_b + LH_OFF_WRITE_SEQUENCE)
            .store(2, Ordering::Release);
        let mut consumer_b = ConsumerV2::open(&path, 111, lanes[1]).unwrap();
        assert_eq!(&*consumer_b.try_consume().unwrap().unwrap().payload, b"in");
        assert!(matches!(
            consumer_b.try_consume(),
            Err(V2Error::LateWriteAfterClose {
                final_write: 1,
                write: 2
            })
        ));
        let s = consumer_b.stats();
        assert_eq!(
            (
                s.state,
                s.quarantine_reason,
                s.corrupt_records,
                s.read_sequence
            ),
            (LANE_STATE_QUARANTINED, QUARANTINE_LATE_WRITE, 0, 1)
        );
        drop((mapping, mapping_b, consumer, consumer_b, producer, arena));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn the_pool_never_takes_bound_closing_or_quarantined_lanes() {
        let path = tmp("c13-pool-mixed");
        let ids = [120u16, 121, 122, 123];
        let lanes = ids.map(peer_lane);
        let mut arena = ArenaV2::create(&path, 120, &lanes).unwrap();
        let bound = arena.bind_peer_lane(120, owner(1)).unwrap();
        let closing = arena.bind_peer_lane(121, owner(2)).unwrap();
        arena.begin_close_peer_lane(121, owner(2), closing).unwrap();
        arena.quarantine_lane(122).unwrap();
        let mut pool = PeerLanePoolV2::new(
            arena,
            PoolBoundsV2 {
                max_peers: 3,
                churn_reserve: 1,
            },
        )
        .unwrap();

        let lease = pool.allocate(owner(4)).unwrap();
        assert_eq!(lease.lane_id(), 123);
        let before = peer_snapshot(&pool, &ids);
        assert!(matches!(
            pool.allocate(owner(5)),
            Err(V2Error::PoolExhausted { peer_lanes: 4 })
        ));
        assert_eq!(peer_snapshot(&pool, &ids), before);
        assert_eq!(before[0].lane_epoch, bound);
        let stats = pool.stats();
        assert_eq!(
            (stats.free, stats.bound, stats.closing, stats.quarantined),
            (0, 2, 1, 1)
        );
        drop(pool);
        fs::remove_file(path).unwrap();
    }

    // -----------------------------------------------------------------
    // K11: shutdown refuses binds and publishes, never unmaps a live user
    // -----------------------------------------------------------------

    #[test]
    fn shutdown_refuses_binds_and_publishes_while_live_handles_stay_valid() {
        let path = tmp("c13-shutdown");
        let lanes = [peer_lane(130), peer_lane(131)];
        let arena = ArenaV2::create(&path, 130, &lanes).unwrap();
        let mut pool = PeerLanePoolV2::new(
            arena,
            PoolBoundsV2 {
                max_peers: 1,
                churn_reserve: 1,
            },
        )
        .unwrap();
        let lease = pool.allocate(owner(1)).unwrap();
        let config = peer_lane(lease.lane_id());
        let published = Arc::new(TestAtomicU64::new(0));

        let producer_path = path.clone();
        let producer_count = Arc::clone(&published);
        let producer = thread::spawn(move || {
            let mut producer = ProducerV2::open(&producer_path, 130, config).unwrap();
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                match producer.publish(&record(b"p")) {
                    Ok(_) => {
                        producer_count.fetch_add(1, TestOrdering::Release);
                    }
                    Err(V2Error::QueueFull { .. }) => thread::yield_now(),
                    Err(V2Error::Shutdown) => return,
                    Err(error) => panic!("{error}"),
                }
                assert!(Instant::now() < deadline, "shutdown never observed");
            }
        });

        let mut consumer = ConsumerV2::open(&path, 130, config).unwrap();
        let mut consumed = 0u64;
        while consumed < 8 {
            if consumer.try_consume().unwrap().is_some() {
                consumed += 1;
            }
            thread::yield_now();
        }
        pool.begin_shutdown().unwrap();
        producer.join().unwrap();

        // Everything published is still readable, then the lane reports its end.
        loop {
            match consumer.try_consume() {
                Ok(Some(_)) => consumed += 1,
                Ok(None) => thread::yield_now(),
                Err(V2Error::Shutdown) => break,
                Err(error) => panic!("{error}"),
            }
        }
        assert_eq!(consumed, published.load(TestOrdering::Acquire));
        assert!(matches!(pool.allocate(owner(2)), Err(V2Error::Shutdown)));
        let other = if lease.lane_id() == 130 { 131 } else { 130 };
        assert!(matches!(
            pool.arena_mut().bind_peer_lane(other, owner(3)),
            Err(V2Error::Shutdown)
        ));
        // Close, drain and release stay deterministic during shutdown.
        pool.begin_close(&lease).unwrap();
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        pool.release(&lease).unwrap();
        assert!(matches!(
            pool.arena_mut().bind_peer_lane(lease.lane_id(), owner(4)),
            Err(V2Error::Shutdown)
        ));
        drop((consumer, pool));
        fs::remove_file(path).unwrap();
    }

    // -----------------------------------------------------------------
    // K14: exclusive runtime role leases
    // -----------------------------------------------------------------

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let id = ID.fetch_add(1, TestOrdering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "darkbot-fastlane-v2-{}-{}-{name}",
            std::process::id(),
            id
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn role_leases_are_exclusive_per_role_and_coexist_across_roles() {
        let dir = tmp_dir("lease");
        let geth = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::GethOwner, 7).unwrap();
        let rust = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, 7).unwrap();
        assert_ne!(geth.path(), rust.path());
        assert!(matches!(
            RoleLeaseV2::acquire(&dir, RuntimeRoleV2::GethOwner, 7),
            Err(V2Error::RoleLeaseHeld { role: "geth-owner" })
        ));
        assert!(matches!(
            RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, 8),
            Err(V2Error::RoleLeaseHeld { role: "rust-owner" })
        ));
        assert!(matches!(
            RoleLeaseV2::acquire(&dir, RuntimeRoleV2::GethOwner, 0),
            Err(V2Error::ZeroGeneration)
        ));
        let content = fs::read_to_string(geth.path()).unwrap();
        assert_eq!(
            content,
            format!(
                "version=1\nrole=geth-owner\ngeneration=7\npid={}\n",
                std::process::id()
            )
        );

        // Closing the owner releases the lock; the file stays.
        drop(geth);
        let again = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::GethOwner, 9).unwrap();
        assert_eq!(again.role(), RuntimeRoleV2::GethOwner);
        drop((again, rust));
        fs::remove_dir_all(dir).unwrap();
    }

    const LEASE_CHILD_ENV: &str = "DARKBOT_FASTLANE_LEASE_CHILD_DIR";

    #[test]
    #[ignore = "helper process of role_lease_is_released_when_the_holder_process_dies"]
    fn role_lease_child_holder() {
        let Some(dir) = std::env::var_os(LEASE_CHILD_ENV) else {
            return;
        };
        let _lease =
            RoleLeaseV2::acquire(std::path::Path::new(&dir), RuntimeRoleV2::RustOwner, 1).unwrap();
        // libtest may prefix the line with the test name; the parent searches.
        println!("\nDARKBOT_LEASE_LOCKED");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        thread::sleep(Duration::from_secs(60));
    }

    #[test]
    fn role_lease_is_released_when_the_holder_process_dies() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};

        let dir = tmp_dir("lease-child");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "v2::tests::role_lease_child_holder",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(LEASE_CHILD_ENV, &dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut locked = false;
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap() > 0 {
            if line.contains("DARKBOT_LEASE_LOCKED") {
                locked = true;
                break;
            }
            line.clear();
        }
        assert!(locked, "the child process did not take the lease");
        assert!(matches!(
            RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, 1),
            Err(V2Error::RoleLeaseHeld { .. })
        ));
        child.kill().unwrap();
        child.wait().unwrap();
        let lease = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, 1).unwrap();
        drop(lease);
        fs::remove_dir_all(dir).unwrap();
    }

    // -----------------------------------------------------------------
    // K15: generation, restart, O_EXCL primitives
    // -----------------------------------------------------------------

    #[test]
    fn existing_arenas_are_detected_never_attached_and_never_overwritten() {
        let dir = tmp_dir("probe");
        let lane = peer_lane(140);
        let current = arena_path_v2(&dir, 41);
        assert_eq!(
            current.file_name().unwrap().to_str().unwrap(),
            "fastlane-v2-g00000000000000000041.mmap"
        );
        assert_eq!(probe_arena_v2(&current, 41).unwrap(), ArenaProbeV2::Absent);
        assert!(matches!(
            probe_arena_v2(&current, 0),
            Err(V2Error::ZeroGeneration)
        ));

        drop(ArenaV2::create(&current, 41, &[lane]).unwrap());
        assert_eq!(probe_arena_v2(&current, 41).unwrap(), ArenaProbeV2::Current);
        assert_eq!(
            probe_arena_v2(&current, 42).unwrap(),
            ArenaProbeV2::StaleGeneration { observed: 41 }
        );

        // create_new on an existing arena: explicit result, bytes untouched.
        let before = fs::read(&current).unwrap();
        assert!(matches!(
            ArenaV2::create(&current, 41, &[lane]),
            Err(V2Error::ArenaExists {
                probe: ArenaProbeV2::Current
            })
        ));
        assert!(matches!(
            ArenaV2::create(&current, 42, &[lane]),
            Err(V2Error::ArenaExists {
                probe: ArenaProbeV2::StaleGeneration { observed: 41 }
            })
        ));
        assert_eq!(fs::read(&current).unwrap(), before);
        assert!(matches!(
            ArenaV2::open(&current, 42),
            Err(V2Error::GenerationMismatch {
                expected: 42,
                observed: 41
            })
        ));

        // A file that is not an arena.
        let junk = dir.join("fastlane-v2-g00000000000000000007.mmap");
        fs::write(&junk, b"not an arena").unwrap();
        assert_eq!(probe_arena_v2(&junk, 7).unwrap(), ArenaProbeV2::NotAnArena);
        assert!(matches!(
            ArenaV2::create(&junk, 7, &[lane]),
            Err(V2Error::ArenaExists {
                probe: ArenaProbeV2::NotAnArena
            })
        ));
        assert_eq!(fs::read(&junk).unwrap(), b"not an arena");

        // A new generation gets its own file beside the old ones.
        let next = arena_path_v2(&dir, 42);
        drop(ArenaV2::create(&next, 42, &[lane]).unwrap());
        fs::write(dir.join("unrelated.mmap"), b"x").unwrap();
        let stale = stale_arenas_v2(&dir, 42).unwrap();
        assert_eq!(stale, vec![(7, junk.clone()), (41, current.clone())]);
        assert!(current.exists() && junk.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_undefined_lifecycle_state_quarantines_even_an_empty_lane() {
        let path = tmp("c13-invalid-state");
        // 160: empty, polled; 161: one record waiting, polled; 162: empty, acked.
        let lanes = [peer_lane(160), peer_lane(161), peer_lane(162)];
        let mut arena = ArenaV2::create(&path, 160, &lanes).unwrap();
        arena.bind_peer_lane(161, owner(1)).unwrap();
        let mut producer = ProducerV2::open(&path, 160, lanes[1]).unwrap();
        producer.publish(&record(b"r")).unwrap();
        for lane in lanes {
            let (mapping, lane_offset) = open_lane(&path, 160, lane).unwrap();
            mapping
                .atomic_u32(lane_offset + LH_OFF_STATE)
                .store(99, Ordering::Release);
        }
        for (index, lane) in lanes.iter().enumerate() {
            let mut consumer = ConsumerV2::open(&path, 160, *lane).unwrap();
            let first = if index == 2 {
                consumer.acknowledge_drained_if_closing().map(|_| ())
            } else {
                consumer.try_consume().map(|_| ())
            };
            assert!(
                matches!(&first, Err(V2Error::InvalidLifecycleState { observed: 99 })),
                "lane {index}: {first:?}"
            );
            for _ in 0..3 {
                assert!(matches!(
                    consumer.try_consume(),
                    Err(V2Error::LaneQuarantined {
                        reason: QUARANTINE_INVALID_STATE
                    })
                ));
            }
            let s = consumer.stats();
            assert_eq!(
                (s.state, s.corrupt_records, s.quarantine_reason, s.shutdown),
                (LANE_STATE_QUARANTINED, 1, QUARANTINE_INVALID_STATE, true),
                "lane {index}"
            );
        }
        // The waiting record was never consumed.
        assert_eq!(stats_of(&arena, 161).read_sequence, 0);
        drop((producer, arena));
        fs::remove_file(path).unwrap();
    }

    // -----------------------------------------------------------------
    // C1.3 Go<->Rust lifecycle interop. Four alternating processes on one
    // arena, run in order by bsc/native/tools/c13_lifecycle_interop.sh:
    // geth phase 1 -> rust phase 1 -> geth phase 2 -> rust phase 2.
    // -----------------------------------------------------------------

    const INTEROP_ENV: &str = "DARKBOT_C13_INTEROP_DIR";
    const INTEROP_GENERATION: u64 = 1303;

    /// Non-uniform, so a word-order mismatch between Go and Rust cannot hide.
    fn interop_owner(base: u8) -> [u8; 32] {
        std::array::from_fn(|i| base + i as u8)
    }

    fn interop_arena() -> Option<std::path::PathBuf> {
        std::env::var_os(INTEROP_ENV)
            .map(|dir| arena_path_v2(std::path::Path::new(&dir), INTEROP_GENERATION))
    }

    /// Phase 1: read what Go wrote, consume lane 1 to its final write, ack
    /// the drain; find Go's broken record on lane 2 and quarantine it.
    #[test]
    #[ignore = "C1.3 Go<->Rust interop phase; run by bsc/native/tools/c13_lifecycle_interop.sh"]
    fn interop_rust_phase_1() {
        let Some(path) = interop_arena() else {
            return;
        };
        assert_eq!(
            probe_arena_v2(&path, INTEROP_GENERATION).unwrap(),
            ArenaProbeV2::Current
        );
        let arena = ArenaV2::open(&path, INTEROP_GENERATION).unwrap();
        let s = arena.lane_stats(1).unwrap();
        assert_eq!(
            (s.state, s.lane_epoch, s.owner, s.final_write_sequence),
            (LANE_STATE_CLOSING, 2, interop_owner(0x10), 3)
        );
        assert_eq!(
            (s.write_sequence, s.read_sequence, s.generation),
            (3, 0, 1303)
        );
        let s = arena.lane_stats(2).unwrap();
        assert_eq!(
            (s.state, s.lane_epoch, s.owner, s.write_sequence),
            (LANE_STATE_BOUND, 2, interop_owner(0x40), 1)
        );

        let mut consumer = ConsumerV2::open(&path, INTEROP_GENERATION, peer_lane(1)).unwrap();
        for n in 0..3u64 {
            let out = consumer.try_consume().unwrap().unwrap();
            assert_eq!(
                (
                    out.sequence,
                    out.lane_id,
                    out.lane_epoch,
                    out.kind,
                    out.flags
                ),
                (n, 1, 2, RecordKind::EarlyTxSeen, 3)
            );
            assert_eq!(
                (out.timestamp_ns, out.clock_id),
                (123_456_789, CLOCK_ID_MONOTONIC)
            );
            assert_eq!(
                (out.source, out.peer, out.hash, out.aux),
                ([0x11; 16], [0x22; 32], [0x33; 32], [0x44; 32])
            );
            assert_eq!(&*out.payload, format!("interop-{n}").as_bytes());
        }
        assert!(consumer.try_consume().unwrap().is_none());
        assert!(consumer.acknowledge_drained_if_closing().unwrap());
        assert_eq!(arena.lane_stats(1).unwrap().drained_epoch, 2);

        let mut broken = ConsumerV2::open(&path, INTEROP_GENERATION, peer_lane(2)).unwrap();
        assert!(matches!(
            broken.try_consume(),
            Err(V2Error::ChecksumMismatch { .. })
        ));
        assert!(matches!(
            broken.try_consume(),
            Err(V2Error::LaneQuarantined {
                reason: QUARANTINE_CHECKSUM
            })
        ));
        println!("\nC13_INTEROP_RUST_PHASE1_OK");
    }

    /// Phase 2: Go released and rebound lane 1 with epoch + 1; consume the
    /// new binding's record, sequence continuing at 3.
    #[test]
    #[ignore = "C1.3 Go<->Rust interop phase; run by bsc/native/tools/c13_lifecycle_interop.sh"]
    fn interop_rust_phase_2() {
        let Some(path) = interop_arena() else {
            return;
        };
        let arena = ArenaV2::open(&path, INTEROP_GENERATION).unwrap();
        let s = arena.lane_stats(1).unwrap();
        assert_eq!(
            (s.state, s.lane_epoch, s.owner, s.final_write_sequence),
            (LANE_STATE_BOUND, 3, interop_owner(0x70), 0)
        );
        assert_eq!(
            (s.write_sequence, s.read_sequence, s.drained_epoch),
            (4, 3, 2)
        );
        let mut consumer = ConsumerV2::open(&path, INTEROP_GENERATION, peer_lane(1)).unwrap();
        let out = consumer.try_consume().unwrap().unwrap();
        assert_eq!(
            (out.sequence, out.lane_epoch, &*out.payload),
            (3, 3, &b"interop-3"[..])
        );
        assert!(consumer.try_consume().unwrap().is_none());
        let s = arena.lane_stats(2).unwrap();
        assert_eq!(
            (
                s.state,
                s.quarantine_reason,
                s.corrupt_records,
                s.read_sequence,
                s.shutdown
            ),
            (LANE_STATE_QUARANTINED, QUARANTINE_CHECKSUM, 1, 0, true)
        );
        println!("\nC13_INTEROP_RUST_PHASE2_OK");
    }
}
