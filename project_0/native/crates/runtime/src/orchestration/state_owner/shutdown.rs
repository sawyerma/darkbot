//! Ordered evidence flush and worker shutdown.
use super::*;
impl ShadowOwner {
    pub(super) fn finish(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Review E4: every exit publishes the final owner state (also an error exit inside the
        // rate limit), so the reporter's last frame is the run's end, never an older state.
        #[cfg(feature = "crate_health")]
        {
            self.status_published = None;
            self.publish_status();
        }
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
        // DEC-01: the blockhash parking once at shutdown (parked entries end like lane entries).
        let parking = self.blockhash_park_line();
        owner_log!(self, "{parking}");
        // S1-02: the observing source ends with the owner; its measurement once.
        let observer = self.stop_observer();
        let sources = self.sources.line();
        owner_log!(self, "{sources}");
        // A23-01: doorbell wakeups (signal → owner running); quantiles are bucket upper bounds.
        let w = self.metrics.owner_wakeup_ns.snapshot();
        let q = |v: Option<u64>| v.map_or_else(|| "none".to_string(), |v| v.to_string());
        owner_log!(
            self,
            "owner_wakeup_ns: n={} p50={} p95={} p99={} max={}",
            w.count,
            q(w.p50),
            q(w.p95),
            q(w.p99),
            q(w.max)
        );
        // P6-MT.13 (MT3): the lane waits and lane changes once at shutdown.
        #[cfg(feature = "crate_health")]
        {
            let line = self.lane_line();
            owner_log!(self, "{line}");
        }
        if let Some(line) = self.bootstrap_probe_line() {
            owner_log!(self, "{line}");
        }
        // DEC-16: an unbounded run's open window ends with the run (its record carries the end).
        let mut until_stopped = self.windows.take();
        if let Some(w) = until_stopped.as_mut() {
            let counters = self.owner_counters();
            let now_s = (self.clock.unix_ns(self.clock.now_ns()) / 1_000_000_000) as u64;
            w.end(
                self.end_result.unwrap_or("RUN_END"),
                now_s,
                &mut self.evidence,
                counters,
            );
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
        let remote = if self.econ.is_some() || self.operational_window.is_some() {
            self.remote.wait_stopped(shutdown_budget)
        } else {
            Ok(())
        };
        // A bounded run ends on a failed worker stop here; an unbounded one seals its windows first.
        if until_stopped.is_none() {
            remote.clone()?;
        }
        // Final MT0 snapshot, seal and join; a lost snapshot makes the run's evidence INCOMPLETE.
        // It is reported last, so it never hides an evidence or operational failure.
        let mt0_complete = self.mt0_reporter.finish();
        // DEC-16: the windows journal is complete only for an ordered end — workers stopped,
        // ingress quiescent, every journal sealed (open winner watches are counted, D-7).
        if let Some(w) = until_stopped {
            let write_error = evidence_result
                .as_ref()
                .err()
                .map(|e| e.0)
                .or((!mt0_complete).then_some("MT0_EVIDENCE_INCOMPLETE"))
                .or(observer.as_ref().err().map(|_| "SOURCE_OBSERVER_FAILED"));
            let quiescent = if ingress_quiescent {
                Ok(())
            } else {
                Err("INGRESS_NOT_QUIESCENT".to_string())
            };
            let in_order = crate::orchestration::until_stopped::ended_in_order(
                w.reason(),
                remote.is_ok(),
                write_error,
                None,
                &quiescent,
            );
            w.finish(in_order);
        }
        remote?;
        evidence_result?;
        observer?;
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
