//! Latest-wins, read-only status channel. No I/O, protobuf or socket on producer path.
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use crate::storage::writer::StorageStatus;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarketSnapshot {
    pub observing: bool,
    pub stream_ready: bool,
    pub gap_count: u64,
    pub reconnect_count: u64,
    pub liquidation_events: u64,
    pub last_observed_slot: Option<u64>,
    pub live_verdict: &'static str,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransportSnapshot {
    pub connected: bool,
    pub last_frame_age_ns: u64,
    pub last_slot: Option<u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StorageSnapshot {
    pub queue_bytes: u64,
    pub lag_frames: u64,
}
impl From<&StorageStatus> for StorageSnapshot {
    fn from(s: &StorageStatus) -> Self {
        Self { queue_bytes: s.queue_bytes, lag_frames: s.lag_frames }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusSnapshot {
    pub seq: u64,
    pub market: Option<MarketSnapshot>,
    pub transport: Option<TransportSnapshot>,
    pub storage: Option<StorageSnapshot>,
}
struct Inner { latest: Mutex<StatusSnapshot>, changed: Condvar }
#[derive(Clone)]
pub struct StatusTap(Arc<Inner>);
impl Default for StatusTap { fn default() -> Self { Self::new() } }
impl StatusTap {
    pub fn new() -> Self {
        Self(Arc::new(Inner { latest: Mutex::new(StatusSnapshot::default()), changed: Condvar::new() }))
    }
    fn publish(&self, f: impl FnOnce(&mut StatusSnapshot)) {
        let mut s = self.0.latest.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut s);
        s.seq = s.seq.checked_add(1).expect("status-tap sequence overflow");
        drop(s);
        self.0.changed.notify_one();
    }
    pub fn publish_market(&self, s: MarketSnapshot) { self.publish(|v| v.market = Some(s)); }
    pub fn publish_transport(&self, s: TransportSnapshot) { self.publish(|v| v.transport = Some(s)); }
    pub fn publish_storage(&self, s: StorageSnapshot) { self.publish(|v| v.storage = Some(s)); }
    pub fn reader(&self) -> Reader { Reader { tap: self.clone(), last: 0 } }
}
pub struct Reader { tap: StatusTap, last: u64 }
impl Reader {
    pub fn wait_changed(&mut self, timeout: Duration) -> Option<StatusSnapshot> {
        let inner = &self.tap.0;
        let mut s = inner.latest.lock().unwrap_or_else(|e| e.into_inner());
        if s.seq == self.last {
            let (next, _) = inner.changed.wait_timeout(s, timeout).unwrap_or_else(|e| e.into_inner());
            s = next;
        }
        if s.seq == self.last { return None; }
        self.last = s.seq;
        Some(s.clone())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_only() {
        let tap = StatusTap::new();
        let mut reader = tap.reader();
        for i in 1..=1000 { tap.publish_market(MarketSnapshot { gap_count: i, ..Default::default() }); }
        let value = reader.wait_changed(Duration::from_millis(5)).unwrap();
        assert_eq!(value.seq, 1000);
        assert_eq!(value.market.unwrap().gap_count, 1000);
        assert!(reader.wait_changed(Duration::from_millis(5)).is_none());
    }
    #[test]
    fn optional_storage_is_not_invented() {
        let tap = StatusTap::new();
        let mut reader = tap.reader();
        tap.publish_market(MarketSnapshot::default());
        assert!(reader.wait_changed(Duration::from_millis(5)).unwrap().storage.is_none());
    }
}
