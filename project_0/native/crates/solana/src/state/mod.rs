//! S2: Kanonischer Solana-RAM-State.
//!
//! Regeln (PHASENPLAN §S2):
//! - Bootstrap race-frei: Stream starten → puffern → Snapshot mit Slot S →
//!   installieren → gepufferte Updates > S anwenden → bestätigen → freigeben.
//! - Ordnung: neuerer Slot gewinnt; gleicher Slot: höhere write_version gewinnt;
//!   ohne write_version (WSS) gewinnt das spätere Update im selben Slot (Zähler `same_slot_no_wv`
//!   im Ledger ausweisen — das ist eine bekannte Schwäche der JSON-Quelle).
//! - Fork/Root: Updates unter dem letzten Root werden verworfen (`below_root`).
//!   Processed-Updates sind forkbar; die Provenienz (`SlotStatus`) wird gespeichert.
//! - Reconnect ohne Replay → HALT → Snapshot → Reconcile → explizit freigeben.
//! - Kein Trading auf unreconciled State: `RunState` aus darkbot_common.

pub mod blockhash;
pub mod fork;
pub mod recovery;
pub mod snapshot;

use crate::ingress::{AccountUpdate, IngressEvent, SlotStatus, SourceKind};
use darkbot_common::error::HaltReason;
use darkbot_common::lifecycle::RunState;
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountRecord {
    pub owner: Pubkey,
    pub lamports: u64,
    pub data: Vec<u8>,
    pub slot: u64,
    pub write_version: Option<u64>,
    pub status: SlotStatus,
    pub source: SourceKind,
    pub t_rx_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateCounters {
    pub applied: u64,
    pub stale_slot: u64,
    pub stale_write_version: u64,
    pub same_slot_no_wv: u64,
    pub below_root: u64,
    pub buffered: u64,
    pub buffer_dropped_le_snapshot: u64,
    pub reconcile_mismatch: u64,
    pub reconcile_missing_local: u64,
    pub reconcile_missing_remote: u64,
    pub orphaned_writes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    StaleSlot,
    StaleWriteVersion,
    BelowRoot,
    Buffered,
}

/// Ergebnis einer Reconciliation gegen einen Snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub snapshot_slot: u64,
    pub checked: u64,
    pub mismatch: Vec<Pubkey>,
    pub missing_local: Vec<Pubkey>,
    pub missing_remote: Vec<Pubkey>,
}

impl ReconcileReport {
    pub fn is_clean(&self) -> bool {
        self.mismatch.is_empty() && self.missing_local.is_empty() && self.missing_remote.is_empty()
    }
}

pub struct StateCore {
    accounts: HashMap<Pubkey, AccountRecord>,
    run_state: RunState,
    /// Letzter kanonischer Snapshot-Slot (Bootstrap/Reconcile).
    snapshot_slot: Option<u64>,
    /// Höchster gesehener Slot (Provenienz).
    highest_slot: u64,
    /// Letzter Root (Finalisierung). Updates darunter sind ungültig.
    root: Option<u64>,
    /// Bootstrap-Puffer bis Snapshot installiert ist.
    buffer: VecDeque<AccountUpdate>,
    buffer_cap: usize,
    counters: StateCounters,
    forks: fork::ForkTracker,
    /// Gesetzt durch einen sauberen `reconcile()` nach dem letzten `install_snapshot()`.
    /// `confirm_ready()` ohne dieses Flag ist ein No-op (Safety-Gate).
    reconciled_clean: bool,
}

impl StateCore {
    pub fn new(buffer_cap: usize) -> Self {
        StateCore {
            accounts: HashMap::new(),
            run_state: RunState::Bootstrapping,
            snapshot_slot: None,
            highest_slot: 0,
            root: None,
            buffer: VecDeque::new(),
            buffer_cap,
            counters: StateCounters::default(),
            reconciled_clean: false,
            forks: fork::ForkTracker::new(),
        }
    }

    /// Accounts, deren letzter Write auf einem verwaisten Fork lag, sind keine Wahrheit mehr:
    /// Einträge werden entfernt und der State hält bis zur Reconciliation.
    pub fn invalidate_slots(&mut self, orphaned: &[u64]) {
        let before = self.accounts.len();
        self.accounts.retain(|_, r| !orphaned.contains(&r.slot));
        if self.accounts.len() != before {
            self.counters.orphaned_writes += (before - self.accounts.len()) as u64;
            self.halt(HaltReason::StateGap);
        }
    }

    pub fn forks(&self) -> &fork::ForkTracker {
        &self.forks
    }

    pub fn run_state(&self) -> RunState {
        self.run_state
    }

    pub fn counters(&self) -> StateCounters {
        self.counters
    }

    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    pub fn get(&self, key: &Pubkey) -> Option<&AccountRecord> {
        self.accounts.get(key)
    }

    pub fn snapshot_slot(&self) -> Option<u64> {
        self.snapshot_slot
    }

    pub fn highest_slot(&self) -> u64 {
        self.highest_slot
    }

    pub fn root(&self) -> Option<u64> {
        self.root
    }

    /// Alle Accounts eines Owners (für Reverse-Index-Aufbau in P2).
    pub fn iter_owned_by<'a>(&'a self, owner: &'a Pubkey) -> impl Iterator<Item = (&'a Pubkey, &'a AccountRecord)> + 'a {
        self.accounts.iter().filter(move |(_, r)| &r.owner == owner)
    }

    /// Fail-closed Halt (Feed-Gap, Program-Upgrade, …). Erster Grund bleibt.
    pub fn halt(&mut self, reason: HaltReason) {
        self.run_state = self.run_state.halt(reason);
    }

    /// Ingress-Event verarbeiten. Gibt für Account-Updates das Ergebnis zurück.
    pub fn on_event(&mut self, ev: IngressEvent) -> Option<ApplyOutcome> {
        match ev {
            IngressEvent::Account(a) => Some(self.on_account(a, SlotStatus::Confirmed)),
            IngressEvent::Slot(s) => {
                self.highest_slot = self.highest_slot.max(s.slot);
                self.forks.on_slot(s.slot, s.parent);
                None
            }
            IngressEvent::Transaction(t) => {
                self.highest_slot = self.highest_slot.max(t.slot);
                None // Shadow/H0-Evidence, kein State
            }
            IngressEvent::Logs(l) => {
                self.highest_slot = self.highest_slot.max(l.slot);
                None // [C0-MO] Market evidence, kein State
            }
            IngressEvent::Health(_) | IngressEvent::Transport { .. } => None, // Telemetrie, kein State
            IngressEvent::Root { slot, .. } => {
                self.root = Some(self.root.map_or(slot, |r| r.max(slot)));
                let orphaned = self.forks.on_root(slot);
                if !orphaned.is_empty() {
                    self.invalidate_slots(&orphaned);
                }
                None
            }
            IngressEvent::Gap { .. } | IngressEvent::Reconnected { .. } => {
                self.halt(HaltReason::FeedGap);
                None
            }
            IngressEvent::Fatal { .. } => {
                self.halt(HaltReason::FeedGap);
                None
            }
        }
    }

    fn on_account(&mut self, a: AccountUpdate, status: SlotStatus) -> ApplyOutcome {
        self.highest_slot = self.highest_slot.max(a.slot);
        if self.snapshot_slot.is_none() {
            // Bootstrap: puffern bis Snapshot installiert ist.
            if self.buffer.len() >= self.buffer_cap {
                // Puffer voll = Lücke; fail closed, Bootstrap muss neu starten.
                self.halt(HaltReason::StateGap);
                return ApplyOutcome::Buffered;
            }
            self.buffer.push_back(a);
            self.counters.buffered += 1;
            return ApplyOutcome::Buffered;
        }
        self.apply(a, status)
    }

    fn apply(&mut self, a: AccountUpdate, status: SlotStatus) -> ApplyOutcome {
        if let Some(root) = self.root {
            if a.slot < root {
                self.counters.below_root += 1;
                return ApplyOutcome::BelowRoot;
            }
        }
        if let Some(existing) = self.accounts.get(&a.pubkey) {
            if a.slot < existing.slot {
                self.counters.stale_slot += 1;
                return ApplyOutcome::StaleSlot;
            }
            if a.slot == existing.slot {
                match (a.write_version, existing.write_version) {
                    (Some(n), Some(e)) if n <= e => {
                        self.counters.stale_write_version += 1;
                        return ApplyOutcome::StaleWriteVersion;
                    }
                    (Some(_), Some(_)) => {}
                    _ => {
                        // Ohne write_version keine Ordnung im Slot: später gewinnt, gezählt.
                        self.counters.same_slot_no_wv += 1;
                    }
                }
            }
        }
        self.accounts.insert(
            a.pubkey,
            AccountRecord {
                owner: a.owner,
                lamports: a.lamports,
                data: a.data,
                slot: a.slot,
                write_version: a.write_version,
                status,
                source: a.source,
                t_rx_ns: a.t_rx_ns,
            },
        );
        self.counters.applied += 1;
        ApplyOutcome::Applied
    }

    /// Snapshot installieren (Bootstrap oder Recovery). Jeder Account behält den
    /// Context-Slot seiner eigenen Antwort (Batches haben verschiedene Slots);
    /// `snapshot_slot` ist die **untere Schranke** über alle Batches. Gepufferte
    /// Updates werden per Account-Ordnungsregel angewendet (neuer als der
    /// Account-Snapshot-Slot) — nicht gegen einen zusammengezogenen Slot.
    /// Übergang: Bootstrapping/Halted → Reconciling. Freigabe erst nach sauberem
    /// `reconcile()` und `confirm_ready()`.
    pub fn install_snapshot(&mut self, snapshot_lower_bound_slot: u64, accounts: Vec<AccountUpdate>) {
        self.run_state = self.run_state.begin_reconcile();
        self.reconciled_clean = false;
        self.accounts.clear();
        let mut min_slot = snapshot_lower_bound_slot;
        for a in accounts {
            min_slot = min_slot.min(a.slot);
            self.highest_slot = self.highest_slot.max(a.slot);
            let rec = AccountRecord {
                owner: a.owner,
                lamports: a.lamports,
                data: a.data,
                slot: a.slot,
                write_version: None,
                status: SlotStatus::Confirmed,
                source: SourceKind::Snapshot,
                t_rx_ns: a.t_rx_ns,
            };
            self.accounts.insert(a.pubkey, rec);
        }
        self.snapshot_slot = Some(min_slot);
        let buffered: Vec<AccountUpdate> = self.buffer.drain(..).collect();
        for a in buffered {
            let newer = match self.accounts.get(&a.pubkey) {
                Some(existing) => a.slot > existing.slot,
                None => a.slot > min_slot,
            };
            if newer {
                self.apply(a, SlotStatus::Confirmed);
            } else {
                self.counters.buffer_dropped_le_snapshot += 1;
            }
        }
    }

    /// Reconciliation gegen einen zweiten Snapshot (Recovery): vergleicht Daten
    /// aller Accounts, die im Snapshot **bei Slot ≤ lokalem Slot** liegen müssten.
    /// Lokale Accounts mit Slot > snapshot_slot werden nicht als Mismatch gewertet
    /// (sie sind neuer). Ergebnis entscheidet über `confirm_ready`.
    pub fn reconcile(&mut self, snapshot_slot: u64, remote: &[AccountUpdate]) -> ReconcileReport {
        let mut rep = ReconcileReport { snapshot_slot, ..Default::default() };
        let mut seen: HashMap<Pubkey, ()> = HashMap::with_capacity(remote.len());
        for r in remote {
            seen.insert(r.pubkey, ());
            rep.checked += 1;
            match self.accounts.get(&r.pubkey) {
                None => rep.missing_local.push(r.pubkey),
                Some(local) => {
                    // [FIX-28] accountbezogen gegen den Context-Slot **dieses** Remote-Accounts,
                    // nicht gegen den globalen Lower-Bound (Batches haben verschiedene Slots).
                    if local.slot > r.slot {
                        continue;
                    }
                    if local.data != r.data || local.lamports != r.lamports || local.owner != r.owner {
                        rep.mismatch.push(r.pubkey);
                    }
                }
            }
        }
        for (k, local) in &self.accounts {
            if local.slot <= snapshot_slot && !seen.contains_key(k) {
                rep.missing_remote.push(*k);
            }
        }
        self.counters.reconcile_mismatch += rep.mismatch.len() as u64;
        self.counters.reconcile_missing_local += rep.missing_local.len() as u64;
        self.counters.reconcile_missing_remote += rep.missing_remote.len() as u64;
        if !rep.is_clean() {
            self.halt(HaltReason::ReconcileFailure);
        } else {
            self.reconciled_clean = true;
        }
        rep
    }

    /// Explizite Freigabe. Nur nach `install_snapshot` + sauberem `reconcile`;
    /// sonst bleibt der Zustand unverändert (kein Trading auf unreconciled State).
    pub fn confirm_ready(&mut self) -> RunState {
        if !self.reconciled_clean {
            return self.run_state;
        }
        self.run_state = self.run_state.reconcile_ok();
        self.run_state
    }

    pub fn is_reconciled(&self) -> bool {
        self.reconciled_clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingress::SlotUpdate;

    fn upd(pk: Pubkey, slot: u64, wv: Option<u64>, data: u8) -> AccountUpdate {
        AccountUpdate {
            pubkey: pk,
            owner: Pubkey::new_from_array([0xA5; 32]), // [FIX-57] stable owner across snapshots
            lamports: 1,
            data: vec![data],
            slot,
            write_version: wv,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }
    }

    #[test]
    fn bootstrap_buffers_then_applies_only_newer_than_snapshot() {
        let mut st = StateCore::new(100);
        let pk = Pubkey::new_unique();
        assert!(!st.run_state().is_tradable());
        assert_eq!(st.on_event(IngressEvent::Account(upd(pk, 10, None, 1))), Some(ApplyOutcome::Buffered));
        assert_eq!(st.on_event(IngressEvent::Account(upd(pk, 12, None, 2))), Some(ApplyOutcome::Buffered));
        st.install_snapshot(11, vec![upd(pk, 11, None, 9)]);
        assert_eq!(st.get(&pk).unwrap().data, vec![2], "Update > S angewendet");
        assert_eq!(st.counters().buffer_dropped_le_snapshot, 1);
        assert!(!st.run_state().is_tradable(), "erst reconcile + confirm_ready");
        assert!(!st.confirm_ready().is_tradable(), "ohne reconcile keine Freigabe");
        // Reconcile gegen zweiten Snapshot bei Slot 12: lokaler Stand ist data=[2] @12
        let rep = st.reconcile(12, &[upd(pk, 12, None, 2)]);
        assert!(rep.is_clean());
        assert!(st.confirm_ready().is_tradable());
    }

    #[test]
    fn ordering_rules() {
        let mut st = StateCore::new(10);
        let pk = Pubkey::new_unique();
        st.install_snapshot(5, vec![]);
        st.reconcile(5, &[]);
        st.confirm_ready();
        assert_eq!(st.on_event(IngressEvent::Account(upd(pk, 10, Some(5), 1))), Some(ApplyOutcome::Applied));
        assert_eq!(st.on_event(IngressEvent::Account(upd(pk, 9, Some(9), 2))), Some(ApplyOutcome::StaleSlot));
        assert_eq!(st.on_event(IngressEvent::Account(upd(pk, 10, Some(4), 3))), Some(ApplyOutcome::StaleWriteVersion));
        assert_eq!(st.on_event(IngressEvent::Account(upd(pk, 10, Some(6), 4))), Some(ApplyOutcome::Applied));
        assert_eq!(st.get(&pk).unwrap().data, vec![4]);
        // ohne write_version im selben Slot: angewendet, aber gezählt
        assert_eq!(st.on_event(IngressEvent::Account(upd(pk, 10, None, 5))), Some(ApplyOutcome::Applied));
        assert_eq!(st.counters().same_slot_no_wv, 1);
    }

    #[test]
    fn root_policy_rejects_below_root() {
        let mut st = StateCore::new(10);
        st.install_snapshot(5, vec![]);
        st.reconcile(5, &[]);
        st.confirm_ready();
        st.on_event(IngressEvent::Root { slot: 100, source: SourceKind::WsRpcControl, t_rx_ns: 0 });
        let pk = Pubkey::new_unique();
        assert_eq!(st.on_event(IngressEvent::Account(upd(pk, 99, None, 1))), Some(ApplyOutcome::BelowRoot));
        assert_eq!(st.on_event(IngressEvent::Account(upd(pk, 100, None, 1))), Some(ApplyOutcome::Applied));
    }

    #[test]
    fn gap_halts_and_reconcile_recovers() {
        let mut st = StateCore::new(10);
        let pk = Pubkey::new_unique();
        st.install_snapshot(5, vec![upd(pk, 5, None, 1)]);
        st.reconcile(5, &[upd(pk, 5, None, 1)]);
        st.confirm_ready();
        st.on_event(IngressEvent::Gap { source: SourceKind::WsRpcControl, from_slot: Some(5), to_slot: None, t_rx_ns: 0 });
        assert_eq!(st.run_state(), RunState::Halted(HaltReason::FeedGap));
        // Recovery: neuer Snapshot, Reconcile sauber, Freigabe
        st.install_snapshot(8, vec![upd(pk, 8, None, 1)]);
        let rep = st.reconcile(8, &[upd(pk, 8, None, 1)]);
        assert!(rep.is_clean());
        assert!(st.confirm_ready().is_tradable());
    }

    #[test]
    fn reconcile_mixed_slot_batches_detects_stale_local() {
        // [FIX-28] Regression: lower bound 100, lokal Slot 105, remote (zweiter Batch) Slot 110 mit anderen Daten.
        let mut st = StateCore::new(10);
        let pk = Pubkey::new_unique();
        st.install_snapshot(100, vec![upd(pk, 105, None, 1)]);
        let rep = st.reconcile(100, &[upd(pk, 110, None, 2)]);
        assert_eq!(rep.mismatch, vec![pk], "lokal 105 < remote 110 muss verglichen werden");
        let mut st = StateCore::new(10);
        st.install_snapshot(100, vec![upd(pk, 120, None, 1)]);
        let rep = st.reconcile(100, &[upd(pk, 110, None, 2)]);
        assert!(rep.is_clean(), "lokal 120 > remote 110 → lokal neuer, kein Mismatch");
    }

    #[test]
    fn reconcile_mismatch_halts() {
        let mut st = StateCore::new(10);
        let pk = Pubkey::new_unique();
        st.install_snapshot(8, vec![upd(pk, 8, None, 1)]);
        let rep = st.reconcile(8, &[upd(pk, 8, None, 2)]);
        assert_eq!(rep.mismatch, vec![pk]);
        assert!(!st.confirm_ready().is_tradable());
        assert_eq!(st.run_state(), RunState::Halted(HaltReason::ReconcileFailure));
    }

    #[test]
    fn reconcile_owner_mismatch_halts() {
        let mut st = StateCore::new(10);
        let pk = Pubkey::new_unique();
        st.install_snapshot(8, vec![upd(pk, 8, None, 1)]);
        let mut remote = upd(pk, 8, None, 1);
        remote.owner = Pubkey::new_from_array([0x5A; 32]);
        let rep = st.reconcile(8, &[remote]);
        assert_eq!(rep.mismatch, vec![pk], "owner is part of canonical account state");
        assert_eq!(st.run_state(), RunState::Halted(HaltReason::ReconcileFailure));
    }

    #[test]
    fn slot_events_track_highest() {
        let mut st = StateCore::new(10);
        st.on_event(IngressEvent::Slot(SlotUpdate { slot: 77, parent: Some(76), status: SlotStatus::Processed, source: SourceKind::WsRpcControl, t_rx_ns: 0 }));
        assert_eq!(st.highest_slot(), 77);
    }
}
