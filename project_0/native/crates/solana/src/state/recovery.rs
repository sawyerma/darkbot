//! S1-07 / S2-04: Vollständiger Recovery-Loop.
//!
//! ```text
//! Gap/Reconnected → HALT(FeedGap)
//!   → Snapshot A holen (Stream läuft weiter und puffert)
//!   → install_snapshot(A)   [buffered Deltas > per-Account-Slot angewendet]
//!   → Snapshot B holen (Reconcile-Referenz)
//!   → reconcile(B)          [Mismatch → HALT(ReconcileFailure), kein Weiterlaufen]
//!   → confirm_ready()       [nur wenn sauber]
//! ```
//! **Barrier:** zwischen Snapshot-Install, Reconcile-Snapshot und Freigabe wird der äußere
//! Ingress-Channel über `drain` in den State gezogen; Deltas, die während des RPC-Fetches
//! eintrafen, sind damit vor `confirm_ready` eingeflossen.
//! **Dedupe/Barriere:** `Gap` hält nur an (Stream ist unten); `Reconnected` ist StreamReady
//! ([FIX-36]: erst nach allen Subscription-ACKs emittiert) und startet genau eine Recovery.
//! Der Snapshot-Fetch ist injiziert (`SnapshotFn`), damit der Loop offline testbar ist
//! und der Runner RPC/Provider außerhalb der Businesslogik hält.
//! Begrenzte Versuche: nach `max_attempts` bleibt der State HALTED und der Runner
//! muss eskalieren (Alarm), nicht endlos pollen.

use super::{AccountUpdate, StateCore};
use crate::ingress::IngressEvent;
use darkbot_common::error::HaltReason;
use darkbot_common::lifecycle::RunState;
use std::time::{Duration, Instant};

pub struct SnapshotData {
    pub slot: u64,
    pub accounts: Vec<AccountUpdate>,
    /// [FIX-37] Required Dependencies, die der Snapshot nicht liefern konnte → Recovery nicht sauber.
    pub missing_required: Vec<(solana_sdk::pubkey::Pubkey, u64)>,
}

pub type SnapshotFn<'a> = dyn FnMut() -> Result<SnapshotData, String> + 'a;
/// Zieht alle aktuell im Ingress-Channel liegenden Events in den State (ohne zu blockieren).
pub type DrainFn<'a> = dyn FnMut(&mut StateCore) + 'a;

fn safe_recovery_error(s: &str) -> String {
    // Provider transport errors may embed credential-bearing URLs. Keep the useful prefix only.
    let no_url = s.split(" for url ").next().unwrap_or(s);
    if let Some((head, _)) = no_url.split_once("?api-key=") {
        format!("{head}?api-key=<redacted>")
    } else {
        no_url.to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryOutcome {
    Ready,
    /// Owner stopped or measurement aborted; never promotes a partial recovery.
    Cancelled,
    /// Nach allen Versuchen nicht sauber; State bleibt HALTED.
    Exhausted { attempts: u32 },
}

// A root-only progress watchdog. Time values are elapsed from a single Instant,
// so decision logic can be unit-tested without wall-clock sleeps.
#[derive(Clone, Copy, Debug)]
struct RootProgressLimits {
    stall: Duration,
    hard: Duration,
}

#[derive(Debug)]
struct RootProgressWatchdog {
    last_root: Option<u64>,
    last_progress_elapsed: Duration,
}

impl RootProgressWatchdog {
    fn new(initial_root: Option<u64>) -> Self {
        Self { last_root: initial_root, last_progress_elapsed: Duration::ZERO }
    }

    fn observe_root(&mut self, root: Option<u64>, elapsed: Duration) {
        // Only a strictly increasing finalized root counts. Repeated notifications,
        // processed slots and account updates do NOT extend the stall deadline.
        if root > self.last_root {
            self.last_root = root;
            self.last_progress_elapsed = elapsed;
        }
    }

    fn timeout_reason(
        &self,
        elapsed: Duration,
        soft: Duration,
        adaptive: Option<RootProgressLimits>,
    ) -> Option<&'static str> {
        match adaptive {
            Some(limits) if elapsed >= limits.hard => Some("hard"),
            Some(limits) if elapsed >= soft
                && elapsed.saturating_sub(self.last_progress_elapsed) >= limits.stall => Some("stall"),
            Some(_) => None,
            None if elapsed >= soft => Some("fixed"),
            None => None,
        }
    }
}

pub struct RecoveryDriver {
    pub max_attempts: u32,
    pub attempts_total: u64,
    pub reconcile_failures: u64,
    /// [FIX-96] Optional production barrier: before reconciling Snapshot B, wait until
    /// the WSS root has reached every per-account B context slot, while continuously
    /// draining ingress. Disabled by default so offline unit tests stay deterministic.
    root_catchup_timeout: Option<Duration>,
    // Production-only bounded adaptive extension; legacy fixed-timeout calls unchanged.
    root_progress_limits: Option<RootProgressLimits>,
    // Progress lines, in memory only: the caller (the Shadow owner) forwards them through its
    // non-blocking evidence writer, so recovery never writes stderr on the owner thread.
    diagnostics: Vec<String>,
    diagnostics_beyond: u64,
}

const DIAGNOSTICS_MAX: usize = 256;

impl RecoveryDriver {
    pub fn new(max_attempts: u32) -> Self {
        RecoveryDriver {
            max_attempts,
            attempts_total: 0,
            reconcile_failures: 0,
            root_catchup_timeout: None,
            root_progress_limits: None,
            diagnostics: Vec::new(),
            diagnostics_beyond: 0,
        }
    }

    fn note(&mut self, line: String) {
        if self.diagnostics.len() < DIAGNOSTICS_MAX {
            self.diagnostics.push(line);
        } else {
            self.diagnostics_beyond += 1;
        }
    }

    /// Progress lines since the last call, and how many exceeded the bound (counted, not lost
    /// silently).
    pub fn take_diagnostics(&mut self) -> (Vec<String>, u64) {
        (
            std::mem::take(&mut self.diagnostics),
            std::mem::take(&mut self.diagnostics_beyond),
        )
    }

    pub fn with_root_catchup(mut self, timeout: Duration) -> Self {
        self.root_catchup_timeout = Some(timeout);
        self
    }

    /// Extend a configured root-catchup soft timeout only while real root progress
    /// continues, and never past the mandatory hard maximum. Runner validates values.
    pub fn with_root_progress_limits(mut self, stall: Duration, hard: Duration) -> Self {
        assert!(self.root_catchup_timeout.is_some());
        assert!(!stall.is_zero() && hard >= self.root_catchup_timeout.unwrap());
        self.root_progress_limits = Some(RootProgressLimits { stall, hard });
        self
    }

    /// Verarbeitet ein Ingress-Event und startet bei Gap/Reconnect/Fatal die Recovery.
    /// Gibt `Some(outcome)` zurück, wenn eine Recovery gelaufen ist.
    pub fn on_event(&mut self, st: &mut StateCore, ev: IngressEvent, fetch: &mut SnapshotFn<'_>, drain: &mut DrainFn<'_>) -> Option<RecoveryOutcome> {
        self.on_event_cancellable(st, ev, fetch, drain, &|| false)
    }

    pub fn on_event_cancellable(&mut self, st: &mut StateCore, ev: IngressEvent,
        fetch: &mut SnapshotFn<'_>, drain: &mut DrainFn<'_>, cancelled: &dyn Fn() -> bool,
    ) -> Option<RecoveryOutcome> {
        let trigger = matches!(ev, IngressEvent::Reconnected { .. });
        st.on_event(ev);
        if trigger { Some(self.recover_cancellable(st, fetch, drain, cancelled)) } else { None }
    }

    /// Bootstrap und Recovery sind derselbe Pfad.
    pub fn recover(&mut self, st: &mut StateCore, fetch: &mut SnapshotFn<'_>, drain: &mut DrainFn<'_>) -> RecoveryOutcome {
        self.recover_cancellable(st, fetch, drain, &|| false)
    }

    pub fn recover_cancellable(&mut self, st: &mut StateCore, fetch: &mut SnapshotFn<'_>,
        drain: &mut DrainFn<'_>, cancelled: &dyn Fn() -> bool,
    ) -> RecoveryOutcome {
        macro_rules! check_cancelled {
            () => { if cancelled() {
                st.halt(HaltReason::FeedGap);
                return RecoveryOutcome::Cancelled;
            } };
        }
        'attempts: for attempt in 1..=self.max_attempts {
            check_cancelled!();
            self.attempts_total += 1;
            self.note(format!("[recovery] attempt={}/{} snapshot=A start", attempt, self.max_attempts));
            let a = match fetch() {
                Ok(s) => s,
                Err(e) => {
                    self.note(format!(
                        "[recovery] attempt={}/{} snapshot=A error={}",
                        attempt,
                        self.max_attempts,
                        safe_recovery_error(&e)
                    ));
                    continue;
                }
            };
            self.note(format!(
                "[recovery] attempt={}/{} snapshot=A ok slot={} accounts={} missing_required={}",
                attempt,
                self.max_attempts,
                a.slot,
                a.accounts.len(),
                a.missing_required.len()
            ));
            if !a.missing_required.is_empty() {
                // [FIX-37] fehlende required Accounts: kein READY, nächster Versuch.
                st.halt(HaltReason::ReconcileFailure);
                self.reconcile_failures += 1;
                continue;
            }
            check_cancelled!();
            st.install_snapshot(a.slot, a.accounts);
            drain(st); // Deltas, die während Fetch A ankamen
            self.note(format!("[recovery] attempt={}/{} snapshot=B start", attempt, self.max_attempts));
            let b = match fetch() {
                Ok(s) => s,
                Err(e) => {
                    self.note(format!(
                        "[recovery] attempt={}/{} snapshot=B error={}",
                        attempt,
                        self.max_attempts,
                        safe_recovery_error(&e)
                    ));
                    st.halt(HaltReason::ReconcileFailure);
                    continue;
                }
            };
            self.note(format!(
                "[recovery] attempt={}/{} snapshot=B ok slot={} accounts={} missing_required={}",
                attempt,
                self.max_attempts,
                b.slot,
                b.accounts.len(),
                b.missing_required.len()
            ));
            check_cancelled!();
            drain(st); // Deltas, die während Fetch B ankamen (vor dem Vergleich)
            if !b.missing_required.is_empty() {
                // [FIX-41] Snapshot B ohne required Dependency → kein Vergleich möglich → nicht sauber.
                st.halt(HaltReason::ReconcileFailure);
                self.reconcile_failures += 1;
                continue;
            }

            // [FIX-96] Snapshot B can be ahead of account notifications still queued/being
            // delivered by the same confirmed WSS stream. Comparing immediately creates false
            // mismatches. Root is the conservative barrier: once root >= every B account context
            // slot, all B state is finalized; keep draining ingress until that point, then compare.
            if let Some(timeout) = self.root_catchup_timeout {
                let target_slot = b.accounts.iter().map(|a| a.slot).max().unwrap_or(b.slot);
                let started = Instant::now();
                let mut progress = RootProgressWatchdog::new(st.root());
                let mut extension_logged = false;
                self.note(format!(
                    "[recovery] attempt={}/{} root_catchup start target={} current={:?} soft_ms={} stall_ms={:?} hard_ms={:?}",
                    attempt, self.max_attempts, target_slot, st.root(), timeout.as_millis(),
                    self.root_progress_limits.map(|p| p.stall.as_millis()),
                    self.root_progress_limits.map(|p| p.hard.as_millis())
                ));
                loop {
                    check_cancelled!();
                    drain(st);
                    let elapsed = started.elapsed();
                    let root = st.root();
                    progress.observe_root(root, elapsed);
                    let reason = progress.timeout_reason(elapsed, timeout, self.root_progress_limits);
                    // The hard limit takes precedence even if a blocked drain returned
                    // with a new root. Never promote an expired recovery attempt.
                    if reason == Some("hard") {
                        self.note(format!(
                            "[recovery] attempt={}/{} root_catchup timeout reason=hard target={} current={:?} elapsed_ms={} stalled_ms={}",
                            attempt, self.max_attempts, target_slot, root,
                            elapsed.as_millis(), elapsed.saturating_sub(progress.last_progress_elapsed).as_millis()
                        ));
                        st.halt(HaltReason::ReconcileFailure);
                        self.reconcile_failures += 1;
                        continue 'attempts;
                    }
                    if root.is_some_and(|r| r >= target_slot) {
                        // Flush all already-enqueued notifications before reconciliation;
                        // a gap during drain must still leave StateCore HALTED.
                        drain(st);
                        self.note(format!(
                            "[recovery] attempt={}/{} root_catchup ok target={} root={} elapsed_ms={}",
                            attempt, self.max_attempts, target_slot,
                            st.root().unwrap_or(0), started.elapsed().as_millis()
                        ));
                        break;
                    }
                    if let Some(reason) = reason {
                        self.note(format!(
                            "[recovery] attempt={}/{} root_catchup timeout reason={} target={} current={:?} elapsed_ms={} stalled_ms={}",
                            attempt, self.max_attempts, reason, target_slot, root,
                            elapsed.as_millis(), elapsed.saturating_sub(progress.last_progress_elapsed).as_millis()
                        ));
                        st.halt(HaltReason::ReconcileFailure);
                        self.reconcile_failures += 1;
                        continue 'attempts;
                    }
                    if self.root_progress_limits.is_some() && elapsed >= timeout && !extension_logged {
                        extension_logged = true;
                        self.note(format!(
                            "[recovery] attempt={}/{} root_catchup extending target={} current={:?} elapsed_ms={}",
                            attempt, self.max_attempts, target_slot, root, elapsed.as_millis()
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }

            check_cancelled!();
            let rep = st.reconcile(b.slot, &b.accounts);
            if rep.is_clean() {
                self.note(format!(
                    "[recovery] attempt={}/{} reconcile=clean snapshot_slot={}",
                    attempt,
                    self.max_attempts,
                    b.slot
                ));
                if st.confirm_ready().is_tradable() {
                    return RecoveryOutcome::Ready;
                }
            } else {
                self.note(format!(
                    "[recovery] attempt={}/{} reconcile=dirty report={:?}",
                    attempt,
                    self.max_attempts,
                    rep
                ));
                self.reconcile_failures += 1;
            }
            // Reconcile schmutzig: run_state ist Halted(ReconcileFailure); nächster Versuch
            // beginnt mit frischem Snapshot (begin_reconcile aus Halted erlaubt).
            let _ = attempt;
        }
        debug_assert!(!st.run_state().is_tradable());
        RecoveryOutcome::Exhausted { attempts: self.max_attempts }
    }
}

pub fn is_halted(st: &StateCore) -> bool {
    matches!(st.run_state(), RunState::Halted(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingress::{AccountUpdate, SourceKind};
    use solana_sdk::pubkey::Pubkey;

    fn upd(pk: Pubkey, slot: u64, data: u8) -> AccountUpdate {
        AccountUpdate { pubkey: pk, owner: Pubkey::new_from_array([0xA5; 32]), lamports: 1, data: vec![data], slot, write_version: None, source: SourceKind::Snapshot, t_rx_ns: 0 } // [FIX-57] stable owner across A/B
    }

    #[test]
    fn gap_to_ready_full_loop() {
        let pk = Pubkey::new_unique();
        let mut st = StateCore::new(100);
        let mut drv = RecoveryDriver::new(3);
        let mut calls = 0;
        let mut fetch = move || {
            calls += 1;
            Ok(SnapshotData { slot: 100 + calls, accounts: vec![upd(pk, 100 + calls, 7)], missing_required: vec![] })
        };
        let mut drained = 0;
        let mut drain = |_st: &mut StateCore| { drained += 1; };
        // Bootstrap
        assert_eq!(drv.recover(&mut st, &mut fetch, &mut drain), RecoveryOutcome::Ready);
        assert!(st.run_state().is_tradable());
        // Gap hält nur an — keine Recovery (Stream ist unten)
        let out = drv.on_event(&mut st, IngressEvent::Gap { source: SourceKind::WsRpcControl, from_slot: Some(101), to_slot: None, t_rx_ns: 0 }, &mut fetch, &mut drain);
        assert_eq!(out, None);
        assert!(!st.run_state().is_tradable());
        // Reconnected → genau eine Recovery
        let out = drv.on_event(&mut st, IngressEvent::Reconnected { source: SourceKind::WsRpcControl, attempt: 1, t_rx_ns: 0 }, &mut fetch, &mut drain);
        assert_eq!(out, Some(RecoveryOutcome::Ready));
        assert!(st.run_state().is_tradable());
        assert_eq!(drv.attempts_total, 2);
        assert_eq!(drained, 4, "Barrier: zweimal pro Recovery gedraint");
    }

    #[test]
    fn missing_required_dependency_blocks_ready() {
        // [FIX-37]
        let pk = Pubkey::new_unique();
        let mut st = StateCore::new(10);
        let mut drv = RecoveryDriver::new(1);
        let mut fetch = move || Ok(SnapshotData { slot: 10, accounts: vec![upd(pk, 10, 1)], missing_required: vec![(Pubkey::new_unique(), 10)] });
        let mut drain = |_st: &mut StateCore| {};
        assert_eq!(drv.recover(&mut st, &mut fetch, &mut drain), RecoveryOutcome::Exhausted { attempts: 1 });
        assert!(!st.run_state().is_tradable());
    }

    #[test]
    fn snapshot_b_missing_required_blocks_ready() {
        // [FIX-41] A vollständig, B fehlt ein required Key → nie READY.
        let pk = Pubkey::new_unique();
        let mut st = StateCore::new(10);
        let mut drv = RecoveryDriver::new(2);
        let mut n = 0u8;
        let mut fetch = move || {
            n += 1;
            let missing = if n % 2 == 0 { vec![(Pubkey::new_unique(), 10)] } else { vec![] };
            Ok(SnapshotData { slot: 10, accounts: vec![upd(pk, 10, 1)], missing_required: missing })
        };
        let mut drain = |_st: &mut StateCore| {};
        assert_eq!(drv.recover(&mut st, &mut fetch, &mut drain), RecoveryOutcome::Exhausted { attempts: 2 });
        assert!(!st.run_state().is_tradable());
        assert_eq!(drv.reconcile_failures, 2);
    }

    #[test]
    fn dirty_reconcile_stays_halted() {
        let pk = Pubkey::new_unique();
        let mut st = StateCore::new(100);
        let mut drv = RecoveryDriver::new(2);
        let mut n = 0u8;
        let mut fetch = move || {
            n += 1;
            Ok(SnapshotData { slot: 50, accounts: vec![upd(pk, 50, n)], missing_required: vec![] }) // jedes Mal andere Daten → nie sauber
        };
        let mut drain = |_st: &mut StateCore| {};
        assert_eq!(drv.recover(&mut st, &mut fetch, &mut drain), RecoveryOutcome::Exhausted { attempts: 2 });
        assert!(is_halted(&st));
        assert_eq!(drv.reconcile_failures, 2);
    }
    #[test]
    fn adaptive_root_progress_extends_soft_budget_but_stalls_and_hard_caps() {
        let soft = Duration::from_secs(120);
        let limits = Some(RootProgressLimits {
            stall: Duration::from_secs(60), hard: Duration::from_secs(300),
        });
        let mut w = RootProgressWatchdog::new(Some(455385463));
        w.observe_root(Some(455385799), Duration::from_secs(119));
        w.observe_root(Some(455385799), Duration::from_secs(151));
        assert_eq!(w.last_progress_elapsed, Duration::from_secs(119));
        assert_eq!(w.timeout_reason(Duration::from_secs(120), soft, limits), None);
        assert_eq!(w.timeout_reason(Duration::from_secs(178), soft, limits), None);
        assert_eq!(w.timeout_reason(Duration::from_secs(179), soft, limits), Some("stall"));
        w.observe_root(Some(455385800), Duration::from_secs(178));
        assert_eq!(w.timeout_reason(Duration::from_secs(179), soft, limits), None);
        assert_eq!(w.timeout_reason(Duration::from_secs(300), soft, limits), Some("hard"));
    }

    #[test]
    fn legacy_fixed_budget_and_adaptive_soft_boundary_are_fail_closed() {
        let soft = Duration::from_secs(120);
        let w = RootProgressWatchdog::new(None);
        assert_eq!(w.timeout_reason(Duration::from_secs(119), soft, None), None);
        assert_eq!(w.timeout_reason(Duration::from_secs(120), soft, None), Some("fixed"));
        let limits = Some(RootProgressLimits {
            stall: Duration::from_secs(60), hard: Duration::from_secs(300),
        });
        assert_eq!(w.timeout_reason(Duration::from_secs(119), soft, limits), None);
        assert_eq!(w.timeout_reason(Duration::from_secs(120), soft, limits), Some("stall"));
    }

    #[test]
    fn adaptive_recovery_requires_real_root_before_ready() {
        let pk = Pubkey::new_unique();
        let mut state = StateCore::new(10);
        let mut driver = RecoveryDriver::new(1)
            .with_root_catchup(Duration::from_secs(1))
            .with_root_progress_limits(Duration::from_secs(1), Duration::from_secs(2));
        let mut fetch = || Ok(SnapshotData {
            slot: 10, accounts: vec![upd(pk, 10, 1)], missing_required: vec![],
        });
        let mut drain = |st: &mut StateCore| {
            st.on_event(IngressEvent::Root {
                slot: 10, source: SourceKind::WsRpcControl, t_rx_ns: 0,
            });
        };
        assert_eq!(driver.recover(&mut state, &mut fetch, &mut drain), RecoveryOutcome::Ready);
        assert!(state.run_state().is_tradable());
    }

    #[test]
    fn cancellation_during_root_catchup_never_confirms_ready_or_retries() {
        use std::cell::Cell;
        let stop=Cell::new(false);
        let pk=Pubkey::new_unique();
        let mut state=StateCore::new(10);
        let mut driver=RecoveryDriver::new(3).with_root_catchup(Duration::from_secs(30))
            .with_root_progress_limits(Duration::from_secs(60), Duration::from_secs(300));
        let calls=Cell::new(0);
        let mut fetch=|| {calls.set(calls.get()+1); Ok(SnapshotData{slot:10,accounts:vec![upd(pk,10,1)],missing_required:vec![]})};
        let mut drains=0;
        let mut drain=|_: &mut StateCore| {drains+=1;if drains==3 {stop.set(true);} };
        assert_eq!(driver.recover_cancellable(&mut state,&mut fetch,&mut drain,&||stop.get()),RecoveryOutcome::Cancelled);
        assert_eq!(calls.get(),2);
        assert!(!state.run_state().is_tradable());
        assert_eq!(driver.attempts_total,1);
    }

    #[test]
    fn cancellation_after_snapshot_a_does_not_fetch_b() {
        use std::cell::Cell;
        let stop=Cell::new(false);
        let mut state=StateCore::new(10);
        let mut driver=RecoveryDriver::new(3);
        let calls=Cell::new(0);
        let mut fetch=|| {calls.set(calls.get()+1);stop.set(true);Ok(SnapshotData{slot:10,accounts:vec![],missing_required:vec![]})};
        assert_eq!(driver.recover_cancellable(&mut state,&mut fetch,&mut |_|{},&||stop.get()),RecoveryOutcome::Cancelled);
        assert_eq!(calls.get(),1);
        assert!(!state.run_state().is_tradable());
    }

}
