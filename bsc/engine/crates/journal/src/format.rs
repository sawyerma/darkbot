//! On-disk journal format: record kinds, records, the append-only writer,
//! its manifest and the journal errors. Frames are versioned, checksummed
//! (CRC-32) per frame and hashed (SHA-256) over the exact bytes written.

use std::fmt;
use std::io::{self, Write};

use crate::{Sha256, frame_crc};

pub const JOURNAL_MAGIC: [u8; 8] = *b"DBJNL001";
pub const JOURNAL_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 36;
pub const DEFAULT_MAX_PAYLOAD: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum RecordKind {
    RawSignedTx = 1,
    TxpoolAccepted = 2,
    Canonical = 3,
    Reorg = 4,
    Gap = 5,
    Runtime = 6,
}

impl TryFrom<u16> for RecordKind {
    type Error = JournalError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::RawSignedTx),
            2 => Ok(Self::TxpoolAccepted),
            3 => Ok(Self::Canonical),
            4 => Ok(Self::Reorg),
            5 => Ok(Self::Gap),
            6 => Ok(Self::Runtime),
            _ => Err(JournalError::UnknownKind(value)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureRecord {
    pub sequence: u64,
    pub kind: RecordKind,
    pub timestamp_ns: u64,
    pub payload: Box<[u8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JournalManifest {
    pub version: u16,
    pub records: u64,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
    pub bytes: u64,
    pub sha256: [u8; 32],
}

pub struct JournalWriter<W> {
    inner: W,
    records: u64,
    first_sequence: Option<u64>,
    last_sequence: Option<u64>,
    bytes: u64,
    hasher: Sha256,
}

impl<W: Write> JournalWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            records: 0,
            first_sequence: None,
            last_sequence: None,
            bytes: 0,
            hasher: Sha256::new(),
        }
    }

    pub fn append(&mut self, record: &CaptureRecord) -> Result<(), JournalError> {
        if record.payload.len() > u32::MAX as usize {
            return Err(JournalError::PayloadTooLarge);
        }
        if let Some(last) = self.last_sequence {
            let expected = last.checked_add(1).ok_or(JournalError::SequenceOverflow)?;
            if record.sequence != expected {
                return Err(JournalError::SequenceGap {
                    expected,
                    observed: record.sequence,
                });
            }
        }

        let mut header = [0u8; HEADER_LEN];
        header[..8].copy_from_slice(&JOURNAL_MAGIC);
        header[8..10].copy_from_slice(&JOURNAL_VERSION.to_le_bytes());
        header[10..12].copy_from_slice(&(record.kind as u16).to_le_bytes());
        header[12..20].copy_from_slice(&record.sequence.to_le_bytes());
        header[20..28].copy_from_slice(&record.timestamp_ns.to_le_bytes());
        header[28..32].copy_from_slice(&(record.payload.len() as u32).to_le_bytes());
        let checksum = frame_crc(&header, &record.payload);
        header[32..36].copy_from_slice(&checksum.to_le_bytes());

        self.inner.write_all(&header).map_err(JournalError::Io)?;
        self.inner
            .write_all(&record.payload)
            .map_err(JournalError::Io)?;
        self.hasher.update(&header);
        self.hasher.update(&record.payload);

        self.records = self.records.saturating_add(1);
        self.first_sequence.get_or_insert(record.sequence);
        self.last_sequence = Some(record.sequence);
        self.bytes = self
            .bytes
            .saturating_add(u64::try_from(HEADER_LEN + record.payload.len()).unwrap_or(u64::MAX));
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), JournalError> {
        self.inner.flush().map_err(JournalError::Io)
    }

    pub fn manifest(&self) -> JournalManifest {
        JournalManifest {
            version: JOURNAL_VERSION,
            records: self.records,
            first_sequence: self.first_sequence,
            last_sequence: self.last_sequence,
            bytes: self.bytes,
            sha256: self.hasher.clone().finalize(),
        }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

#[derive(Debug)]
pub enum JournalError {
    InvalidBound,
    BadMagic,
    BadVersion(u16),
    UnknownKind(u16),
    PayloadTooLarge,
    PayloadBound { len: usize, max: usize },
    Truncated,
    Checksum { expected: u32, actual: u32 },
    SequenceGap { expected: u64, observed: u64 },
    SequenceOverflow,
    QueueDisconnected,
    Io(io::Error),
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBound => f.write_str("capture: invalid bound"),
            Self::BadMagic => f.write_str("capture: bad journal magic"),
            Self::BadVersion(version) => write!(f, "capture: unsupported version {version}"),
            Self::UnknownKind(kind) => write!(f, "capture: unknown record kind {kind}"),
            Self::PayloadTooLarge => f.write_str("capture: payload cannot be represented"),
            Self::PayloadBound { len, max } => {
                write!(f, "capture: payload {len} exceeds bound {max}")
            }
            Self::Truncated => f.write_str("capture: truncated frame"),
            Self::Checksum { expected, actual } => {
                write!(f, "capture: checksum mismatch {actual:#x} != {expected:#x}")
            }
            Self::SequenceGap { expected, observed } => {
                write!(
                    f,
                    "capture: sequence gap expected {expected} observed {observed}"
                )
            }
            Self::SequenceOverflow => f.write_str("capture: sequence overflow"),
            Self::QueueDisconnected => f.write_str("capture: recorder queue disconnected"),
            Self::Io(error) => write!(f, "capture: I/O {error}"),
        }
    }
}

impl std::error::Error for JournalError {}
