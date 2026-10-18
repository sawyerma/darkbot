//! Read-only, non-blocking snapshot tap for the future Project_0 IPC reporter.
//! Deliberately NOT wired into the capture owner until owner review and performance gates.
//! Never call MarketJournalWorker::status() from this module: that method samples rates.
use super::MarketStats;
use crate::storage::writer::StorageStatus;
use std::sync::mpsc::{SyncSender, TrySendError};

/// Captured once by the existing terminal cadence, never by a second sampler.
#[derive(Clone, Debug)]
pub struct CaptureSnapshot {
    pub run_id: String,
    pub observing: bool,
    pub last_observed_slot: Option<u64>,
    pub gap_count: u64,
    pub reconnect_count: u64,
    pub liquidation_events: u64,
    pub storage: Option<StorageStatus>,
}

impl CaptureSnapshot {
    pub fn from_existing_sample(
        run_id: &str,
        observing: bool,
        stats: &MarketStats,
        storage: Option<StorageStatus>,
    ) -> Self {
        Self {
            run_id: run_id.to_owned(),
            observing,
            last_observed_slot: stats.last_observed_slot,
            gap_count: stats.gap_count,
            reconnect_count: stats.reconnect_count,
            liquidation_events: stats.liquidation_events,
            storage,
        }
    }
}

/// Optional one-way tap. A disconnected or saturated reporter never delays capture.
/// The receiving IPC worker must live on a separate thread and handle serialization.
#[derive(Clone)]
pub struct ReporterTap {
    tx: SyncSender<CaptureSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferResult {
    Accepted,
    DroppedBackpressure,
    ReporterGone,
}

impl ReporterTap {
    pub fn new(tx: SyncSender<CaptureSnapshot>) -> Self {
        Self { tx }
    }

    pub fn offer(&self, snapshot: CaptureSnapshot) -> OfferResult {
        match self.tx.try_send(snapshot) {
            Ok(()) => OfferResult::Accepted,
            Err(TrySendError::Full(_)) => OfferResult::DroppedBackpressure,
            Err(TrySendError::Disconnected(_)) => OfferResult::ReporterGone,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn snapshot_uses_existing_stats_without_resampling_storage() {
        let stats = MarketStats {
            last_observed_slot: Some(321),
            gap_count: 2,
            reconnect_count: 3,
            liquidation_events: 4,
            ..Default::default()
        };
        let storage = StorageStatus {
            queue_bytes: 2048,
            lag_frames: 7,
            ..Default::default()
        };
        let s = CaptureSnapshot::from_existing_sample("test-only", true, &stats, Some(storage));
        assert_eq!(s.last_observed_slot, Some(321));
        assert_eq!(
            (s.gap_count, s.reconnect_count, s.liquidation_events),
            (2, 3, 4)
        );
        assert_eq!(s.storage.unwrap().lag_frames, 7);
    }

    #[test]
    fn backpressure_and_disconnect_are_fail_open_for_observation() {
        let (tx, rx) = sync_channel(1);
        let tap = ReporterTap::new(tx);
        let make = || {
            CaptureSnapshot::from_existing_sample("test-only", false, &MarketStats::default(), None)
        };
        assert_eq!(tap.offer(make()), OfferResult::Accepted);
        assert_eq!(tap.offer(make()), OfferResult::DroppedBackpressure);
        drop(rx);
        assert_eq!(tap.offer(make()), OfferResult::ReporterGone);
    }
}
