//! Bounded recorder: the producer never waits for the writer (`try_send`);
//! overflow is returned to the caller and counted, never silently dropped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

use crate::format::{CaptureRecord, JournalError, RecordKind};

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
