//! Existing owner telemetry at the unchanged event boundary.
use super::*;

impl ShadowOwner {
    pub(super) fn telemetry(&mut self) {
        // [FIX-108][R4] Winner/Reconcile RPC is handled only by bounded workers above.
        // Detection/Pipeline (P2→P5) wird hier eingehängt, sobald HealthEngine (P3) verfügbar ist.
        // Bis dahin: nur State + Zähler, damit S1-LT/S2 messbar sind (Ledger).
        let update_total = self.counts.iter().sum::<u64>();
        self.mt0_owner.enter(darkbot_common::mt0::OwnerCat::Stats);
        if update_total > 0
            && update_total % 1000 == 0
            && update_total != self.last_reported_update_total
        {
            self.last_reported_update_total = update_total;
            owner_log!(
                self,
                "updates: accounts={} banks={} groups={} fee={} liqrec={} unknown={} channel={:?} channel_dropped={}",
                self.counts[0],
                self.counts[1],
                self.counts[2],
                self.counts[3],
                self.counts[4],
                self.counts[5],
                self.consumer.stats(),
                self.consumer.stats().dropped
            );
            // [F1] Winner-watch telemetry. RPC request / 429 counts need the MT1 sender: N/A, not 0.
            let ws = self.winner_tracker.stats();
            owner_log!(self,
                "winner_watch_stats: len={} max={} observed={} deduped={} expired={} overflow={} dispatched={} queue_full_backoffs={} results={} stale_results={} errors={} resolved={} winner_requests=N/A winner_429=N/A",
                ws.len, ws.max, ws.observed, ws.deduped, ws.expired, ws.overflow, ws.dispatched, ws.queue_full_backoffs, ws.results, ws.stale_results, ws.errors, ws.resolved
            );
            #[cfg(feature = "crate_health")]
            let pipeline_timing = pipeline_timing_snapshot();
            #[cfg(feature = "crate_health")]
            owner_log!(self,
                "oracle_trigger_stats: threshold_ready={} tracked={} fallback={} bootstrap_remaining={} dependency_updates={} candidate_hits={} enqueued={} coalesced={} price_fallbacks={} pending={} max_pending={} detect_queue={} bank_projection_pending={} bank_projection_submitted={} bank_projection_ready={} bank_projection_failed={} bank_projection_stale={} ingress_backlog={} ingress_since_detect={} threshold_trigger={} sweep_checks={} sweep_healthy={} sweep_liquidatable={} sweep_errors={} sweep_batches={} sweep_dirty_banks={} sweep_account_refs={} sweep_unique_accounts={} sweep_forwarded={} sweep_max_unique_batch={} sweep_max_dirty_banks={} candidate_cache_entries={} candidate_cache_suppressed={} candidate_cache_softcap_suppressed={} candidate_cache_transition_forwards={} candidate_cache_revision_forwards={} candidate_pending_coalesced={} candidate_revision_coalesced={} candidate_physical_enqueues={} candidate_cache_periodic_forwards={} candidate_cache_forced_forwards={} candidate_repeat_wait_slots_max={} candidate_repeat_wait_slots_1={} candidate_repeat_wait_slots_2_3={} candidate_repeat_wait_slots_4_7={} candidate_repeat_wait_slots_8_15={} candidate_repeat_wait_slots_ge_16={} candidate_fairness_rotations={} candidate_periodic_due_but_deferred={} dependency_content_unchanged={} dependency_content_changed={} detect_health_calls={} detect_health_ns_total={} detect_health_ns_max={} detect_pipeline_calls={} detect_pipeline_ns_total={} detect_pipeline_ns_max={} pipeline_planning_calls={} pipeline_planning_ns_total={} pipeline_planning_ns_max={} pipeline_fee_calls={} pipeline_fee_ns_total={} pipeline_fee_ns_max={} pipeline_simulation_calls={} pipeline_simulation_ns_total={} pipeline_simulation_ns_max={} candidate_recheck_slots={} candidate_error_recheck_slots={} candidate_repeat_soft_cap={}",
                self.threshold_ready,
                self.threshold_index.tracked_accounts(),
                self.threshold_fallback.len(),
                self.threshold_bootstrap.len(),
                self.metrics.oracle_dependency_updates,
                self.metrics.oracle_candidate_hits,
                self.metrics.oracle_enqueued,
                self.metrics.oracle_coalesced,
                self.metrics.oracle_price_fallbacks,
                self.oracle_detect_pending.len(),
                self.metrics.oracle_max_pending,
                self.detect_queue.len(),
                self.oracle_bank_projection_pending.len(),
                self.metrics.oracle_bank_projection_submitted,
                self.metrics.oracle_bank_projection_ready,
                self.metrics.oracle_bank_projection_failed,
                self.metrics.oracle_bank_projection_stale,
                self.consumer.stats().sent.saturating_sub(self.consumer.stats().received),
                self.ingress_since_detect,
                self.use_threshold_trigger,
                self.metrics.health_sweep_checks,
                self.metrics.health_sweep_healthy,
                self.metrics.health_sweep_liquidatable,
                self.metrics.health_sweep_errors,
                self.metrics.sweep_batches,
                self.sweep_dirty_banks.len(),
                self.metrics.sweep_account_refs,
                self.metrics.sweep_unique_accounts,
                self.metrics.sweep_forwarded,
                self.metrics.sweep_max_unique_batch,
                self.metrics.sweep_max_dirty_banks,
                self.candidate_cache.len(),
                self.metrics.candidate_cache_suppressed,
                self.metrics.candidate_cache_softcap_suppressed,
                self.metrics.candidate_cache_transition_forwards,
                self.metrics.candidate_revision_forwards,
                self.metrics.candidate_pending_coalesced,
                self.metrics.candidate_revision_coalesced,
                self.metrics.candidate_physical_enqueues,
                self.metrics.candidate_cache_periodic_forwards,
                self.metrics.candidate_cache_forced_forwards,
                self.metrics.candidate_repeat_wait_slots_max,
                self.metrics.candidate_repeat_wait_slots_1,
                self.metrics.candidate_repeat_wait_slots_2_3,
                self.metrics.candidate_repeat_wait_slots_4_7,
                self.metrics.candidate_repeat_wait_slots_8_15,
                self.metrics.candidate_repeat_wait_slots_ge_16,
                self.metrics.candidate_fairness_rotations,
                self.metrics.candidate_periodic_due_but_deferred,
                self.metrics.dependency_content_unchanged,
                self.metrics.dependency_content_changed,
                self.metrics.detect_health_calls,
                self.metrics.detect_health_ns_total,
                self.metrics.detect_health_ns_max,
                self.metrics.detect_pipeline_calls,
                self.metrics.detect_pipeline_ns_total,
                self.metrics.detect_pipeline_ns_max,
                pipeline_timing.planning_calls,
                pipeline_timing.planning_ns_total,
                pipeline_timing.planning_ns_max,
                pipeline_timing.fee_calls,
                pipeline_timing.fee_ns_total,
                pipeline_timing.fee_ns_max,
                pipeline_timing.simulation_calls,
                pipeline_timing.simulation_ns_total,
                pipeline_timing.simulation_ns_max,
                self.candidate_recheck_slots,
                self.candidate_error_recheck_slots,
                self.candidate_repeat_soft_cap
            );
            // D3: separate line, so the pinned R126/R127 parsers of the line above are unchanged.
            #[cfg(feature = "crate_health")]
            owner_log!(
                self,
                "sweep_timing: batches={} owner_block_ns_total={} owner_block_ns_max={} max_block_accounts={} max_unique_batch={} ingress_capacity={}",
                self.metrics.sweep_batches,
                self.metrics.sweep_batch_ns_total,
                self.metrics.sweep_batch_ns_max,
                self.metrics.sweep_batch_ns_max_accounts,
                self.metrics.sweep_max_unique_batch,
                self.consumer.stats().capacity
            );
        }
    }
}
