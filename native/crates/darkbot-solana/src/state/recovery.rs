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
//! **Dedupe:** `Gap` hält nur an (Stream ist unten, Recovery wäre sofort wieder lückenhaft);
//! `Reconnected` startet genau eine Recovery, wenn der Stream wieder puffert.
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
}

pub type SnapshotFn<'a> = dyn FnMut() -> Result<SnapshotData, String> + 'a;
/// Zieht alle aktuell im Ingress-Channel liegenden Events in den State (ohne zu blockieren).
pub type DrainFn<'a> = dyn FnMut(&mut StateCore) + 'a;

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
            let a = match fetch() {
                Ok(s) => s,
                Err(_) => continue,
            };
            st.install_snapshot(a.slot, a.accounts);
            drain(st); // Deltas, die während Fetch A ankamen
            let b = match fetch() {
                Ok(s) => s,
                Err(_) => {
                    st.halt(HaltReason::ReconcileFailure);
                    continue;
                }
            };
            drain(st); // Deltas, die während Fetch B ankamen (vor dem Vergleich)
            let rep = st.reconcile(b.slot, &b.accounts);
            if rep.is_clean() {
                if st.confirm_ready().is_tradable() {
                    return RecoveryOutcome::Ready;
                }
            } else {
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
        AccountUpdate { pubkey: pk, owner: Pubkey::new_unique(), lamports: 1, data: vec![data], slot, write_version: None, source: SourceKind::Snapshot, t_rx_ns: 0 }
    }

    #[test]
    fn gap_to_ready_full_loop() {
        let pk = Pubkey::new_unique();
        let mut st = StateCore::new(100);
        let mut drv = RecoveryDriver::new(3);
        let mut calls = 0;
        let mut fetch = move || {
            calls += 1;
            Ok(SnapshotData { slot: 100 + calls, accounts: vec![upd(pk, 100 + calls, 7)] })
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
    fn dirty_reconcile_stays_halted() {
        let pk = Pubkey::new_unique();
        let mut st = StateCore::new(100);
        let mut drv = RecoveryDriver::new(2);
        let mut n = 0u8;
        let mut fetch = move || {
            n += 1;
            Ok(SnapshotData { slot: 50, accounts: vec![upd(pk, 50, n)] }) // jedes Mal andere Daten → nie sauber
        };
        let mut drain = |_st: &mut StateCore| {};
        assert_eq!(drv.recover(&mut st, &mut fetch, &mut drain), RecoveryOutcome::Exhausted { attempts: 2 });
        assert!(is_halted(&st));
        assert_eq!(drv.reconcile_failures, 2);
    }
}
