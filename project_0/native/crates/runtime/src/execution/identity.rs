//! P6-MT.2 (MT5a): the typed identity of one candidate evaluation, the owner's generations it
//! is bound to, and the one central result policy. Every result of candidate work — today the
//! projection results (`remote_apply`), with the async execution tickets MT7–MT9 every remote
//! and execution result — is judged by [`result_action`] against the owner's current
//! generations, readset epochs, clock, newest revision and in-flight ownership; nothing else
//! decides whether a result may commit. An old result never deletes a newer in-flight entry:
//! the only action that touches an account's in-flight entry is [`ResultAction::Accept`], and a
//! foreign in-flight entry turns every result into [`ResultAction::DropForeignInflight`].
use darkbot_common::mt0::TriggerClass;
use darkbot_project0::readset::{EpochSource, ReadSet, ReadSetOverflow};
use solana_sdk::pubkey::Pubkey;

/// The policy generation of a run. Nothing changes the owner's policy at runtime today (the
/// economics, Safety limits, allowlist and freshness are read once at setup), so it is fixed;
/// a runtime policy change must increment it (test #40 proves the invalidation).
pub const POLICY_GENERATION: u64 = 0;
/// The configuration generation of a run: the same — configuration is read once at setup.
pub const CONFIG_GENERATION: u64 = 0;

/// P6-MT.2: the readset bound of one candidate preparation, derived from the protocol: the
/// fixed keys (candidate, group, FeeState, ProgramData, liquidator wallet, rent sysvar, the
/// liquidation record PDA, the published protocol LUTs) plus, per bank of the candidate's
/// balances and the SOL bank of the cost conversion, the bank's scope keys (bank, liquidity
/// vault, mint, at most four observation keys, at most three integration accounts — the R4
/// `MAX_SCOPE_KEYS_PER_BANK` of the FE qualification) and the liquidator's ATA of its mint.
pub const READ_SET_FIXED_KEYS: usize = 7 + crate::execution::lookup_tables::PUBLISHED_COUNT;
pub const READ_SET_KEYS_PER_BANK: usize = 10 + 1;
pub const READ_SET_BANKS: usize = MAX_LENDING_ACCOUNT_BALANCES + 1;
pub const READ_SET_MAX_KEYS: usize = READ_SET_FIXED_KEYS + READ_SET_BANKS * READ_SET_KEYS_PER_BANK;
/// The pinned program's `MAX_LENDING_ACCOUNT_BALANCES` (asserted against the crate under
/// `crate_health`, where the type crate is available).
const MAX_LENDING_ACCOUNT_BALANCES: usize = 16;

/// P6-MT.5 trigger-priority lane of a candidate (the lanes' behaviour lives in the owner; the
/// type is defined here so every identity names it in every build).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lane {
    /// Transition / forced re-arm (incl. a due P2-04/P2-05 premium or t_profitable wake-up).
    L0 = 0,
    /// Material canonical revision of an already relevant candidate.
    L1 = 1,
    /// Periodic repeat / Error / maintenance.
    L2 = 2,
}

/// The owner-held generations a piece of candidate work is bound to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Generations {
    /// `StateCore::recovery_generation`: ended Ready periods (every recovery starts one).
    pub recovery: u64,
    /// Incremented when the owner latches a ProgramData change.
    pub program: u64,
    pub policy: u64,
    pub config: u64,
}

/// The exact protocol clock a candidate was evaluated at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Evaluation {
    pub slot: u64,
    pub unix_s: u64,
}

/// P6-MT.2: what one candidate evaluation is bound to. Kept in memory only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateIdentity {
    /// Monotone per run, owner-allocated.
    pub candidate_id: u64,
    pub account: Pubkey,
    pub trigger: TriggerClass,
    /// The lane the trigger was served from; `None` for an immediate detection (account
    /// update, bootstrap/integration probe, projection consumption).
    pub lane: Option<Lane>,
    pub generations: Generations,
    /// Monotone per account and run (reset with the candidate cache at a recovery).
    pub candidate_revision: u64,
    /// Receive time of the first coalesced trigger, and of the newest (the FIX-21 origin).
    pub t_rx_first_ns: u64,
    pub t_rx_latest_ns: u64,
    pub trigger_slot: u64,
    /// Set once the candidate was evaluated.
    pub evaluation: Option<Evaluation>,
    /// `t_rx_latest + max_opportunity_age` (FIX-21, unchanged): a result after it is stale.
    /// `None` for intermediate work whose consumer applies the freshness itself.
    pub valid_until_ns: Option<u64>,
    /// Everything the preparation read, or the overflow (no partial readset).
    pub read_set: Result<ReadSet, ReadSetOverflow>,
}

/// The plan's result actions (P6-MT.2), decided only by [`result_action`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultAction {
    Accept,
    DropOldGeneration,
    DropForeignInflight,
    RetryNewestPending,
    RecheckCurrent,
    Drop,
}

pub const RESULT_ACTIONS: [ResultAction; 6] = [
    ResultAction::Accept,
    ResultAction::DropOldGeneration,
    ResultAction::DropForeignInflight,
    ResultAction::RetryNewestPending,
    ResultAction::RecheckCurrent,
    ResultAction::Drop,
];

impl ResultAction {
    pub fn name(self) -> &'static str {
        match self {
            ResultAction::Accept => "accept",
            ResultAction::DropOldGeneration => "drop_old_generation",
            ResultAction::DropForeignInflight => "drop_foreign_inflight",
            ResultAction::RetryNewestPending => "retry_newest_pending",
            ResultAction::RecheckCurrent => "recheck_current",
            ResultAction::Drop => "drop",
        }
    }
    fn idx(self) -> usize {
        match self {
            ResultAction::Accept => 0,
            ResultAction::DropOldGeneration => 1,
            ResultAction::DropForeignInflight => 2,
            ResultAction::RetryNewestPending => 3,
            ResultAction::RecheckCurrent => 4,
            ResultAction::Drop => 5,
        }
    }
    /// The only action that may commit a result and touch the account's in-flight entry
    /// (F-14: the tests' predicate; the owner matches `Accept` itself).
    #[cfg(test)]
    pub fn commits(self) -> bool {
        matches!(self, ResultAction::Accept)
    }
    /// The result produces no follow-up work (P6-MT.2: old results create none).
    pub fn is_drop(self) -> bool {
        matches!(
            self,
            ResultAction::DropOldGeneration
                | ResultAction::DropForeignInflight
                | ResultAction::Drop
        )
    }
}

/// The account's in-flight execution entry (`execution::ownership` owns its lifecycle, MT5b;
/// the policy only reads it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inflight {
    /// EXECUTING/SIGNED by this candidate of the run.
    Candidate(u64),
    /// SUBMITTED by a durable FE-04 attempt (the reservation book's target hold, also one
    /// reopened from the journal after a restart): foreign to every candidate of this run.
    Attempt,
}

/// The owner's current view a result is judged against.
pub struct Current<'a> {
    pub generations: Generations,
    pub now_ns: u64,
    pub epochs: &'a dyn EpochSource,
    /// The newest revision of the account known to the owner (pending or dispatched); `None`
    /// when the owner tracks none for this result's path.
    pub newest_revision: Option<u64>,
    /// The account's in-flight entry; `None` when none exists (or the path tracks none).
    pub inflight: Option<Inflight>,
}

/// The one central result policy. In order: an older generation of any kind is dropped; a
/// foreign in-flight entry drops the result (and is never touched); an incomplete readset or a
/// result past its validity is dropped; an older revision than the newest known yields to it;
/// a changed readset means the result is for another state (re-check from the current one);
/// otherwise the result is accepted.
pub fn result_action(identity: &CandidateIdentity, current: &Current<'_>) -> ResultAction {
    if identity.generations != current.generations {
        return ResultAction::DropOldGeneration;
    }
    if let Some(inflight) = current.inflight {
        if inflight != Inflight::Candidate(identity.candidate_id) {
            return ResultAction::DropForeignInflight;
        }
    }
    let Ok(read_set) = &identity.read_set else {
        return ResultAction::Drop;
    };
    if identity
        .valid_until_ns
        .is_some_and(|until| current.now_ns > until)
    {
        return ResultAction::Drop;
    }
    if current
        .newest_revision
        .is_some_and(|newest| newest > identity.candidate_revision)
    {
        return ResultAction::RetryNewestPending;
    }
    if read_set.changed_key(current.epochs).is_some() {
        return ResultAction::RecheckCurrent;
    }
    ResultAction::Accept
}

/// P6-MT.13: result-action counters (owner-side only; no persisted MT0 layout change).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResultActionCounts {
    counts: [u64; 6],
}

impl ResultActionCounts {
    pub fn count(&mut self, action: ResultAction) {
        let c = &mut self.counts[action.idx()];
        *c = c.saturating_add(1);
    }
    pub fn get(&self, action: ResultAction) -> u64 {
        self.counts[action.idx()]
    }
    pub fn total(&self) -> u64 {
        self.counts.iter().sum()
    }
    /// `name=count` pairs in the plan's order.
    pub fn line(&self) -> String {
        RESULT_ACTIONS
            .iter()
            .map(|a| format!("{}={}", a.name(), self.get(*a)))
            .collect::<Vec<_>>()
            .join(" ")
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

    fn gens(st: &StateCore) -> Generations {
        Generations {
            recovery: st.recovery_generation(),
            program: 0,
            policy: POLICY_GENERATION,
            config: CONFIG_GENERATION,
        }
    }

    fn identity(
        st: &StateCore,
        account: Pubkey,
        keys: &[Pubkey],
        id: u64,
        rev: u64,
    ) -> CandidateIdentity {
        CandidateIdentity {
            candidate_id: id,
            account,
            trigger: TriggerClass::AccountUpdate,
            lane: Some(Lane::L0),
            generations: gens(st),
            candidate_revision: rev,
            t_rx_first_ns: 100,
            t_rx_latest_ns: 100,
            trigger_slot: 5,
            evaluation: Some(Evaluation {
                slot: 5,
                unix_s: 1_700_000_000,
            }),
            valid_until_ns: Some(1_100),
            read_set: ReadSet::observe_keys(keys.iter().copied(), st, READ_SET_MAX_KEYS),
        }
    }

    fn current(st: &StateCore) -> Current<'_> {
        Current {
            generations: gens(st),
            now_ns: 200,
            epochs: st,
            newest_revision: None,
            inflight: None,
        }
    }

    /// Baseline: an unchanged state, generation, clock and ownership accepts.
    #[test]
    fn unchanged_everything_accepts() {
        let mut st = StateCore::new(8);
        let (a, dep, absent) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        ready(&mut st, 5, vec![upd(a, 5, 1), upd(dep, 5, 1)]);
        let id = identity(&st, a, &[a, dep, absent], 1, 1);
        assert_eq!(result_action(&id, &current(&st)), ResultAction::Accept);
        assert!(ResultAction::Accept.commits());
    }

    /// #5: an old result never deletes a newer in-flight entry — against a foreign in-flight
    /// entry the action is `DropForeignInflight`, which neither commits nor touches it; only
    /// the owner of the entry can accept.
    #[test]
    fn test5_old_result_never_touches_a_newer_inflight_entry() {
        let mut st = StateCore::new(8);
        let a = Pubkey::new_unique();
        ready(&mut st, 5, vec![upd(a, 5, 1)]);
        let old = identity(&st, a, &[a], 1, 1);
        let newer = identity(&st, a, &[a], 2, 2);
        let mut cur = current(&st);
        cur.inflight = Some(Inflight::Candidate(2));
        let action = result_action(&old, &cur);
        assert_eq!(action, ResultAction::DropForeignInflight);
        assert!(!action.commits() && action.is_drop());
        assert_eq!(result_action(&newer, &cur), ResultAction::Accept);
        // Even a result that would otherwise be re-checked or retried is dropped foreign first.
        cur.newest_revision = Some(3);
        assert_eq!(result_action(&old, &cur), ResultAction::DropForeignInflight);
        // A durable attempt (SUBMITTED from the book) is foreign to every candidate.
        cur.inflight = Some(Inflight::Attempt);
        assert_eq!(
            result_action(&newer, &cur),
            ResultAction::DropForeignInflight
        );
    }

    /// #6/#7: a recovery (the Ready period ended by a gap) invalidates older work; a projection
    /// result of the older recovery generation cannot commit, whatever else holds.
    #[test]
    fn test6_7_old_recovery_generation_is_dropped() {
        let mut st = StateCore::new(8);
        let a = Pubkey::new_unique();
        ready(&mut st, 5, vec![upd(a, 5, 1)]);
        let id = identity(&st, a, &[a], 1, 1);
        assert_eq!(result_action(&id, &current(&st)), ResultAction::Accept);
        st.on_event(IngressEvent::Gap {
            source: SourceKind::WsRpcControl,
            from_slot: Some(5),
            to_slot: None,
            t_rx_ns: 0,
        });
        let action = result_action(&id, &current(&st));
        assert_eq!(action, ResultAction::DropOldGeneration);
        assert!(action.is_drop());
        // After the recovery the state is byte-identical, still the generation decides.
        ready(&mut st, 6, vec![upd(a, 5, 1)]);
        assert_eq!(
            result_action(&id, &current(&st)),
            ResultAction::DropOldGeneration
        );
        // A program generation change does the same.
        let id = identity(&st, a, &[a], 2, 2);
        let mut cur = current(&st);
        assert_eq!(result_action(&id, &cur), ResultAction::Accept);
        cur.generations.program += 1;
        assert_eq!(result_action(&id, &cur), ResultAction::DropOldGeneration);
    }

    /// #8: a changed readset cannot commit — a write to any read key, including an unrelated
    /// unchanged-bytes re-write, yields `RecheckCurrent`; a write to a key not read changes
    /// nothing.
    #[test]
    fn test8_changed_readset_rechecks_current() {
        let mut st = StateCore::new(8);
        let (a, dep, other) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        ready(
            &mut st,
            5,
            vec![upd(a, 5, 1), upd(dep, 5, 1), upd(other, 5, 1)],
        );
        let id = identity(&st, a, &[a, dep], 1, 1);
        st.on_event(IngressEvent::Account(upd(other, 6, 2)));
        assert_eq!(result_action(&id, &current(&st)), ResultAction::Accept);
        st.on_event(IngressEvent::Account(upd(dep, 6, 1)));
        assert_eq!(
            result_action(&id, &current(&st)),
            ResultAction::RecheckCurrent
        );
    }

    /// #9: an `ABSENT` entry is invalidated when the key is created; a present entry when the
    /// key is removed (fork orphaning of its slot).
    #[test]
    fn test9_absent_invalidated_by_creation_and_present_by_removal() {
        let mut st = StateCore::new(8);
        let (a, absent) = (Pubkey::new_unique(), Pubkey::new_unique());
        ready(&mut st, 5, vec![upd(a, 5, 1)]);
        let id = identity(&st, a, &[a, absent], 1, 1);
        assert_eq!(result_action(&id, &current(&st)), ResultAction::Accept);
        st.on_event(IngressEvent::Account(upd(absent, 6, 1)));
        assert_eq!(
            result_action(&id, &current(&st)),
            ResultAction::RecheckCurrent
        );
        // Present key removed: a fresh identity over `a` only, then `a`'s slot is orphaned.
        let mut st = StateCore::new(8);
        ready(&mut st, 5, vec![]);
        st.on_event(IngressEvent::Account(upd(a, 12, 1)));
        let id = identity(&st, a, &[a], 1, 1);
        assert_eq!(result_action(&id, &current(&st)), ResultAction::Accept);
        for (slot, parent) in [(12, 11), (13, 11)] {
            st.on_event(IngressEvent::Slot(darkbot_solana::ingress::SlotUpdate {
                slot,
                parent: Some(parent),
                status: darkbot_solana::ingress::SlotStatus::Processed,
                source: SourceKind::WsRpcControl,
                t_rx_ns: 0,
            }));
        }
        st.on_event(IngressEvent::Root {
            slot: 13,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        });
        assert!(st.get(&a).is_none());
        // The orphaning halted the state (a new generation) — the generation check comes first;
        // the readset alone, judged at the identity's generation, is changed as well.
        assert_eq!(
            result_action(&id, &current(&st)),
            ResultAction::DropOldGeneration
        );
        let mut cur = current(&st);
        cur.generations = id.generations;
        assert_eq!(result_action(&id, &cur), ResultAction::RecheckCurrent);
    }

    /// #13: the completion order of two results for one account never changes the business
    /// commit: the newest revision is accepted and the older yields to it (or is dropped
    /// foreign), in either order, ending in the same committed set.
    #[test]
    fn test13_completion_order_does_not_change_the_commit() {
        let mut st = StateCore::new(8);
        let a = Pubkey::new_unique();
        ready(&mut st, 5, vec![upd(a, 5, 1)]);
        let r1 = identity(&st, a, &[a], 1, 1);
        let r2 = identity(&st, a, &[a], 2, 2);
        let run = |order: [&CandidateIdentity; 2], inflight: Option<Inflight>| {
            let mut committed: Vec<u64> = vec![];
            let mut actions = vec![];
            for r in order {
                let cur = Current {
                    generations: gens(&st),
                    now_ns: 200,
                    epochs: &st,
                    newest_revision: Some(2),
                    inflight,
                };
                let action = result_action(r, &cur);
                if action.commits() {
                    committed.push(r.candidate_id);
                }
                actions.push((r.candidate_id, action));
            }
            committed.sort_unstable();
            (committed, actions)
        };
        let (c12, a12) = run([&r1, &r2], None);
        let (c21, a21) = run([&r2, &r1], None);
        assert_eq!(c12, vec![2]);
        assert_eq!(c12, c21);
        assert!(a12.contains(&(1, ResultAction::RetryNewestPending)));
        assert!(a21.contains(&(1, ResultAction::RetryNewestPending)));
        let own = Some(Inflight::Candidate(2));
        assert_eq!(run([&r1, &r2], own).0, vec![2]);
        assert_eq!(run([&r2, &r1], own).0, vec![2]);
        assert!(run([&r1, &r2], own)
            .1
            .contains(&(1, ResultAction::DropForeignInflight)));
    }

    /// #33: `valid_until` (FIX-21 `t_rx + max_opportunity_age`, unchanged) invalidates a late
    /// result exactly when the clock passes it; intermediate work without a deadline is not
    /// time-bound by the policy.
    #[test]
    fn test33_valid_until_drops_a_late_result() {
        let mut st = StateCore::new(8);
        let a = Pubkey::new_unique();
        ready(&mut st, 5, vec![upd(a, 5, 1)]);
        let id = identity(&st, a, &[a], 1, 1);
        let mut cur = current(&st);
        cur.now_ns = 1_100;
        assert_eq!(
            result_action(&id, &cur),
            ResultAction::Accept,
            "at the bound"
        );
        cur.now_ns = 1_101;
        assert_eq!(result_action(&id, &cur), ResultAction::Drop);
        let mut unbound = id.clone();
        unbound.valid_until_ns = None;
        assert_eq!(result_action(&unbound, &cur), ResultAction::Accept);
        // An incomplete readset is dropped as well (fail closed).
        let mut overflow = id.clone();
        overflow.read_set = Err(ReadSetOverflow { bound: 1, reads: 2 });
        cur.now_ns = 200;
        assert_eq!(result_action(&overflow, &cur), ResultAction::Drop);
    }

    /// #40: a policy or configuration generation change invalidates an older candidate.
    #[test]
    fn test40_policy_and_config_generation_invalidate() {
        let mut st = StateCore::new(8);
        let a = Pubkey::new_unique();
        ready(&mut st, 5, vec![upd(a, 5, 1)]);
        let id = identity(&st, a, &[a], 1, 1);
        let mut cur = current(&st);
        cur.generations.policy += 1;
        assert_eq!(result_action(&id, &cur), ResultAction::DropOldGeneration);
        let mut cur = current(&st);
        cur.generations.config += 1;
        assert_eq!(result_action(&id, &cur), ResultAction::DropOldGeneration);
    }

    #[test]
    fn counters_count_every_action() {
        let mut c = ResultActionCounts::default();
        for a in RESULT_ACTIONS {
            c.count(a);
        }
        c.count(ResultAction::RecheckCurrent);
        assert_eq!(c.total(), 7);
        assert_eq!(c.get(ResultAction::RecheckCurrent), 2);
        assert_eq!(
            c.line(),
            "accept=1 drop_old_generation=1 drop_foreign_inflight=1 retry_newest_pending=1 recheck_current=2 drop=1"
        );
    }

    /// The readset bound is the protocol-derived one.
    #[cfg(feature = "crate_health")]
    #[test]
    fn readset_bound_follows_the_pinned_protocol() {
        assert_eq!(
            MAX_LENDING_ACCOUNT_BALANCES,
            marginfi_type_crate::types::MAX_LENDING_ACCOUNT_BALANCES
        );
        assert_eq!(
            READ_SET_KEYS_PER_BANK as u64,
            crate::orchestration::fe_qualification::MAX_SCOPE_KEYS_PER_BANK + 1
        );
        assert_eq!(READ_SET_MAX_KEYS, 12 + 17 * 11);
    }
}
