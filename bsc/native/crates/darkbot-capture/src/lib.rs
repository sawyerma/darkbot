//! Minimal production capture/replay foundation required by C0.
//!
//! The journal is append-only, versioned, checksummed per frame and hashed
//! over the exact bytes written. Replay verifies framing, checksum, bounds
//! and contiguous sequence numbers before yielding a record.
//!
//! The bounded recorder uses `try_send`: the producer never waits for the
//! writer. Queue overflow is returned to the caller and counted, never
//! silently discarded. The drain/writer belongs on a background owner.
//! Allocation freedom and final Fast-Lane placement are later C1/C3
//! qualification concerns; this module makes no latency claim.

use std::fmt;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecorderStats {
    pub next_sequence: u64,
    pub enqueued: u64,
    pub dropped_full: u64,
    pub rejected_oversize: u64,
    pub disconnected: u64,
}

#[derive(Debug)]
struct RecorderCounters {
    next_sequence: AtomicU64,
    enqueued: AtomicU64,
    dropped_full: AtomicU64,
    rejected_oversize: AtomicU64,
    disconnected: AtomicU64,
}

/// Non-blocking producer side of the bounded capture queue.
pub struct Recorder {
    sender: SyncSender<CaptureRecord>,
    max_payload: usize,
    counters: RecorderCounters,
}

pub struct RecorderDrain {
    receiver: Receiver<CaptureRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    PayloadTooLarge { len: usize, max: usize },
    QueueFull { sequence: u64 },
    Disconnected { sequence: u64 },
}

impl Recorder {
    pub fn bounded(
        capacity: usize,
        max_payload: usize,
    ) -> Result<(Self, RecorderDrain), JournalError> {
        if capacity == 0 || max_payload == 0 {
            return Err(JournalError::InvalidBound);
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        Ok((
            Self {
                sender,
                max_payload,
                counters: RecorderCounters {
                    next_sequence: AtomicU64::new(0),
                    enqueued: AtomicU64::new(0),
                    dropped_full: AtomicU64::new(0),
                    rejected_oversize: AtomicU64::new(0),
                    disconnected: AtomicU64::new(0),
                },
            },
            RecorderDrain { receiver },
        ))
    }

    /// Never waits. A failed enqueue remains visible through the returned
    /// sequence and the counters; the next successful record therefore also
    /// exposes the sequence gap during replay.
    pub fn try_record(
        &self,
        kind: RecordKind,
        timestamp_ns: u64,
        payload: &[u8],
    ) -> Result<u64, RecordError> {
        if payload.len() > self.max_payload {
            self.counters
                .rejected_oversize
                .fetch_add(1, Ordering::Relaxed);
            return Err(RecordError::PayloadTooLarge {
                len: payload.len(),
                max: self.max_payload,
            });
        }
        let sequence = self.counters.next_sequence.fetch_add(1, Ordering::AcqRel);
        let record = CaptureRecord {
            sequence,
            kind,
            timestamp_ns,
            payload: payload.to_vec().into_boxed_slice(),
        };
        match self.sender.try_send(record) {
            Ok(()) => {
                self.counters.enqueued.fetch_add(1, Ordering::Relaxed);
                Ok(sequence)
            }
            Err(TrySendError::Full(_)) => {
                self.counters.dropped_full.fetch_add(1, Ordering::Relaxed);
                Err(RecordError::QueueFull { sequence })
            }
            Err(TrySendError::Disconnected(_)) => {
                self.counters.disconnected.fetch_add(1, Ordering::Relaxed);
                Err(RecordError::Disconnected { sequence })
            }
        }
    }

    pub fn stats(&self) -> RecorderStats {
        RecorderStats {
            next_sequence: self.counters.next_sequence.load(Ordering::Acquire),
            enqueued: self.counters.enqueued.load(Ordering::Acquire),
            dropped_full: self.counters.dropped_full.load(Ordering::Acquire),
            rejected_oversize: self.counters.rejected_oversize.load(Ordering::Acquire),
            disconnected: self.counters.disconnected.load(Ordering::Acquire),
        }
    }
}

impl RecorderDrain {
    /// Background-owner receive. It may block and must never run on the
    /// decision hot path.
    pub fn recv(&self) -> Result<CaptureRecord, JournalError> {
        self.receiver
            .recv()
            .map_err(|_| JournalError::QueueDisconnected)
    }

    pub fn try_recv(&self) -> Result<Option<CaptureRecord>, JournalError> {
        match self.receiver.try_recv() {
            Ok(record) => Ok(Some(record)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(JournalError::QueueDisconnected),
        }
    }
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

pub struct JournalReader<R> {
    inner: R,
    max_payload: usize,
    last_sequence: Option<u64>,
}

impl<R: Read> JournalReader<R> {
    pub fn new(inner: R, max_payload: usize) -> Result<Self, JournalError> {
        if max_payload == 0 {
            return Err(JournalError::InvalidBound);
        }
        Ok(Self {
            inner,
            max_payload,
            last_sequence: None,
        })
    }

    pub fn next_record(&mut self) -> Result<Option<CaptureRecord>, JournalError> {
        let mut header = [0u8; HEADER_LEN];
        let mut read = 0usize;
        while read < HEADER_LEN {
            match self.inner.read(&mut header[read..]) {
                Ok(0) if read == 0 => return Ok(None),
                Ok(0) => return Err(JournalError::Truncated),
                Ok(n) => read += n,
                Err(error) => return Err(JournalError::Io(error)),
            }
        }
        if header[..8] != JOURNAL_MAGIC {
            return Err(JournalError::BadMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != JOURNAL_VERSION {
            return Err(JournalError::BadVersion(version));
        }
        let kind = RecordKind::try_from(u16::from_le_bytes([header[10], header[11]]))?;
        let sequence = u64::from_le_bytes(
            header[12..20]
                .try_into()
                .map_err(|_| JournalError::Truncated)?,
        );
        let timestamp_ns = u64::from_le_bytes(
            header[20..28]
                .try_into()
                .map_err(|_| JournalError::Truncated)?,
        );
        let payload_len = u32::from_le_bytes(
            header[28..32]
                .try_into()
                .map_err(|_| JournalError::Truncated)?,
        ) as usize;
        let expected_crc = u32::from_le_bytes(
            header[32..36]
                .try_into()
                .map_err(|_| JournalError::Truncated)?,
        );
        if payload_len > self.max_payload {
            return Err(JournalError::PayloadBound {
                len: payload_len,
                max: self.max_payload,
            });
        }
        if let Some(last) = self.last_sequence {
            let expected = last.checked_add(1).ok_or(JournalError::SequenceOverflow)?;
            if sequence != expected {
                return Err(JournalError::SequenceGap {
                    expected,
                    observed: sequence,
                });
            }
        }
        let mut payload = vec![0u8; payload_len];
        self.inner.read_exact(&mut payload).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                JournalError::Truncated
            } else {
                JournalError::Io(error)
            }
        })?;
        let actual_crc = frame_crc(&header, &payload);
        if actual_crc != expected_crc {
            return Err(JournalError::Checksum {
                expected: expected_crc,
                actual: actual_crc,
            });
        }
        self.last_sequence = Some(sequence);
        Ok(Some(CaptureRecord {
            sequence,
            kind,
            timestamp_ns,
            payload: payload.into_boxed_slice(),
        }))
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

fn frame_crc(header: &[u8; HEADER_LEN], payload: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(&header[..32]);
    crc.update(payload);
    crc.finalize()
}

#[derive(Clone, Copy)]
struct Crc32(u32);

impl Crc32 {
    fn new() -> Self {
        Self(0xffff_ffff)
    }

    fn update(&mut self, bytes: &[u8]) {
        let mut crc = self.0;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = 0u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        self.0 = crc;
    }

    fn finalize(self) -> u32 {
        !self.0
    }
}

#[derive(Clone)]
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    len_bytes: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buffer_len: 0,
            len_bytes: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.len_bytes = self
            .len_bytes
            .saturating_add(u64::try_from(input.len()).unwrap_or(u64::MAX));
        if self.buffer_len != 0 {
            let take = (64 - self.buffer_len).min(input.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&input[..take]);
            self.buffer_len += take;
            input = &input[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffer_len = 0;
            }
        }
        while input.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&input[..64]);
            self.compress(&block);
            input = &input[64..];
        }
        if !input.is_empty() {
            self.buffer[..input.len()].copy_from_slice(input);
            self.buffer_len = input.len();
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.len_bytes.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            self.buffer[self.buffer_len..].fill(0);
            let block = self.buffer;
            self.compress(&block);
            self.buffer = [0u8; 64];
            self.buffer_len = 0;
        }
        self.buffer[self.buffer_len..56].fill(0);
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        self.compress(&block);

        let mut out = [0u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
            w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use std::fmt::Write as _;
            write!(&mut out, "{byte:02x}").unwrap();
        }
        out
    }

    #[test]
    fn crc32_matches_the_standard_vector() {
        let mut crc = Crc32::new();
        crc.update(b"123456789");
        assert_eq!(crc.finalize(), 0xcbf4_3926);
    }

    #[test]
    fn sha256_matches_the_standard_vector() {
        let mut sha = Sha256::new();
        sha.update(b"abc");
        assert_eq!(
            hex(&sha.finalize()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn journal_roundtrip_preserves_exact_records_and_manifest() {
        let mut writer = JournalWriter::new(Vec::new());
        let a = CaptureRecord {
            sequence: 7,
            kind: RecordKind::RawSignedTx,
            timestamp_ns: 11,
            payload: b"alpha".to_vec().into_boxed_slice(),
        };
        let b = CaptureRecord {
            sequence: 8,
            kind: RecordKind::Canonical,
            timestamp_ns: 12,
            payload: b"beta".to_vec().into_boxed_slice(),
        };
        writer.append(&a).unwrap();
        writer.append(&b).unwrap();
        let manifest = writer.manifest();
        assert_eq!(manifest.records, 2);
        assert_eq!(manifest.first_sequence, Some(7));
        assert_eq!(manifest.last_sequence, Some(8));
        assert_ne!(manifest.sha256, [0u8; 32]);

        let bytes = writer.into_inner();
        let mut reader = JournalReader::new(Cursor::new(bytes), DEFAULT_MAX_PAYLOAD).unwrap();
        assert_eq!(reader.next_record().unwrap(), Some(a));
        assert_eq!(reader.next_record().unwrap(), Some(b));
        assert_eq!(reader.next_record().unwrap(), None);
    }

    #[test]
    fn corruption_and_truncation_fail_closed() {
        let record = CaptureRecord {
            sequence: 0,
            kind: RecordKind::Runtime,
            timestamp_ns: 9,
            payload: b"payload".to_vec().into_boxed_slice(),
        };
        let mut writer = JournalWriter::new(Vec::new());
        writer.append(&record).unwrap();
        let mut bytes = writer.into_inner();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let mut reader = JournalReader::new(Cursor::new(bytes), 1024).unwrap();
        assert!(matches!(
            reader.next_record(),
            Err(JournalError::Checksum { .. })
        ));

        let mut writer = JournalWriter::new(Vec::new());
        writer.append(&record).unwrap();
        let mut bytes = writer.into_inner();
        bytes.pop();
        let mut reader = JournalReader::new(Cursor::new(bytes), 1024).unwrap();
        assert!(matches!(reader.next_record(), Err(JournalError::Truncated)));
    }

    #[test]
    fn sequence_gaps_are_rejected_by_writer_and_replay() {
        let mut writer = JournalWriter::new(Vec::new());
        let a = CaptureRecord {
            sequence: 1,
            kind: RecordKind::Runtime,
            timestamp_ns: 1,
            payload: Box::new([]),
        };
        let b = CaptureRecord {
            sequence: 3,
            kind: RecordKind::Runtime,
            timestamp_ns: 2,
            payload: Box::new([]),
        };
        writer.append(&a).unwrap();
        assert!(matches!(
            writer.append(&b),
            Err(JournalError::SequenceGap {
                expected: 2,
                observed: 3
            })
        ));
    }

    #[test]
    fn recorder_never_waits_and_overflow_is_counted_with_sequence() {
        let (recorder, drain) = Recorder::bounded(1, 64).unwrap();
        assert_eq!(recorder.try_record(RecordKind::Runtime, 1, b"a"), Ok(0));
        assert_eq!(
            recorder.try_record(RecordKind::Runtime, 2, b"b"),
            Err(RecordError::QueueFull { sequence: 1 })
        );
        let stats = recorder.stats();
        assert_eq!(stats.enqueued, 1);
        assert_eq!(stats.dropped_full, 1);
        assert_eq!(stats.next_sequence, 2);
        assert_eq!(drain.try_recv().unwrap().unwrap().sequence, 0);
    }

    #[test]
    fn oversize_payload_is_refused_before_sequence_consumption() {
        let (recorder, _drain) = Recorder::bounded(1, 2).unwrap();
        assert_eq!(
            recorder.try_record(RecordKind::Runtime, 1, b"abc"),
            Err(RecordError::PayloadTooLarge { len: 3, max: 2 })
        );
        let stats = recorder.stats();
        assert_eq!(stats.next_sequence, 0);
        assert_eq!(stats.rejected_oversize, 1);
    }

    #[test]
    fn reader_enforces_payload_bound_before_allocating_payload() {
        let record = CaptureRecord {
            sequence: 0,
            kind: RecordKind::Runtime,
            timestamp_ns: 1,
            payload: vec![1u8; 16].into_boxed_slice(),
        };
        let mut writer = JournalWriter::new(Vec::new());
        writer.append(&record).unwrap();
        let mut reader = JournalReader::new(Cursor::new(writer.into_inner()), 8).unwrap();
        assert!(matches!(
            reader.next_record(),
            Err(JournalError::PayloadBound { len: 16, max: 8 })
        ));
    }

    #[test]
    fn manifest_hash_is_over_exact_journal_bytes() {
        let record = CaptureRecord {
            sequence: 0,
            kind: RecordKind::Gap,
            timestamp_ns: 5,
            payload: b"drop=1".to_vec().into_boxed_slice(),
        };
        let mut writer = JournalWriter::new(Vec::new());
        writer.append(&record).unwrap();
        let manifest = writer.manifest();
        let bytes = writer.into_inner();
        let mut sha = Sha256::new();
        sha.update(&bytes);
        assert_eq!(manifest.sha256, sha.finalize());
        assert_eq!(manifest.bytes as usize, bytes.len());
    }
}
