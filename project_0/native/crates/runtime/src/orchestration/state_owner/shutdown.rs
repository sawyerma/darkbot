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
            h.emit(crate::analytics::econ::Evidence::ShadowStats(serde_json::json!({
            "winner_watch_current": ws.len, "winner_watch_max": ws.max, "winner_watch_overflow":ws.overflow,
            "winner_watch_errors":ws.errors,"winner_watch_resolved":ws.resolved,"winner_watch_expired":ws.expired,
            "winner_queue_full_backoffs":ws.queue_full_backoffs,
            "ingress_overflow":self.consumer.stats().overflow,
            "winner_pending_at_deadline":ws.len,
            "shadow_state_ready_at_shutdown":self.st.run_state().is_tradable(),
            "remote_winner_queue_depth":self.remote.winner_jobs.stats().sent.saturating_sub(self.remote.winner_jobs.stats().received),
            "remote_projection_queue_depth":self.remote.projection_jobs.stats().sent.saturating_sub(self.remote.projection_jobs.stats().received),
        })));
            self.shutdown.request();
            self.ingress_session.request();
        }
        if self.operational_window.is_some() {
            self.shutdown.request();
            self.ingress_session.request();
        }
        // [FIX-46] Letzten (Teil-)Tag beim regulären Shutdown schreiben, damit das Ledger vollständig ist.
        let final_summary = self.ledger.finish();
        if let Ok(j) = serde_json::to_string(&final_summary) {
            let _ = writeln!(self.summaries, "{}", j);
        }
        let _ = self.summaries.flush();
        let _ = self.records.flush();
        if self.econ.is_some() || self.operational_window.is_some() {
            self.remote.wait_stopped(Duration::from_secs(
                darkbot_solana::runtime_config::active()
                    .limits
                    .shutdown_seconds,
            ))?;
        }
        if let Some(reason) = self.operational_failure.take() {
            return Err(reason.into());
        }
        Ok(())
    }
}
