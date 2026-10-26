//! Bounded recorder: the producer never waits for the writer (`try_send`);
//! overflow is returned to the caller and counted, never silently dropped.

use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

use crate::format::{CaptureRecord, JournalError, RecordKind};
use crate::storage::budget::{BytePermit, MemoryBudget};

/// Bytes one queued record holds besides its payload: the queue slot.
pub const RECORD_OVERHEAD: usize = std::mem::size_of::<Queued>();

/// A record in the queue, with the bytes it holds of the capture budget.
#[derive(Debug)]
struct Queued {
    record: CaptureRecord,
    permit: Option<BytePermit>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecorderStats {
    pub next_sequence: u64,
    pub exhausted: bool,
    pub enqueued: u64,
    pub dropped_full: u64,
    /// Records the capture's byte budget refused (a visible loss like a
    /// full queue: the sequence is consumed).
    pub dropped_budget: u64,
    pub rejected_oversize: u64,
    pub disconnected: u64,
}

#[derive(Debug)]
struct RecorderCounters {
    exhausted: AtomicBool,
    next_sequence: AtomicU64,
    enqueued: AtomicU64,
    dropped_full: AtomicU64,
    dropped_budget: AtomicU64,
    rejected_oversize: AtomicU64,
    disconnected: AtomicU64,
}

/// Non-blocking producer side of the bounded capture queue.
pub struct Recorder {
    sender: SyncSender<Queued>,
    max_payload: usize,
    budget: Option<Arc<MemoryBudget>>,
    counters: Arc<RecorderCounters>,
}

pub struct RecorderDrain {
    receiver: Receiver<Queued>,
    counters: Arc<RecorderCounters>,
}

/// Exclusive V2 drain. It cannot be mixed with raw receives.
pub struct SpannedDrain {
    receiver: Receiver<Queued>,
    counters: Arc<RecorderCounters>,
    expected: u64,
    reported_lost: u64,
    pending: Option<Queued>,
    /// The bytes of the record returned last: released at the next
    /// receive, after the caller wrote it.
    held: Option<BytePermit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    PayloadTooLarge { len: usize, max: usize },
    QueueFull { sequence: u64 },
    /// The capture's byte budget is exhausted; the sequence is consumed.
    BudgetExhausted { sequence: u64 },
    Disconnected { sequence: u64 },
    SequenceOverflow,
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
        let counters = Arc::new(RecorderCounters {
            exhausted: AtomicBool::new(false),
            next_sequence: AtomicU64::new(0), enqueued: AtomicU64::new(0),
            dropped_full: AtomicU64::new(0), dropped_budget: AtomicU64::new(0),
            rejected_oversize: AtomicU64::new(0), disconnected: AtomicU64::new(0),
        });
        Ok((Self { sender, max_payload, budget: None, counters: Arc::clone(&counters) },
            RecorderDrain { receiver, counters }))
    }

    pub fn bounded_spanned(capacity: usize, max_payload: usize) -> Result<(Self, SpannedDrain), JournalError> {
        let (recorder, drain) = Self::bounded(capacity, max_payload)?;
        Ok((recorder, SpannedDrain { receiver: drain.receiver, counters: drain.counters,
            expected: 0, reported_lost: 0, pending: None, held: None }))
    }

    /// A spanned recorder whose queued records hold their bytes
    /// (`RECORD_OVERHEAD` + payload) in `budget` until the writer took them.
    pub fn bounded_spanned_budgeted(
        capacity: usize,
        max_payload: usize,
        budget: Arc<MemoryBudget>,
    ) -> Result<(Self, SpannedDrain), JournalError> {
        let (mut recorder, drain) = Self::bounded_spanned(capacity, max_payload)?;
        recorder.budget = Some(budget);
        Ok((recorder, drain))
    }

    /// Never waits. A failed enqueue remains visible through the returned
    /// sequence and the counters; the next successful record therefore also
    /// exposes the sequence gap during replay.
    /// A single mutable owner assigns and enqueues in one order. Sharing an
    /// immutable Recorder between concurrent producers cannot call this API.
    /// ```compile_fail
    /// use darkbot_bsc_journal::{Recorder, RecordKind};
    /// let (recorder, _) = Recorder::bounded(2, 64).unwrap();
    /// let shared = &recorder;
    /// shared.try_record(RecordKind::Runtime, 0, b"event");
    /// ```
    pub fn try_record(
        &mut self,
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
        let sequence = self.counters.next_sequence.load(Ordering::Acquire);
        let Some(next) = sequence.checked_add(1) else {
            self.counters.exhausted.store(true, Ordering::Release);
            return Err(RecordError::SequenceOverflow);
        };
        // The bytes are admitted before they are allocated. A refusal
        // consumes the sequence: replay shows the loss as a gap.
        let permit = match &self.budget {
            None => None,
            Some(budget) => match budget.try_reserve(RECORD_OVERHEAD.saturating_add(payload.len())) {
                Some(permit) => Some(permit),
                None => {
                    self.counters.next_sequence.store(next, Ordering::Release);
                    self.counters.dropped_budget.fetch_add(1, Ordering::Relaxed);
                    return Err(RecordError::BudgetExhausted { sequence });
                }
            },
        };
        // Allocate before consuming a sequence: allocation failure cannot
        // fabricate a queue-loss span after a caught unwind.
        let payload = payload.to_vec().into_boxed_slice();
        self.counters.next_sequence.store(next, Ordering::Release);
        let record = CaptureRecord {
            sequence,
            kind,
            timestamp_ns,
            payload,
        };
        match self.sender.try_send(Queued { record, permit }) {
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
            exhausted: self.counters.exhausted.load(Ordering::Acquire),
            next_sequence: self.counters.next_sequence.load(Ordering::Acquire),
            enqueued: self.counters.enqueued.load(Ordering::Acquire),
            dropped_full: self.counters.dropped_full.load(Ordering::Acquire),
            dropped_budget: self.counters.dropped_budget.load(Ordering::Acquire),
            rejected_oversize: self.counters.rejected_oversize.load(Ordering::Acquire),
            disconnected: self.counters.disconnected.load(Ordering::Acquire),
        }
    }
}

impl SpannedDrain {
    /// V2 drain with explicit start cursor zero. Only the single producer's
    /// QueueFull holes can precede a later success. At channel close its final
    /// cursor also exposes trailing losses. Gap timestamp zero means unknown.
    pub fn recv_spanned(&mut self) -> Result<CaptureRecord, JournalError> {
        // The caller wrote the previous record: its bytes are free.
        self.held = None;
        let queued = self.pending.take().or_else(|| self.receiver.recv().ok());
        let next = queued.as_ref().map_or_else(
            || self.counters.next_sequence.load(Ordering::Acquire), |q| q.record.sequence);
        if next < self.expected { return Err(JournalError::SequenceGap { expected: self.expected, observed: next }); }
        if next > self.expected {
            let span = next - self.expected;
            // A hole must be backed by actual QueueFull accounting; absence
            // alone never establishes a loss reason.
            let dropped = self.counters.dropped_full.load(Ordering::Acquire)
                .saturating_add(self.counters.dropped_budget.load(Ordering::Acquire));
            if span > dropped.saturating_sub(self.reported_lost) { return Err(JournalError::InvalidSpan); }
            let gap = CaptureRecord { sequence: self.expected, kind: RecordKind::GapSpan,
                timestamp_ns: 0, payload: span.to_le_bytes().to_vec().into_boxed_slice() };
            self.reported_lost += span;
            self.pending = queued;
            self.expected = next;
            return Ok(gap);
        }
        let Queued { record, permit } = queued.ok_or(JournalError::QueueDisconnected)?;
        self.expected = self.expected.checked_add(1).ok_or(JournalError::SequenceOverflow)?;
        self.held = permit;
        Ok(record)
    }

}

impl RecorderDrain {
    /// Background-owner receive. It may block and must never run on the
    /// decision hot path.
    pub fn recv(&self) -> Result<CaptureRecord, JournalError> {
        self.receiver
            .recv()
            .map(|queued| queued.record)
            .map_err(|_| JournalError::QueueDisconnected)
    }

    pub fn try_recv(&self) -> Result<Option<CaptureRecord>, JournalError> {
        match self.receiver.try_recv() {
            Ok(queued) => Ok(Some(queued.record)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(JournalError::QueueDisconnected),
        }
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::*;
    #[test]
    fn bound_zero_cursor_reports_initial_loss_and_exhaustion_never_wraps() {
        let (mut recorder, mut drain) = Recorder::bounded_spanned(1, 64).unwrap();
        // Explicit synthetic queue-loss state; not a Mainnet capture claim.
        recorder.counters.next_sequence.store(3, Ordering::Release);
        recorder.counters.dropped_full.store(3, Ordering::Release);
        recorder.try_record(RecordKind::Runtime, 0, b"next").unwrap();
        let gap = drain.recv_spanned().unwrap();
        assert_eq!((gap.sequence, gap.kind), (0, RecordKind::GapSpan));
        assert_eq!(u64::from_le_bytes(gap.payload.as_ref().try_into().unwrap()), 3);
        assert_eq!(drain.recv_spanned().unwrap().sequence, 3);
        recorder.counters.next_sequence.store(u64::MAX, Ordering::Release);
        assert_eq!(recorder.try_record(RecordKind::Runtime, 0, b"next"), Err(RecordError::SequenceOverflow));
        assert_eq!(recorder.stats().next_sequence, u64::MAX);
        assert!(recorder.stats().exhausted);
    }
    /// Storage contract: the queue is bounded by bytes. A record the budget
    /// refuses is a counted loss with its sequence consumed, so the drain
    /// reports it as a gap; a record's bytes are held until the writer
    /// asks for the next one.
    #[test]
    fn the_byte_budget_bounds_the_queue_and_its_losses_are_gaps() {
        let budget = MemoryBudget::new(2 * RECORD_OVERHEAD + 10).unwrap();
        let (mut recorder, mut drain) =
            Recorder::bounded_spanned_budgeted(8, 64, Arc::clone(&budget)).unwrap();
        recorder.try_record(RecordKind::Runtime, 0, &[1; 6]).unwrap();
        assert_eq!(
            recorder.try_record(RecordKind::Runtime, 0, &[2; 6]),
            Err(RecordError::BudgetExhausted { sequence: 1 })
        );
        recorder.try_record(RecordKind::Runtime, 0, &[3; 4]).unwrap();
        assert_eq!(budget.in_use(), 2 * RECORD_OVERHEAD + 10);
        assert_eq!(recorder.stats().dropped_budget, 1);
        assert_eq!(drain.recv_spanned().unwrap().sequence, 0);
        // Held until the next receive.
        assert_eq!(budget.in_use(), 2 * RECORD_OVERHEAD + 10);
        let gap = drain.recv_spanned().unwrap();
        assert_eq!((gap.sequence, gap.kind), (1, RecordKind::GapSpan));
        assert_eq!(budget.in_use(), RECORD_OVERHEAD + 4);
        assert_eq!(drain.recv_spanned().unwrap().sequence, 2);
        recorder.try_record(RecordKind::Runtime, 0, &[4; 6]).unwrap();
        assert_eq!(drain.recv_spanned().unwrap().sequence, 3);
        assert_eq!(budget.in_use(), RECORD_OVERHEAD + 6);
        assert_eq!(budget.peak(), 2 * RECORD_OVERHEAD + 10);
    }

    #[test]
    fn single_loss_is_emitted_before_the_following_success() {
        let (mut recorder, mut drain) = Recorder::bounded_spanned(1, 64).unwrap();
        recorder.try_record(RecordKind::Runtime, 0, b"zero").unwrap();
        assert_eq!(recorder.try_record(RecordKind::Runtime, 0, b"lost"), Err(RecordError::QueueFull { sequence: 1 }));
        assert_eq!(drain.recv_spanned().unwrap().sequence, 0);
        assert_eq!(recorder.try_record(RecordKind::Runtime, 0, b"two"), Ok(2));
        let gap = drain.recv_spanned().unwrap();
        assert_eq!((gap.sequence, gap.kind), (1, RecordKind::GapSpan));
        assert_eq!(gap.payload.as_ref(), &1u64.to_le_bytes());
        assert_eq!(drain.recv_spanned().unwrap().sequence, 2);
    }
}
