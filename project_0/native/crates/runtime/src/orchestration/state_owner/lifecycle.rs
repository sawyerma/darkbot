//! Readiness, operational window and threshold bootstrap.
use super::*;

impl ShadowOwner {
    pub(super) fn lifecycle(&mut self) -> bool {
        // P8-RES-02: the book follows the durable FE-04 journal (a finalized reconciliation
        // releases, UNKNOWN keeps holding, an unreadable journal blocks every offer).
        // FE-04/C3: a held journal changes only through this owner (the book is rebuilt there).
        // DEC-13b (review M2): an admission (and its wallet lock) is open only while LIVE is.
        #[cfg(feature = "crate_health")]
        if self.fe04_admission.is_some() && !self.live_effective() {
            self.fe04_admission = None;
        }
        if let Some(dir) = &self.fe04_journal {
            if self.fe04_refreshed.elapsed() >= self.fe04_refresh {
                let wallet = self.liquidator.to_string();
                if self.fe04_held.is_none() {
                    use crate::execution::reservation::WalletReservations;
                    // Review M1: an open admission holds the journal and its lock; the book
                    // follows that journal.
                    self.reservations = match &self.fe04_admission {
                        Some(a) => WalletReservations::rebuild(a.journal.active_hold())
                            .unwrap_or_else(|_| WalletReservations::unreadable()),
                        None => WalletReservations::refreshed(dir, &wallet),
                    };
                }
                // DEC-04: the B evidence of the journals (unreadable: none, ACTIVE impossible).
                self.hold_gate.evidence = crate::execution::accounting::Ledger::load(dir, &wallet)
                    .map(|l| l.b_evidence())
                    .unwrap_or_default();
                // P0-PROD-CTRL-04: the accounting the status projection carries, read only when
                // a status tap is installed (read-only, no wallet lock).
                // P8-CTRL.5: exits (and their approval) need every gate but ECON_ATTEMPT.
                #[cfg(feature = "crate_health")]
                let live = self.exit_live_effective();
                #[cfg(feature = "crate_health")]
                if crate::status_tap::installed().is_some() {
                    self.ledger_view = Some(status_view::LedgerView::load(dir, &wallet));
                }
                // DEC-10: the approval may be replaced or spent between refreshes; its status is
                // published only through the tap. DEC-13b: while LIVE is effective the
                // admissions use it, so it is re-read then too.
                #[cfg(feature = "crate_health")]
                if crate::status_tap::installed().is_some() || live {
                    if let Some(source) = self.fe04_approval.as_mut() {
                        source.refresh(Some(dir), &wallet);
                    }
                }
                self.fe04_refreshed = std::time::Instant::now();
                // DEC-13b: an open position of the held attempt exits with its approval.
                #[cfg(feature = "crate_health")]
                if live {
                    self.fe04_exit_admission();
                }
            }
        }
        self.refresh_hold_gate(false);
        // P8-CTRL.3 (review M1): the FE-06 previews at the journal refresh cadence, for the tap.
        #[cfg(feature = "crate_health")]
        if crate::status_tap::installed().is_some() {
            self.sources
                .capabilities
                .refresh_previews(self.fe04_refresh);
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
            // DEC-06 (review H1): only an admitting threshold trigger waits for its index; beside
            // the authoritative sweep (OBSERVE, SWEEP) readiness is the sweep's.
            #[cfg(feature = "crate_health")]
            let ready = ready && (!self.threshold_authority() || self.threshold_ready);
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
        // DEC-16: an unbounded Shadow run's windows (the first opens at the first tradable READY).
        if self.windows.is_some() {
            let ready = self.st.run_state().is_tradable() && !self.program_upgrade_latched;
            // DEC-06 (review H1): only an admitting threshold trigger waits for its index; beside
            // the authoritative sweep (OBSERVE, SWEEP) readiness is the sweep's.
            #[cfg(feature = "crate_health")]
            let ready = ready && (!self.threshold_authority() || self.threshold_ready);
            if !self.until_stopped_tick(ready) {
                return false;
            }
        }
        let rolling = self.windows.is_some();
        if let Some(window) = &mut self.operational_window {
            let ready = self.st.run_state().is_tradable() && !self.program_upgrade_latched;
            // DEC-06 (review H1): only an admitting threshold trigger waits for its index; beside
            // the authoritative sweep (OBSERVE, SWEEP) readiness is the sweep's.
            #[cfg(feature = "crate_health")]
            let ready = ready
                && (self.threshold.effective
                    != crate::orchestration::threshold_mode::Effective::Active
                    || self.threshold_ready);
            match window.poll(
                self.clock.now_ns().saturating_sub(self.operational_start),
                ready,
            ) {
                // An unbounded run keeps only the bootstrap budget: it ends by its stop.
                Ok(true) if !rolling => return false,
                Err(reason) => {
                    self.operational_failure = Some(reason);
                    return false;
                }
                Ok(_) => {}
            }
        }
        if self.shutdown.is_requested() {
            return false;
        }
        if (self.stop_poll)() {
            if let Some(w) = &mut self.windows {
                w.request_stop(crate::orchestration::until_stopped::OPERATOR_STOP);
            }
            return false;
        }
        if self.halt.is_raised() {
            self.st.halt(darkbot_common::error::HaltReason::FeedGap);
        }

        #[cfg(feature = "crate_health")]
        if self.threshold_indexing()
            && self.st.run_state().is_tradable()
            && !self.threshold_ready
            && !self.threshold_bootstrap.is_empty()
            // DEC-06: beside the sweep the bootstrap is OBSERVE work within its budget (B-9);
            // waiting for it is no sampled unit (review L4).
            && self.threshold_budget_allows()
        {
            let bootstrap_started = std::time::Instant::now();
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
                self.threshold_bootstrap_queued.remove(&account);
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
            if !self.threshold_authority() {
                self.threshold_charge(bootstrap_started);
            }
            if self.threshold_bootstrap.is_empty() {
                self.threshold_ready = true;
                self.threshold.bootstrapped = true;
                owner_log!(
                    self,
                    "threshold_index_ready: tracked={} fallback={}",
                    self.threshold_index.tracked_accounts(),
                    self.threshold_fallback.len()
                );
            }
        }
        // DEC-06: the effective threshold mode follows the state, the index and the evidence.
        #[cfg(feature = "crate_health")]
        self.threshold_refresh("STATE");

        true
    }
}
