//! Ordered evidence flush and worker shutdown.
use super::*;
impl ShadowOwner {
    pub(super) fn finish(mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(h) = &self.econ {
            let ws = self.winner_tracker.stats();
            h.shared.submission_attempts.store(
                self.submitter
                    .attempts
                    .load(std::sync::atomic::Ordering::Relaxed),
                std::sync::atomic::Ordering::Relaxed,
            );
            let winner_jobs = self.remote.winner_jobs.stats();
            let projection_jobs = self.remote.projection_jobs.stats();
            h.emit(crate::analytics::econ::Evidence::ShadowStats(
                crate::analytics::econ::ShadowStats {
                    winner_watch_current: ws.len as u64,
                    winner_watch_max: ws.max as u64,
                    winner_watch_overflow: ws.overflow,
                    winner_watch_errors: ws.errors,
                    winner_watch_resolved: ws.resolved,
                    winner_watch_expired: ws.expired,
                    winner_queue_full_backoffs: ws.queue_full_backoffs,
                    ingress_overflow: self.consumer.stats().overflow,
                    winner_pending_at_deadline: ws.len as u64,
                    shadow_state_ready_at_shutdown: self.st.run_state().is_tradable(),
                    remote_winner_queue_depth: winner_jobs
                        .sent
                        .saturating_sub(winner_jobs.received),
                    remote_projection_queue_depth: projection_jobs
                        .sent
                        .saturating_sub(projection_jobs.received),
                },
            ));
            self.shutdown.request();
            self.ingress_session.request();
        }
        if self.operational_window.is_some() {
            self.shutdown.request();
            self.ingress_session.request();
        }
        // N4: the projection outcomes once at shutdown, whatever the update count reached.
        let projection = self
            .projection_stats
            .line(self.projection_pending.len(), self.projection_cache.len());
        owner_log!(self, "{projection}");
        if let Some(line) = self.bootstrap_probe_line() {
            owner_log!(self, "{line}");
        }
        // [FIX-46] Letzten (Teil-)Tag beim regulären Shutdown schreiben, damit das Ledger vollständig ist.
        let final_summary = self.ledger.finish();
        let evidence_result = self.evidence.finish(Some(final_summary));
        // M3: the blockhash refresh ends on the general shutdown in every mode (also Shadow
        // without econ and after a HALT). The join is bounded by its 250 ms wait steps and the
        // 30 s RPC timeout of a refresh in flight.
        self.shutdown.request();
        // C6/D10: the WSS session ends with its only consumer in every mode and is joined within
        // the shutdown budget; one still running is never reported as quiescent.
        self.ingress_session.request();
        if let Some(h) = self.blockhash_thread.take() {
            let _ = h.join();
        }
        let shutdown_budget = Duration::from_secs(
            darkbot_solana::runtime_config::active()
                .limits
                .shutdown_seconds,
        );
        let ingress_quiescent = self.ingress_session.stop_and_join(shutdown_budget);
        if self.econ.is_some() || self.operational_window.is_some() {
            self.remote.wait_stopped(shutdown_budget)?;
        }
        // Final MT0 snapshot, seal and join; a lost snapshot makes the run's evidence INCOMPLETE.
        // It is reported last, so it never hides an evidence or operational failure.
        let mt0_complete = self.mt0_reporter.finish();
        evidence_result?;
        if let Some(reason) = self.operational_failure.take() {
            return Err(reason.into());
        }
        if !ingress_quiescent {
            return Err(Box::new(OwnerTerminal("INGRESS_NOT_QUIESCENT")));
        }
        if !mt0_complete {
            return Err("MT0_EVIDENCE_INCOMPLETE".into());
        }
        Ok(())
    }
}
