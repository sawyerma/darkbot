//! P6-MT.1 (MT5b): the owner's side of the per-account ownership table
//! (`execution::ownership`). The table is mutated on this thread only: `detect` acquires
//! EXECUTING(candidate_id) before anything is evaluated and releases by the owning candidate;
//! SUBMITTED is synchronized from the P8-RES-02 reservation book (the durable FE-04 attempt's
//! target hold) whenever the book changes; a trigger for an owned account is deferred as the
//! account's one newest pending revision and re-armed through the lanes at its release.
use super::*;
use crate::execution::ownership::{Deferred, PendingRevision, Refusal, Submission};

impl ShadowOwner {
    /// Strict synchronization of SUBMITTED with the reservation book: every target the book
    /// holds (reserved, handed off with an UNKNOWN outcome, or an open exit — also reopened
    /// from the journal after a restart) is SUBMITTED with the attempt's bound signature and
    /// `last_valid_block_height`; a target the book no longer holds (released by the FE-04
    /// settlement path) is RELEASED and its pending revision re-armed. Idempotent; called
    /// wherever the book is replaced and once per owner pass.
    pub(super) fn sync_ownership(&mut self) {
        let mut held: Vec<(Pubkey, Submission)> = Vec::new();
        let mut unparsable = 0u64;
        for (target, hold) in self.reservations.targets() {
            match target.parse::<Pubkey>() {
                Ok(account) => held.push((
                    account,
                    Submission {
                        attempt: hold.attempt.clone(),
                        signature: hold.signature.clone(),
                        last_valid_block_height: hold.last_valid_block_height,
                    },
                )),
                Err(_) => unparsable = unparsable.saturating_add(1),
            }
        }
        self.ownership.stats.unparsable_targets = unparsable;
        for (account, pending) in self.ownership.sync_submitted(held) {
            self.rearm(account, pending);
        }
    }

    /// A trigger for an account owned by another candidate or by a durable attempt: not
    /// evaluated, kept as the account's one newest pending revision (`t_rx_first` from the
    /// lanes' origin, the newest work wins) and re-armed at the release. The first deferral of
    /// an ownership period is logged, later ones are coalesced and counted only.
    pub(super) fn defer_owned(
        &mut self,
        (acct, t_rx, slot, trigger): DetectWork,
        origin: Option<generations::DetectOrigin>,
        refusal: &Refusal,
    ) {
        let revision = PendingRevision {
            t_rx_first_ns: origin.map_or(t_rx, |o| o.t_rx_first_ns.min(t_rx)),
            t_rx_ns: t_rx,
            slot,
            trigger,
        };
        match self.ownership.defer(acct, revision) {
            Ok(Deferred::First) => owner_log!(
                self,
                "ownership: deferred account={acct} trigger={trigger} slot={slot} held={refusal:?}"
            ),
            Ok(Deferred::Coalesced) => {}
            Err(e) => owner_log!(
                self,
                "ownership: deferral refused account={acct} trigger={trigger} {e:?} (acquire refused {refusal:?})"
            ),
        }
    }

    /// `EXECUTING|SIGNED → RELEASED` by the owning candidate; its pending revision is re-armed.
    /// A refusal (the entry is not this candidate's) is counted by the table and logged — the
    /// newer ownership stays.
    pub(super) fn release_candidate(&mut self, acct: Pubkey, candidate_id: u64) {
        match self.ownership.release(acct, candidate_id) {
            Ok(Some(pending)) => self.rearm(acct, pending),
            Ok(None) => {}
            Err(e) => owner_log!(
                self,
                "ownership: release refused account={acct} candidate_id={candidate_id} {e:?}"
            ),
        }
    }

    /// Re-arms a released account's pending revision through the lanes at L0 (the forced
    /// re-arm lane): one entry per account — the lanes coalesce it with any pending entry of
    /// the account, the newest work wins there as well and the deferred trigger's first
    /// receive time is kept (P6-MT.5). The revision is unknown to the lanes
    /// (0: it may promote, never demote).
    fn rearm(&mut self, acct: Pubkey, p: PendingRevision) {
        let last_forwarded_slot = self
            .candidate_cache
            .get(&acct)
            .map_or(0, |e| e.last_forwarded_slot);
        enqueue_or_coalesce_candidate_from(
            acct,
            (
                LaneAdmission {
                    t_rx_ns: p.t_rx_ns,
                    slot: p.slot,
                    trigger: p.trigger,
                    lane: Lane::L0,
                    revision: 0,
                    last_forwarded_slot,
                },
                p.t_rx_first_ns,
            ),
            &mut self.oracle_lanes,
            &mut self.metrics.oracle_enqueued,
            &mut self.metrics.oracle_coalesced,
            &mut self.metrics.oracle_max_pending,
        );
    }
}
