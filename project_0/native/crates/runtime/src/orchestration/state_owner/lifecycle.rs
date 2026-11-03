//! Readiness, operational window and threshold bootstrap.
use super::*;

impl ShadowOwner {
    /// P8-RES-02/DEC-17b: one wallet's journal refresh — its book (unless an attempt it holds
    /// keeps it), the B evidence, its attempt records (the grant quotas), the accounting the
    /// status projection carries (`tap`) and its approval (`tap`, or while exits are `live`).
    fn fe04_refresh_wallet(&mut self, i: usize, dir: &std::path::Path, live: bool, tap: bool) {
        use crate::execution::reservation::WalletReservations;
        let wallet = self.wallets[i].liquidator.to_string();
        let w = &mut self.wallets[i];
        if w.fe04_held.is_none() {
            // Review M1: an open admission holds the journal and its lock; the book follows that
            // journal.
            w.reservations = match &w.fe04_admission {
                Some(a) => WalletReservations::rebuild(a.journal.active_hold())
                    .unwrap_or_else(|_| WalletReservations::unreadable()),
                None => WalletReservations::refreshed(dir, &wallet),
            };
        }
        let ledger = crate::execution::accounting::Ledger::load(dir, &wallet);
        w.hold_evidence = ledger.as_ref().ok().map(|l| l.b_evidence());
        // Unreadable now: the records of the last read stay (only this owner writes them).
        if let Ok(l) = &ledger {
            w.records = Some(l.attempts.clone());
        }
        if tap {
            w.ledger_view = Some(ledger.map(|l| status_view::LedgerView::of(&l)));
        }
        // DEC-10: the approval may be replaced or spent between refreshes; its status is
        // published only through the tap. DEC-13b: while LIVE is effective the admissions use
        // it, so it is re-read then too.
        if tap || live {
            if let Some(source) = w.fe04_approval.as_mut() {
                source.refresh(Some(dir), &wallet);
            }
        }
    }

    pub(super) fn lifecycle(&mut self) -> bool {
        // P6-MT.10: the run's first fee parity record, and a mismatch at once (synced).
        self.fee_parity_pass();
        // P6-MT.12: the run's host, journaled at its first pass.
        if self.host_journaled.is_none() {
            self.host_record();
        }
        // P8-RES-02: every wallet's book follows its durable FE-04 journal (a settlement releases
        // the attempt, its open positions stay held until exited, UNKNOWN keeps holding, an
        // unreadable journal blocks every offer of that wallet — DEC-17b: of that wallet only).
        // FE-04/C3: a held journal changes only through this owner (the book is rebuilt there).
        // DEC-13b (review M2): an admission (and its wallet lock) is open only while LIVE is.
        if self.wallets.iter().any(|w| w.fe04_admission.is_some()) && !self.live_effective() {
            for w in &mut self.wallets {
                w.fe04_admission = None;
            }
        }
        // S4-04: while LIVE is, each wallet's admission is opened ahead of the first candidate.
        for i in 0..self.wallets.len() {
            if self.wallets[i].runs_candidates() {
                self.fe04_prewarm(i);
            }
        }
        if let Some(dir) = self.fe04_journal.clone() {
            if self.fe04_refreshed.elapsed() >= self.fe04_refresh {
                // P8-CTRL.5: exits (and their approval) need every gate but ECON_ATTEMPT.
                let live = self.exit_live_effective();
                // P0-PROD-CTRL-04: the accounting the status projection carries, read only when
                // a status tap is installed (read-only, no wallet lock).
                let tap = crate::status_tap::installed().is_some();
                let mut evidence = Some(crate::execution::hold_b::BEvidence::default());
                for i in 0..self.wallets.len() {
                    if self.wallets[i].runs_candidates() {
                        self.fe04_refresh_wallet(i, &dir, live, tap);
                        evidence = evidence.zip(self.wallets[i].hold_evidence.as_ref()).map(
                            |(mut all, own)| {
                                all.merge(own);
                                all
                            },
                        );
                    }
                }
                // DEC-04: the B evidence of every wallet's journals (one unreadable: none, ACTIVE
                // impossible).
                self.hold_gate.evidence = evidence.unwrap_or_default();
                self.fe04_refreshed = std::time::Instant::now();
                // DEC-13b: an open position of a held attempt exits with its approval.
                if live {
                    for i in 0..self.wallets.len() {
                        self.fe04_exit_admission(i);
                    }
                }
            }
        }
        self.refresh_hold_gate(false);
        // P8-CTRL.3 (review M1): the FE-06 previews at the journal refresh cadence, for the tap.
        if crate::status_tap::installed().is_some() {
            self.sources
                .capabilities
                .refresh_previews(self.fe04_refresh);
        }
        self.rebalance_observe();
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
