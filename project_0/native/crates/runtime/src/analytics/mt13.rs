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
//! (`staterace_by_queue_age`); and the owner's own (`owner`): per lane the wait from the first
//! receive to dispatch (`lane_wait_ns{L0,L1,L2}`), the lane changes of pending entries
//! (`lane_promotions/demotions/cancellations`), the route-cap hint cache's hits and misses and
//! the Jupiter requests of a routed evaluation (`jupiter_requests_per_routed_revision`).
//! Journaled by the owner every `mt0_interval_seconds` (on its telemetry tick) and at shutdown;
//! the MT0 snapshot's layout stays as it is.
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

/// P6-MT.13: the telemetry the owner keeps itself (MT3 lanes, MT4 route-cap hints).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mt13Owner {
    /// Per lane (`L0`–`L2`): `t_rx_first` → dispatch of an entry.
    pub lane_wait_ns: Vec<(String, Hist)>,
    pub lane_promotions: u64,
    pub lane_demotions: u64,
    /// No cancellation rule exists (P6-MT.5 allows a newer revision to cancel): 0.
    pub lane_cancellations: u64,
    pub route_cap_hits: u64,
    /// Re-probes included.
    pub route_cap_misses: u64,
    /// Jupiter requests of one evaluation that made at least one.
    pub jupiter_requests_per_routed_revision: Hist,
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
    /// `None`: a schema-2 record (written before the owner's telemetry was part of it).
    pub owner: Option<Mt13Owner>,
}

impl crate::analytics::payload::Payload for Mt13Telemetry {
    const KIND: u16 = crate::analytics::journal::kind::MT13_TELEMETRY;
    /// 3: the owner's telemetry.
    const SCHEMA: u16 = 3;
    /// Schema 2 (the layout of `7aa75c94`) had no owner's telemetry: readable without it. The
    /// first schema-2 writer (`1022f091`, pushed with `7aa75c94` and never run: no journal holds
    /// it) wrote another layout under the same schema; such a frame fails closed
    /// (`PAYLOAD_DECODE`).
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<Self>> {
        (schema == crate::analytics::payload::TYPED).then(|| {
            crate::analytics::payload::decode_wire::<Mt13TelemetryV2>(bytes).map(|v| Self {
                admission: v.admission,
                candidate_budget_denied: v.candidate_budget_denied,
                reservation_conflicts: v.reservation_conflicts,
                intents_live: v.intents_live,
                intents_live_max: v.intents_live_max,
                evidence_queue_depth: v.evidence_queue_depth,
                evidence_overflow: v.evidence_overflow,
                staterace_queue_age_ns: v.staterace_queue_age_ns,
                owner: None,
            })
        })
    }
}

/// `Mt13Telemetry` schema 2 (`7aa75c94`).
#[derive(Serialize, Deserialize)]
pub(crate) struct Mt13TelemetryV2 {
    pub(crate) admission: Vec<Mt13Admission>,
    pub(crate) candidate_budget_denied: Vec<(String, String, u64)>,
    pub(crate) reservation_conflicts: u64,
    pub(crate) intents_live: u64,
    pub(crate) intents_live_max: u64,
    pub(crate) evidence_queue_depth: Hist,
    pub(crate) evidence_overflow: u64,
    pub(crate) staterace_queue_age_ns: Hist,
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
            owner: None,
        }
    }
}

impl Mt13Telemetry {
    /// The registry's now, with the owner's telemetry `owner`.
    pub fn now(owner: Option<Mt13Owner>) -> Self {
        Self {
            owner,
            ..(&darkbot_common::mt0::MT0.mt13()).into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::payload::{decode, encode, encode_wire, Payload};

    /// P6-MT.13: the record round-trips with the owner's telemetry, and a schema-2 record (no
    /// owner's telemetry) stays readable — without it, never as zeros.
    #[test]
    fn the_mt13_record_carries_the_owner_and_reads_its_earlier_layout() {
        let hist = |count| Hist {
            count,
            p50: Some(1),
            p95: Some(2),
            p99: Some(3),
            max: Some(4),
        };
        let mut record = Mt13Telemetry::now(Some(Mt13Owner {
            lane_wait_ns: vec![("L0".into(), hist(3)), ("L2".into(), hist(1))],
            lane_promotions: 2,
            lane_demotions: 1,
            lane_cancellations: 0,
            route_cap_hits: 5,
            route_cap_misses: 4,
            jupiter_requests_per_routed_revision: hist(9),
        }));
        let bytes = encode(&record).unwrap();
        assert_eq!(
            decode::<Mt13Telemetry>(Mt13Telemetry::KIND, Mt13Telemetry::SCHEMA, &bytes).unwrap(),
            record
        );
        let v2 = Mt13TelemetryV2 {
            admission: record.admission.clone(),
            candidate_budget_denied: record.candidate_budget_denied.clone(),
            reservation_conflicts: record.reservation_conflicts,
            intents_live: record.intents_live,
            intents_live_max: record.intents_live_max,
            evidence_queue_depth: record.evidence_queue_depth.clone(),
            evidence_overflow: record.evidence_overflow,
            staterace_queue_age_ns: record.staterace_queue_age_ns.clone(),
        };
        let earlier = encode_wire(&v2).unwrap();
        record.owner = None;
        assert_eq!(
            decode::<Mt13Telemetry>(Mt13Telemetry::KIND, 2, &earlier).unwrap(),
            record
        );
        assert!(decode::<Mt13Telemetry>(Mt13Telemetry::KIND, 3, &earlier).is_err());
    }
}
