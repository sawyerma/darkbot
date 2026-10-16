//! Readiness, operational window and threshold bootstrap.
use super::*;

impl ShadowOwner {
    pub(super) fn lifecycle(&mut self) -> bool {
        self.mt0_owner
            .enter(darkbot_common::mt0::OwnerCat::RemoteResults);
        #[cfg(feature = "crate_health")]
        {
            // Backlog age covers both queues counted in `pending_current`: head-of-line token of the
            // oracle queue (FIFO) and the oldest entry of `detect_queue`.
            let oldest_rx = self
                .mt0_order_rx
                .front()
                .copied()
                .into_iter()
                .chain(self.detect_queue.iter().map(|w| w.1))
                .min();
            let oldest = oldest_rx.map(|t| self.clock.now_ns().saturating_sub(t));
            darkbot_common::mt0::MT0.set_backlog(
                (self.oracle_detect_pending.len() + self.detect_queue.len()) as u64,
                oldest,
            );
        }
        if let Some(h) = &self.econ {
            let ready = self.st.run_state().is_tradable() && !self.program_upgrade_latched;
            #[cfg(feature = "crate_health")]
            let ready = ready && self.threshold_ready;
            h.shared
                .shadow_ready
                .store(ready, std::sync::atomic::Ordering::Release);
            if ready {
                let _ = h.shared.shadow_ready_ns.compare_exchange(
                    0,
                    self.clock.now_ns(),
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                );
            }
            h.shared.upgrade.store(
                self.program_upgrade_latched,
                std::sync::atomic::Ordering::Relaxed,
            );
            if h.drain_expired() {
                return false;
            }
        }
        if let Some(window) = &mut self.operational_window {
            let ready = self.st.run_state().is_tradable() && !self.program_upgrade_latched;
            #[cfg(feature = "crate_health")]
            let ready = ready && self.threshold_ready;
            match window.poll(
                self.clock.now_ns().saturating_sub(self.operational_start),
                ready,
            ) {
                Ok(true) => return false,
                Err(reason) => {
                    self.operational_failure = Some(reason);
                    return false;
                }
                Ok(false) => {}
            }
        }
        if self.shutdown.is_requested() {
            return false;
        }
        if self.halt.is_raised() {
            self.st.halt(darkbot_common::error::HaltReason::FeedGap);
        }

        #[cfg(feature = "crate_health")]
        if self.use_threshold_trigger
            && self.st.run_state().is_tradable()
            && !self.threshold_ready
            && !self.threshold_bootstrap.is_empty()
        {
            let now_s = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let slot = self.slot_watch.get();
            let view = StateView(&self.st);
            let engine = darkbot_project0::health::crate_impl::CrateHealth {
                state: &view,
                group: self.group,
            };
            for _ in 0..self.threshold_bootstrap_batch {
                let Some(account) = self.threshold_bootstrap.pop_front() else {
                    break;
                };
                match engine.thresholds(&account, now_s, slot) {
                    Ok(t) => {
                        self.threshold_index.set_account(account, &t.thresholds);
                        self.threshold_fallback.remove(&account);
                    }
                    Err(e) => {
                        self.threshold_index.remove_account(&account);
                        self.threshold_fallback.insert(account);
                        eprintln!(
                            "threshold_bootstrap_fallback: account={} error={}",
                            account, e
                        );
                    }
                }
            }
            if self.threshold_bootstrap.is_empty() {
                self.threshold_ready = true;
                eprintln!(
                    "threshold_index_ready: tracked={} fallback={}",
                    self.threshold_index.tracked_accounts(),
                    self.threshold_fallback.len()
                );
            }
        }

        true
    }
}
