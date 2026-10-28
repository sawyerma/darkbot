//! Apply bounded worker results on the single state owner.
use super::*;

/// V2-13: one oracle admission — threshold hits (L0), re-checks (L2) and the trigger's
/// `(t_rx_ns, slot)`.
#[cfg(feature = "crate_health")]
type OracleAdmission = (HashSet<Pubkey>, HashSet<Pubkey>, (u64, u64));

impl ShadowOwner {
    /// A23-01: a worker result or the census outcome waits for `remote_apply`.
    pub(super) fn remote_pending(&self) -> bool {
        self.remote.winner_results.pending()
            || self.remote.projection_results.pending()
            || self.remote.oracle_price_projection_results.pending()
            || self.c0_census.as_ref().is_some_and(|c| c.outcome_pending())
            || self.remote.fe04_settlement_results.pending()
    }

    /// A23-01: a stop (shutdown or econ abort) waits for the loop's lifecycle check.
    pub(super) fn stop_pending(&self) -> bool {
        self.shutdown.is_requested()
            || self
                .econ
                .as_ref()
                .is_some_and(|h| h.shared.abort.load(std::sync::atomic::Ordering::Acquire))
    }

    /// A23-01 drain wait (admission closed, ingress no longer applied): sleeps on the doorbell
    /// until the next worker result or stop, at most `bound` (else `IDLE_TICK`). The token is
    /// taken before the checks, so a result after them is never slept through.
    pub(super) fn await_remote(&mut self, bound: Option<Duration>) {
        let token = self.wake.token();
        if self.remote_pending() || self.stop_pending() {
            return;
        }
        self.sleep_on_wake(token, bound.map_or(IDLE_TICK, |b| b.min(IDLE_TICK)));
    }

    pub(super) fn sleep_on_wake(&mut self, token: u64, timeout: Duration) {
        if timeout.is_zero() {
            return;
        }
        if let darkbot_common::channel::Woken::Signalled { latency_ns } =
            self.wake.wait(token, timeout)
        {
            self.metrics.owner_wakeup_ns.record(latency_ns);
        }
    }

    pub(super) fn remote_apply(&mut self) -> Result<u64, Box<dyn std::error::Error>> {
        #[cfg(feature = "crate_health")]
        self.poll_c0_census();
        self.fe04_settlement();
        // [FIX-108][R4] Apply completed remote work on the state thread, but never perform RPC here.
        let worker_now_s = wall_unix_s()?;

        while let Some(done) = self.remote.winner_results.try_pop() {
            // [F1] Resolution, rescheduling and TTL expiry (Ok(None) and Err) live in the tracker.
            if let WinnerResolution::Resolved {
                watch: w,
                winner: win,
            } = self
                .winner_tracker
                .on_result(done.watch, done.result, worker_now_s)
            {
                {
                    if let Some(h) = &self.econ {
                        h.emit(crate::analytics::econ::Evidence::Winner {
                            target: w.account.to_string(),
                            rx_slot: w.rx_slot,
                            winner: win.clone(),
                        });
                    }
                    let out = race_outcome(
                        win.slot,
                        w.rx_slot,
                        w.detect_slot,
                        w.sign_slot,
                        w.shadow_ready,
                        w.our_loss,
                    );
                    match out {
                        RaceOutcome::ReadyBeforeWinner => {
                            self.ledger.on_race(true, None, w.expected_net)
                        }
                        RaceOutcome::SameSlotUnknown => self.ledger.on_same_slot(),
                        RaceOutcome::Loss(l) => self.ledger.on_race(false, Some(l), None),
                    }
                    let line = RecordEvent::Race {
                        account: w.account,
                        winner: (&win).into(),
                        our_sign_slot: w.sign_slot,
                        outcome: out,
                        expected_net: w.expected_net,
                    };
                    self.evidence.emit(OwnerEvidence::Record(line))?;
                }
            }
        }

        // DEC-06: ACTIVE's sweep cross-check results.
        #[cfg(feature = "crate_health")]
        while let Some(done) = self
            .remote
            .crosscheck
            .as_ref()
            .and_then(|q| q.results.try_pop())
        {
            self.threshold_crosscheck_result(done);
        }

        #[cfg(feature = "crate_health")]
        while let Some(done) = self.remote.oracle_price_projection_results.try_pop() {
            let bank = done.bank;
            let base_slot = done.base_slot;
            let base_bank_hash = done.base_bank_hash;
            let latest = self.oracle_bank_projection_pending.remove(&bank);
            // DEC-06: a projection of the threshold trigger after ACTIVE was left admits nothing:
            // the full sweep at that change covers its bank.
            if !self.threshold_authority() {
                self.metrics.oracle_bank_projection_stale =
                    self.metrics.oracle_bank_projection_stale.saturating_add(1);
                continue;
            }
            // P6-MT.5 lanes: threshold-index hits from the projected price (potential
            // transition) → L0; accounts with unknown thresholds → L2 (hits stay hits). A failed
            // or stale projection re-checks the whole bank (L2, maintenance work without a
            // per-account signal) — V2-13: only then is the bank's account list read.
            let mut admission: Option<OracleAdmission> = None;
            let mut fallback: Option<String> = None;
            match done.result {
                Ok((projection_slot, projection_unix_s, accounts, _units)) => {
                    let canonical_matches = self
                        .st
                        .get(&bank)
                        .map(|r| r.slot == base_slot && hash(&r.data) == base_bank_hash)
                        .unwrap_or(false);
                    let newest_event_slot = latest.map(|(_, slot)| slot).unwrap_or(base_slot);
                    if !canonical_matches || projection_slot < newest_event_slot {
                        self.metrics.oracle_bank_projection_stale =
                            self.metrics.oracle_bank_projection_stale.saturating_add(1);
                        fallback = Some(if !canonical_matches {
                            "canonical bank changed".into()
                        } else {
                            "projection older than newest dependency update".into()
                        });
                    } else {
                        let view = StateView(&self.st);
                        match darkbot_project0::projection::ProjectedState::new(
                            &view,
                            projection_slot,
                            projection_unix_s,
                            accounts,
                        ) {
                            Ok(projected) => {
                                let engine = darkbot_project0::health::crate_impl::CrateHealth {
                                    state: &projected,
                                    group: self.group,
                                };
                                match engine.maintenance_risk_price_bits(
                                    &bank,
                                    projection_unix_s,
                                    projection_slot,
                                ) {
                                    Ok((asset_price, liability_price)) => {
                                        let hits: HashSet<Pubkey> = self
                                            .threshold_index
                                            .candidates(&bank, asset_price, liability_price, 0)
                                            .into_iter()
                                            .collect();
                                        let rechecks: HashSet<Pubkey> = self
                                            .threshold_fallback
                                            .iter()
                                            .copied()
                                            .filter(|a| {
                                                self.reverse_index.bank_has_account(&bank, a)
                                            })
                                            .collect();
                                        admission = Some((
                                            hits,
                                            rechecks,
                                            latest
                                                .unwrap_or((self.clock.now_ns(), projection_slot)),
                                        ));
                                        self.metrics.oracle_bank_projection_ready = self
                                            .metrics
                                            .oracle_bank_projection_ready
                                            .saturating_add(1);
                                    }
                                    Err(e) => {
                                        self.metrics.oracle_bank_projection_failed = self
                                            .metrics
                                            .oracle_bank_projection_failed
                                            .saturating_add(1);
                                        fallback = Some(format!("projected price error: {e}"));
                                    }
                                }
                            }
                            Err(e) => {
                                self.metrics.oracle_bank_projection_failed =
                                    self.metrics.oracle_bank_projection_failed.saturating_add(1);
                                fallback = Some(format!("projected state invalid: {e}"));
                            }
                        }
                    }
                }
                Err(e) => {
                    self.metrics.oracle_bank_projection_failed =
                        self.metrics.oracle_bank_projection_failed.saturating_add(1);
                    fallback = Some(format!("simulation failed: {e}"));
                }
            }
            if let Some(reason) = fallback {
                self.metrics.oracle_price_fallbacks =
                    self.metrics.oracle_price_fallbacks.saturating_add(1);
                let all: HashSet<Pubkey> = self
                    .reverse_index
                    .accounts_for_bank(&bank)
                    .copied()
                    .collect();
                owner_log!(
                    self,
                    "oracle_bank_projection_fallback: bank={} reason={} accounts={}",
                    bank,
                    reason,
                    all.len()
                );
                admission = Some((
                    HashSet::new(),
                    all,
                    latest.unwrap_or((self.clock.now_ns(), base_slot)),
                ));
            }
            if let Some((hits, rechecks, at)) = admission {
                // The bank's input revision of the trigger this projection was requested for.
                let revision = self.bank_input_revision.get(&bank).copied().unwrap_or(0);
                self.admit_oracle_accounts(hits, rechecks, at, revision);
            }
        }

        #[cfg(feature = "crate_health")]
        while let Some(done) = self.remote.projection_results.try_pop() {
            self.projection_pending.remove(&done.account);
            let targeted_probe_result = self.integration_probe_expected.contains(&done.account);
            let mut targeted_status: Option<&'static str> = None;
            // P6-MT.2 (MT5a): the central policy judges the result first. An older generation
            // (a recovery or program upgrade since the request), a foreign in-flight entry
            // (MT5b: the account is SUBMITTED by a durable attempt) or an otherwise dropped
            // result gets no cache entry and no follow-up work; a result of an older revision
            // than the account's newest yields to it (re-requested from the current state, the
            // bounded E9 requeue); a changed dependency readset is the E9 stale case; only an
            // accepted result reaches the E9 identity checks below, which stay as they are.
            let action = self.judge(&done.identity);
            let retry = action == crate::execution::identity::ResultAction::RetryNewestPending;
            let dropped = action.is_drop();
            let recheck = action == crate::execution::identity::ResultAction::RecheckCurrent;
            match done.result {
                _ if dropped || retry => {
                    if targeted_probe_result {
                        targeted_status = Some("stale");
                    }
                    let removed = self.projection_cache.remove(&done.account);
                    self.projection_stats.dropped_if(removed);
                    if retry {
                        self.requeue_projection(done.account, done.t_rx_ns, None);
                    }
                }
                Ok((projection_slot, projection_unix_s, accounts, units)) => {
                    let canonical = self.st.get(&done.account);
                    let base_matches = canonical
                        .map(|r| {
                            r.slot == done.base_slot && hash(&r.data) == done.base_account_hash
                        })
                        .unwrap_or(false);
                    // E9: a dependency revised while the worker projected makes it stale too
                    // (N4: counted, no line per discard); MT5a: so does any re-write of a key
                    // of the request's readset.
                    let dependencies_match = dependency_fingerprint(&self.st, &done.dependencies)
                        == done.dependency_fingerprint
                        && !recheck;
                    let canonical_matches = self
                        .projection_stats
                        .result(base_matches, dependencies_match);

                    // [FIX-116b] The candidate MarginfiAccount is readonly during the external
                    // refresh prefix and is therefore intentionally not requested as a post-account.
                    // Freshness is proven against canonical StateCore via exact slot+hash identity.
                    if canonical_matches {
                        if targeted_probe_result {
                            targeted_status = Some("ready");
                        }
                        let replaced = self.projection_cache.insert(
                            done.account,
                            CachedProjection {
                                base_account_hash: done.base_account_hash,
                                projection_slot,
                                projection_unix_s,
                                accounts,
                                dependencies: done.dependencies,
                                dependency_fingerprint: done.dependency_fingerprint,
                            },
                        );
                        self.projection_stats.dropped_if(replaced);
                        self.detect_queue.push_front((
                            done.account,
                            done.t_rx_ns,
                            done.base_slot,
                            TRIGGER_INTEGRATION_PROJECTION,
                        ));
                        let (integration_kinds, pricing_integration_kinds) = {
                            let view = StateView(&self.st);
                            let withdraw = darkbot_project0::integration::kinds_for_account(
                                &view,
                                &self.program,
                                &done.account,
                            )
                            .unwrap_or_default()
                            .into_iter()
                            .map(darkbot_project0::integration::kind_name)
                            .collect::<Vec<_>>();
                            let pricing = darkbot_project0::integration::pricing_kinds_for_account(
                                &view,
                                &self.program,
                                &done.account,
                            )
                            .unwrap_or_default()
                            .into_iter()
                            .map(darkbot_project0::integration::kind_name)
                            .collect::<Vec<_>>();
                            (withdraw, pricing)
                        };
                        let line = RecordEvent::ProjectionReady {
                            account: done.account,
                            base_slot: done.base_slot,
                            projection_slot,
                            projection_unix_s,
                            integration_kinds: integration_kinds
                                .into_iter()
                                .map(Into::into)
                                .collect(),
                            pricing_integration_kinds: pricing_integration_kinds
                                .into_iter()
                                .map(Into::into)
                                .collect(),
                            simulated_cu: units,
                        };
                        self.evidence.emit(OwnerEvidence::Record(line))?;
                    } else {
                        if targeted_probe_result {
                            targeted_status = Some("stale");
                        }
                        let removed = self.projection_cache.remove(&done.account);
                        self.projection_stats.dropped_if(removed);
                        let line = RecordEvent::ProjectionStale {
                            account: done.account,
                            base_slot: done.base_slot,
                            projection_slot,
                            canonical_matches,
                        };
                        self.evidence.emit(OwnerEvidence::Record(line))?;
                        // E9: the candidate or a dependency changed while the worker
                        // projected — request it again from the current state (bounded).
                        self.requeue_projection(done.account, done.t_rx_ns, None);
                    }
                }
                Err(failure) => {
                    if targeted_probe_result {
                        targeted_status = Some("failed");
                    }
                    self.projection_stats.failed += 1;
                    let removed = self.projection_cache.remove(&done.account);
                    self.projection_stats.dropped_if(removed);
                    let line = RecordEvent::ProjectionFailed {
                        account: done.account,
                        base_slot: done.base_slot,
                        error: failure.error,
                    };
                    self.evidence.emit(OwnerEvidence::Record(line))?;
                    // E9: a node behind the required context may catch up — retried (bounded)
                    // after at least one more processed slot; any other failure waits for the
                    // candidate's next trigger.
                    if failure.context_behind {
                        let after = processed_level(&self.st).saturating_add(1);
                        self.requeue_projection(done.account, done.t_rx_ns, Some(after));
                    }
                }
            }

            if targeted_probe_result && self.integration_probe_terminal.insert(done.account) {
                match targeted_status {
                    Some("ready") => self.integration_probe_projection_ready += 1,
                    Some("failed") => self.integration_probe_projection_failed += 1,
                    Some("stale") => self.integration_probe_projection_stale += 1,
                    _ => {}
                }
            }
            if self.integration_probe_enabled
                && !self.integration_probe_completion_reported
                && !self.integration_probe_expected.is_empty()
                && self.integration_probe_terminal.len() == self.integration_probe_expected.len()
            {
                self.integration_probe_completion_reported = true;
                owner_log!(
                    self,
                    "integration_target_projection_complete: total={} ready={} failed={} stale={}",
                    self.integration_probe_expected.len(),
                    self.integration_probe_projection_ready,
                    self.integration_probe_projection_failed,
                    self.integration_probe_projection_stale
                );
            }
        }

        // [F1] Bounded non-blocking dispatch of due watches only (min-heap, no full rebuild per
        // loop). The first full-queue refusal ends the round; the tracker keeps and backs off the
        // watch, so a saturated worker queue costs at most one attempt per loop.
        self.winner_tracker.dispatch_due(
            worker_now_s,
            self.remote_worker_capacity,
            |w| match self.remote.winner_jobs.try_push(w) {
                Ok(()) => Ok(WinnerDispatch::Sent),
                Err(ChannelError::Full(_)) => Ok(WinnerDispatch::QueueFull),
                Err(ChannelError::Disconnected(_)) => Err("winner RPC worker disconnected"),
            },
        )?;

        Ok(worker_now_s)
    }
}

impl ShadowOwner {
    /// V2-13: admits a threshold trigger's hits (L0) and re-checks (L2; a hit stays a hit)
    /// through the repeat gate (`RepeatGate`) — an account passes with a new cause only (first
    /// admission, forced re-arm, newer input `revision`) or as a due repeat within the soft cap.
    /// Returns the distinct accounts admitted.
    #[cfg(feature = "crate_health")]
    pub(super) fn admit_oracle_accounts(
        &mut self,
        hits: HashSet<Pubkey>,
        mut rechecks: HashSet<Pubkey>,
        (t_rx_ns, event_slot): (u64, u64),
        revision: u64,
    ) -> usize {
        rechecks.retain(|a| !hits.contains(a));
        let repeat_capacity = self.oracle_lanes.len() < self.candidate_repeat_soft_cap;
        let mut gate = RepeatGate {
            cache: &mut self.candidate_cache,
            forced: &mut self.candidate_force_rearm,
            recheck_slots: self.candidate_recheck_slots,
            repeat_capacity,
            suppressed: &mut self.metrics.candidate_cache_suppressed,
            softcap_suppressed: &mut self.metrics.candidate_cache_softcap_suppressed,
        };
        let hits = gate.pass(hits, event_slot, revision);
        let rechecks = gate.pass(rechecks, event_slot, revision);
        let mut count = 0usize;
        for (accounts, lane) in [(hits, Lane::L0), (rechecks, Lane::L2)] {
            count += enqueue_oracle_accounts(
                accounts.keys().copied(),
                (t_rx_ns, event_slot, "oracle_threshold"),
                lane,
                |a| accounts.get(a).copied().unwrap_or(0),
                &mut self.oracle_lanes,
                (
                    &mut self.metrics.oracle_enqueued,
                    &mut self.metrics.oracle_coalesced,
                    &mut self.metrics.oracle_max_pending,
                ),
            );
        }
        self.metrics.oracle_candidate_hits = self
            .metrics
            .oracle_candidate_hits
            .saturating_add(count as u64);
        count
    }

    /// E9: requests a candidate's projection again after a stale result or a lagging node —
    /// through the ordinary detect queue (behind waiting work, so neither it nor others starve),
    /// or parked until `after_slot` is processed (backoff), and within the candidate's bound
    /// (`projection_retry_allowed`). A candidate no longer in state is not requeued.
    #[cfg(feature = "crate_health")]
    pub(super) fn requeue_projection(
        &mut self,
        account: Pubkey,
        t_rx_ns: u64,
        after_slot: Option<u64>,
    ) {
        let Some(slot) = self.st.get(&account).map(|r| r.slot) else {
            self.projection_requeues.remove(&account);
            return;
        };
        if projection_retry_allowed(
            &mut self.projection_requeues,
            &mut self.projection_stats,
            account,
            self.projection_requeue_max,
        ) {
            let work = (account, t_rx_ns, slot, TRIGGER_PROJECTION_REQUEUE);
            match after_slot {
                None => self.detect_queue.push_back(work),
                Some(after) => self.projection_parked.park(after, work),
            }
        }
    }

    /// E9: parked projection work whose slot the owner has processed joins the detect queue
    /// (behind waiting work).
    #[cfg(feature = "crate_health")]
    pub(super) fn release_parked_projections(&mut self) {
        for work in self.projection_parked.release(&self.st) {
            self.detect_queue.push_back(work);
        }
    }
}
