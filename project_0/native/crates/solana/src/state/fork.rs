//! S2-02: Fork-Tracker. Slot → Parent, Root-Pruning, Erkennung verwaister Slots.
//!
//! Bei Commitment `confirmed` (Stufe 1) sind Updates selten auf verwaisten Forks; bei
//! `processed` (Race) regelmäßig. Der Tracker ist unabhängig vom Commitment aktiv:
//! Ein neuer Root macht alle Slots ≥ altem Root, die **nicht** Vorfahren des neuen Roots
//! sind, zu Orphans. Accounts mit letztem Write auf einem Orphan sind keine Wahrheit.
//! Unbekannte Vorfahren (Lücke in Slot-Updates) werden konservativ als Orphan behandelt.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct ForkTracker {
    parent: HashMap<u64, u64>,
    root: Option<u64>,
    pub orphaned_total: u64,
}

impl ForkTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_slot(&mut self, slot: u64, parent: Option<u64>) {
        if let Some(p) = parent {
            if self.root.is_none_or(|r| slot >= r) {
                self.parent.insert(slot, p);
            }
        }
    }

    pub fn root(&self) -> Option<u64> {
        self.root
    }

    /// Ist `slot` Vorfahre von (oder gleich) `tip` entlang bekannter Parent-Kanten?
    pub fn is_ancestor(&self, slot: u64, tip: u64) -> bool {
        let mut cur = tip;
        loop {
            if cur == slot {
                return true;
            }
            match self.parent.get(&cur) {
                Some(p) if *p < cur => cur = *p,
                _ => return false,
            }
            if cur < slot {
                return false;
            }
        }
    }

    /// Neuer Root. Gibt die verwaisten Slots (alter Root ≤ s < neuer Root, nicht auf dem Root-Pfad) zurück
    /// und prunt alles unter dem neuen Root.
    pub fn on_root(&mut self, new_root: u64) -> Vec<u64> {
        let old = self.root.unwrap_or(0);
        if new_root <= old && self.root.is_some() {
            return Vec::new();
        }
        let mut canonical: HashSet<u64> = HashSet::new();
        let mut cur = new_root;
        canonical.insert(cur);
        while let Some(p) = self.parent.get(&cur) {
            if *p >= cur || *p < old {
                break;
            }
            canonical.insert(*p);
            cur = *p;
        }
        let mut orphaned: Vec<u64> = self
            .parent
            .keys()
            .copied()
            .filter(|s| *s >= old && *s < new_root && !canonical.contains(s))
            .collect();
        orphaned.sort_unstable();
        self.orphaned_total += orphaned.len() as u64;
        self.parent.retain(|s, _| *s >= new_root);
        self.root = Some(new_root);
        orphaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_orphaned_branch() {
        let mut f = ForkTracker::new();
        f.on_root(100);
        f.on_slot(101, Some(100));
        f.on_slot(102, Some(101));
        f.on_slot(103, Some(101)); // Fork: 103 konkurriert mit 102
        f.on_slot(104, Some(103));
        assert!(f.is_ancestor(101, 104));
        assert!(!f.is_ancestor(102, 104));
        let orphans = f.on_root(104);
        assert_eq!(orphans, vec![102]);
        assert_eq!(f.root(), Some(104));
        assert_eq!(f.orphaned_total, 1);
    }

    #[test]
    fn root_backwards_is_ignored() {
        let mut f = ForkTracker::new();
        f.on_root(50);
        assert!(f.on_root(40).is_empty());
        assert_eq!(f.root(), Some(50));
    }
}
