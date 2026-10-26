//! `StorageMetrics` (master plan §11, storage contract): what one capture's
//! writer did, readable by the status owner without a lock: records and
//! bytes appended, segments sealed, the durability watermark (the last
//! sequence known to be on disk), losses and write errors. The byte budget
//! publishes its own in-use, peak and refusals.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Shared counters of one capture writer.
#[derive(Debug, Default)]
pub struct StorageMetrics {
    records: AtomicU64,
    bytes: AtomicU64,
    lost_records: AtomicU64,
    segments_sealed: AtomicU64,
    syncs: AtomicU64,
    /// One more than the last durable sequence (0: none yet).
    durable_next: AtomicU64,
    write_errors: AtomicU64,
    failed: AtomicBool,
}

/// A consistent-enough reading of [`StorageMetrics`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MetricsView {
    pub records: u64,
    pub bytes: u64,
    pub lost_records: u64,
    pub segments_sealed: u64,
    pub syncs: u64,
    /// The last sequence on disk after a sync, if any.
    pub durable_sequence: Option<u64>,
    pub write_errors: u64,
    pub failed: bool,
}

impl StorageMetrics {
    /// One record of `frame_bytes` appended; `lost` of it are a gap span.
    pub fn appended(&self, frame_bytes: u64, lost: u64) {
        self.records.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(frame_bytes, Ordering::Relaxed);
        self.lost_records.fetch_add(lost, Ordering::Relaxed);
    }

    pub fn sealed(&self) {
        self.segments_sealed.fetch_add(1, Ordering::Relaxed);
    }

    /// Everything up to `last_sequence` is on disk.
    pub fn synced(&self, last_sequence: Option<u64>) {
        self.syncs.fetch_add(1, Ordering::Relaxed);
        if let Some(last) = last_sequence {
            self.durable_next
                .fetch_max(last.saturating_add(1), Ordering::AcqRel);
        }
    }

    /// A write, sync or seal failed: the capture is no longer complete.
    pub fn write_failed(&self) {
        self.write_errors.fetch_add(1, Ordering::Relaxed);
        self.failed.store(true, Ordering::Release);
    }

    pub fn view(&self) -> MetricsView {
        let durable = self.durable_next.load(Ordering::Acquire);
        MetricsView {
            records: self.records.load(Ordering::Acquire),
            bytes: self.bytes.load(Ordering::Acquire),
            lost_records: self.lost_records.load(Ordering::Acquire),
            segments_sealed: self.segments_sealed.load(Ordering::Acquire),
            syncs: self.syncs.load(Ordering::Acquire),
            durable_sequence: durable.checked_sub(1),
            write_errors: self.write_errors.load(Ordering::Acquire),
            failed: self.failed.load(Ordering::Acquire),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watermark_only_moves_forward_and_a_failure_sticks() {
        let m = StorageMetrics::default();
        assert_eq!(m.view().durable_sequence, None);
        m.appended(40, 0);
        m.appended(44, 3);
        m.synced(Some(4));
        m.synced(Some(2));
        m.sealed();
        let v = m.view();
        assert_eq!(
            (
                v.records,
                v.bytes,
                v.lost_records,
                v.segments_sealed,
                v.syncs
            ),
            (2, 84, 3, 1, 2)
        );
        assert_eq!(v.durable_sequence, Some(4));
        assert!(!v.failed);
        m.write_failed();
        assert!(m.view().failed && m.view().write_errors == 1);
    }
}
