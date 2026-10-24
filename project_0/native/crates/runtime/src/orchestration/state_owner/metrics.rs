//! Existing counters, owned and updated by the same state-owner thread.
#[derive(Default)]
pub(super) struct OwnerMetrics {
    pub(super) oracle_dependency_updates: u64,
    pub(super) oracle_candidate_hits: u64,
    pub(super) oracle_enqueued: u64,
    pub(super) oracle_coalesced: u64,
    pub(super) oracle_price_fallbacks: u64,
    pub(super) oracle_max_pending: usize,
    pub(super) oracle_bank_projection_submitted: u64,
    pub(super) oracle_bank_projection_ready: u64,
    pub(super) oracle_bank_projection_failed: u64,
    pub(super) oracle_bank_projection_stale: u64,
    pub(super) health_sweep_checks: u64,
    pub(super) health_sweep_healthy: u64,
    pub(super) health_sweep_liquidatable: u64,
    pub(super) health_sweep_errors: u64,
    pub(super) sweep_batches: u64,
    pub(super) sweep_account_refs: u64,
    pub(super) sweep_unique_accounts: u64,
    pub(super) sweep_forwarded: u64,
    pub(super) sweep_max_unique_batch: usize,
    pub(super) sweep_max_dirty_banks: usize,
    pub(super) candidate_revision_forwards: u64,
    pub(super) candidate_pending_coalesced: u64,
    pub(super) candidate_revision_coalesced: u64,
    pub(super) candidate_physical_enqueues: u64,
    pub(super) candidate_cache_suppressed: u64,
    pub(super) candidate_cache_softcap_suppressed: u64,
    pub(super) candidate_cache_transition_forwards: u64,
    pub(super) candidate_cache_periodic_forwards: u64,
    pub(super) candidate_cache_forced_forwards: u64,
    pub(super) candidate_repeat_wait_slots_max: u64,
    pub(super) candidate_repeat_wait_slots_1: u64,
    pub(super) candidate_repeat_wait_slots_2_3: u64,
    pub(super) candidate_repeat_wait_slots_4_7: u64,
    pub(super) candidate_repeat_wait_slots_8_15: u64,
    pub(super) candidate_repeat_wait_slots_ge_16: u64,
    pub(super) candidate_fairness_rotations: u64,
    pub(super) candidate_periodic_due_but_deferred: u64,
    pub(super) dependency_content_unchanged: u64,
    pub(super) dependency_content_changed: u64,
    pub(super) detect_health_calls: u64,
    pub(super) detect_health_ns_total: u64,
    pub(super) detect_health_ns_max: u64,
    pub(super) detect_pipeline_calls: u64,
    pub(super) detect_pipeline_ns_total: u64,
    pub(super) detect_pipeline_ns_max: u64,
    /// D3: owner time per sweep batch (ingress is not served meanwhile): total, maximum and the
    /// unique accounts of the slowest batch.
    pub(super) sweep_batch_ns_total: u64,
    pub(super) sweep_batch_ns_max: u64,
    pub(super) sweep_batch_ns_max_accounts: usize,
    /// V13: bootstrap probes scheduled (Z14); taken for detection = scheduled − still queued.
    pub(super) bootstrap_probes_queued: usize,
    /// A23-01: doorbell → owner running again, per wakeup by a notification (not by the tick).
    pub(super) owner_wakeup_ns: darkbot_common::mt0::Hist,
    /// P6-MT.13 `lane_wait_ns{L0,L1,L2}` (MT3): `t_rx_first` → dispatch of a lane entry, as
    /// count/p50/p95/p99/max per lane. Owner-side only; the persisted MT0 layout is unchanged.
    #[cfg(feature = "crate_health")]
    pub(super) lane_wait_ns: [darkbot_common::mt0::Hist; super::LANES],
    /// P6-MT.13 `jupiter_requests_per_routed_revision` (MT4): Jupiter requests of one
    /// candidate evaluation that made at least one, as count/p50/p95/p99/max. Owner-side only;
    /// the persisted MT0 layout is unchanged.
    #[cfg(feature = "crate_health")]
    pub(super) jupiter_requests_per_routed_revision: darkbot_common::mt0::Hist,
    /// P6-MT.2/MT.13 (MT5a): distinct keys of a detection's readset, as count/p50/p95/p99/max;
    /// detections whose readset exceeded its bound (no identity: fail closed), reported once.
    /// Owner-side only; the persisted MT0 layout is unchanged.
    pub(super) readset_keys: darkbot_common::mt0::Hist,
    pub(super) readset_overflows: u64,
    pub(super) readset_overflow_reported: bool,
    /// P6-MT.2/MT.13 (MT5a): the central result policy's verdicts (every judged result).
    pub(super) result_actions: crate::execution::identity::ResultActionCounts,
}
