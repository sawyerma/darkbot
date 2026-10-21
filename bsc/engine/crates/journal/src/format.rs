//! On-disk journal format: record kinds, records, the append-only writer,
//! its manifest and the journal errors. Frames are versioned, checksummed
//! (CRC-32) per frame and hashed (SHA-256) over the exact bytes written.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::{Sha256, frame_crc};

pub const JOURNAL_MAGIC: [u8; 8] = *b"DBJNL001";
pub const JOURNAL_VERSION: u16 = 1;
pub const JOURNAL_SPAN_VERSION: u16 = 2;
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
    /// V2 only: u64 LE count of proven consecutive QueueFull losses.
    GapSpan = 7,
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
            7 => Ok(Self::GapSpan),
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
    pub logical_records: u64,
    pub lost_records: u64,
    /// Diagnostic accepted-prefix hash only when true; never a complete file proof.
    pub poisoned: bool,
    pub version: u16,
    pub records: u64,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
    pub bytes: u64,
    pub sha256: [u8; 32],
}

pub struct JournalWriter<W> {
    inner: W,
    version: u16,
    logical_records: u64,
    lost_records: u64,
    records: u64,
    first_sequence: Option<u64>,
    last_sequence: Option<u64>,
    bytes: u64,
    hasher: Sha256,
    poisoned: bool,
}

impl<W: Write> JournalWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            version: JOURNAL_VERSION,
            logical_records: 0,
            lost_records: 0,
            records: 0,
            first_sequence: None,
            last_sequence: None,
            bytes: 0,
            hasher: Sha256::new(),
            poisoned: false,
        }
    }

    pub fn new_v2(inner: W) -> Self {
        let mut writer = Self::new(inner);
        writer.version = JOURNAL_SPAN_VERSION;
        writer
    }

    pub fn append(&mut self, record: &CaptureRecord) -> Result<(), JournalError> {
        if self.poisoned { return Err(JournalError::Poisoned); }
        let span = logical_span(self.version, record.kind, &record.payload)?;
        let last = record.sequence.checked_add(span - 1).ok_or(JournalError::SequenceOverflow)?;
        let logical_records = self.logical_records.checked_add(span).ok_or(JournalError::SequenceOverflow)?;
        let lost_records = self.lost_records.checked_add(if record.kind == RecordKind::GapSpan { span } else { 0 }).ok_or(JournalError::SequenceOverflow)?;
        let records = self.records.checked_add(1).ok_or(JournalError::SequenceOverflow)?;
        let bytes = self.bytes.checked_add(HEADER_LEN as u64)
            .and_then(|n| n.checked_add(record.payload.len() as u64))
            .ok_or(JournalError::SequenceOverflow)?;
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
        header[8..10].copy_from_slice(&self.version.to_le_bytes());
        header[10..12].copy_from_slice(&(record.kind as u16).to_le_bytes());
        header[12..20].copy_from_slice(&record.sequence.to_le_bytes());
        header[20..28].copy_from_slice(&record.timestamp_ns.to_le_bytes());
        header[28..32].copy_from_slice(&(record.payload.len() as u32).to_le_bytes());
        let checksum = frame_crc(&header, &record.payload);
        header[32..36].copy_from_slice(&checksum.to_le_bytes());

        if let Err(error) = self.inner.write_all(&header).and_then(|()| self.inner.write_all(&record.payload)) {
            self.poisoned = true;
            return Err(JournalError::Io(error));
        }
        self.hasher.update(&header);
        self.hasher.update(&record.payload);

        self.records = records;
        self.first_sequence.get_or_insert(record.sequence);
        self.last_sequence = Some(last);
        self.logical_records = logical_records;
        self.lost_records = lost_records;
        self.bytes = bytes;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), JournalError> {
        if self.poisoned { return Err(JournalError::Poisoned); }
        if let Err(error) = self.inner.flush() {
            self.poisoned = true;
            return Err(JournalError::Io(error));
        }
        Ok(())
    }

    /// Accepted-prefix diagnostics, NOT durability authority. Poisoned means
    /// the physical tail may differ and this hash must not certify the file.
    pub fn manifest(&self) -> JournalManifest {
        JournalManifest {
            poisoned: self.poisoned,            version: self.version,
            logical_records: self.logical_records,
            lost_records: self.lost_records,
            records: self.records,
            first_sequence: self.first_sequence,
            last_sequence: self.last_sequence,
            bytes: self.bytes,
            sha256: self.hasher.clone().finalize(),
        }
    }

    fn sync_with(&mut self, sync: impl FnOnce(&mut W) -> io::Result<()>) -> Result<(), JournalError> {
        self.flush()?;
        if let Err(error) = sync(&mut self.inner) {
            self.poisoned = true;
            return Err(JournalError::Io(error));
        }
        Ok(())
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl JournalWriter<std::fs::File> {
    /// File-only durability checkpoint; no generic Write::flush guarantee.
    pub fn sync_file(&mut self) -> Result<(), JournalError> {
        self.sync_with(|file| file.sync_all())
    }

    /// Consume the sole file writer, flush and fsync, then hash/read back the
    /// exact file before returning its manifest. Caller must sync the containing
    /// directory and atomically commit its session evidence/manifest separately.
    /// Requires a read/write, create-new file; errors never yield a committed hash.
    pub fn commit_file(mut self) -> Result<JournalManifest, JournalError> {
        self.sync_file()?;
        let manifest = self.manifest();
        if self.inner.metadata().map_err(JournalError::Io)?.len() != manifest.bytes {
            return Err(JournalError::FileMismatch);
        }
        self.inner.seek(SeekFrom::Start(0)).map_err(JournalError::Io)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 8192];
        loop {
            let read = self.inner.read(&mut buffer).map_err(JournalError::Io)?;
            if read == 0 { break; }
            hash.update(&buffer[..read]);
        }
        if hash.finalize() != manifest.sha256 { return Err(JournalError::FileMismatch); }
        Ok(manifest)
    }
}

pub(crate) fn logical_span(version: u16, kind: RecordKind, payload: &[u8]) -> Result<u64, JournalError> {
    if kind != RecordKind::GapSpan { return Ok(1); }
    if version != JOURNAL_SPAN_VERSION || payload.len() != 8 { return Err(JournalError::InvalidSpan); }
    let span = u64::from_le_bytes(payload.try_into().map_err(|_| JournalError::InvalidSpan)?);
    if span == 0 { return Err(JournalError::InvalidSpan); }
    Ok(span)
}

#[derive(Debug)]
pub enum JournalError {
    InvalidBound,
    InvalidSpan,
    Poisoned,
    FileMismatch,
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
            Self::Poisoned => f.write_str("capture: writer poisoned; incomplete physical tail"),
            Self::FileMismatch => f.write_str("capture: persistent bytes differ from accepted frames"),
            Self::InvalidSpan => f.write_str("capture: invalid/version-incompatible GAP span"),
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

#[cfg(test)]
mod write_failure_tests {
    use super::*;
    struct FailWrite { bytes: Vec<u8>, remaining: usize, flush_fails: bool }
    impl Write for FailWrite {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 { return Err(io::Error::other("injected write failure")); }
            let n = self.remaining.min(bytes.len());
            self.bytes.extend_from_slice(&bytes[..n]); self.remaining -= n; Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.flush_fails { Err(io::Error::other("injected flush failure")) } else { Ok(()) }
        }
    }
    fn record(sequence: u64) -> CaptureRecord {
        CaptureRecord { sequence, kind: RecordKind::Runtime, timestamp_ns: 0, payload: b"test".to_vec().into_boxed_slice() }
    }
    #[test]
    fn every_partial_write_and_flush_failure_is_terminal() {
        for accepted in [0, 7, HEADER_LEN, HEADER_LEN + 2] {
            let mut writer = JournalWriter::new(FailWrite { bytes: vec![], remaining: accepted, flush_fails: false });
            assert!(writer.append(&record(0)).is_err());
            assert!(writer.manifest().poisoned);
            assert_eq!(writer.manifest().records, 0);
            assert!(matches!(writer.append(&record(1)), Err(JournalError::Poisoned)));
            assert!(matches!(writer.flush(), Err(JournalError::Poisoned)));
            assert_eq!(writer.into_inner().bytes.len(), accepted);
        }
        let mut writer = JournalWriter::new(FailWrite { bytes: vec![], remaining: usize::MAX, flush_fails: true });
        writer.append(&record(0)).unwrap();
        assert!(writer.flush().is_err());
        assert!(writer.manifest().poisoned);
        assert!(matches!(writer.append(&record(1)), Err(JournalError::Poisoned)));
    }
    #[test]
    fn failed_sync_and_second_frame_never_certify_a_complete_segment() {
        let mut writer = JournalWriter::new(vec![]);
        writer.append(&record(0)).unwrap();
        assert!(writer.sync_with(|_| Err(io::Error::other("injected fsync"))).is_err());
        assert!(writer.manifest().poisoned);
        assert!(matches!(writer.append(&record(1)), Err(JournalError::Poisoned)));
        let mut writer = JournalWriter::new(FailWrite { bytes: vec![], remaining: HEADER_LEN + 4 + 9, flush_fails: false });
        writer.append(&record(0)).unwrap();
        assert!(writer.append(&record(1)).is_err());
        assert_eq!(writer.manifest().records, 1);
        assert!(writer.manifest().poisoned);
        let bytes = writer.into_inner().bytes;
        let mut reader = crate::JournalReader::new(bytes.as_slice(), 64).unwrap();
        assert!(reader.next_record().unwrap().is_some());
        assert!(matches!(reader.next_record(), Err(JournalError::Truncated)));
    }

    #[test]
    fn validation_errors_do_not_poison_but_counters_never_saturate() {
        let mut writer = JournalWriter::new(vec![]);
        writer.append(&record(0)).unwrap();
        assert!(writer.append(&record(2)).is_err());
        assert!(!writer.manifest().poisoned);
        writer.append(&record(1)).unwrap();
        writer.records = u64::MAX;
        assert!(matches!(writer.append(&record(2)), Err(JournalError::SequenceOverflow)));
        writer.records = 2; writer.bytes = u64::MAX;
        assert!(matches!(writer.append(&record(2)), Err(JournalError::SequenceOverflow)));
    }
    #[test]
    fn file_commit_matches_bytes_and_rejects_modified_or_unreadable_file() {
        for variant in 0..3 {
            let path = std::env::temp_dir().join(format!("journal-commit-{}-{variant}", std::process::id()));
            let file = std::fs::OpenOptions::new().create_new(true).read(variant != 2).write(true).open(&path).unwrap();
            let mut writer = JournalWriter::new(file);
            writer.append(&record(0)).unwrap();
            if variant == 1 { std::fs::write(&path, b"foreign bytes").unwrap(); }
            let result = writer.commit_file();
            if variant == 0 {
                let manifest = result.unwrap();
                let bytes = std::fs::read(&path).unwrap();
                let mut hash = Sha256::new(); hash.update(&bytes);
                assert_eq!(manifest.sha256, hash.finalize());
                assert!(!manifest.poisoned);
            } else { assert!(result.is_err()); }
            std::fs::remove_file(path).unwrap();
        }
    }
}
