//! P6-MT.13 (kind `MT13_TELEMETRY`): the mandatory telemetry the MT0 snapshot does not carry,
//! cumulative since the process started — per RPC budget workload its admission waits, the
//! admissions queued and the calls in flight at each admission, the admissions the budget
//! refused (`remote_queue_depth{class}`, `remote_inflight{class}`, `budget_denied{class}`, every
//! refusal of the budget, a deadline-bound one too); the candidates the budget itself refused
//! per stage and lane (`budget_denied{class,lane}` from the candidate's side: a refusal bound by
//! the candidate's deadline is its `RPC_DEADLINE`, counted in its own loss, not here); the
//! candidates that reached their send boundary while another attempt held the wallet
//! (`reservation_conflicts`); the signed intents in flight (`intents_live`, a gauge); the owner
//! evidence queue's depth and the events it refused full (`evidence_queue_depth`,
//! `evidence_overflow`); the queue age of each candidate stale before planning
//! (`staterace_by_queue_age`). Journaled by the owner every `mt0_interval_seconds` (on its
//! telemetry tick) and at shutdown; the MT0 snapshot's layout stays as it is.
use super::econ::Hist;
use serde::{Deserialize, Serialize};

/// One RPC budget workload's admissions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mt13Admission {
    pub class: String,
    pub wait_ns: Hist,
    pub queue_depth: Hist,
    pub inflight: Hist,
    pub denied: u64,
}

/// P6-MT.13: the record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mt13Telemetry {
    pub admission: Vec<Mt13Admission>,
    /// Per stage (`network`, `fee_quote`, `simulation`) and lane (`L0`–`L2`, `none`).
    pub candidate_budget_denied: Vec<(String, String, u64)>,
    pub reservation_conflicts: u64,
    pub intents_live: u64,
    pub intents_live_max: u64,
    pub evidence_queue_depth: Hist,
    pub evidence_overflow: u64,
    pub staterace_queue_age_ns: Hist,
}

impl crate::analytics::payload::Payload for Mt13Telemetry {
    const KIND: u16 = crate::analytics::journal::kind::MT13_TELEMETRY;
}

impl From<&darkbot_common::mt0::Mt13Snapshot> for Mt13Telemetry {
    fn from(s: &darkbot_common::mt0::Mt13Snapshot) -> Self {
        Self {
            admission: s
                .admission
                .iter()
                .map(|a| Mt13Admission {
                    class: a.class.into(),
                    wait_ns: (&a.wait_ns).into(),
                    queue_depth: (&a.queue_depth).into(),
                    inflight: (&a.inflight).into(),
                    denied: a.denied,
                })
                .collect(),
            candidate_budget_denied: s
                .candidate_budget_denied
                .iter()
                .map(|(stage, lane, n)| (stage.to_string(), lane.to_string(), *n))
                .collect(),
            reservation_conflicts: s.reservation_conflicts,
            intents_live: s.intents_live,
            intents_live_max: s.intents_live_max,
            evidence_queue_depth: (&s.evidence_queue_depth).into(),
            evidence_overflow: s.evidence_overflow,
            staterace_queue_age_ns: (&s.staterace_queue_age_ns).into(),
        }
    }
}

impl Mt13Telemetry {
    /// The registry's now.
    pub fn now() -> Self {
        (&darkbot_common::mt0::MT0.mt13()).into()
    }
}
