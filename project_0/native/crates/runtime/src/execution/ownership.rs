//! P6-MT.1 (MT5b): the owner's per-account ownership table — the plan's lifecycle
//! `IDLE → EXECUTING(candidate_id) → SIGNED → SUBMITTED(signature, last_valid_block_height) →
//! RELEASED` with the invariant "at most one EXECUTING|SIGNED|SUBMITTED per account" plus, per
//! owned account, at most one newest pending revision (a trigger that arrived while the account
//! was owned, coalesced like the lanes coalesce: `t_rx_first` kept, the newest work wins).
//!
//! Only the owner thread mutates it. Every transition is checked and fails closed: a transition
//! from the wrong phase, or by a candidate that does not own the entry, is refused and counted,
//! never applied. IDLE and RELEASED accounts carry no entry (bounded by the in-flight accounts).
//!
//! SUBMITTED is never a second source of truth: it is the durable FE-04 attempt's target hold
//! as the P8-RES-02 reservation book carries it (`WalletReservations::targets`), synchronized
//! by [`AccountOwnership::sync_submitted`] whenever the book changes — a target the book holds
//! is SUBMITTED (also after a restart, reopened from the journal), and only the book's release
//! (the FE-04 settlement path) releases it. No candidate can release or re-acquire it.
use crate::execution::identity::Inflight;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

/// The durable attempt's submission on a target, as the reservation book carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submission {
    pub attempt: String,
    /// The intent's first bound signature; `None` when the hold carries none.
    pub signature: Option<String>,
    pub last_valid_block_height: Option<u64>,
}

/// The newest pending revision of an owned account: the trigger that arrived while the account
/// was owned, re-armed when the ownership is released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingRevision {
    pub t_rx_first_ns: u64,
    pub t_rx_ns: u64,
    pub slot: u64,
    pub trigger: &'static str,
}

impl PendingRevision {
    /// Coalesces a later trigger: `t_rx_first` stays the earliest, the newest work wins.
    fn coalesce(&mut self, later: PendingRevision) {
        self.t_rx_first_ns = self.t_rx_first_ns.min(later.t_rx_first_ns);
        if later.t_rx_ns >= self.t_rx_ns {
            self.t_rx_ns = later.t_rx_ns;
            self.slot = later.slot;
            self.trigger = later.trigger;
        }
    }
}

/// The public view of an owned account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Executing { candidate_id: u64 },
    Signed { candidate_id: u64 },
    Submitted(Submission),
}

impl State {
    pub fn name(&self) -> &'static str {
        match self {
            State::Executing { .. } => "EXECUTING",
            State::Signed { .. } => "SIGNED",
            State::Submitted(_) => "SUBMITTED",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Owned {
    Candidate { candidate_id: u64, signed: bool },
    Attempt(Submission),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    owned: Owned,
    pending: Option<PendingRevision>,
}

impl Entry {
    fn state(&self) -> State {
        match &self.owned {
            Owned::Candidate {
                candidate_id,
                signed: false,
            } => State::Executing {
                candidate_id: *candidate_id,
            },
            Owned::Candidate {
                candidate_id,
                signed: true,
            } => State::Signed {
                candidate_id: *candidate_id,
            },
            Owned::Attempt(s) => State::Submitted(s.clone()),
        }
    }
}

/// Why a transition was refused (counted, never applied).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `acquire`: the account is owned (by the state's candidate or attempt).
    Held(State),
    /// The account has no entry (IDLE/RELEASED): nothing to sign, release or defer on.
    NotOwned,
    /// The entry is owned by another candidate or by a durable attempt.
    Foreign(State),
    /// The entry's phase does not allow the transition (e.g. SIGNED signed again).
    WrongPhase(State),
}

/// Whether a deferred trigger opened the account's pending slot or coalesced into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deferred {
    First,
    Coalesced,
}

/// P6-MT.13: ownership counters (owner-side only; no persisted MT0 layout change).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnershipStats {
    pub acquired: u64,
    pub signed: u64,
    pub released: u64,
    pub refused_acquire: u64,
    pub refused_signed: u64,
    pub refused_release: u64,
    pub refused_defer: u64,
    pub deferred: u64,
    pub coalesced: u64,
    /// Pending revisions handed back for re-arming (by a release or a book release).
    pub rearmed: u64,
    /// Targets the book held that the table took over as SUBMITTED.
    pub submitted_synced: u64,
    /// Book targets that replaced a candidate's EXECUTING/SIGNED entry (the durable attempt is
    /// the truth; counted as a refused candidate ownership).
    pub submitted_over_candidate: u64,
    /// SUBMITTED entries released because the book no longer holds their target.
    pub submitted_released: u64,
    /// Book targets that are no account key (the owner's last synchronization): none expected,
    /// the journal validates every key; never silently dropped.
    pub unparsable_targets: u64,
}

/// Per-state counts of the table now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub executing: usize,
    pub signed: usize,
    pub submitted: usize,
    /// Owned accounts with a pending revision.
    pub pending: usize,
}

#[derive(Debug, Default)]
pub struct AccountOwnership {
    entries: HashMap<Pubkey, Entry>,
    pub stats: OwnershipStats,
}

impl AccountOwnership {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn state(&self, account: &Pubkey) -> Option<State> {
        self.entries.get(account).map(Entry::state)
    }

    /// The account's in-flight entry for the central result policy: the owning candidate, or a
    /// durable attempt (foreign to every candidate of this run).
    pub fn inflight(&self, account: &Pubkey) -> Option<Inflight> {
        self.entries.get(account).map(|e| match &e.owned {
            Owned::Candidate { candidate_id, .. } => Inflight::Candidate(*candidate_id),
            Owned::Attempt(_) => Inflight::Attempt,
        })
    }

    pub fn pending(&self, account: &Pubkey) -> Option<PendingRevision> {
        self.entries.get(account).and_then(|e| e.pending)
    }

    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for e in self.entries.values() {
            match &e.owned {
                Owned::Candidate { signed: false, .. } => c.executing += 1,
                Owned::Candidate { signed: true, .. } => c.signed += 1,
                Owned::Attempt(_) => c.submitted += 1,
            }
            if e.pending.is_some() {
                c.pending += 1;
            }
        }
        c
    }

    /// `IDLE → EXECUTING(candidate_id)`: refused while the account is owned by anyone (also by
    /// the same candidate: an account is never executing twice).
    pub fn acquire(&mut self, account: Pubkey, candidate_id: u64) -> Result<(), Refusal> {
        if let Some(e) = self.entries.get(&account) {
            self.stats.refused_acquire = self.stats.refused_acquire.saturating_add(1);
            return Err(Refusal::Held(e.state()));
        }
        self.entries.insert(
            account,
            Entry {
                owned: Owned::Candidate {
                    candidate_id,
                    signed: false,
                },
                pending: None,
            },
        );
        self.stats.acquired = self.stats.acquired.saturating_add(1);
        Ok(())
    }

    /// `EXECUTING(candidate_id) → SIGNED`, by the owning candidate only.
    pub fn signed(&mut self, account: Pubkey, candidate_id: u64) -> Result<(), Refusal> {
        let refused = |stats: &mut OwnershipStats, r| {
            stats.refused_signed = stats.refused_signed.saturating_add(1);
            Err(r)
        };
        let Some(e) = self.entries.get_mut(&account) else {
            return refused(&mut self.stats, Refusal::NotOwned);
        };
        match &mut e.owned {
            Owned::Candidate {
                candidate_id: owner,
                signed,
            } if *owner == candidate_id => {
                if *signed {
                    let state = e.state();
                    return refused(&mut self.stats, Refusal::WrongPhase(state));
                }
                *signed = true;
                self.stats.signed = self.stats.signed.saturating_add(1);
                Ok(())
            }
            _ => {
                let state = e.state();
                refused(&mut self.stats, Refusal::Foreign(state))
            }
        }
    }

    /// `EXECUTING|SIGNED → RELEASED`, by the owning candidate only: the entry is removed and
    /// its pending revision handed back for re-arming. A SUBMITTED entry is never released by
    /// a candidate (only the book's release does), and a foreign candidate never deletes a
    /// newer ownership.
    pub fn release(
        &mut self,
        account: Pubkey,
        candidate_id: u64,
    ) -> Result<Option<PendingRevision>, Refusal> {
        let refused = |stats: &mut OwnershipStats, r| {
            stats.refused_release = stats.refused_release.saturating_add(1);
            Err(r)
        };
        let Some(e) = self.entries.get(&account) else {
            return refused(&mut self.stats, Refusal::NotOwned);
        };
        match &e.owned {
            Owned::Candidate {
                candidate_id: owner,
                ..
            } if *owner == candidate_id => {
                let pending = self.entries.remove(&account).and_then(|e| e.pending);
                self.stats.released = self.stats.released.saturating_add(1);
                if pending.is_some() {
                    self.stats.rearmed = self.stats.rearmed.saturating_add(1);
                }
                Ok(pending)
            }
            _ => {
                let state = e.state();
                refused(&mut self.stats, Refusal::Foreign(state))
            }
        }
    }

    /// A trigger for an owned account: kept as the account's one newest pending revision
    /// (coalesced), re-armed when the ownership is released. Refused for an account nobody owns
    /// (it belongs in the lanes, not here).
    pub fn defer(
        &mut self,
        account: Pubkey,
        revision: PendingRevision,
    ) -> Result<Deferred, Refusal> {
        let Some(e) = self.entries.get_mut(&account) else {
            self.stats.refused_defer = self.stats.refused_defer.saturating_add(1);
            return Err(Refusal::NotOwned);
        };
        match &mut e.pending {
            Some(p) => {
                p.coalesce(revision);
                self.stats.coalesced = self.stats.coalesced.saturating_add(1);
                Ok(Deferred::Coalesced)
            }
            slot @ None => {
                *slot = Some(revision);
                self.stats.deferred = self.stats.deferred.saturating_add(1);
                Ok(Deferred::First)
            }
        }
    }

    /// Strict synchronization with the reservation book's target holds: every held target is
    /// SUBMITTED (a candidate's entry on it yields to the durable attempt, counted); a SUBMITTED
    /// entry whose target the book no longer holds is RELEASED — its pending revision is
    /// returned for re-arming.
    pub fn sync_submitted(
        &mut self,
        held: impl IntoIterator<Item = (Pubkey, Submission)>,
    ) -> Vec<(Pubkey, PendingRevision)> {
        let mut seen: HashMap<Pubkey, ()> = HashMap::new();
        for (account, submission) in held {
            seen.insert(account, ());
            match self.entries.get_mut(&account) {
                Some(Entry {
                    owned: Owned::Attempt(current),
                    ..
                }) => {
                    if *current != submission {
                        *current = submission;
                    }
                }
                Some(e) => {
                    self.stats.submitted_over_candidate =
                        self.stats.submitted_over_candidate.saturating_add(1);
                    self.stats.submitted_synced = self.stats.submitted_synced.saturating_add(1);
                    e.owned = Owned::Attempt(submission);
                }
                None => {
                    self.stats.submitted_synced = self.stats.submitted_synced.saturating_add(1);
                    self.entries.insert(
                        account,
                        Entry {
                            owned: Owned::Attempt(submission),
                            pending: None,
                        },
                    );
                }
            }
        }
        let released: Vec<Pubkey> = self
            .entries
            .iter()
            .filter(|(k, e)| matches!(e.owned, Owned::Attempt(_)) && !seen.contains_key(k))
            .map(|(k, _)| *k)
            .collect();
        let mut rearm = Vec::with_capacity(released.len());
        for account in released {
            self.stats.submitted_released = self.stats.submitted_released.saturating_add(1);
            if let Some(pending) = self.entries.remove(&account).and_then(|e| e.pending) {
                self.stats.rearmed = self.stats.rearmed.saturating_add(1);
                rearm.push((account, pending));
            }
        }
        rearm.sort_by_key(|(k, _)| *k);
        rearm
    }

    /// `key=value` pairs for the owner's `identity_stats` line.
    pub fn line(&self) -> String {
        let c = self.counts();
        let s = self.stats;
        format!(
            "ownership: executing={} signed={} submitted={} pending={} acquired={} signed_total={} released={} rearmed={} deferred={} coalesced={} refused_acquire={} refused_signed={} refused_release={} refused_defer={} submitted_synced={} submitted_over_candidate={} submitted_released={} unparsable_targets={}",
            c.executing,
            c.signed,
            c.submitted,
            c.pending,
            s.acquired,
            s.signed,
            s.released,
            s.rearmed,
            s.deferred,
            s.coalesced,
            s.refused_acquire,
            s.refused_signed,
            s.refused_release,
            s.refused_defer,
            s.submitted_synced,
            s.submitted_over_candidate,
            s.submitted_released,
            s.unparsable_targets,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::identity::{
        result_action, CandidateIdentity, Current, Generations, ResultAction,
    };
    use crate::execution::one_shot::ActiveHold;
    use crate::execution::reservation::WalletReservations;
    use darkbot_common::mt0::TriggerClass;
    use darkbot_solana::state::StateCore;
    use std::collections::BTreeMap;

    fn rev(t_rx_first_ns: u64, t_rx_ns: u64, slot: u64, trigger: &'static str) -> PendingRevision {
        PendingRevision {
            t_rx_first_ns,
            t_rx_ns,
            slot,
            trigger,
        }
    }

    fn submission(attempt: &str) -> Submission {
        Submission {
            attempt: attempt.into(),
            signature: Some(format!("sig-{attempt}")),
            last_valid_block_height: Some(900),
        }
    }

    /// #2: an account is never executing twice — a second acquire is refused (and counted,
    /// never applied) while the account is EXECUTING, SIGNED or SUBMITTED, by the same or by
    /// another candidate; released, it can be acquired again.
    #[test]
    fn test2_an_account_is_never_executing_twice() {
        let a = Pubkey::new_unique();
        let mut t = AccountOwnership::default();
        t.acquire(a, 1).unwrap();
        assert_eq!(
            t.acquire(a, 1),
            Err(Refusal::Held(State::Executing { candidate_id: 1 }))
        );
        assert_eq!(
            t.acquire(a, 2),
            Err(Refusal::Held(State::Executing { candidate_id: 1 }))
        );
        t.signed(a, 1).unwrap();
        assert_eq!(
            t.acquire(a, 2),
            Err(Refusal::Held(State::Signed { candidate_id: 1 }))
        );
        assert_eq!(t.counts().signed, 1);
        assert_eq!(t.release(a, 1), Ok(None));
        assert_eq!(t.state(&a), None);
        t.acquire(a, 2).unwrap();
        assert_eq!(t.inflight(&a), Some(Inflight::Candidate(2)));
        // Another account runs in parallel (#1).
        let b = Pubkey::new_unique();
        t.acquire(b, 3).unwrap();
        assert_eq!(t.counts().executing, 2);
        // SUBMITTED from the book: refused for every candidate.
        t.release(a, 2).unwrap();
        t.sync_submitted([(a, submission("A1"))]);
        assert_eq!(
            t.acquire(a, 4),
            Err(Refusal::Held(State::Submitted(submission("A1"))))
        );
        assert_eq!(t.inflight(&a), Some(Inflight::Attempt));
        assert_eq!(t.stats.refused_acquire, 4);
        assert_eq!(t.stats.acquired, 3);
    }

    /// #3: newest pending wins — triggers for an owned account are kept as its one pending
    /// revision (`t_rx_first` kept, the newest work wins), and the release hands exactly that
    /// revision back for re-arming; nothing is lost and nothing is evaluated twice.
    #[test]
    fn test3_newest_pending_revision_survives_the_ownership() {
        let a = Pubkey::new_unique();
        let mut t = AccountOwnership::default();
        assert_eq!(t.defer(a, rev(1, 1, 1, "x")), Err(Refusal::NotOwned));
        t.acquire(a, 1).unwrap();
        assert_eq!(
            t.defer(a, rev(100, 100, 7, "oracle_threshold")),
            Ok(Deferred::First)
        );
        assert_eq!(
            t.defer(a, rev(300, 300, 9, "account_update")),
            Ok(Deferred::Coalesced)
        );
        // An out-of-order older trigger never wins over the newest work.
        assert_eq!(
            t.defer(a, rev(50, 200, 8, "oracle_threshold")),
            Ok(Deferred::Coalesced)
        );
        assert_eq!(t.counts().pending, 1);
        assert_eq!(
            t.pending(&a),
            Some(rev(50, 300, 9, "account_update")),
            "t_rx_first earliest, newest work"
        );
        assert_eq!(t.release(a, 1), Ok(Some(rev(50, 300, 9, "account_update"))));
        assert_eq!(t.state(&a), None);
        assert_eq!(
            (t.stats.deferred, t.stats.coalesced, t.stats.rearmed),
            (1, 2, 1)
        );
        // The same for a SUBMITTED target: the book's release re-arms the pending revision.
        t.sync_submitted([(a, submission("A1"))]);
        assert_eq!(
            t.defer(a, rev(400, 400, 10, "account_update")),
            Ok(Deferred::First)
        );
        assert!(t.sync_submitted([(a, submission("A1"))]).is_empty());
        assert_eq!(
            t.sync_submitted([]),
            vec![(a, rev(400, 400, 10, "account_update"))]
        );
        assert!(t.is_empty());
    }

    /// #5: an old result never deletes a newer ownership — a release or a sign by a candidate
    /// that does not own the entry is refused and the entry is untouched; the same against a
    /// SUBMITTED attempt; the central policy drops the old candidate's result as foreign.
    #[test]
    fn test5_an_old_result_never_releases_a_newer_ownership() {
        let a = Pubkey::new_unique();
        let mut t = AccountOwnership::default();
        t.acquire(a, 1).unwrap();
        t.release(a, 1).unwrap();
        t.acquire(a, 2).unwrap();
        t.defer(a, rev(5, 5, 5, "account_update")).unwrap();
        assert_eq!(
            t.release(a, 1),
            Err(Refusal::Foreign(State::Executing { candidate_id: 2 }))
        );
        assert_eq!(
            t.signed(a, 1),
            Err(Refusal::Foreign(State::Executing { candidate_id: 2 }))
        );
        assert_eq!(t.state(&a), Some(State::Executing { candidate_id: 2 }));
        assert_eq!(t.pending(&a), Some(rev(5, 5, 5, "account_update")));
        t.signed(a, 2).unwrap();
        assert_eq!(
            t.signed(a, 2),
            Err(Refusal::WrongPhase(State::Signed { candidate_id: 2 }))
        );
        assert_eq!(
            t.release(a, 3),
            Err(Refusal::Foreign(State::Signed { candidate_id: 2 }))
        );
        assert_eq!((t.stats.refused_release, t.stats.refused_signed), (2, 2));
        // Nobody owns it after the owner's release: an old release is NotOwned, not applied.
        assert_eq!(t.release(a, 2), Ok(Some(rev(5, 5, 5, "account_update"))));
        assert_eq!(t.release(a, 1), Err(Refusal::NotOwned));
        // SUBMITTED by a durable attempt: no candidate releases or signs it.
        t.sync_submitted([(a, submission("A1"))]);
        assert_eq!(
            t.release(a, 2),
            Err(Refusal::Foreign(State::Submitted(submission("A1"))))
        );
        assert_eq!(t.state(&a), Some(State::Submitted(submission("A1"))));
        // The policy: the old candidate's result is dropped foreign, never committed.
        let st = StateCore::new(8);
        let identity = CandidateIdentity {
            candidate_id: 1,
            account: a,
            trigger: TriggerClass::AccountUpdate,
            lane: None,
            generations: Generations::default(),
            candidate_revision: 1,
            t_rx_first_ns: 0,
            t_rx_latest_ns: 0,
            trigger_slot: 0,
            evaluation: None,
            valid_until_ns: None,
            read_set: Ok(Default::default()),
        };
        let current = Current {
            generations: Generations::default(),
            now_ns: 0,
            epochs: &st,
            newest_revision: None,
            inflight: t.inflight(&a),
        };
        assert_eq!(
            result_action(&identity, &current),
            ResultAction::DropForeignInflight
        );
    }

    /// #34 / restart: SUBMITTED is the reservation book's target hold. A durable attempt
    /// reopened from the journal (rebuilt book) makes its target SUBMITTED with the bound
    /// signature and `last_valid_block_height`; the book refuses a second attempt on that
    /// target (`RESERVATION_TARGET_CONTENDED`) exactly while the table says SUBMITTED, and both
    /// release together through the book's release — never through a candidate.
    #[test]
    fn test34_submitted_follows_the_reservation_book_and_prevents_a_second_attempt() {
        let target = Pubkey::new_unique();
        let holds: BTreeMap<String, u64> = [("SOL".to_string(), 20u64)].into();
        let balances: BTreeMap<String, u64> = [("SOL".to_string(), 100u64)].into();
        let hold = ActiveHold {
            attempt: "A1".into(),
            target: target.to_string(),
            unknown_outcome: true,
            holds: holds.clone(),
            signature: Some("sig-A1".into()),
            last_valid_block_height: Some(900),
        };
        let mut book = WalletReservations::rebuild([hold.clone()]).unwrap();
        let mut t = AccountOwnership::default();
        let held = || -> Vec<(Pubkey, Submission)> {
            vec![(
                target,
                Submission {
                    attempt: "A1".into(),
                    signature: Some("sig-A1".into()),
                    last_valid_block_height: Some(900),
                },
            )]
        };
        assert!(t.sync_submitted(held()).is_empty());
        assert_eq!(
            t.state(&target),
            Some(State::Submitted(Submission {
                attempt: "A1".into(),
                signature: Some("sig-A1".into()),
                last_valid_block_height: Some(900),
            }))
        );
        let second = ActiveHold {
            attempt: "B".into(),
            ..hold.clone()
        };
        assert_eq!(
            book.reserve_target(&second, &balances),
            Err("RESERVATION_TARGET_CONTENDED")
        );
        assert_eq!(
            t.acquire(target, 9),
            Err(Refusal::Held(t.state(&target).unwrap()))
        );
        // The book's release (the settlement path) releases both; a candidate never does.
        assert_eq!(
            t.release(target, 9).unwrap_err(),
            Refusal::Foreign(t.state(&target).unwrap())
        );
        book.release("A1").unwrap();
        assert!(t.sync_submitted([]).is_empty());
        assert_eq!(t.state(&target), None);
        assert_eq!(book.reserve_target(&second, &balances), Ok(()));
        assert_eq!(t.acquire(target, 9), Ok(()));
        assert_eq!(
            (t.stats.submitted_synced, t.stats.submitted_released),
            (1, 1)
        );
        // A candidate's entry on a target the book then holds yields to the durable attempt.
        t.sync_submitted(held());
        assert_eq!(t.inflight(&target), Some(Inflight::Attempt));
        assert_eq!(t.stats.submitted_over_candidate, 1);
    }

    #[test]
    fn line_names_every_counter() {
        let mut t = AccountOwnership::default();
        let a = Pubkey::new_unique();
        t.acquire(a, 1).unwrap();
        t.signed(a, 1).unwrap();
        assert_eq!(
            t.line(),
            "ownership: executing=0 signed=1 submitted=0 pending=0 acquired=1 signed_total=1 released=0 rearmed=0 deferred=0 coalesced=0 refused_acquire=0 refused_signed=0 refused_release=0 refused_defer=0 submitted_synced=0 submitted_over_candidate=0 submitted_released=0 unparsable_targets=0"
        );
    }
}
