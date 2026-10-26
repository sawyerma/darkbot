//! `JournalWorker` (master plan §11, storage contract): the one background
//! owner of a capture's writes. It drains the spanned recorder (gaps
//! included) into the segment writer, syncs after a bounded number of
//! records or time (the durability watermark), reports every sealed
//! segment and seals the last one when the recorder is closed. It never
//! runs on the producer's path: producers only enqueue.

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::format::{HEADER_LEN, JournalError, RecordKind};
use crate::recorder::SpannedDrain;
use crate::storage::metrics::StorageMetrics;
use crate::storage::segments::{SealedSegment, SegmentWriter};

/// When the worker makes the appended records durable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncPolicy {
    /// After this many records…
    pub every_records: u64,
    /// …or once this much time passed since the last sync, at the next
    /// record.
    pub every: Duration,
}

/// How a worker ended.
#[derive(Debug)]
pub struct WorkerEnd {
    /// Every sealed segment of the run, in order.
    pub sealed: Vec<SealedSegment>,
    pub last_sequence: Option<u64>,
}

/// Starts the worker on its own thread. `sealed` receives each segment
/// when it is sealed (for the run manifest), if given.
pub fn spawn(
    mut drain: SpannedDrain,
    mut writer: SegmentWriter,
    policy: SyncPolicy,
    metrics: Arc<StorageMetrics>,
    sealed: Option<Sender<SealedSegment>>,
) -> Result<JoinHandle<Result<WorkerEnd, JournalError>>, JournalError> {
    if policy.every_records == 0 || policy.every.is_zero() {
        return Err(JournalError::InvalidBound);
    }
    thread::Builder::new()
        .name("bsc-capture-writer".into())
        .spawn(move || {
            let fail = |e: JournalError| {
                metrics.write_failed();
                e
            };
            let mut done = Vec::new();
            let report = |s: SealedSegment, done: &mut Vec<SealedSegment>| {
                metrics.sealed();
                // A seal syncs: its records are durable.
                metrics.synced(s.manifest.last_sequence);
                if let Some(tx) = &sealed {
                    // A gone receiver only stops the reports.
                    let _ = tx.send(s.clone());
                }
                done.push(s);
            };
            let (mut since_sync, mut synced_at) = (0u64, Instant::now());
            loop {
                let record = match drain.recv_spanned_timeout(policy.every) {
                    Ok(Some(record)) => record,
                    // Idle for a sync interval: what was appended is synced
                    // now, not at the next record (Phase F gate L6).
                    Ok(None) => {
                        if since_sync > 0 {
                            writer.sync().map_err(fail)?;
                            metrics.synced(writer.last_sequence());
                            since_sync = 0;
                            synced_at = Instant::now();
                        }
                        continue;
                    }
                    Err(JournalError::QueueDisconnected) => break,
                    Err(e) => return Err(fail(e)),
                };
                let lost = if record.kind == RecordKind::GapSpan {
                    record
                        .payload
                        .as_ref()
                        .try_into()
                        .map(u64::from_le_bytes)
                        .unwrap_or(0)
                } else {
                    0
                };
                if let Some(s) = writer.append(&record).map_err(fail)? {
                    report(s, &mut done);
                    since_sync = 0;
                    synced_at = Instant::now();
                }
                metrics.appended((HEADER_LEN + record.payload.len()) as u64, lost);
                since_sync += 1;
                if since_sync >= policy.every_records || synced_at.elapsed() >= policy.every {
                    writer.sync().map_err(fail)?;
                    metrics.synced(writer.last_sequence());
                    since_sync = 0;
                    synced_at = Instant::now();
                }
            }
            let last_sequence = writer.last_sequence();
            if let Some(s) = writer.finish().map_err(fail)? {
                report(s, &mut done);
            }
            metrics.synced(last_sequence);
            Ok(WorkerEnd {
                sealed: done,
                last_sequence,
            })
        })
        .map_err(JournalError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::Recorder;
    use crate::storage::budget::MemoryBudget;
    use crate::storage::segments::{SegmentPolicy, recover};
    use std::sync::mpsc;

    /// The worker writes every record (and a budget loss as a gap) into
    /// sealed segments, reports each seal and leaves a complete run.
    #[test]
    fn the_worker_writes_seals_and_reports_a_complete_run() {
        let dir = std::env::temp_dir().join(format!("bsc-worker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let budget = MemoryBudget::new(1 << 20).unwrap();
        let (mut recorder, drain) =
            Recorder::bounded_spanned_budgeted(64, 1024, Arc::clone(&budget)).unwrap();
        let writer = SegmentWriter::create(
            &dir,
            SegmentPolicy {
                max_segment_bytes: 300,
                max_segment_ns: u64::MAX,
            },
        )
        .unwrap();
        let metrics = Arc::new(StorageMetrics::default());
        let (tx, rx) = mpsc::channel();
        let worker = spawn(
            drain,
            writer,
            SyncPolicy {
                every_records: 3,
                every: Duration::from_secs(60),
            },
            Arc::clone(&metrics),
            Some(tx),
        )
        .unwrap();
        for i in 0..10u64 {
            recorder
                .try_record(RecordKind::Runtime, i, &[i as u8; 50])
                .unwrap();
        }
        drop(recorder);
        let end = worker.join().unwrap().unwrap();
        assert_eq!(end.last_sequence, Some(9));
        let reported: Vec<SealedSegment> = rx.iter().collect();
        assert_eq!(reported.len(), end.sealed.len());
        assert!(end.sealed.len() >= 3);
        let v = metrics.view();
        assert_eq!(
            (v.records, v.durable_sequence, v.failed),
            (10, Some(9), false)
        );
        assert_eq!(v.segments_sealed as usize, end.sealed.len());
        assert!(recover(&dir, 1024).unwrap().complete);
        assert_eq!(budget.in_use(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase F gate L6: an appended record becomes durable within the sync
    /// interval even when no next record comes, and a seal advances the
    /// watermark.
    #[test]
    fn an_idle_writer_syncs_by_time_and_a_seal_is_durable() {
        let dir = std::env::temp_dir().join(format!("bsc-worker-idle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mut recorder, drain) = Recorder::bounded_spanned(8, 64).unwrap();
        let writer = SegmentWriter::create(
            &dir,
            SegmentPolicy {
                max_segment_bytes: u64::MAX,
                max_segment_ns: u64::MAX,
            },
        )
        .unwrap();
        let metrics = Arc::new(StorageMetrics::default());
        let worker = spawn(
            drain,
            writer,
            SyncPolicy {
                every_records: 1_000,
                every: Duration::from_millis(20),
            },
            Arc::clone(&metrics),
            None,
        )
        .unwrap();
        recorder
            .try_record(RecordKind::Runtime, 1, &[1; 8])
            .unwrap();
        let started = Instant::now();
        while metrics.view().durable_sequence != Some(0) {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "never synced while idle"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(recorder);
        worker.join().unwrap().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zero_sync_bound_is_refused() {
        let (_, drain) = Recorder::bounded_spanned(1, 8).unwrap();
        let dir = std::env::temp_dir().join(format!("bsc-worker0-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let writer = SegmentWriter::create(
            &dir,
            SegmentPolicy {
                max_segment_bytes: 1,
                max_segment_ns: 1,
            },
        )
        .unwrap();
        assert!(
            spawn(
                drain,
                writer,
                SyncPolicy {
                    every_records: 0,
                    every: Duration::from_secs(1),
                },
                Arc::new(StorageMetrics::default()),
                None,
            )
            .is_err()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
