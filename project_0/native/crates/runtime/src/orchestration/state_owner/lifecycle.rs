//! Readiness, operational window and threshold bootstrap.
use super::*;

impl ShadowOwner {
    pub(super) fn lifecycle(&mut self) -> bool {
        // P8-RES-02: the book follows the durable FE-04 journal (a finalized reconciliation
        // releases, UNKNOWN keeps holding, an unreadable journal blocks every offer).
        // FE-04/C3: a held journal changes only through this owner (the book is rebuilt there).
        if let Some(dir) = &self.fe04_journal {
            if self.fe04_refreshed.elapsed() >= self.fe04_refresh {
                let wallet = self.liquidator.to_string();
                if self.fe04_held.is_none() {
                    self.reservations =
                        crate::execution::reservation::WalletReservations::refreshed(dir, &wallet);
                }
                // P0-PROD-CTRL-04: the accounting the status projection carries, read only when
                // a status tap is installed (read-only, no wallet lock).
                #[cfg(feature = "crate_health")]
                if crate::status_tap::installed().is_some() {
                    self.ledger_view = Some(status_view::LedgerView::load(dir, &wallet));
                    // DEC-10: the approval may be replaced or spent between refreshes; its
                    // status is published only through the tap.
                    if let Some(source) = self.fe04_approval.as_mut() {
                        source.refresh(Some(dir), &wallet);
                    }
                }
                self.fe04_refreshed = std::time::Instant::now();
            }
        }
        #[cfg(feature = "crate_health")]
        self.rebalance_observe();
        #[cfg(feature = "crate_health")]
        self.publish_status();
        // P6-MT.1 (MT5b): the per-account SUBMITTED ownership is the book's target hold —
        // synchronized before this pass judges any result or evaluates any candidate.
        self.sync_ownership();
        self.mt0_owner
            .enter(darkbot_common::mt0::OwnerCat::RemoteResults);
        #[cfg(feature = "crate_health")]
        {
            // Backlog age covers both queues counted in `pending_current`: the oldest
            // `t_rx_first` across the oracle lanes and the oldest entry of `detect_queue`.
            let oldest_rx = self
                .oracle_lanes
                .oldest_t_rx_first()
                .into_iter()
                .chain(self.detect_queue.iter().map(|w| w.1))
                .min();
            let oldest = oldest_rx.map(|t| self.clock.now_ns().saturating_sub(t));
            darkbot_common::mt0::MT0.set_backlog(
                (self.oracle_lanes.len() + self.detect_queue.len()) as u64,
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
        if self.shutdown.is_requested() || (self.stop_poll)() {
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
            // [C0-FIX-03] canonical chain clock, never local or zero time
            let clock = chain_clock(&self.st);
            let view = StateView(&self.st);
            let engine = darkbot_project0::health::crate_impl::CrateHealth {
                state: &view,
                group: self.group,
            };
            for _ in 0..self.threshold_bootstrap_batch {
                let Some(account) = self.threshold_bootstrap.pop_front() else {
                    break;
                };
                match clock
                    .map_err(|e| darkbot_project0::health::HealthError::Stale(e.to_string()))
                    .and_then(|c| engine.thresholds(&account, c.unix_timestamp, c.slot))
                {
                    Ok(t) => {
                        self.threshold_index.set_account(account, &t.thresholds);
                        self.threshold_fallback.remove(&account);
                    }
                    Err(e) => {
                        self.threshold_index.remove_account(&account);
                        self.threshold_fallback.insert(account);
                        owner_log!(
                            self,
                            "threshold_bootstrap_fallback: account={} error={}",
                            account,
                            e
                        );
                    }
                }
            }
            if self.threshold_bootstrap.is_empty() {
                self.threshold_ready = true;
                owner_log!(
                    self,
                    "threshold_index_ready: tracked={} fallback={}",
                    self.threshold_index.tracked_accounts(),
                    self.threshold_fallback.len()
                );
            }
        }

        true
    }
}
