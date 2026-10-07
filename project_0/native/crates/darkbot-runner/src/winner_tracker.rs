//! [F1] Bounded, deduplicated winner-watch tracking. Owner-side bookkeeping only: no I/O, no RPC.
//!
//! Replaces the previous `Vec<WinnerWatch>` which (a) received a watch for every `Skip`
//! including "not liquidatable", (b) had no per-account dedupe, (c) re-queued `Err` forever and
//! (d) was drained and rebuilt in full on every owner-loop iteration (O(n) per ingress event).
//!
//! Invariants:
//! - at most one active watch per MarginfiAccount; newer evidence is coalesced into it and
//!   gets a new monotonic revision; a completion for an older revision never resolves or removes
//!   the newer one (it is counted as stale and the newer revision is dispatched itself);
//! - hard capacity; a rejected new watch is counted (`overflow`), never silently lost;
//! - bounded lifetime (`ttl_s`) for `Ok(None)` **and** `Err` results, and for watches the worker
//!   never reached;
//! - due selection via a min-heap: O(log n) per dispatched watch, no full scan per loop;
//! - a full worker queue stops the dispatch round after the first refusal and backs the refused
//!   watch off by `full_queue_backoff_s` (no pop/push churn on a saturated queue).
//!
//! Winner evidence never influences business safety or execution decisions.
use crate::record::Decision;
use crate::remote_workers::WinnerWatch;
use crate::winner::Winner;
use solana_sdk::pubkey::Pubkey;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Only candidates that exact Health found liquidatable are winner-relevant. A "not liquidatable"
/// `Skip` never creates a watch.
pub fn is_winner_relevant(health_liquidatable: bool, decision: Decision) -> bool {
    health_liquidatable
        && matches!(decision, Decision::ShadowReady | Decision::ShadowSimFailed | Decision::Skip)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinnerTrackerConfig {
    /// Maximum simultaneously tracked accounts.
    pub capacity: usize,
    /// Lifetime of a watch since its first creation, seconds.
    pub ttl_s: u64,
    /// Delay before the first winner lookup, seconds.
    pub initial_delay_s: u64,
    /// Delay between lookups after `Ok(None)` / `Err`, seconds.
    pub recheck_interval_s: u64,
    /// Backoff for a watch refused by a full worker queue, seconds.
    pub full_queue_backoff_s: u64,
}

impl WinnerTrackerConfig {
    /// PROVISIONAL qualification defaults. `ttl_s`, `initial_delay_s` and `recheck_interval_s`
    /// equal the pre-F1 literals (900 / 60 / 5); `capacity` and `full_queue_backoff_s` are new.
    pub const DEFAULT: Self = Self {
        capacity: 8_192,
        ttl_s: 900,
        initial_delay_s: 60,
        recheck_interval_s: 5,
        full_queue_backoff_s: 1,
    };

    pub fn validate(self) -> Result<Self, String> {
        if self.capacity == 0 {
            return Err("P0_WINNER_WATCH_CAPACITY must be > 0".into());
        }
        if self.ttl_s == 0 {
            return Err("P0_WINNER_WATCH_TTL_S must be > 0".into());
        }
        if self.recheck_interval_s == 0 {
            return Err("P0_WINNER_WATCH_RECHECK_S must be > 0".into());
        }
        if self.full_queue_backoff_s == 0 {
            return Err("P0_WINNER_WATCH_FULL_BACKOFF_S must be > 0".into());
        }
        // Every watch must get at least one lookup before it can expire.
        if self.initial_delay_s >= self.ttl_s {
            return Err(format!(
                "P0_WINNER_WATCH_INITIAL_DELAY_S ({}) must be < P0_WINNER_WATCH_TTL_S ({})",
                self.initial_delay_s, self.ttl_s
            ));
        }
        if self.recheck_interval_s >= self.ttl_s {
            return Err(format!(
                "P0_WINNER_WATCH_RECHECK_S ({}) must be < P0_WINNER_WATCH_TTL_S ({})",
                self.recheck_interval_s, self.ttl_s
            ));
        }
        if self.full_queue_backoff_s >= self.ttl_s {
            return Err(format!(
                "P0_WINNER_WATCH_FULL_BACKOFF_S ({}) must be < P0_WINNER_WATCH_TTL_S ({})",
                self.full_queue_backoff_s, self.ttl_s
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WinnerTrackerStats {
    pub len: usize,
    pub max: usize,
    pub observed: u64,
    pub deduped: u64,
    pub expired: u64,
    pub overflow: u64,
    pub dispatched: u64,
    pub queue_full_backoffs: u64,
    pub results: u64,
    pub stale_results: u64,
    pub errors: u64,
    pub resolved: u64,
}

/// Result of handing one watch to the winner worker queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinnerDispatch {
    Sent,
    /// Queue full. The caller drops its copy; the tracker keeps the watch and backs it off.
    QueueFull,
}

#[derive(Debug)]
pub enum WinnerResolution {
    /// A winner was found; the coalesced watch is returned for race classification.
    Resolved { watch: Box<WinnerWatch>, winner: Winner },
    /// No winner yet; rescheduled.
    Pending,
    /// Lifetime exhausted (`Ok(None)` or `Err`); removed.
    Expired,
    /// Result for an older revision while a newer one is tracked; the newer revision stays and
    /// is due immediately.
    Stale,
    /// Result for an account that is no longer tracked; ignored.
    Untracked,
}

struct Entry {
    watch: WinnerWatch,
    /// Revision currently handed to the worker, if any.
    in_flight: Option<u64>,
    /// Sequence of the single live heap item for this entry; older heap items are stale.
    due_seq: u64,
}

pub struct WinnerTracker {
    cfg: WinnerTrackerConfig,
    entries: HashMap<Pubkey, Entry>,
    due: BinaryHeap<Reverse<(u64, u64, Pubkey)>>,
    seq: u64,
    next_revision: u64,
    stats: WinnerTrackerStats,
}

impl WinnerTracker {
    pub fn new(cfg: WinnerTrackerConfig) -> Self {
        Self {
            cfg,
            entries: HashMap::new(),
            due: BinaryHeap::new(),
            seq: 0,
            next_revision: 0,
            stats: WinnerTrackerStats::default(),
        }
    }

    pub fn config(&self) -> WinnerTrackerConfig {
        self.cfg
    }

    pub fn stats(&self) -> WinnerTrackerStats {
        WinnerTrackerStats { len: self.entries.len(), ..self.stats }
    }

    fn note_len(&mut self) {
        self.stats.len = self.entries.len();
        self.stats.max = self.stats.max.max(self.entries.len());
    }

    fn schedule(&mut self, account: Pubkey, next_check_unix_s: u64) {
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;
        if let Some(e) = self.entries.get_mut(&account) {
            e.watch.next_check_unix_s = next_check_unix_s;
            e.due_seq = seq;
            self.due.push(Reverse((next_check_unix_s, seq, account)));
        }
    }

    /// Earliest readiness wins; a not-ready record never overwrites a ready one.
    fn coalesce(existing: &mut WinnerWatch, newer: WinnerWatch) {
        existing.rx_slot = existing.rx_slot.min(newer.rx_slot);
        existing.detect_slot = existing.detect_slot.min(newer.detect_slot);
        let newer_sign = newer.sign_slot.unwrap_or(u64::MAX);
        let existing_sign = existing.sign_slot.unwrap_or(u64::MAX);
        if newer.shadow_ready && (!existing.shadow_ready || newer_sign < existing_sign) {
            existing.shadow_ready = true;
            existing.sign_slot = newer.sign_slot;
            existing.expected_net = newer.expected_net;
            existing.our_loss = None;
        } else if !existing.shadow_ready {
            existing.sign_slot = newer.sign_slot;
            existing.expected_net = newer.expected_net;
            existing.our_loss = newer.our_loss;
        }
    }

    /// Record winner-relevant evidence. Existing watch → coalesce. New watch over capacity →
    /// `overflow` (qualification FAIL), never a silent drop.
    pub fn observe(&mut self, mut watch: WinnerWatch, now_unix_s: u64) {
        self.stats.observed = self.stats.observed.saturating_add(1);
        self.next_revision = self.next_revision.wrapping_add(1);
        let revision = self.next_revision;
        if let Some(e) = self.entries.get_mut(&watch.account) {
            Self::coalesce(&mut e.watch, watch);
            e.watch.revision = revision;
            self.stats.deduped = self.stats.deduped.saturating_add(1);
            return;
        }
        if self.entries.len() >= self.cfg.capacity {
            self.stats.overflow = self.stats.overflow.saturating_add(1);
            return;
        }
        let account = watch.account;
        watch.revision = revision;
        watch.created_unix_s = now_unix_s;
        watch.next_check_unix_s = now_unix_s.saturating_add(self.cfg.initial_delay_s);
        let next = watch.next_check_unix_s;
        self.entries.insert(account, Entry { watch, in_flight: None, due_seq: 0 });
        self.schedule(account, next);
        self.note_len();
    }

    /// Hand due watches (earliest first, at most `max`) to `send`. Watches past their lifetime
    /// that were never dispatched expire here. The first `QueueFull` ends the round and backs the
    /// refused watch off; a `send` error is returned unchanged and leaves no watch in flight.
    pub fn dispatch_due<E>(
        &mut self,
        now_unix_s: u64,
        max: usize,
        mut send: impl FnMut(WinnerWatch) -> Result<WinnerDispatch, E>,
    ) -> Result<usize, E> {
        let mut sent = 0usize;
        while sent < max {
            let Some(Reverse((next, seq, account))) = self.due.peek().copied() else { break };
            if next > now_unix_s {
                break;
            }
            self.due.pop();
            let watch = {
                let Some(e) = self.entries.get_mut(&account) else { continue };
                if e.in_flight.is_some() || e.due_seq != seq {
                    continue; // stale heap item
                }
                if now_unix_s.saturating_sub(e.watch.created_unix_s) >= self.cfg.ttl_s {
                    self.entries.remove(&account);
                    self.stats.expired = self.stats.expired.saturating_add(1);
                    continue;
                }
                e.in_flight = Some(e.watch.revision);
                e.watch.clone()
            };
            match send(watch) {
                Ok(WinnerDispatch::Sent) => {
                    sent += 1;
                    self.stats.dispatched = self.stats.dispatched.saturating_add(1);
                }
                Ok(WinnerDispatch::QueueFull) => {
                    if let Some(e) = self.entries.get_mut(&account) {
                        e.in_flight = None;
                    }
                    self.stats.queue_full_backoffs = self.stats.queue_full_backoffs.saturating_add(1);
                    self.schedule(account, now_unix_s.saturating_add(self.cfg.full_queue_backoff_s));
                    break;
                }
                Err(err) => {
                    if let Some(e) = self.entries.get_mut(&account) {
                        e.in_flight = None;
                    }
                    self.schedule(account, now_unix_s);
                    self.note_len();
                    return Err(err);
                }
            }
        }
        self.note_len();
        Ok(sent)
    }

    pub fn on_result(
        &mut self,
        dispatched: WinnerWatch,
        result: Result<Option<Winner>, String>,
        now_unix_s: u64,
    ) -> WinnerResolution {
        self.stats.results = self.stats.results.saturating_add(1);
        let account = dispatched.account;
        let (created, current_revision, newer_in_flight) = match self.entries.get_mut(&account) {
            Some(e) => {
                if e.in_flight == Some(dispatched.revision) {
                    e.in_flight = None;
                }
                (e.watch.created_unix_s, e.watch.revision, e.in_flight.is_some())
            }
            None => return WinnerResolution::Untracked,
        };
        if result.is_err() {
            self.stats.errors = self.stats.errors.saturating_add(1);
        }
        if dispatched.revision != current_revision {
            // A newer revision was coalesced while this one was in flight: never resolve or remove
            // it with the older completion. The newer revision is dispatched itself, now.
            self.stats.stale_results = self.stats.stale_results.saturating_add(1);
            if !newer_in_flight {
                self.schedule(account, now_unix_s);
            }
            return WinnerResolution::Stale;
        }
        if let Ok(Some(winner)) = result {
            let watch = self.entries.remove(&account).map(|e| e.watch).unwrap_or(dispatched);
            self.stats.resolved = self.stats.resolved.saturating_add(1);
            self.note_len();
            return WinnerResolution::Resolved {
                watch: Box::new(watch),
                winner,
            };
        }
        if now_unix_s.saturating_sub(created) >= self.cfg.ttl_s {
            self.entries.remove(&account);
            self.stats.expired = self.stats.expired.saturating_add(1);
            self.note_len();
            return WinnerResolution::Expired;
        }
        self.schedule(account, now_unix_s.saturating_add(self.cfg.recheck_interval_s));
        WinnerResolution::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::LossReason;

    use crate::winner::WinnerIdentity;

    fn cfg(capacity: usize) -> WinnerTrackerConfig {
        WinnerTrackerConfig { capacity, ttl_s: 100, initial_delay_s: 10, recheck_interval_s: 5, full_queue_backoff_s: 2 }
    }

    /// Drains due watches with an always-accepting sender.
    fn take_due(t: &mut WinnerTracker, now: u64, max: usize) -> Vec<WinnerWatch> {
        let mut out = Vec::new();
        let r: Result<usize, ()> = t.dispatch_due(now, max, |w| {
            out.push(w);
            Ok(WinnerDispatch::Sent)
        });
        assert_eq!(r, Ok(out.len()));
        out
    }

    fn watch(account: Pubkey, rx_slot: u64, ready: bool, sign_slot: Option<u64>) -> WinnerWatch {
        WinnerWatch {
            revision: 0,
            account,
            rx_slot,
            detect_slot: rx_slot,
            sign_slot,
            expected_net: Some(1),
            shadow_ready: ready,
            our_loss: if ready { None } else { Some(LossReason::Build) },
            created_unix_s: 0,
            next_check_unix_s: 0,
        }
    }

    fn winner(slot: u64) -> Winner {
        Winner {
            slot,
            identity: WinnerIdentity::Unique { signature: "s".into(), receiver: "r".into(), tx_version: "v0".into() },
        }
    }

    #[test]
    fn not_liquidatable_skip_creates_no_watch() {
        assert!(!is_winner_relevant(false, Decision::Skip));
        assert!(!is_winner_relevant(false, Decision::ShadowReady));
        assert!(is_winner_relevant(true, Decision::Skip));
        assert!(is_winner_relevant(true, Decision::ShadowSimFailed));
        assert!(!is_winner_relevant(true, Decision::Unavailable));
    }

    #[test]
    fn one_watch_per_account_and_dedupe() {
        let mut t = WinnerTracker::new(cfg(10));
        let a = Pubkey::new_unique();
        for i in 0..3 {
            t.observe(watch(a, 100 + i, false, None), 0);
        }
        let s = t.stats();
        assert_eq!((s.len, s.deduped, s.overflow), (1, 2, 0));
    }

    #[test]
    fn capacity_overflow_is_counted_not_silent() {
        let mut t = WinnerTracker::new(cfg(2));
        for _ in 0..3 {
            t.observe(watch(Pubkey::new_unique(), 1, false, None), 0);
        }
        let s = t.stats();
        assert_eq!((s.len, s.overflow), (2, 1));
    }

    #[test]
    fn due_after_initial_delay_and_never_dispatched_twice() {
        let mut t = WinnerTracker::new(cfg(10));
        let a = Pubkey::new_unique();
        t.observe(watch(a, 1, false, None), 0);
        assert!(take_due(&mut t, 9, 10).is_empty());
        let due = take_due(&mut t, 10, 10);
        assert_eq!(due.len(), 1);
        // in flight: coalesce, but no second dispatch
        t.observe(watch(a, 2, true, Some(50)), 11);
        assert!(take_due(&mut t, 1_000, 10).is_empty());
    }

    #[test]
    fn ok_none_and_err_reschedule_then_expire() {
        let mut t = WinnerTracker::new(cfg(10));
        let a = Pubkey::new_unique();
        t.observe(watch(a, 1, false, None), 0);
        let w = take_due(&mut t, 10, 1).pop().unwrap();
        assert!(matches!(t.on_result(w, Ok(None), 10), WinnerResolution::Pending));
        let w = take_due(&mut t, 15, 1).pop().unwrap();
        assert!(matches!(t.on_result(w, Err("rpc".into()), 15), WinnerResolution::Pending));
        assert_eq!(t.stats().errors, 1);
        // Err path has a bounded lifetime too.
        let w = take_due(&mut t, 20, 1).pop().unwrap();
        assert!(matches!(t.on_result(w, Err("rpc".into()), 100), WinnerResolution::Expired));
        assert_eq!(t.stats().len, 0);
        assert_eq!(t.stats().expired, 1);
    }

    #[test]
    fn undispatched_watch_expires_without_worker() {
        let mut t = WinnerTracker::new(cfg(10));
        t.observe(watch(Pubkey::new_unique(), 1, false, None), 0);
        assert!(take_due(&mut t, 500, 10).is_empty());
        assert_eq!((t.stats().len, t.stats().expired), (0, 1));
    }

    #[test]
    fn full_worker_queue_stops_round_and_backs_off() {
        let mut t = WinnerTracker::new(cfg(10));
        for _ in 0..3 {
            t.observe(watch(Pubkey::new_unique(), 1, false, None), 0);
        }
        // Saturated queue: exactly one attempt, then the round ends.
        let mut attempts = 0;
        let r: Result<usize, ()> = t.dispatch_due(10, 256, |_| {
            attempts += 1;
            Ok(WinnerDispatch::QueueFull)
        });
        assert_eq!((r, attempts), (Ok(0), 1));
        assert_eq!(t.stats().queue_full_backoffs, 1);
        // Same instant: the refused watch is backed off, the two others are still due.
        assert_eq!(take_due(&mut t, 10, 256).len(), 2);
        // Backoff elapsed: the refused watch is due again; nothing was lost.
        assert!(take_due(&mut t, 11, 256).is_empty());
        assert_eq!(take_due(&mut t, 12, 256).len(), 1);
    }

    #[test]
    fn partial_dispatch_until_first_full() {
        let mut t = WinnerTracker::new(cfg(10));
        for _ in 0..3 {
            t.observe(watch(Pubkey::new_unique(), 1, false, None), 0);
        }
        let mut attempts = 0;
        let r: Result<usize, ()> = t.dispatch_due(10, 256, |_| {
            attempts += 1;
            Ok(if attempts == 1 { WinnerDispatch::Sent } else { WinnerDispatch::QueueFull })
        });
        assert_eq!((r, attempts), (Ok(1), 2));
    }

    #[test]
    fn send_error_propagates_and_leaves_nothing_in_flight() {
        let mut t = WinnerTracker::new(cfg(10));
        t.observe(watch(Pubkey::new_unique(), 1, false, None), 0);
        let r: Result<usize, &str> = t.dispatch_due(10, 256, |_| Err("disconnected"));
        assert_eq!(r, Err("disconnected"));
        assert_eq!(take_due(&mut t, 10, 256).len(), 1);
    }

    #[test]
    fn resolved_uses_coalesced_earliest_readiness() {
        let mut t = WinnerTracker::new(cfg(10));
        let a = Pubkey::new_unique();
        t.observe(watch(a, 120, false, None), 0);
        t.observe(watch(a, 110, true, Some(130)), 1);
        t.observe(watch(a, 140, true, Some(150)), 2); // later readiness must not replace
        t.observe(watch(a, 160, false, None), 3); // not-ready must not overwrite ready
        let w = take_due(&mut t, 10, 1).pop().unwrap();
        match t.on_result(w, Ok(Some(winner(200))), 10) {
            WinnerResolution::Resolved { watch, winner } => {
                assert_eq!(winner.slot, 200);
                assert_eq!(watch.rx_slot, 110);
                assert!(watch.shadow_ready);
                assert_eq!(watch.sign_slot, Some(130));
                assert_eq!(watch.our_loss, None);
            }
            other => panic!("expected Resolved, got {other:?}"),
        }
        assert_eq!(t.stats().len, 0);
    }

    #[test]
    fn older_inflight_completion_cannot_resolve_or_remove_newer_revision() {
        let mut t = WinnerTracker::new(cfg(10));
        let a = Pubkey::new_unique();
        t.observe(watch(a, 100, false, None), 0);
        let r1 = take_due(&mut t, 10, 1).pop().unwrap(); // R1 in flight
        t.observe(watch(a, 101, true, Some(120)), 11); // R2 coalesced while R1 in flight
        assert!(take_due(&mut t, 11, 10).is_empty()); // still single-flight
        // R1 completes with a winner: must not resolve or remove R2.
        assert!(matches!(t.on_result(r1.clone(), Ok(Some(winner(200))), 12), WinnerResolution::Stale));
        assert_eq!((t.stats().len, t.stats().resolved, t.stats().stale_results), (1, 0, 1));
        // R2 is dispatched itself, immediately, with a newer revision and the newer evidence.
        let r2 = take_due(&mut t, 12, 1).pop().expect("newer revision dispatched");
        assert!(r2.revision > r1.revision);
        assert!(r2.shadow_ready);
        match t.on_result(r2, Ok(Some(winner(200))), 13) {
            WinnerResolution::Resolved { watch, .. } => assert_eq!(watch.sign_slot, Some(120)),
            other => panic!("expected Resolved, got {other:?}"),
        }
        assert_eq!(t.stats().len, 0);
    }

    #[test]
    fn stale_none_or_error_never_reschedules_the_older_revision() {
        let mut t = WinnerTracker::new(cfg(10));
        let a = Pubkey::new_unique();
        t.observe(watch(a, 1, false, None), 0);
        let r1 = take_due(&mut t, 10, 1).pop().unwrap();
        t.observe(watch(a, 2, false, None), 11);
        assert!(matches!(t.on_result(r1.clone(), Err("rpc".into()), 12), WinnerResolution::Stale));
        let r2 = take_due(&mut t, 12, 1).pop().unwrap();
        assert!(r2.revision > r1.revision);
        assert_eq!(t.stats().errors, 1);
        // A late duplicate of the stale R1 result is still stale and changes nothing.
        assert!(matches!(t.on_result(r1, Ok(None), 13), WinnerResolution::Stale));
        assert!(take_due(&mut t, 13, 10).is_empty()); // R2 still in flight, no double dispatch
    }

    #[test]
    fn result_for_untracked_account_is_ignored() {
        let mut t = WinnerTracker::new(cfg(10));
        let w = watch(Pubkey::new_unique(), 1, false, None);
        assert!(matches!(t.on_result(w, Ok(None), 0), WinnerResolution::Untracked));
    }

    #[test]
    fn config_validation() {
        assert!(WinnerTrackerConfig::DEFAULT.validate().is_ok());
        assert!(WinnerTrackerConfig { capacity: 0, ..WinnerTrackerConfig::DEFAULT }.validate().is_err());
        assert!(WinnerTrackerConfig { ttl_s: 0, ..WinnerTrackerConfig::DEFAULT }.validate().is_err());
        assert!(WinnerTrackerConfig { recheck_interval_s: 0, ..WinnerTrackerConfig::DEFAULT }.validate().is_err());
        assert!(WinnerTrackerConfig { full_queue_backoff_s: 0, ..WinnerTrackerConfig::DEFAULT }.validate().is_err());
        // A watch must get at least one lookup before it can expire.
        let base = WinnerTrackerConfig::DEFAULT;
        assert!(WinnerTrackerConfig { ttl_s: 30, initial_delay_s: 60, ..base }.validate().is_err());
        assert!(WinnerTrackerConfig { ttl_s: 60, initial_delay_s: 60, ..base }.validate().is_err());
        assert!(WinnerTrackerConfig { ttl_s: 100, recheck_interval_s: 100, initial_delay_s: 10, ..base }.validate().is_err());
        assert!(WinnerTrackerConfig { ttl_s: 100, full_queue_backoff_s: 100, initial_delay_s: 10, ..base }.validate().is_err());
        assert!(WinnerTrackerConfig { ttl_s: 61, initial_delay_s: 60, ..base }.validate().is_ok());
    }
}
