//! C1.2 Fast-Lane Wire V2.
//!
//! Wire V2 is intentionally implemented alongside V1 until C1.4 switches the
//! geth ingress hooks. The arena contains independently owned SPSC lanes.
//! C1.3 adds the full lane lifecycle; C1.5 replaces the owned receive copy with
//! a borrowed slot guard.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::atomic::{AtomicU64, Ordering};

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
const EMPTY_SEQUENCE: u64 = u64::MAX;

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
            || self.slot_stride % ALIGN_V2 != 0
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
    GenerationMismatch { expected: u64, observed: u64 },
    LaneNotFound(u16),
    LaneMismatch(&'static str),
    PayloadTooLarge { len: usize, max: usize },
    QueueFull { sequence: u64 },
    Shutdown,
    CorruptSequence { expected: u64, observed: u64 },
    LaneEpochMismatch { expected: u64, observed: u64 },
    ChecksumMismatch { expected: u32, actual: u32 },
    UnknownRecordKind(u16),
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
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(V2Error::Io)?;
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
                put_u32(bytes, lane_offset + LH_OFF_STATE, 1);
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

    pub fn try_consume(&mut self) -> Result<Option<OwnedRecordV2>, V2Error> {
        let read = self.atomic(LH_OFF_READ_SEQUENCE).load(Ordering::Relaxed);
        let write = self.atomic(LH_OFF_WRITE_SEQUENCE).load(Ordering::Acquire);
        if read == write {
            if self.atomic(LH_OFF_SHUTDOWN).load(Ordering::Acquire) != 0 {
                return Err(V2Error::Shutdown);
            }
            return Ok(None);
        }

        let index = (read % self.config.capacity as u64) as usize;
        let slot_offset = self.lane_offset + LANE_HEADER_SIZE_V2 + index * self.config.slot_stride;
        let observed = self
            .mapping
            .atomic_u64(slot_offset + SLOT_OFF_COMMITTED)
            .load(Ordering::Acquire);
        let expected_sequence = read.wrapping_add(1);
        if observed != expected_sequence {
            self.atomic(LH_OFF_CORRUPT).fetch_add(1, Ordering::Relaxed);
            return Err(V2Error::CorruptSequence {
                expected: expected_sequence,
                observed,
            });
        }

        let bytes = self.mapping.bytes();
        let slot = &bytes[slot_offset..slot_offset + self.config.slot_stride];
        let payload_len = get_u32(slot, SLOT_OFF_PAYLOAD_LEN) as usize;
        if payload_len > self.config.max_payload() {
            self.atomic(LH_OFF_CORRUPT).fetch_add(1, Ordering::Relaxed);
            return Err(V2Error::PayloadTooLarge {
                len: payload_len,
                max: self.config.max_payload(),
            });
        }
        let lane_epoch = get_u64(slot, SLOT_OFF_LANE_EPOCH);
        let expected_epoch = self.atomic(LH_OFF_LANE_EPOCH).load(Ordering::Acquire);
        if lane_epoch != expected_epoch {
            self.atomic(LH_OFF_CORRUPT).fetch_add(1, Ordering::Relaxed);
            return Err(V2Error::LaneEpochMismatch {
                expected: expected_epoch,
                observed: lane_epoch,
            });
        }
        let expected_crc = get_u32(slot, SLOT_OFF_CRC32C);
        let actual_crc = record_crc32c(slot, payload_len);
        if actual_crc != expected_crc {
            self.atomic(LH_OFF_CORRUPT).fetch_add(1, Ordering::Relaxed);
            return Err(V2Error::ChecksumMismatch {
                expected: expected_crc,
                actual: actual_crc,
            });
        }

        let kind_raw = get_u16(slot, SLOT_OFF_KIND);
        let kind =
            RecordKind::try_from(kind_raw).map_err(|_| V2Error::UnknownRecordKind(kind_raw))?;
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

    pub fn stats(&self) -> LaneStatsV2 {
        lane_stats(&self.mapping, self.lane_offset)
    }

    fn atomic(&self, rel: usize) -> &AtomicU64 {
        self.mapping.atomic_u64(self.lane_offset + rel)
    }
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
    use std::sync::atomic::{AtomicU64 as TestAtomicU64, Ordering as TestOrdering};
    use std::thread;

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
        let arena = ArenaV2::create(&path, 7, &lanes).unwrap();
        assert_eq!(arena.mapped_len() % ALIGN_V2, 0);
        drop(arena);

        let mut producer = ProducerV2::open(&path, 7, lanes[0]).unwrap();
        let mut consumer = ConsumerV2::open(&path, 7, lanes[0]).unwrap();

        for n in 0..32u8 {
            producer.publish(&record(&[n])).unwrap();
            let out = consumer.try_consume().unwrap().unwrap();
            assert_eq!(out.sequence, u64::from(n));
            assert_eq!(out.lane_id, 10);
            assert_eq!(out.lane_epoch, 1);
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
        let arena = ArenaV2::create(&path, 9, &[lane]).unwrap();
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
            let arena = ArenaV2::create(&path, 11, &[lane]).unwrap();
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

            drop(producer);
            drop(consumer);
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn independent_lanes_support_parallel_single_producers() {
        let path = tmp("parallel");
        let lanes = [peer_lane(20), peer_lane(21)];
        let arena = ArenaV2::create(&path, 13, &lanes).unwrap();
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
}
