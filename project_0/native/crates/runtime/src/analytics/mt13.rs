//! P6-MT.13 (kind `MT13_TELEMETRY`): the mandatory telemetry the MT0 snapshot does not carry,
//! cumulative since the process started — per RPC budget workload its admission waits, the
//! admissions queued and the calls in flight at each admission, the admissions the budget
//! refused (`remote_queue_depth{class}`, `remote_inflight{class}`, `budget_denied{class}`); the
//! candidates a budget refused per stage and lane (`budget_denied{class,lane}` from the
//! candidate's side); the candidates refused because another attempt held the wallet
//! (`reservation_conflicts`); the signed intents in flight (`intents_live`); the owner evidence
//! queue's depth (`evidence_queue_depth`). Journaled by the owner on its telemetry tick when it
//! changed and at shutdown; the MT0 snapshot's layout stays as it is.
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
    pub intents_live: Hist,
    pub evidence_queue_depth: Hist,
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
            intents_live: (&s.intents_live).into(),
            evidence_queue_depth: (&s.evidence_queue_depth).into(),
        }
    }
}

impl Mt13Telemetry {
    /// The registry's now.
    pub fn now() -> Self {
        (&darkbot_common::mt0::MT0.mt13()).into()
    }
}
