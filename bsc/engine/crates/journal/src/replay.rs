//! Replay: verifies framing, checksum, bounds and contiguous sequence numbers
//! before yielding a record.

use std::io::{self, Read};

use crate::format::{
    CaptureRecord, HEADER_LEN, JOURNAL_MAGIC, JOURNAL_VERSION, JournalError, RecordKind,
};
use crate::frame_crc;

pub struct JournalReader<R> {
    inner: R,
    max_payload: usize,
    last_sequence: Option<u64>,
    version: Option<u16>,
    failed: bool,
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
            version: None,
            failed: false,
        })
    }

    pub fn next_record(&mut self) -> Result<Option<CaptureRecord>, JournalError> {
        if self.failed { return Err(JournalError::Poisoned); }
        let result = self.read_frame();
        if result.is_err() { self.failed = true; }
        result
    }

    fn read_frame(&mut self) -> Result<Option<CaptureRecord>, JournalError> {
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
        if ![JOURNAL_VERSION, crate::format::JOURNAL_SPAN_VERSION].contains(&version) || self.version.is_some_and(|v| v != version) {
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
        let span = crate::format::logical_span(version, kind, &payload)?;
        self.last_sequence = Some(sequence.checked_add(span - 1).ok_or(JournalError::SequenceOverflow)?);
        self.version = Some(version);
        Ok(Some(CaptureRecord {
            sequence,
            kind,
            timestamp_ns,
            payload: payload.into_boxed_slice(),
        }))
    }
}
