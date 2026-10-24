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

/// B17: the one commitment both state sources read at — the WSS subscriptions of the state
/// owner (`WsRpcConfig::new`) and the RPC snapshots (`snapshot`). Every account record carries
/// the status this commitment proves (`STATE_SLOT_STATUS`), never a higher one.
pub const STATE_COMMITMENT: &str = "confirmed";
pub const STATE_SLOT_STATUS: SlotStatus = SlotStatus::Confirmed;
/// The slot status a commitment level proves.
pub fn status_of_commitment(commitment: &str) -> Option<SlotStatus> {
    match commitment {
        "processed" => Some(SlotStatus::Processed),
        "confirmed" => Some(SlotStatus::Confirmed),
        "finalized" => Some(SlotStatus::Finalized),
        _ => None,
    }
}
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
    /// P6-MT.2: the `state_epoch` at which this record was created or last written (every
    /// applied write, also a snapshot install, assigns a fresh epoch).
    pub last_changed_epoch: u64,
}

/// P6-MT.2: what the state knows about a key's change history, for readset validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEpoch {
    /// A record exists; its `last_changed_epoch`.
    Present(u64),
    /// No record. `removed_bound`: the key was removed at exactly this epoch when the removal
    /// is still recorded, else the compaction floor (it may have been removed at that epoch or
    /// earlier); `0` = never removed since the state's start.
    Absent { removed_bound: u64 },
}

/// P6-MT.2: the epoch view every readset is recorded against and validated against. The
/// canonical `StateCore` is the one production implementation.
pub trait EpochSource {
    fn key_epoch(&self, key: &Pubkey) -> KeyEpoch;
    /// The current monotone state epoch (the epoch an `ABSENT` observation is made at).
    fn state_epoch(&self) -> u64;
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
    /// E9: newest slot of a received account notification (all at `STATE_COMMITMENT`). In-order
    /// delivery on the one WSS connection: every notification of an earlier slot was received
    /// (of its own slot only once a later slot's notification arrived).
    highest_account_slot: u64,
    /// E9: the lower bound of the last installed snapshot — the state covers every account
    /// update up to it.
    covered_slot: u64,
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
    /// C1: applied `Reconnected` events (stream generations); recovery compares it per attempt,
    /// so a reconnect drained while recovering is never swallowed.
    reconnects_applied: u64,
    /// C7: first terminal ingress `Fatal` detail; sticky, the state never leaves HALT for it.
    fatal_detail: Option<String>,
    /// C1: a `Gap` arrived after the last `Reconnected` (StreamReady): the stream has no valid
    /// generation until the next `Reconnected`, so no recovery may become Ready.
    gap_open: bool,
    /// P6-MT.2: monotone epoch of record mutations; `1` on a fresh state, incremented by every
    /// applied record write and every removal (snapshot install, fork invalidation). Never
    /// decreases.
    state_epoch: u64,
    /// P6-MT.2: removal epochs of keys removed since the last compaction (`removed_floor`), so a
    /// readset `ABSENT` observation is invalidated by a creation-and-removal in between. Bounded
    /// by `removed_cap` (= `buffer_cap`, the one record bound this state already has outside its
    /// account map): at the bound the map is compacted into the floor — conservative, every
    /// `ABSENT` observed before the floor then counts as changed (fail closed), never a
    /// missed removal. Removals come from fork orphaning (rare) and snapshot installs (a
    /// recovery, which invalidates every older work by `recovery_generation` anyway).
    removed: HashMap<Pubkey, u64>,
    removed_floor: u64,
    removed_cap: usize,
    /// P6-MT.2: the number of ended Ready periods. Every transition out of the tradable state
    /// (any halt — gap, reconnect, fork orphan, reconcile failure, program upgrade — or a
    /// snapshot install from Ready) increments it, so every recovery starts a new generation
    /// and work tagged with an older one can never commit.
    recovery_generation: u64,
}

impl StateCore {
    pub fn new(buffer_cap: usize) -> Self {
        StateCore {
            accounts: HashMap::new(),
            run_state: RunState::Bootstrapping,
            snapshot_slot: None,
            highest_slot: 0,
            highest_account_slot: 0,
            covered_slot: 0,
            root: None,
            buffer: VecDeque::new(),
            buffer_cap,
            counters: StateCounters::default(),
            reconciled_clean: false,
            forks: fork::ForkTracker::new(),
            reconnects_applied: 0,
            fatal_detail: None,
            gap_open: false,
            state_epoch: 1,
            removed: HashMap::new(),
            removed_floor: 0,
            removed_cap: buffer_cap.max(1),
            recovery_generation: 0,
        }
    }
    /// P6-MT.2: the ended Ready periods (see the field).
    pub fn recovery_generation(&self) -> u64 {
        self.recovery_generation
    }
    /// P6-MT.2: the compaction floor of the removal epochs (see the field); `0` = none yet.
    pub fn removed_floor(&self) -> u64 {
        self.removed_floor
    }
    fn next_epoch(&mut self) -> u64 {
        self.state_epoch = self.state_epoch.saturating_add(1);
        self.state_epoch
    }
    fn note_removed(&mut self, key: Pubkey) {
        let epoch = self.next_epoch();
        if self.removed.len() >= self.removed_cap && !self.removed.contains_key(&key) {
            self.removed.clear();
            self.removed_floor = epoch;
            return;
        }
        self.removed.insert(key, epoch);
    }
    /// A record is (re)written: fresh epoch, its removal (if recorded) is superseded.
    fn written(&mut self) -> u64 {
        self.next_epoch()
    }
    /// Every transition out of the tradable state ends the current recovery generation.
    fn leaving_tradable(&mut self) {
        if self.run_state.is_tradable() {
            self.recovery_generation = self.recovery_generation.saturating_add(1);
        }
    }
    /// A `Gap` since the last `Reconnected` (C1).
    pub fn gap_open(&self) -> bool {
        self.gap_open
    }
    /// `Reconnected` events applied so far (C1).
    pub fn reconnects_applied(&self) -> u64 {
        self.reconnects_applied
    }
    /// Detail of the first ingress `Fatal` applied, if any (C7).
    pub fn fatal_detail(&self) -> Option<&str> {
        self.fatal_detail.as_deref()
    }

    /// Accounts, deren letzter Write auf einem verwaisten Fork lag, sind keine Wahrheit mehr:
    /// Einträge werden entfernt und der State hält bis zur Reconciliation.
    pub fn invalidate_slots(&mut self, orphaned: &[u64]) {
        let before = self.accounts.len();
        let mut removed: Vec<Pubkey> = Vec::new();
        self.accounts.retain(|k, r| {
            let keep = !orphaned.contains(&r.slot);
            if !keep {
                removed.push(*k);
            }
            keep
        });
        if self.accounts.len() != before {
            removed.sort_unstable();
            for key in removed {
                self.note_removed(key);
            }
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

    /// E9: the newest slot of a received account notification (see the field).
    pub fn highest_account_slot(&self) -> u64 {
        self.highest_account_slot
    }

    /// E9: the lower bound of the last installed snapshot (see the field).
    pub fn covered_slot(&self) -> u64 {
        self.covered_slot
    }

    pub fn root(&self) -> Option<u64> {
        self.root
    }

    /// Alle Accounts eines Owners (für Reverse-Index-Aufbau in P2).
    pub fn iter_owned_by<'a>(
        &'a self,
        owner: &'a Pubkey,
    ) -> impl Iterator<Item = (&'a Pubkey, &'a AccountRecord)> + 'a {
        self.accounts.iter().filter(move |(_, r)| &r.owner == owner)
    }

    /// Fail-closed Halt (Feed-Gap, Program-Upgrade, …). Erster Grund bleibt.
    pub fn halt(&mut self, reason: HaltReason) {
        self.leaving_tradable();
        self.run_state = self.run_state.halt(reason);
    }

    /// Ingress-Event verarbeiten. Gibt für Account-Updates das Ergebnis zurück.
    pub fn on_event(&mut self, ev: IngressEvent) -> Option<ApplyOutcome> {
        match ev {
            IngressEvent::Account(a) => Some(self.on_account(a, STATE_SLOT_STATUS)),
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
                self.highest_slot = self.highest_slot.max(l.logs.slot);
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
            IngressEvent::Gap { .. } => {
                self.gap_open = true;
                self.halt(HaltReason::FeedGap);
                None
            }
            IngressEvent::Reconnected { .. } => {
                self.reconnects_applied = self.reconnects_applied.saturating_add(1);
                self.gap_open = false;
                self.halt(HaltReason::FeedGap);
                None
            }
            IngressEvent::Fatal { detail, .. } => {
                self.fatal_detail.get_or_insert(detail);
                self.halt(HaltReason::FeedGap);
                None
            }
        }
    }

    fn on_account(&mut self, a: AccountUpdate, status: SlotStatus) -> ApplyOutcome {
        self.highest_slot = self.highest_slot.max(a.slot);
        self.highest_account_slot = self.highest_account_slot.max(a.slot);
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
        let last_changed_epoch = self.written();
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
                last_changed_epoch,
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
    pub fn install_snapshot(
        &mut self,
        snapshot_lower_bound_slot: u64,
        accounts: Vec<AccountUpdate>,
    ) {
        self.leaving_tradable();
        self.run_state = self.run_state.begin_reconcile();
        self.reconciled_clean = false;
        // P6-MT.2: every key of the replaced state that the snapshot does not write again is a
        // removal (observable by a readset); every written record gets a fresh epoch.
        let mut previous: Vec<Pubkey> = self.accounts.keys().copied().collect();
        previous.sort_unstable();
        self.accounts.clear();
        // E9: the snapshot state covers every account update up to its lower bound.
        self.covered_slot = self.covered_slot.max(snapshot_lower_bound_slot);
        let mut min_slot = snapshot_lower_bound_slot;
        for a in accounts {
            min_slot = min_slot.min(a.slot);
            self.highest_slot = self.highest_slot.max(a.slot);
            let last_changed_epoch = self.written();
            let rec = AccountRecord {
                owner: a.owner,
                lamports: a.lamports,
                data: a.data,
                slot: a.slot,
                write_version: None,
                status: STATE_SLOT_STATUS,
                source: SourceKind::Snapshot,
                t_rx_ns: a.t_rx_ns,
                last_changed_epoch,
            };
            self.accounts.insert(a.pubkey, rec);
        }
        for key in previous {
            if !self.accounts.contains_key(&key) {
                self.note_removed(key);
            }
        }
        self.snapshot_slot = Some(min_slot);
        let buffered: Vec<AccountUpdate> = self.buffer.drain(..).collect();
        for a in buffered {
            let newer = match self.accounts.get(&a.pubkey) {
                Some(existing) => a.slot > existing.slot,
                None => a.slot > min_slot,
            };
            if newer {
                self.apply(a, STATE_SLOT_STATUS);
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
        let mut rep = ReconcileReport {
            snapshot_slot,
            ..Default::default()
        };
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
                    if local.data != r.data
                        || local.lamports != r.lamports
                        || local.owner != r.owner
                    {
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

impl EpochSource for StateCore {
    fn key_epoch(&self, key: &Pubkey) -> KeyEpoch {
        match self.accounts.get(key) {
            Some(r) => KeyEpoch::Present(r.last_changed_epoch),
            None => KeyEpoch::Absent {
                removed_bound: self.removed.get(key).copied().unwrap_or(self.removed_floor),
            },
        }
    }
    fn state_epoch(&self) -> u64 {
        self.state_epoch
    }
}

#[cfg(test)]
mod epoch_tests {
    use super::*;

    fn upd(pk: Pubkey, slot: u64, data: u8) -> AccountUpdate {
        AccountUpdate {
            pubkey: pk,
            owner: Pubkey::new_from_array([0xA5; 32]),
            lamports: 1,
            data: vec![data],
            slot,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }
    }

    fn ready(st: &mut StateCore, slot: u64, accounts: Vec<AccountUpdate>) {
        st.install_snapshot(slot, accounts.clone());
        assert!(st.reconcile(slot, &accounts).is_clean());
        assert!(st.confirm_ready().is_tradable());
    }

    /// P6-MT.2: `state_epoch` starts at 1 and only ever grows — through writes, a snapshot
    /// install (every record re-epoched), a clean reconcile (no record write: unchanged) and a
    /// fork invalidation (a removal); stale and buffered updates do not move it.
    #[test]
    fn state_epoch_is_monotone_across_writes_snapshot_reconcile_and_fork() {
        let mut st = StateCore::new(8);
        assert_eq!(st.state_epoch(), 1);
        let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
        st.on_event(IngressEvent::Account(upd(a, 10, 1))); // buffered: no record write
        assert_eq!(st.state_epoch(), 1);
        ready(&mut st, 5, vec![upd(a, 5, 1), upd(b, 5, 1)]);
        let after_install = st.state_epoch();
        assert!(after_install > 1);
        let (ea, eb) = (
            st.get(&a).unwrap().last_changed_epoch,
            st.get(&b).unwrap().last_changed_epoch,
        );
        assert!(ea > 1 && eb > 1 && ea != eb);
        assert_eq!(st.key_epoch(&a), KeyEpoch::Present(ea));
        // Reconcile writes no record.
        let before = st.state_epoch();
        assert!(st.reconcile(5, &[upd(a, 5, 1), upd(b, 5, 1)]).is_clean());
        assert_eq!(st.state_epoch(), before);
        // A stale update leaves everything unchanged.
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(a, 4, 9))),
            Some(ApplyOutcome::StaleSlot)
        );
        assert_eq!(st.state_epoch(), before);
        assert_eq!(st.get(&a).unwrap().last_changed_epoch, ea);
        // An applied write moves the record's epoch and the state epoch; `b` keeps its own.
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(a, 12, 2))),
            Some(ApplyOutcome::Applied)
        );
        let ea2 = st.get(&a).unwrap().last_changed_epoch;
        assert!(ea2 > ea && st.state_epoch() == ea2);
        assert_eq!(st.get(&b).unwrap().last_changed_epoch, eb);
        // Fork invalidation removes `a` (slot 12 orphaned): a removal epoch, state halts.
        st.on_event(IngressEvent::Slot(crate::ingress::SlotUpdate {
            slot: 12,
            parent: Some(11),
            status: SlotStatus::Processed,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }));
        st.on_event(IngressEvent::Slot(crate::ingress::SlotUpdate {
            slot: 13,
            parent: Some(11),
            status: SlotStatus::Processed,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }));
        let before_root = st.state_epoch();
        st.on_event(IngressEvent::Root {
            slot: 13,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        });
        assert!(
            st.get(&a).is_none(),
            "slot 12 is orphaned by root 13 (parent 11)"
        );
        let removed = st.state_epoch();
        assert!(removed > before_root);
        assert_eq!(
            st.key_epoch(&a),
            KeyEpoch::Absent {
                removed_bound: removed
            }
        );
        assert!(!st.run_state().is_tradable());
        assert_eq!(
            st.get(&b).unwrap().last_changed_epoch,
            eb,
            "unrelated record untouched"
        );
        // A re-install re-epochs every record above everything before.
        let top = st.state_epoch();
        st.install_snapshot(20, vec![upd(a, 20, 1), upd(b, 20, 1)]);
        assert!(st.get(&a).unwrap().last_changed_epoch > top);
        assert!(st.get(&b).unwrap().last_changed_epoch > top);
        assert!(st.state_epoch() >= st.get(&b).unwrap().last_changed_epoch);
    }

    /// P6-MT.2 (#9 substrate): an absent key is `Absent{0}` on a fresh state; a creation makes
    /// it `Present`; a snapshot that drops it records the removal epoch (> the state epoch any
    /// earlier ABSENT observation was made at); a creation after a removal is `Present` again
    /// with a newer epoch.
    #[test]
    fn removal_and_creation_are_observable_through_key_epoch() {
        let mut st = StateCore::new(8);
        let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
        ready(&mut st, 5, vec![upd(b, 5, 1)]);
        let observed_absent_at = st.state_epoch();
        assert_eq!(st.key_epoch(&a), KeyEpoch::Absent { removed_bound: 0 });
        st.on_event(IngressEvent::Account(upd(a, 6, 1)));
        let KeyEpoch::Present(created) = st.key_epoch(&a) else {
            panic!("created")
        };
        assert!(created > observed_absent_at);
        // Snapshot without `a`: removed at an epoch above the earlier ABSENT observation.
        st.install_snapshot(7, vec![upd(b, 7, 1)]);
        let KeyEpoch::Absent { removed_bound } = st.key_epoch(&a) else {
            panic!("removed")
        };
        assert!(removed_bound > observed_absent_at);
        assert_eq!(st.removed_floor(), 0, "no compaction at this size");
        st.install_snapshot(8, vec![upd(a, 8, 1), upd(b, 8, 1)]);
        let KeyEpoch::Present(again) = st.key_epoch(&a) else {
            panic!("re-created")
        };
        assert!(again > removed_bound);
    }

    /// P6-MT.2: the removal map is bounded by `buffer_cap`; at the bound it compacts into the
    /// floor, which then answers every unknown absent key conservatively (the floor epoch, so
    /// an ABSENT observed before it counts as changed); nothing grows without bound.
    #[test]
    fn removal_epochs_are_bounded_and_compact_into_a_conservative_floor() {
        let mut st = StateCore::new(2);
        let keys: Vec<Pubkey> = (0..4).map(|_| Pubkey::new_unique()).collect();
        ready(&mut st, 5, keys.iter().map(|k| upd(*k, 5, 1)).collect());
        let observed_absent_at = st.state_epoch();
        let unknown = Pubkey::new_unique();
        assert_eq!(
            st.key_epoch(&unknown),
            KeyEpoch::Absent { removed_bound: 0 }
        );
        // Four removals through a bound of two: compaction happened.
        st.install_snapshot(6, vec![]);
        assert!(st.removed.len() <= 2);
        assert!(st.removed_floor() > observed_absent_at);
        for k in &keys {
            let KeyEpoch::Absent { removed_bound } = st.key_epoch(k) else {
                panic!("removed")
            };
            assert!(
                removed_bound > observed_absent_at,
                "exact or floor, never below"
            );
        }
        // A key never written is answered with the floor as well: conservative.
        assert_eq!(
            st.key_epoch(&unknown),
            KeyEpoch::Absent {
                removed_bound: st.removed_floor()
            }
        );
    }

    /// P6-MT.2: `recovery_generation` counts ended Ready periods: 0 through bootstrap, +1 at
    /// the first halt out of Ready (a later halt while already halted or a re-install while
    /// halted adds nothing), +1 again for the next Ready period's end; a snapshot install
    /// directly from Ready (periodic reconcile) ends the period too.
    #[test]
    fn recovery_generation_increments_once_per_ended_ready_period() {
        let mut st = StateCore::new(8);
        let a = Pubkey::new_unique();
        assert_eq!(st.recovery_generation(), 0);
        st.halt(HaltReason::FeedGap); // bootstrapping: no Ready period ended
        assert_eq!(st.recovery_generation(), 0);
        ready(&mut st, 5, vec![upd(a, 5, 1)]);
        assert_eq!(st.recovery_generation(), 0);
        st.on_event(IngressEvent::Gap {
            source: SourceKind::WsRpcControl,
            from_slot: Some(5),
            to_slot: None,
            t_rx_ns: 0,
        });
        assert_eq!(st.recovery_generation(), 1);
        st.on_event(IngressEvent::Reconnected {
            source: SourceKind::WsRpcControl,
            attempt: 1,
            t_rx_ns: 0,
        });
        assert_eq!(st.recovery_generation(), 1, "halt while halted");
        st.install_snapshot(6, vec![upd(a, 6, 1)]);
        assert_eq!(st.recovery_generation(), 1, "install while halted");
        assert!(st.reconcile(6, &[upd(a, 6, 1)]).is_clean());
        assert!(st.confirm_ready().is_tradable());
        assert_eq!(st.recovery_generation(), 1);
        ready(&mut st, 7, vec![upd(a, 7, 1)]); // periodic reconcile from Ready
        assert_eq!(st.recovery_generation(), 2);
        st.halt(HaltReason::ProgramUpgrade);
        assert_eq!(st.recovery_generation(), 3);
    }
}

#[cfg(test)]
mod commitment_tests {
    use super::*;

    /// B17: the recorded account status is exactly what the state sources' commitment proves:
    /// the owner's WSS default and the snapshot reads use `STATE_COMMITMENT`.
    #[test]
    fn b17_account_status_is_the_subscription_commitment() {
        assert_eq!(
            status_of_commitment(STATE_COMMITMENT),
            Some(STATE_SLOT_STATUS)
        );
        let ws = crate::ingress::ws_rpc::WsRpcConfig::new(
            "wss://unused.invalid".into(),
            Default::default(),
        );
        assert_eq!(ws.commitment, STATE_COMMITMENT);
        assert_eq!(snapshot::STATE_SNAPSHOT_COMMITMENT, STATE_COMMITMENT);
        assert_eq!(
            snapshot::state_commitment_config().commitment.to_string(),
            STATE_COMMITMENT
        );
        let mut st = StateCore::new(4);
        st.install_snapshot(1, vec![]);
        let key = Pubkey::new_unique();
        st.on_event(IngressEvent::Account(AccountUpdate {
            pubkey: key,
            owner: Pubkey::new_unique(),
            lamports: 1,
            data: vec![1],
            slot: 5,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }));
        assert_eq!(st.get(&key).unwrap().status, STATE_SLOT_STATUS);
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
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(pk, 10, None, 1))),
            Some(ApplyOutcome::Buffered)
        );
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(pk, 12, None, 2))),
            Some(ApplyOutcome::Buffered)
        );
        st.install_snapshot(11, vec![upd(pk, 11, None, 9)]);
        assert_eq!(st.get(&pk).unwrap().data, vec![2], "Update > S angewendet");
        assert_eq!(st.counters().buffer_dropped_le_snapshot, 1);
        assert!(
            !st.run_state().is_tradable(),
            "erst reconcile + confirm_ready"
        );
        assert!(
            !st.confirm_ready().is_tradable(),
            "ohne reconcile keine Freigabe"
        );
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
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(pk, 10, Some(5), 1))),
            Some(ApplyOutcome::Applied)
        );
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(pk, 9, Some(9), 2))),
            Some(ApplyOutcome::StaleSlot)
        );
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(pk, 10, Some(4), 3))),
            Some(ApplyOutcome::StaleWriteVersion)
        );
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(pk, 10, Some(6), 4))),
            Some(ApplyOutcome::Applied)
        );
        assert_eq!(st.get(&pk).unwrap().data, vec![4]);
        // ohne write_version im selben Slot: angewendet, aber gezählt
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(pk, 10, None, 5))),
            Some(ApplyOutcome::Applied)
        );
        assert_eq!(st.counters().same_slot_no_wv, 1);
    }

    #[test]
    fn root_policy_rejects_below_root() {
        let mut st = StateCore::new(10);
        st.install_snapshot(5, vec![]);
        st.reconcile(5, &[]);
        st.confirm_ready();
        st.on_event(IngressEvent::Root {
            slot: 100,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        });
        let pk = Pubkey::new_unique();
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(pk, 99, None, 1))),
            Some(ApplyOutcome::BelowRoot)
        );
        assert_eq!(
            st.on_event(IngressEvent::Account(upd(pk, 100, None, 1))),
            Some(ApplyOutcome::Applied)
        );
    }

    #[test]
    fn gap_halts_and_reconcile_recovers() {
        let mut st = StateCore::new(10);
        let pk = Pubkey::new_unique();
        st.install_snapshot(5, vec![upd(pk, 5, None, 1)]);
        st.reconcile(5, &[upd(pk, 5, None, 1)]);
        st.confirm_ready();
        st.on_event(IngressEvent::Gap {
            source: SourceKind::WsRpcControl,
            from_slot: Some(5),
            to_slot: None,
            t_rx_ns: 0,
        });
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
        assert_eq!(
            rep.mismatch,
            vec![pk],
            "lokal 105 < remote 110 muss verglichen werden"
        );
        let mut st = StateCore::new(10);
        st.install_snapshot(100, vec![upd(pk, 120, None, 1)]);
        let rep = st.reconcile(100, &[upd(pk, 110, None, 2)]);
        assert!(
            rep.is_clean(),
            "lokal 120 > remote 110 → lokal neuer, kein Mismatch"
        );
    }

    #[test]
    fn reconcile_mismatch_halts() {
        let mut st = StateCore::new(10);
        let pk = Pubkey::new_unique();
        st.install_snapshot(8, vec![upd(pk, 8, None, 1)]);
        let rep = st.reconcile(8, &[upd(pk, 8, None, 2)]);
        assert_eq!(rep.mismatch, vec![pk]);
        assert!(!st.confirm_ready().is_tradable());
        assert_eq!(
            st.run_state(),
            RunState::Halted(HaltReason::ReconcileFailure)
        );
    }

    #[test]
    fn reconcile_owner_mismatch_halts() {
        let mut st = StateCore::new(10);
        let pk = Pubkey::new_unique();
        st.install_snapshot(8, vec![upd(pk, 8, None, 1)]);
        let mut remote = upd(pk, 8, None, 1);
        remote.owner = Pubkey::new_from_array([0x5A; 32]);
        let rep = st.reconcile(8, &[remote]);
        assert_eq!(
            rep.mismatch,
            vec![pk],
            "owner is part of canonical account state"
        );
        assert_eq!(
            st.run_state(),
            RunState::Halted(HaltReason::ReconcileFailure)
        );
    }

    #[test]
    fn slot_events_track_highest() {
        let mut st = StateCore::new(10);
        st.on_event(IngressEvent::Slot(SlotUpdate {
            slot: 77,
            parent: Some(76),
            status: SlotStatus::Processed,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }));
        assert_eq!(st.highest_slot(), 77);
    }

    /// E9: the account-notification watermark follows received account notifications only — a
    /// (processed) slot event does not move it, an older notification does not lower it.
    #[test]
    fn e9_account_watermark_follows_account_notifications_only() {
        let mut st = StateCore::new(10);
        let account = |slot: u64| {
            IngressEvent::Account(AccountUpdate {
                pubkey: Pubkey::new_unique(),
                owner: Pubkey::new_unique(),
                lamports: 1,
                data: vec![1],
                slot,
                write_version: None,
                source: SourceKind::WsRpcControl,
                t_rx_ns: 0,
            })
        };
        st.on_event(IngressEvent::Slot(SlotUpdate {
            slot: 90,
            parent: Some(89),
            status: SlotStatus::Processed,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }));
        assert_eq!(st.highest_account_slot(), 0);
        st.on_event(account(80));
        assert_eq!(st.highest_account_slot(), 80);
        st.on_event(account(70));
        assert_eq!(st.highest_account_slot(), 80);
        assert_eq!(st.highest_slot(), 90);
        // A snapshot covers up to its lower bound; it is no notification.
        st.install_snapshot(85, vec![]);
        assert_eq!((st.covered_slot(), st.highest_account_slot()), (85, 80));
    }
}
