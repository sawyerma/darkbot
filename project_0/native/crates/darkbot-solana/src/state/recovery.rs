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
    /// Nach allen Versuchen nicht sauber; State bleibt HALTED.
    Exhausted { attempts: u32 },
}

pub struct RecoveryDriver {
    pub max_attempts: u32,
    pub attempts_total: u64,
    pub reconcile_failures: u64,
}

impl RecoveryDriver {
    pub fn new(max_attempts: u32) -> Self {
        RecoveryDriver { max_attempts, attempts_total: 0, reconcile_failures: 0 }
    }

    /// Verarbeitet ein Ingress-Event und startet bei Gap/Reconnect/Fatal die Recovery.
    /// Gibt `Some(outcome)` zurück, wenn eine Recovery gelaufen ist.
    pub fn on_event(&mut self, st: &mut StateCore, ev: IngressEvent, fetch: &mut SnapshotFn<'_>, drain: &mut DrainFn<'_>) -> Option<RecoveryOutcome> {
        let trigger = matches!(ev, IngressEvent::Reconnected { .. });
        st.on_event(ev); // Gap/Fatal → HALT; Reconnected → HALT + Recovery
        if trigger {
            Some(self.recover(st, fetch, drain))
        } else {
            None
        }
    }

    /// Bootstrap und Recovery sind derselbe Pfad.
    pub fn recover(&mut self, st: &mut StateCore, fetch: &mut SnapshotFn<'_>, drain: &mut DrainFn<'_>) -> RecoveryOutcome {
        for attempt in 1..=self.max_attempts {
            self.attempts_total += 1;
            eprintln!("[recovery] attempt={}/{} snapshot=A start", attempt, self.max_attempts);
            let a = match fetch() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "[recovery] attempt={}/{} snapshot=A error={}",
                        attempt,
                        self.max_attempts,
                        safe_recovery_error(&e)
                    );
                    continue;
                }
            };
            eprintln!(
                "[recovery] attempt={}/{} snapshot=A ok slot={} accounts={} missing_required={}",
                attempt,
                self.max_attempts,
                a.slot,
                a.accounts.len(),
                a.missing_required.len()
            );
            if !a.missing_required.is_empty() {
                // [FIX-37] fehlende required Accounts: kein READY, nächster Versuch.
                st.halt(HaltReason::ReconcileFailure);
                self.reconcile_failures += 1;
                continue;
            }
            st.install_snapshot(a.slot, a.accounts);
            drain(st); // Deltas, die während Fetch A ankamen
            eprintln!("[recovery] attempt={}/{} snapshot=B start", attempt, self.max_attempts);
            let b = match fetch() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "[recovery] attempt={}/{} snapshot=B error={}",
                        attempt,
                        self.max_attempts,
                        safe_recovery_error(&e)
                    );
                    st.halt(HaltReason::ReconcileFailure);
                    continue;
                }
            };
            eprintln!(
                "[recovery] attempt={}/{} snapshot=B ok slot={} accounts={} missing_required={}",
                attempt,
                self.max_attempts,
                b.slot,
                b.accounts.len(),
                b.missing_required.len()
            );
            drain(st); // Deltas, die während Fetch B ankamen (vor dem Vergleich)
            if !b.missing_required.is_empty() {
                // [FIX-41] Snapshot B ohne required Dependency → kein Vergleich möglich → nicht sauber.
                st.halt(HaltReason::ReconcileFailure);
                self.reconcile_failures += 1;
                continue;
            }
            let rep = st.reconcile(b.slot, &b.accounts);
            if rep.is_clean() {
                eprintln!(
                    "[recovery] attempt={}/{} reconcile=clean snapshot_slot={}",
                    attempt,
                    self.max_attempts,
                    b.slot
                );
                if st.confirm_ready().is_tradable() {
                    return RecoveryOutcome::Ready;
                }
            } else {
                eprintln!(
                    "[recovery] attempt={}/{} reconcile=dirty report={:?}",
                    attempt,
                    self.max_attempts,
                    rep
                );
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
}
