//! P6-MT.2 (MT5a): the readset of a candidate preparation. Every key read through the
//! [`ReadSetRecorder`] during Health, eligibility, sizing and plan building is recorded as
//! `(pubkey, observed epoch)` — present with the record's `last_changed_epoch`, or `ABSENT` at
//! the state epoch of the observation. A readset is validated against the canonical
//! [`EpochSource`] later: any changed entry (a newer write, a removal, a creation, or a
//! creation-and-removal behind an `ABSENT` observation) means the preparation's inputs are no
//! longer the canonical state. Deterministic order (by key), bounded (a preparation reading more
//! distinct keys than its bound yields no readset: fail closed, never a partial one).
use crate::health::AccountSource;
pub use darkbot_solana::state::{EpochSource, KeyEpoch};
use solana_sdk::pubkey::Pubkey;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

/// One observation of a key at preparation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observed {
    /// The record existed with this `last_changed_epoch`.
    Present(u64),
    /// No record existed; observed at this state epoch.
    Absent { at_state_epoch: u64 },
}

/// A preparation read more distinct keys than its bound: no readset (fail closed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadSetOverflow {
    pub bound: usize,
    /// Distinct keys the preparation read (including those beyond the bound).
    pub reads: usize,
}

impl std::fmt::Display for ReadSetOverflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "READ_SET_OVERFLOW: {} distinct keys read, bound {}",
            self.reads, self.bound
        )
    }
}

/// The complete, ordered readset of one preparation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadSet {
    entries: BTreeMap<Pubkey, Observed>,
}

/// What a key's observation is now, against the canonical epochs.
pub fn observe<E: EpochSource + ?Sized>(epochs: &E, key: &Pubkey) -> Observed {
    match epochs.key_epoch(key) {
        KeyEpoch::Present(e) => Observed::Present(e),
        KeyEpoch::Absent { .. } => Observed::Absent {
            at_state_epoch: epochs.state_epoch(),
        },
    }
}

/// Whether an observation no longer describes the key: a present record with another epoch
/// (every applied write assigns a fresh one) or none; an absent key that exists now, or was
/// removed (hence created) at an epoch after the observation — the removal bound is exact
/// while recorded and the conservative compaction floor afterwards.
pub fn changed(observed: Observed, now: KeyEpoch) -> bool {
    match (observed, now) {
        (Observed::Present(was), KeyEpoch::Present(is)) => was != is,
        (Observed::Present(_), KeyEpoch::Absent { .. }) => true,
        (Observed::Absent { .. }, KeyEpoch::Present(_)) => true,
        (Observed::Absent { at_state_epoch }, KeyEpoch::Absent { removed_bound }) => {
            removed_bound > at_state_epoch
        }
    }
}

impl ReadSet {
    /// The readset of an explicit key set (e.g. a projection's dependency set) observed now.
    pub fn observe_keys<E: EpochSource + ?Sized>(
        keys: impl IntoIterator<Item = Pubkey>,
        epochs: &E,
        bound: usize,
    ) -> Result<ReadSet, ReadSetOverflow> {
        let mut entries = BTreeMap::new();
        let mut reads = 0usize;
        for key in keys {
            if entries.contains_key(&key) {
                continue;
            }
            reads += 1;
            if entries.len() < bound {
                entries.insert(key, observe(epochs, &key));
            }
        }
        if reads > bound {
            return Err(ReadSetOverflow { bound, reads });
        }
        Ok(ReadSet { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn contains(&self, key: &Pubkey) -> bool {
        self.entries.contains_key(key)
    }
    pub fn get(&self, key: &Pubkey) -> Option<Observed> {
        self.entries.get(key).copied()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&Pubkey, &Observed)> {
        self.entries.iter()
    }

    /// The first (by key) entry that no longer describes the canonical state, with what it is
    /// now; `None` while every entry is current.
    pub fn changed_key<E: EpochSource + ?Sized>(
        &self,
        epochs: &E,
    ) -> Option<(Pubkey, Observed, KeyEpoch)> {
        self.entries.iter().find_map(|(key, observed)| {
            let now = epochs.key_epoch(key);
            changed(*observed, now).then_some((*key, *observed, now))
        })
    }
}

/// An [`AccountSource`] that records every key read through it, with its canonical epoch at
/// the time of the read. Reads beyond the bound still pass through (the preparation itself is
/// not cut short) but are counted, and [`finish`](Self::finish) then yields the overflow.
pub struct ReadSetRecorder<'a, S: ?Sized, E: ?Sized> {
    inner: &'a S,
    epochs: &'a E,
    bound: usize,
    reads: RefCell<BTreeMap<Pubkey, Observed>>,
    beyond: Cell<usize>,
}

impl<'a, S: AccountSource + ?Sized, E: EpochSource + ?Sized> ReadSetRecorder<'a, S, E> {
    pub fn new(inner: &'a S, epochs: &'a E, bound: usize) -> Self {
        Self {
            inner,
            epochs,
            bound,
            reads: RefCell::new(BTreeMap::new()),
            beyond: Cell::new(0),
        }
    }

    /// A key read outside the account path that the preparation depends on as well.
    pub fn note(&self, key: &Pubkey) {
        let mut reads = self.reads.borrow_mut();
        if reads.contains_key(key) {
            return;
        }
        if reads.len() < self.bound {
            reads.insert(*key, observe(self.epochs, key));
        } else {
            self.beyond.set(self.beyond.get().saturating_add(1));
        }
    }

    /// Distinct keys read so far (including those beyond the bound).
    pub fn distinct_reads(&self) -> usize {
        self.reads.borrow().len() + self.beyond.get()
    }

    /// The complete readset, or the overflow (fail closed: no partial readset).
    pub fn finish(&self) -> Result<ReadSet, ReadSetOverflow> {
        if self.beyond.get() > 0 {
            return Err(ReadSetOverflow {
                bound: self.bound,
                reads: self.distinct_reads(),
            });
        }
        Ok(ReadSet {
            entries: self.reads.borrow().clone(),
        })
    }
}

impl<S: AccountSource + ?Sized, E: EpochSource + ?Sized> AccountSource
    for ReadSetRecorder<'_, S, E>
{
    fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
        self.note(key);
        self.inner.account(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_solana::ingress::{AccountUpdate, IngressEvent, SourceKind};
    use darkbot_solana::state::StateCore;

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

    struct View<'a>(&'a StateCore);
    impl AccountSource for View<'_> {
        fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            self.0
                .get(key)
                .map(|r| (r.owner, r.lamports, r.data.as_slice()))
        }
    }

    /// Every read is recorded once, in key order whatever the read order; present keys carry
    /// the record epoch, absent keys the state epoch of the observation.
    #[test]
    fn recorder_is_complete_deterministic_and_deduplicated() {
        let mut st = StateCore::new(8);
        let (a, b, missing) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        ready(&mut st, 5, vec![upd(a, 5, 1), upd(b, 5, 2)]);
        let view = View(&st);
        let read = |order: &[Pubkey]| {
            let rec = ReadSetRecorder::new(&view, &st, 8);
            for k in order {
                let _ = rec.account(k);
            }
            let _ = rec.account(&order[0]); // duplicate
            rec.finish().unwrap()
        };
        let one = read(&[a, missing, b]);
        let two = read(&[b, a, missing]);
        assert_eq!(one, two, "order of reads never changes the readset");
        assert_eq!(one.len(), 3);
        let keys: Vec<Pubkey> = one.iter().map(|(k, _)| *k).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
        assert_eq!(
            one.get(&a),
            Some(Observed::Present(st.get(&a).unwrap().last_changed_epoch))
        );
        assert_eq!(
            one.get(&missing),
            Some(Observed::Absent {
                at_state_epoch: st.state_epoch()
            })
        );
        assert!(one.changed_key(&st).is_none());
    }

    /// Fail closed: one key beyond the bound yields no readset; the reads themselves pass through.
    #[test]
    fn recorder_overflow_yields_no_partial_readset() {
        let mut st = StateCore::new(8);
        let keys: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
        ready(&mut st, 5, keys.iter().map(|k| upd(*k, 5, 1)).collect());
        let view = View(&st);
        let rec = ReadSetRecorder::new(&view, &st, 2);
        for k in &keys {
            assert!(rec.account(k).is_some(), "the read itself still answers");
        }
        assert_eq!(rec.distinct_reads(), 3);
        assert_eq!(rec.finish(), Err(ReadSetOverflow { bound: 2, reads: 3 }));
        assert_eq!(
            ReadSet::observe_keys(keys.iter().copied(), &st, 2),
            Err(ReadSetOverflow { bound: 2, reads: 3 })
        );
        assert_eq!(
            ReadSet::observe_keys(keys.iter().copied(), &st, 3)
                .unwrap()
                .len(),
            3
        );
    }

    /// P6-MT.14 #9 at the readset level: ABSENT is invalidated by a creation, and by a
    /// creation-and-removal behind it (exact removal epoch, and the compaction floor); a
    /// present key is invalidated by a write and by a removal; unrelated writes change nothing.
    #[test]
    fn absent_and_present_observations_are_invalidated_by_creation_write_and_removal() {
        let mut st = StateCore::new(8);
        let (a, b, c, other) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        ready(&mut st, 5, vec![upd(a, 5, 1), upd(other, 5, 1)]);
        let view = View(&st);
        let rec = ReadSetRecorder::new(&view, &st, 8);
        let _ = rec.account(&a); // present
        let _ = rec.account(&b); // absent
        let _ = rec.account(&c); // absent
        let set = rec.finish().unwrap();
        // Unrelated write: current.
        st.on_event(IngressEvent::Account(upd(other, 6, 2)));
        assert!(set.changed_key(&st).is_none());
        // Creation of `b`: ABSENT invalidated.
        st.on_event(IngressEvent::Account(upd(b, 6, 1)));
        assert_eq!(set.changed_key(&st).map(|(k, _, _)| k), Some(b));
        // Removal of `b` again (snapshot without it) — still invalid: created-and-removed
        // behind the observation (exact removal epoch).
        st.install_snapshot(7, vec![upd(a, 5, 1), upd(other, 7, 2)]);
        // `a` was re-written by the install (fresh epoch): changed as well; check `b` directly.
        assert!(set.changed_key(&st).is_some(), "install re-epochs");
        let b_now = st.key_epoch(&b);
        assert!(changed(set.get(&b).unwrap(), b_now), "{b_now:?}");
        assert!(
            !changed(set.get(&c).unwrap(), st.key_epoch(&c)),
            "never touched"
        );
        // Present key written: invalid; removed: invalid.
        let mut st2 = StateCore::new(8);
        ready(&mut st2, 5, vec![upd(a, 5, 1)]);
        let view2 = View(&st2);
        let rec = ReadSetRecorder::new(&view2, &st2, 8);
        let _ = rec.account(&a);
        let set2 = rec.finish().unwrap();
        st2.on_event(IngressEvent::Account(upd(a, 6, 2)));
        assert_eq!(set2.changed_key(&st2).map(|(k, _, _)| k), Some(a));
        st2.install_snapshot(7, vec![]);
        assert!(matches!(
            set2.changed_key(&st2),
            Some((k, Observed::Present(_), KeyEpoch::Absent { .. })) if k == a
        ));
        // Compaction floor: an ABSENT observed before the floor counts as changed (conservative).
        let mut st3 = StateCore::new(1);
        let keys: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
        ready(&mut st3, 5, keys.iter().map(|k| upd(*k, 5, 1)).collect());
        let never = Pubkey::new_unique();
        let view3 = View(&st3);
        let rec = ReadSetRecorder::new(&view3, &st3, 8);
        let _ = rec.account(&never);
        let set3 = rec.finish().unwrap();
        assert!(set3.changed_key(&st3).is_none());
        st3.install_snapshot(6, vec![]); // three removals through a bound of one: compacted
        assert!(st3.removed_floor() > 0);
        assert_eq!(set3.changed_key(&st3).map(|(k, _, _)| k), Some(never));
    }
}
