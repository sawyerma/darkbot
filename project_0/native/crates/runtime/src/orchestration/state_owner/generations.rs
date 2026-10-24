//! P6-MT.2 (MT5a): the owner's generations, candidate identity allocation and the central
//! result policy's application on the owner thread.
use super::*;
use crate::execution::identity::{
    result_action, CandidateIdentity, Current, Generations, Lane, ResultAction, CONFIG_GENERATION,
    POLICY_GENERATION,
};

/// Lane and first receive time of a detection selected from the lanes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DetectOrigin {
    pub(super) lane: Lane,
    pub(super) t_rx_first_ns: u64,
}

impl ShadowOwner {
    /// The owner's current generations: the recovery generation is the canonical state's
    /// (ended Ready periods), the program generation the owner's upgrade latch count, policy
    /// and configuration are fixed for the run.
    pub(super) fn generations(&self) -> Generations {
        Generations {
            recovery: self.st.recovery_generation(),
            program: self.program_generation,
            policy: POLICY_GENERATION,
            config: CONFIG_GENERATION,
        }
    }

    /// The one latch of a ProgramData change: the first latch of a run increments the program
    /// generation (every older candidate work is then dropped by the policy).
    pub(super) fn latch_program_upgrade(&mut self) {
        if !self.program_upgrade_latched {
            self.program_upgrade_latched = true;
            self.program_generation = self.program_generation.saturating_add(1);
        }
    }

    /// Allocates the identity of a detection: a fresh candidate id, the account's next
    /// revision, the trigger class, the lane and `t_rx_first` when it came from the lanes (else
    /// an immediate detection: `t_rx_first = t_rx`), and the owner's generations now. The
    /// evaluation point, validity and readset are completed by the detection.
    pub(super) fn begin_candidate(
        &mut self,
        account: Pubkey,
        trigger: &'static str,
        origin: Option<DetectOrigin>,
        t_rx_ns: u64,
        trigger_slot: u64,
    ) -> CandidateIdentity {
        let candidate_id = self.next_candidate_id;
        self.next_candidate_id = self.next_candidate_id.saturating_add(1);
        let generations = self.generations();
        let revision = self.candidate_revisions.entry(account).or_insert(0);
        *revision = revision.saturating_add(1);
        CandidateIdentity {
            candidate_id,
            account,
            trigger: darkbot_common::mt0::TriggerClass::from_trigger(trigger),
            lane: origin.map(|o| o.lane),
            generations,
            candidate_revision: *revision,
            t_rx_first_ns: origin.map_or(t_rx_ns, |o| o.t_rx_first_ns.min(t_rx_ns)),
            t_rx_latest_ns: t_rx_ns,
            trigger_slot,
            evaluation: None,
            valid_until_ns: None,
            read_set: Ok(Default::default()),
        }
    }

    /// The central policy's verdict on a result against the owner now, counted. The newest
    /// revision and the in-flight entry are not consulted on any path yet: the projection
    /// result is coalesced into the account's pending detection as before (MT5b owns the
    /// in-flight lifecycle, MT7–MT9 the async results that need both).
    pub(super) fn judge(&mut self, identity: &CandidateIdentity) -> ResultAction {
        let action = {
            let current = Current {
                generations: self.generations(),
                now_ns: self.clock.now_ns(),
                epochs: &self.st,
                newest_revision: None,
                inflight: None,
            };
            result_action(identity, &current)
        };
        self.metrics.result_actions.count(action);
        action
    }

    /// Telemetry of a completed detection's readset: its size, or the overflow (counted, the
    /// first one reported — the bound is then wrong for this protocol state, never a per-line
    /// flood of the evidence queue).
    pub(super) fn note_readset(&mut self, identity: &CandidateIdentity) {
        match &identity.read_set {
            Ok(set) => self.metrics.readset_keys.record(set.len() as u64),
            Err(overflow) => {
                self.metrics.readset_overflows = self.metrics.readset_overflows.saturating_add(1);
                if !self.metrics.readset_overflow_reported {
                    self.metrics.readset_overflow_reported = true;
                    owner_log!(
                        self,
                        "readset_overflow: account={} candidate_id={} {overflow}",
                        identity.account,
                        identity.candidate_id
                    );
                }
            }
        }
    }
}
