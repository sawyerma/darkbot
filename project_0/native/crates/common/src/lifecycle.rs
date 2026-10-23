//! Lifecycle (S0-05): Shutdown-Signal und RunState.
//!
//! RunState ist fail-closed: aus `Halted` gibt es keinen impliziten Weg zurück
//! nach `Ready`; nur eine explizite Recovery (`reconcile_ok`) darf freigeben.

use crate::channel::Wakeup;
use crate::error::HaltReason;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct Shutdown {
    flag: Arc<AtomicBool>,
    /// A23-01: doorbells rung by `request`, so a sleeping consumer sees the stop at once.
    wakes: Arc<Mutex<Vec<Wakeup>>>,
}

impl Shutdown {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request(&self) {
        self.flag.store(true, Ordering::SeqCst);
        for w in self.wakes.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            w.notify();
        }
    }

    /// A23-01: `request` rings `wake`; rung at once when the stop was already requested.
    pub fn ring_on_request(&self, wake: &Wakeup) {
        self.wakes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(wake.clone());
        if self.is_requested() {
            wake.notify();
        }
    }

    #[inline]
    pub fn is_requested(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    /// Snapshot/Stream werden aufgebaut; kein Trading.
    Bootstrapping,
    /// Reconciliation läuft; kein Trading.
    Reconciling,
    /// State kanonisch bestätigt; Execution-Klasse darf freigegeben werden.
    Ready,
    /// Fail closed. Kein Trading bis explizite Recovery.
    Halted(HaltReason),
}

impl RunState {
    #[inline]
    pub fn is_tradable(&self) -> bool {
        matches!(self, RunState::Ready)
    }

    /// Jeder Zustand kann halten; ein bereits gehaltener Zustand behält den
    /// **ersten** Grund (Provenienz), weitere Gründe werden nicht überschrieben.
    pub fn halt(self, reason: HaltReason) -> RunState {
        match self {
            RunState::Halted(first) => RunState::Halted(first),
            _ => RunState::Halted(reason),
        }
    }

    /// Beginn einer Recovery/periodischen Reconciliation: aus jedem Zustand — auch aus Ready,
    /// damit ein laufender Snapshot-Austausch nie auf einem tradable State passiert.
    pub fn begin_reconcile(self) -> RunState {
        match self {
            RunState::Halted(_)
            | RunState::Bootstrapping
            | RunState::Ready
            | RunState::Reconciling => RunState::Reconciling,
        }
    }

    /// Erfolgreiche Reconciliation gibt frei; aus jedem anderen Zustand ist
    /// `Ready` nicht erreichbar (kein stilles Weiterlaufen).
    pub fn reconcile_ok(self) -> RunState {
        match self {
            RunState::Reconciling => RunState::Ready,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_flag() {
        let s = Shutdown::new();
        let s2 = s.clone();
        assert!(!s.is_requested());
        s2.request();
        assert!(s.is_requested());
    }

    /// A23-01: a request rings every registered doorbell (also through clones); one registered
    /// after the request is rung at once.
    #[test]
    fn shutdown_rings_registered_wakeups() {
        let s = Shutdown::new();
        let (a, b) = (Wakeup::default(), Wakeup::default());
        s.ring_on_request(&a);
        s.clone().ring_on_request(&b);
        let (ta, tb) = (a.token(), b.token());
        s.clone().request();
        assert!(a.token() > ta && b.token() > tb);
        let late = Wakeup::default();
        let t = late.token();
        s.ring_on_request(&late);
        assert!(late.token() > t);
    }

    #[test]
    fn halted_keeps_first_reason_and_needs_explicit_recovery() {
        let st = RunState::Ready;
        let st = st.halt(HaltReason::FeedGap);
        let st = st.halt(HaltReason::ProgramUpgrade);
        assert_eq!(st, RunState::Halted(HaltReason::FeedGap));
        assert!(!st.is_tradable());
        // reconcile_ok aus Halted darf NICHT freigeben
        assert_eq!(st.reconcile_ok(), st);
        let st = st.begin_reconcile();
        assert_eq!(st, RunState::Reconciling);
        assert!(!st.is_tradable());
        let st = st.reconcile_ok();
        assert!(st.is_tradable());
    }

    #[test]
    fn bootstrapping_is_not_tradable() {
        assert!(!RunState::Bootstrapping.is_tradable());
        assert_eq!(
            RunState::Bootstrapping.reconcile_ok(),
            RunState::Bootstrapping
        );
    }
}
