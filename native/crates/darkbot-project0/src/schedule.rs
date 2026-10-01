//! P2-04: Premium-Scheduler — nur Accounts mit Liability in einer Bank mit PREMIUM_ACTIVE.
//! P2-05: Tag-Scheduler — pro getaggtem Account `t_profitable`.
//!
//! Beide sind Zeit-Trigger (b/c) derselben Health-Engine. Sie wecken Detection
//! zur richtigen Chain-Zeit (Unix-Sekunden wie `liquidation_tagged_at`), sie
//! entscheiden nichts selbst. Kein Polling aller Accounts: Min-Heap über
//! `next_check`.
//!
//! Invalidation (Pflicht, P2-06-Liste): Tag gelöscht, Timer-Reset (25 %-Regel),
//! PREMIUM_ACTIVE-Flag-Wechsel, Account-Update → `remove`/`upsert`.

use solana_sdk::pubkey::Pubkey;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Trigger {
    /// Premium-Akkumulation kann Health unter 0 drücken (b).
    PremiumBoundary,
    /// Tagging-Prämie erreicht die Profit-Schwelle (c).
    TagProfitable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Due {
    pub account: Pubkey,
    pub trigger: Trigger,
    pub at_unix_s: u64,
}

#[derive(Default, Debug)]
pub struct TimeScheduler {
    heap: BinaryHeap<Reverse<(u64, u64)>>, // (at, seq) → Min-Heap
    by_seq: HashMap<u64, Due>,
    current: HashMap<(Pubkey, Trigger), u64>, // → seq (für Invalidation)
    next_seq: u64,
}

impl TimeScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Setzt/ersetzt den Weckzeitpunkt. `None` entfernt (z. B. Tag gelöscht, Flag aus).
    pub fn upsert(&mut self, account: Pubkey, trigger: Trigger, at_unix_s: Option<u64>) {
        if let Some(old) = self.current.remove(&(account, trigger)) {
            self.by_seq.remove(&old);
        }
        if let Some(at) = at_unix_s {
            let seq = self.next_seq;
            self.next_seq += 1;
            self.heap.push(Reverse((at, seq)));
            self.by_seq.insert(seq, Due { account, trigger, at_unix_s: at });
            self.current.insert((account, trigger), seq);
        }
    }

    pub fn remove_account(&mut self, account: &Pubkey) {
        for t in [Trigger::PremiumBoundary, Trigger::TagProfitable] {
            self.upsert(*account, t, None);
        }
    }

    /// Alle fälligen Einträge bis `now`. Verwaiste Heap-Einträge (ersetzt/entfernt)
    /// werden übersprungen.
    pub fn pop_due(&mut self, now_unix_s: u64) -> Vec<Due> {
        let mut out = Vec::new();
        while let Some(Reverse((at, seq))) = self.heap.peek().copied() {
            if at > now_unix_s {
                break;
            }
            self.heap.pop();
            if let Some(d) = self.by_seq.remove(&seq) {
                self.current.remove(&(d.account, d.trigger));
                out.push(d);
            }
        }
        out
    }

    pub fn next_due(&mut self) -> Option<u64> {
        while let Some(Reverse((at, seq))) = self.heap.peek().copied() {
            if self.by_seq.contains_key(&seq) {
                return Some(at);
            }
            self.heap.pop();
        }
        None
    }

    pub fn len(&self) -> usize {
        self.by_seq.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_seq.is_empty()
    }
}

/// Premium-Grenze: frühester Zeitpunkt, an dem Health (Maintenance) 0 erreicht,
/// wenn sich nur die Premium-Schuld ändert (Preise konstant).
///
/// Modell aus Release 0.1.12: debt(t) = principal + premium_outstanding
///                                     + principal × rate × (t − snapshot) / year.
/// Health-Puffer `health_value` (Assets − gewichtete Liabilities, Fixed-Point wie Crate)
/// sinkt pro Sekunde um Σ_i principal_i × rate_i × liab_weight_i / YEAR_S.
/// Rückgabe None, wenn kein Premium wirkt (Flag aus) oder Rate 0 → kein Eintrag.
///
/// Alle Größen Fixed-Point mit gemeinsamer Skalierung `scale` (aus P3), kein f64.
pub fn premium_boundary_unix_s(
    now_unix_s: u64,
    health_value: i128,
    per_second_decay: u128,
) -> Option<u64> {
    if health_value <= 0 {
        return Some(now_unix_s); // schon unter/auf 0: sofort prüfen
    }
    if per_second_decay == 0 {
        return None;
    }
    let secs = (health_value as u128 + per_second_decay - 1) / per_second_decay; // ceil
    Some(now_unix_s.saturating_add(secs.min(u64::MAX as u128) as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_orders_and_invalidates() {
        let mut s = TimeScheduler::new();
        let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
        s.upsert(a, Trigger::TagProfitable, Some(100));
        s.upsert(b, Trigger::PremiumBoundary, Some(50));
        s.upsert(a, Trigger::TagProfitable, Some(70)); // ersetzt 100
        assert_eq!(s.len(), 2);
        assert_eq!(s.next_due(), Some(50));
        let due = s.pop_due(60);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].account, b);
        s.upsert(a, Trigger::TagProfitable, None); // Tag gelöscht
        assert!(s.pop_due(1_000).is_empty());
        assert!(s.is_empty());
    }

    #[test]
    fn premium_boundary_math() {
        assert_eq!(premium_boundary_unix_s(1000, 0, 5), Some(1000));
        assert_eq!(premium_boundary_unix_s(1000, 100, 0), None);
        assert_eq!(premium_boundary_unix_s(1000, 100, 7), Some(1015)); // ceil(100/7)=15
    }
}
