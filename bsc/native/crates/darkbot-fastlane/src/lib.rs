//! C1 bidirectional shared-memory Fast Lane.
//!
//! This crate defines the production wire/ring contract used between pinned
//! bsc-geth and the Rust runtime. It is deliberately transport-only: it does
//! not perform venue semantics, chain reconciliation, signing, provider
//! submission or LIVE authorization.
//!
//! The ring is one-producer/one-consumer, fixed-capacity, mmap-backed and
//! sequence-numbered. Producer publication is Release; consumer observation
//! is Acquire. Queue overflow is explicit and never overwrites unread data.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io;
use std::mem::{align_of, size_of};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::atomic::{AtomicU64, Ordering};

pub const FASTLANE_MAGIC: [u8; 8] = *b"DBFLN001";
pub const FASTLANE_VERSION: u16 = 1;
pub const RING_HEADER_SIZE: usize = 128;
pub const SLOT_HEADER_SIZE: usize = 192;
pub const MIN_SLOT_SIZE: usize = 512;
pub const MAX_SLOT_SIZE: usize = 256 * 1024;
pub const MAX_CAPACITY: usize = 4096;
pub const SOURCE_LEN: usize = 16;
pub const PEER_LEN: usize = 32;
pub const HASH_LEN: usize = 32;
pub const AUX_LEN: usize = 32;
const SLOT_ALIGN: usize = 64;
const EMPTY_SEQUENCE: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum RecordKind {
    EarlyTxSeen = 1,
    TxpoolAccepted = 2,
    CanonicalHead = 3,
    Reorg = 4,
    OutboundSignedTx = 5,
    OutboundPeerHandoff = 6,
    Gap = 7,
    Shutdown = 8,
}

impl TryFrom<u16> for RecordKind {
    type Error = FastLaneError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::EarlyTxSeen),
            2 => Ok(Self::TxpoolAccepted),
            3 => Ok(Self::CanonicalHead),
            4 => Ok(Self::Reorg),
            5 => Ok(Self::OutboundSignedTx),
            6 => Ok(Self::OutboundPeerHandoff),
            7 => Ok(Self::Gap),
            8 => Ok(Self::Shutdown),
            _ => Err(FastLaneError::UnknownRecordKind(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Direction {
    GethToRust = 1,
    RustToGeth = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingConfig {
    pub direction: Direction,
    pub generation: u64,
    pub capacity: usize,
    pub slot_size: usize,
}

impl RingConfig {
    pub fn validate(self) -> Result<(), FastLaneError> {
        if self.generation == 0 {
            return Err(FastLaneError::ZeroGeneration);
        }
        if self.capacity == 0 || self.capacity > MAX_CAPACITY {
            return Err(FastLaneError::InvalidCapacity(self.capacity));
        }
        if self.slot_size < MIN_SLOT_SIZE
            || self.slot_size > MAX_SLOT_SIZE
            || !self.slot_size.is_multiple_of(SLOT_ALIGN)
            || self.slot_size <= size_of::<SlotHeader>()
        {
            return Err(FastLaneError::InvalidSlotSize(self.slot_size));
        }
        let _ = total_len(self.capacity, self.slot_size)?;
        Ok(())
    }

    pub fn max_payload(self) -> usize {
        self.slot_size - size_of::<SlotHeader>()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record<'a> {
    pub kind: RecordKind,
    pub flags: u16,
    pub timestamp_ns: u64,
    pub source: [u8; SOURCE_LEN],
    pub peer: [u8; PEER_LEN],
    pub hash: [u8; HASH_LEN],
    pub aux: [u8; AUX_LEN],
    pub payload: &'a [u8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedRecord {
    pub sequence: u64,
    pub kind: RecordKind,
    pub flags: u16,
    pub timestamp_ns: u64,
    pub source: [u8; SOURCE_LEN],
    pub peer: [u8; PEER_LEN],
    pub hash: [u8; HASH_LEN],
    pub aux: [u8; AUX_LEN],
    pub payload: Box<[u8]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RingStats {
    pub write_sequence: u64,
    pub read_sequence: u64,
    pub dropped_full: u64,
    pub corrupt_records: u64,
    pub generation: u64,
    pub shutdown: bool,
}

#[derive(Debug)]
pub enum FastLaneError {
    ZeroGeneration,
    InvalidCapacity(usize),
    InvalidSlotSize(usize),
    SizeOverflow,
    Io(io::Error),
    MapFailed(io::Error),
    BadMagic,
    BadVersion(u16),
    DirectionMismatch { expected: Direction, observed: u16 },
    GenerationMismatch { expected: u64, observed: u64 },
    CapacityMismatch { expected: usize, observed: usize },
    SlotSizeMismatch { expected: usize, observed: usize },
    PayloadTooLarge { len: usize, max: usize },
    QueueFull { sequence: u64 },
    Shutdown,
    CorruptSequence { expected: u64, observed: u64 },
    ChecksumMismatch { expected: u32, actual: u32 },
    UnknownRecordKind(u16),
}

impl fmt::Display for FastLaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroGeneration => f.write_str("fastlane: generation must be nonzero"),
            Self::InvalidCapacity(value) => write!(f, "fastlane: invalid capacity {value}"),
            Self::InvalidSlotSize(value) => write!(f, "fastlane: invalid slot size {value}"),
            Self::SizeOverflow => f.write_str("fastlane: mapping size overflow"),
            Self::Io(error) => write!(f, "fastlane: I/O {error}"),
            Self::MapFailed(error) => write!(f, "fastlane: mmap {error}"),
            Self::BadMagic => f.write_str("fastlane: bad magic"),
            Self::BadVersion(value) => write!(f, "fastlane: unsupported version {value}"),
            Self::DirectionMismatch { expected, observed } => {
                write!(f, "fastlane: direction mismatch {observed} != {expected:?}")
            }
            Self::GenerationMismatch { expected, observed } => {
                write!(f, "fastlane: generation mismatch {observed} != {expected}")
            }
            Self::CapacityMismatch { expected, observed } => {
                write!(f, "fastlane: capacity mismatch {observed} != {expected}")
            }
            Self::SlotSizeMismatch { expected, observed } => {
                write!(f, "fastlane: slot size mismatch {observed} != {expected}")
            }
            Self::PayloadTooLarge { len, max } => {
                write!(f, "fastlane: payload {len} exceeds slot bound {max}")
            }
            Self::QueueFull { sequence } => write!(f, "fastlane: queue full at {sequence}"),
            Self::Shutdown => f.write_str("fastlane: ring is shut down"),
            Self::CorruptSequence { expected, observed } => {
                write!(f, "fastlane: corrupt slot sequence {observed} != {expected}")
            }
            Self::ChecksumMismatch { expected, actual } => {
                write!(f, "fastlane: checksum mismatch {actual:#x} != {expected:#x}")
            }
            Self::UnknownRecordKind(value) => {
                write!(f, "fastlane: unknown record kind {value}")
            }
        }
    }
}

impl std::error::Error for FastLaneError {}

#[repr(C, align(64))]
struct RingHeader {
    magic: [u8; 8],
    version: u16,
    direction: u16,
    header_size: u32,
    generation: u64,
    capacity: u32,
    slot_size: u32,
    reserved0: [u8; 24],
    write_sequence: AtomicU64,
    read_sequence: AtomicU64,
    dropped_full: AtomicU64,
    corrupt_records: AtomicU64,
    shutdown: AtomicU64,
    reserved1: [u8; 24],
}

#[repr(C, align(64))]
struct SlotHeader {
    committed_sequence: AtomicU64,
    kind: u16,
    flags: u16,
    payload_len: u32,
    timestamp_ns: u64,
    source: [u8; SOURCE_LEN],
    peer: [u8; PEER_LEN],
    hash: [u8; HASH_LEN],
    aux: [u8; AUX_LEN],
    payload_crc32: u32,
    reserved: [u8; 20],
}

const _: () = assert!(align_of::<RingHeader>() == SLOT_ALIGN);
const _: () = assert!(align_of::<SlotHeader>() == SLOT_ALIGN);
const _: () = assert!(size_of::<RingHeader>() == RING_HEADER_SIZE);
const _: () = assert!(size_of::<SlotHeader>() == SLOT_HEADER_SIZE);

struct Mapping {
    _file: File,
    ptr: NonNull<u8>,
    len: usize,
}

// SAFETY: Mapping points at a MAP_SHARED region whose concurrent mutable fields
// are restricted to the atomics defined by this module and to disjoint SPSC
// producer/consumer slot ownership.
unsafe impl Send for Mapping {}
// SAFETY: See the Send rationale above; publication/consumption uses
// Acquire/Release ordering and static metadata is immutable after creation.
unsafe impl Sync for Mapping {}

impl Mapping {
    fn create(path: &Path, config: RingConfig) -> Result<Self, FastLaneError> {
        config.validate()?;
        let len = total_len(config.capacity, config.slot_size)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(FastLaneError::Io)?;
        file.set_len(u64::try_from(len).map_err(|_| FastLaneError::SizeOverflow)?)
            .map_err(FastLaneError::Io)?;
        let mapping = Self::map(file, len)?;
        // SAFETY: The freshly created file is exclusively owned by this
        // constructor and sized for RingHeader plus all slots.
        unsafe {
            ptr::write(
                mapping.ptr.as_ptr().cast::<RingHeader>(),
                RingHeader {
                    magic: FASTLANE_MAGIC,
                    version: FASTLANE_VERSION,
                    direction: config.direction as u16,
                    header_size: u32::try_from(size_of::<RingHeader>())
                        .map_err(|_| FastLaneError::SizeOverflow)?,
                    generation: config.generation,
                    capacity: u32::try_from(config.capacity)
                        .map_err(|_| FastLaneError::SizeOverflow)?,
                    slot_size: u32::try_from(config.slot_size)
                        .map_err(|_| FastLaneError::SizeOverflow)?,
                    reserved0: [0u8; 24],
                    write_sequence: AtomicU64::new(0),
                    read_sequence: AtomicU64::new(0),
                    dropped_full: AtomicU64::new(0),
                    corrupt_records: AtomicU64::new(0),
                    shutdown: AtomicU64::new(0),
                    reserved1: [0u8; 24],
                },
            );
            for index in 0..config.capacity {
                let slot = mapping.slot_ptr(index, config.slot_size).cast::<SlotHeader>();
                ptr::write(
                    slot,
                    SlotHeader {
                        committed_sequence: AtomicU64::new(EMPTY_SEQUENCE),
                        kind: 0,
                        flags: 0,
                        payload_len: 0,
                        timestamp_ns: 0,
                        source: [0u8; SOURCE_LEN],
                        peer: [0u8; PEER_LEN],
                        hash: [0u8; HASH_LEN],
                        aux: [0u8; AUX_LEN],
                        payload_crc32: 0,
                        reserved: [0u8; 20],
                    },
                );
            }
        }
        mapping.sync_metadata()?;
        Ok(mapping)
    }

    fn open(path: &Path, expected: RingConfig) -> Result<Self, FastLaneError> {
        expected.validate()?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(FastLaneError::Io)?;
        let metadata = file.metadata().map_err(FastLaneError::Io)?;
        let observed_len = usize::try_from(metadata.len()).map_err(|_| FastLaneError::SizeOverflow)?;
        let expected_len = total_len(expected.capacity, expected.slot_size)?;
        if observed_len != expected_len {
            return Err(FastLaneError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "mapping length mismatch",
            )));
        }
        let mapping = Self::map(file, observed_len)?;
        mapping.validate_header(expected)?;
        Ok(mapping)
    }

    fn map(file: File, len: usize) -> Result<Self, FastLaneError> {
        // SAFETY: mmap is called with a valid file descriptor and positive
        // length. The returned mapping is owned by Self and unmapped in Drop.
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
            return Err(FastLaneError::MapFailed(io::Error::last_os_error()));
        }
        let ptr = NonNull::new(raw.cast::<u8>()).ok_or_else(|| {
            FastLaneError::MapFailed(io::Error::other("mmap returned null"))
        })?;
        Ok(Self {
            _file: file,
            ptr,
            len,
        })
    }

    fn header(&self) -> &RingHeader {
        // SAFETY: Every Mapping starts with an initialized RingHeader.
        unsafe { &*self.ptr.as_ptr().cast::<RingHeader>() }
    }

    fn slot_ptr(&self, index: usize, slot_size: usize) -> *mut u8 {
        let offset = size_of::<RingHeader>() + index * slot_size;
        // SAFETY: Config validation and total_len guarantee offset is inside
        // the mapping and slot boundaries are SLOT_ALIGN aligned.
        unsafe { self.ptr.as_ptr().add(offset) }
    }

    fn validate_header(&self, expected: RingConfig) -> Result<(), FastLaneError> {
        let header = self.header();
        if header.magic != FASTLANE_MAGIC {
            return Err(FastLaneError::BadMagic);
        }
        if header.version != FASTLANE_VERSION {
            return Err(FastLaneError::BadVersion(header.version));
        }
        if header.direction != expected.direction as u16 {
            return Err(FastLaneError::DirectionMismatch {
                expected: expected.direction,
                observed: header.direction,
            });
        }
        if header.generation != expected.generation {
            return Err(FastLaneError::GenerationMismatch {
                expected: expected.generation,
                observed: header.generation,
            });
        }
        let capacity = header.capacity as usize;
        if capacity != expected.capacity {
            return Err(FastLaneError::CapacityMismatch {
                expected: expected.capacity,
                observed: capacity,
            });
        }
        let slot_size = header.slot_size as usize;
        if slot_size != expected.slot_size {
            return Err(FastLaneError::SlotSizeMismatch {
                expected: expected.slot_size,
                observed: slot_size,
            });
        }
        if header.header_size as usize != size_of::<RingHeader>() {
            return Err(FastLaneError::BadVersion(header.version));
        }
        Ok(())
    }

    fn sync_metadata(&self) -> Result<(), FastLaneError> {
        // SAFETY: ptr/len describe this live mmap mapping.
        let rc = unsafe { libc::msync(self.ptr.as_ptr().cast(), self.len, libc::MS_SYNC) };
        if rc != 0 {
            return Err(FastLaneError::Io(io::Error::last_os_error()));
        }
        Ok(())
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: ptr/len describe the mapping created by mmap and this is its
        // unique Drop.
        unsafe {
            libc::munmap(self.ptr.as_ptr().cast(), self.len);
        }
    }
}

pub struct Producer {
    mapping: Mapping,
    config: RingConfig,
}

pub struct Consumer {
    mapping: Mapping,
    config: RingConfig,
}

impl Producer {
    pub fn create(path: &Path, config: RingConfig) -> Result<Self, FastLaneError> {
        let mapping = Mapping::create(path, config)?;
        Ok(Self { mapping, config })
    }

    pub fn open(path: &Path, config: RingConfig) -> Result<Self, FastLaneError> {
        let mapping = Mapping::open(path, config)?;
        Ok(Self { mapping, config })
    }

    pub fn publish(&mut self, record: &Record<'_>) -> Result<u64, FastLaneError> {
        if record.payload.len() > self.config.max_payload() {
            return Err(FastLaneError::PayloadTooLarge {
                len: record.payload.len(),
                max: self.config.max_payload(),
            });
        }
        let header = self.mapping.header();
        if header.shutdown.load(Ordering::Acquire) != 0 {
            return Err(FastLaneError::Shutdown);
        }
        let write = header.write_sequence.load(Ordering::Relaxed);
        let read = header.read_sequence.load(Ordering::Acquire);
        if write.wrapping_sub(read) >= self.config.capacity as u64 {
            header.dropped_full.fetch_add(1, Ordering::Relaxed);
            return Err(FastLaneError::QueueFull { sequence: write });
        }

        let index = (write % self.config.capacity as u64) as usize;
        let slot_ptr = self
            .mapping
            .slot_ptr(index, self.config.slot_size)
            .cast::<SlotHeader>();

        // SAFETY: SPSC ownership guarantees this producer is the only writer
        // to the chosen free slot. Consumer cannot observe it until the final
        // committed_sequence Release store.
        unsafe {
            let slot = &mut *slot_ptr;
            slot.committed_sequence
                .store(EMPTY_SEQUENCE, Ordering::Relaxed);
            slot.kind = record.kind as u16;
            slot.flags = record.flags;
            slot.payload_len = record.payload.len() as u32;
            slot.timestamp_ns = record.timestamp_ns;
            slot.source = record.source;
            slot.peer = record.peer;
            slot.hash = record.hash;
            slot.aux = record.aux;
            slot.payload_crc32 = crc32(record.payload);

            let payload_ptr = self
                .mapping
                .slot_ptr(index, self.config.slot_size)
                .add(size_of::<SlotHeader>());
            ptr::copy_nonoverlapping(record.payload.as_ptr(), payload_ptr, record.payload.len());

            slot.committed_sequence
                .store(write.wrapping_add(1), Ordering::Release);
        }
        header
            .write_sequence
            .store(write.wrapping_add(1), Ordering::Release);
        Ok(write)
    }

    pub fn shutdown(&self) {
        self.mapping.header().shutdown.store(1, Ordering::Release);
    }

    pub fn stats(&self) -> RingStats {
        stats(self.mapping.header())
    }
}

impl Consumer {
    pub fn create(path: &Path, config: RingConfig) -> Result<Self, FastLaneError> {
        let mapping = Mapping::create(path, config)?;
        Ok(Self { mapping, config })
    }

    pub fn open(path: &Path, config: RingConfig) -> Result<Self, FastLaneError> {
        let mapping = Mapping::open(path, config)?;
        Ok(Self { mapping, config })
    }

    pub fn try_consume(&mut self) -> Result<Option<OwnedRecord>, FastLaneError> {
        let header = self.mapping.header();
        let read = header.read_sequence.load(Ordering::Relaxed);
        let write = header.write_sequence.load(Ordering::Acquire);
        if read == write {
            if header.shutdown.load(Ordering::Acquire) != 0 {
                return Err(FastLaneError::Shutdown);
            }
            return Ok(None);
        }

        let index = (read % self.config.capacity as u64) as usize;
        let slot_ptr = self
            .mapping
            .slot_ptr(index, self.config.slot_size)
            .cast::<SlotHeader>();
        // SAFETY: The producer published this slot with a Release store before
        // advancing write_sequence. Acquire below makes metadata/payload visible.
        let slot = unsafe { &*slot_ptr };
        let observed = slot.committed_sequence.load(Ordering::Acquire);
        let expected = read.wrapping_add(1);
        if observed != expected {
            header.corrupt_records.fetch_add(1, Ordering::Relaxed);
            return Err(FastLaneError::CorruptSequence { expected, observed });
        }

        let payload_len = slot.payload_len as usize;
        if payload_len > self.config.max_payload() {
            header.corrupt_records.fetch_add(1, Ordering::Relaxed);
            return Err(FastLaneError::PayloadTooLarge {
                len: payload_len,
                max: self.config.max_payload(),
            });
        }
        let kind = RecordKind::try_from(slot.kind)?;
        // SAFETY: payload_len is bounded by the slot size and the producer
        // cannot reuse this slot until read_sequence advances below.
        let payload = unsafe {
            let ptr = self
                .mapping
                .slot_ptr(index, self.config.slot_size)
                .add(size_of::<SlotHeader>());
            slice::from_raw_parts(ptr, payload_len)
        };
        let actual_crc = crc32(payload);
        if actual_crc != slot.payload_crc32 {
            header.corrupt_records.fetch_add(1, Ordering::Relaxed);
            return Err(FastLaneError::ChecksumMismatch {
                expected: slot.payload_crc32,
                actual: actual_crc,
            });
        }

        let record = OwnedRecord {
            sequence: read,
            kind,
            flags: slot.flags,
            timestamp_ns: slot.timestamp_ns,
            source: slot.source,
            peer: slot.peer,
            hash: slot.hash,
            aux: slot.aux,
            payload: payload.to_vec().into_boxed_slice(),
        };
        header
            .read_sequence
            .store(read.wrapping_add(1), Ordering::Release);
        Ok(Some(record))
    }

    pub fn stats(&self) -> RingStats {
        stats(self.mapping.header())
    }
}

fn stats(header: &RingHeader) -> RingStats {
    RingStats {
        write_sequence: header.write_sequence.load(Ordering::Acquire),
        read_sequence: header.read_sequence.load(Ordering::Acquire),
        dropped_full: header.dropped_full.load(Ordering::Acquire),
        corrupt_records: header.corrupt_records.load(Ordering::Acquire),
        generation: header.generation,
        shutdown: header.shutdown.load(Ordering::Acquire) != 0,
    }
}

fn total_len(capacity: usize, slot_size: usize) -> Result<usize, FastLaneError> {
    capacity
        .checked_mul(slot_size)
        .and_then(|slots| size_of::<RingHeader>().checked_add(slots))
        .ok_or(FastLaneError::SizeOverflow)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static ID: AtomicU64 = AtomicU64::new(0);

    fn tmp(name: &str) -> std::path::PathBuf {
        let id = ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "darkbot-fastlane-{}-{}-{name}.mmap",
            std::process::id(),
            id
        ))
    }

    fn config(direction: Direction) -> RingConfig {
        RingConfig {
            direction,
            generation: 7,
            capacity: 4,
            slot_size: 1024,
        }
    }

    fn record<'a>(kind: RecordKind, payload: &'a [u8]) -> Record<'a> {
        Record {
            kind,
            flags: 3,
            timestamp_ns: 123_456,
            source: [0x11; SOURCE_LEN],
            peer: [0x22; PEER_LEN],
            hash: [0x33; HASH_LEN],
            aux: [0x44; AUX_LEN],
            payload,
        }
    }

    #[test]
    fn config_rejects_zero_generation_bad_capacity_and_bad_slot_size() {
        let mut c = config(Direction::GethToRust);
        c.generation = 0;
        assert!(matches!(c.validate(), Err(FastLaneError::ZeroGeneration)));

        let mut c = config(Direction::GethToRust);
        c.capacity = 0;
        assert!(matches!(
            c.validate(),
            Err(FastLaneError::InvalidCapacity(0))
        ));

        let mut c = config(Direction::GethToRust);
        c.slot_size = 513;
        assert!(matches!(
            c.validate(),
            Err(FastLaneError::InvalidSlotSize(513))
        ));
    }

    #[test]
    fn creator_role_is_independent_from_spsc_role() {
        let path = tmp("creator-role");
        let cfg = config(Direction::RustToGeth);
        let consumer = Consumer::create(&path, cfg).unwrap();
        let mut producer = Producer::open(&path, cfg).unwrap();
        producer
            .publish(&record(RecordKind::OutboundSignedTx, b"buy"))
            .unwrap();
        let mut consumer = consumer;
        let out = consumer.try_consume().unwrap().unwrap();
        assert_eq!(out.kind, RecordKind::OutboundSignedTx);
        assert_eq!(&*out.payload, b"buy");

        drop(consumer);
        drop(producer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn roundtrip_preserves_wire_metadata_and_payload() {
        let path = tmp("roundtrip");
        let cfg = config(Direction::GethToRust);
        let mut producer = Producer::create(&path, cfg).unwrap();
        let mut consumer = Consumer::open(&path, cfg).unwrap();

        let sequence = producer
            .publish(&record(RecordKind::EarlyTxSeen, b"signed-transaction"))
            .unwrap();
        assert_eq!(sequence, 0);

        let out = consumer.try_consume().unwrap().unwrap();
        assert_eq!(out.sequence, 0);
        assert_eq!(out.kind, RecordKind::EarlyTxSeen);
        assert_eq!(out.flags, 3);
        assert_eq!(out.timestamp_ns, 123_456);
        assert_eq!(out.source, [0x11; SOURCE_LEN]);
        assert_eq!(out.peer, [0x22; PEER_LEN]);
        assert_eq!(out.hash, [0x33; HASH_LEN]);
        assert_eq!(out.aux, [0x44; AUX_LEN]);
        assert_eq!(&*out.payload, b"signed-transaction");
        assert_eq!(consumer.stats().read_sequence, 1);

        drop(consumer);
        drop(producer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn full_ring_never_overwrites_unread_slots_and_counts_drop() {
        let path = tmp("full");
        let cfg = config(Direction::GethToRust);
        let mut producer = Producer::create(&path, cfg).unwrap();
        let mut consumer = Consumer::open(&path, cfg).unwrap();

        for n in 0..cfg.capacity {
            let payload = [n as u8];
            assert_eq!(
                producer
                    .publish(&record(RecordKind::EarlyTxSeen, &payload))
                    .unwrap(),
                n as u64
            );
        }
        assert!(matches!(
            producer.publish(&record(RecordKind::EarlyTxSeen, b"x")),
            Err(FastLaneError::QueueFull { sequence: 4 })
        ));
        assert_eq!(producer.stats().dropped_full, 1);

        for n in 0..cfg.capacity {
            let out = consumer.try_consume().unwrap().unwrap();
            assert_eq!(out.sequence, n as u64);
            assert_eq!(&*out.payload, &[n as u8]);
        }

        drop(consumer);
        drop(producer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn slots_wrap_only_after_consumer_releases_them() {
        let path = tmp("wrap");
        let cfg = config(Direction::RustToGeth);
        let mut producer = Producer::create(&path, cfg).unwrap();
        let mut consumer = Consumer::open(&path, cfg).unwrap();

        for n in 0..32u8 {
            producer
                .publish(&record(RecordKind::OutboundSignedTx, &[n]))
                .unwrap();
            let out = consumer.try_consume().unwrap().unwrap();
            assert_eq!(out.sequence, u64::from(n));
            assert_eq!(&*out.payload, &[n]);
        }
        assert_eq!(producer.stats().write_sequence, 32);
        assert_eq!(consumer.stats().read_sequence, 32);

        drop(consumer);
        drop(producer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn generation_and_direction_mismatch_fail_closed() {
        let path = tmp("identity");
        let cfg = config(Direction::GethToRust);
        let producer = Producer::create(&path, cfg).unwrap();

        let mut wrong_generation = cfg;
        wrong_generation.generation += 1;
        assert!(matches!(
            Consumer::open(&path, wrong_generation),
            Err(FastLaneError::GenerationMismatch { .. })
        ));

        let mut wrong_direction = cfg;
        wrong_direction.direction = Direction::RustToGeth;
        assert!(matches!(
            Consumer::open(&path, wrong_direction),
            Err(FastLaneError::DirectionMismatch { .. })
        ));

        drop(producer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn payload_bound_is_checked_before_publication() {
        let path = tmp("payload");
        let cfg = config(Direction::GethToRust);
        let mut producer = Producer::create(&path, cfg).unwrap();
        let payload = vec![0u8; cfg.max_payload() + 1];

        assert!(matches!(
            producer.publish(&record(RecordKind::EarlyTxSeen, &payload)),
            Err(FastLaneError::PayloadTooLarge { .. })
        ));
        assert_eq!(producer.stats().write_sequence, 0);

        drop(producer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn shutdown_is_visible_across_mappings() {
        let path = tmp("shutdown");
        let cfg = config(Direction::RustToGeth);
        let producer = Producer::create(&path, cfg).unwrap();
        let mut consumer = Consumer::open(&path, cfg).unwrap();

        producer.shutdown();
        assert!(matches!(
            consumer.try_consume(),
            Err(FastLaneError::Shutdown)
        ));
        assert!(consumer.stats().shutdown);

        drop(consumer);
        drop(producer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn crc32_matches_standard_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
}
