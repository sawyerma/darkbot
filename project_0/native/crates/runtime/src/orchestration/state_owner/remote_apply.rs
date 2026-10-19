//! Apply bounded worker results on the single state owner.
use super::*;

impl ShadowOwner {
    pub(super) fn remote_apply(&mut self) -> Result<u64, Box<dyn std::error::Error>> {
        #[cfg(feature = "crate_health")]
        self.poll_c0_census();
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

        while let Some(done) = self.remote.reconcile_results.try_pop() {
            let mut work = done.work;
            match done.result {
                Err(e) => {
                    // [P8] Keep the unresolved submission (exposure stays held) and halt.
                    self.pending.push(work);
                    return Err(format!("RECONCILE_CLOCK_UNAVAILABLE:{e}").into());
                }
                Ok(Some(r)) => {
                    self.safety.record_outcome(
                        worker_now_s,
                        r.landed_success,
                        r.realized_pnl,
                        r.record.exposure_released,
                        0,
                        // B15: the shadow pipeline reserves no inventory hold (B1: no send).
                        0,
                    );
                    self.evidence.emit(OwnerEvidence::Reconcile(r.record))?;
                }
                Ok(None) => {
                    work.next_check_unix_s = worker_now_s.saturating_add(1);
                    self.pending.push(work);
                }
            }
        }

        #[cfg(feature = "crate_health")]
        while let Some(done) = self.remote.oracle_price_projection_results.try_pop() {
            let bank = done.bank;
            let base_slot = done.base_slot;
            let base_bank_hash = done.base_bank_hash;
            let result = done.result;
            let latest = self.oracle_bank_projection_pending.remove(&bank);
            let all_accounts: Vec<Pubkey> = self
                .reverse_index
                .accounts_for_bank(&bank)
                .copied()
                .collect();
            let mut fallback_all =
                |reason: &str,
                 oracle_price_fallbacks: &mut u64,
                 oracle_candidate_hits: &mut u64,
                 oracle_enqueued: &mut u64,
                 oracle_coalesced: &mut u64,
                 oracle_max_pending: &mut usize,
                 oracle_detect_order: &mut VecDeque<Pubkey>,
                 oracle_detect_pending: &mut HashMap<Pubkey, DetectWork>,
                 mt0_order_rx: &mut VecDeque<u64>| {
                    *oracle_price_fallbacks = oracle_price_fallbacks.saturating_add(1);
                    let (t_rx_ns, event_slot) = latest.unwrap_or((self.clock.now_ns(), base_slot));
                    let mt0_len_before = oracle_detect_order.len(); // [MT0]
                    let count = enqueue_oracle_accounts(
                        all_accounts.iter().copied(),
                        (t_rx_ns, event_slot, "oracle_threshold"),
                        oracle_detect_order,
                        oracle_detect_pending,
                        oracle_enqueued,
                        oracle_coalesced,
                        oracle_max_pending,
                    );
                    for _ in mt0_len_before..oracle_detect_order.len() {
                        mt0_order_rx.push_back(t_rx_ns);
                    }
                    *oracle_candidate_hits = oracle_candidate_hits.saturating_add(count as u64);
                    owner_log!(
                        self,
                        "oracle_bank_projection_fallback: bank={} reason={} accounts={}",
                        bank,
                        reason,
                        count
                    );
                };

            match result {
                Ok((projection_slot, projection_unix_s, accounts, _units)) => {
                    let canonical_matches = self
                        .st
                        .get(&done.bank)
                        .map(|r| r.slot == base_slot && hash(&r.data) == base_bank_hash)
                        .unwrap_or(false);
                    let newest_event_slot = latest.map(|(_, slot)| slot).unwrap_or(base_slot);
                    if !canonical_matches || projection_slot < newest_event_slot {
                        self.metrics.oracle_bank_projection_stale =
                            self.metrics.oracle_bank_projection_stale.saturating_add(1);
                        fallback_all(
                            if !canonical_matches {
                                "canonical bank changed"
                            } else {
                                "projection older than newest dependency update"
                            },
                            &mut self.metrics.oracle_price_fallbacks,
                            &mut self.metrics.oracle_candidate_hits,
                            &mut self.metrics.oracle_enqueued,
                            &mut self.metrics.oracle_coalesced,
                            &mut self.metrics.oracle_max_pending,
                            &mut self.oracle_detect_order,
                            &mut self.oracle_detect_pending,
                            &mut self.mt0_order_rx,
                        );
                        continue;
                    }

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
                                    let mut candidates: HashSet<Pubkey> = self
                                        .threshold_index
                                        .candidates(&bank, asset_price, liability_price, 0)
                                        .into_iter()
                                        .collect();
                                    candidates.extend(
                                        all_accounts
                                            .iter()
                                            .copied()
                                            .filter(|a| self.threshold_fallback.contains(a)),
                                    );
                                    let (t_rx_ns, event_slot) =
                                        latest.unwrap_or((self.clock.now_ns(), projection_slot));
                                    let mt0_len_before = self.oracle_detect_order.len(); // [MT0]
                                    let count = enqueue_oracle_accounts(
                                        candidates,
                                        (t_rx_ns, event_slot, "oracle_threshold"),
                                        &mut self.oracle_detect_order,
                                        &mut self.oracle_detect_pending,
                                        &mut self.metrics.oracle_enqueued,
                                        &mut self.metrics.oracle_coalesced,
                                        &mut self.metrics.oracle_max_pending,
                                    );
                                    for _ in mt0_len_before..self.oracle_detect_order.len() {
                                        self.mt0_order_rx.push_back(t_rx_ns);
                                    }
                                    self.metrics.oracle_candidate_hits = self
                                        .metrics
                                        .oracle_candidate_hits
                                        .saturating_add(count as u64);
                                    self.metrics.oracle_bank_projection_ready =
                                        self.metrics.oracle_bank_projection_ready.saturating_add(1);
                                }
                                Err(e) => {
                                    self.metrics.oracle_bank_projection_failed = self
                                        .metrics
                                        .oracle_bank_projection_failed
                                        .saturating_add(1);
                                    fallback_all(
                                        &format!("projected price error: {e}"),
                                        &mut self.metrics.oracle_price_fallbacks,
                                        &mut self.metrics.oracle_candidate_hits,
                                        &mut self.metrics.oracle_enqueued,
                                        &mut self.metrics.oracle_coalesced,
                                        &mut self.metrics.oracle_max_pending,
                                        &mut self.oracle_detect_order,
                                        &mut self.oracle_detect_pending,
                                        &mut self.mt0_order_rx,
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            self.metrics.oracle_bank_projection_failed =
                                self.metrics.oracle_bank_projection_failed.saturating_add(1);
                            fallback_all(
                                &format!("projected state invalid: {e}"),
                                &mut self.metrics.oracle_price_fallbacks,
                                &mut self.metrics.oracle_candidate_hits,
                                &mut self.metrics.oracle_enqueued,
                                &mut self.metrics.oracle_coalesced,
                                &mut self.metrics.oracle_max_pending,
                                &mut self.oracle_detect_order,
                                &mut self.oracle_detect_pending,
                                &mut self.mt0_order_rx,
                            );
                        }
                    }
                }
                Err(e) => {
                    self.metrics.oracle_bank_projection_failed =
                        self.metrics.oracle_bank_projection_failed.saturating_add(1);
                    fallback_all(
                        &format!("simulation failed: {e}"),
                        &mut self.metrics.oracle_price_fallbacks,
                        &mut self.metrics.oracle_candidate_hits,
                        &mut self.metrics.oracle_enqueued,
                        &mut self.metrics.oracle_coalesced,
                        &mut self.metrics.oracle_max_pending,
                        &mut self.oracle_detect_order,
                        &mut self.oracle_detect_pending,
                        &mut self.mt0_order_rx,
                    );
                }
            }
        }

        #[cfg(feature = "crate_health")]
        while let Some(done) = self.remote.projection_results.try_pop() {
            self.projection_pending.remove(&done.account);
            let targeted_probe_result = self.integration_probe_expected.contains(&done.account);
            let mut targeted_status: Option<&'static str> = None;
            match done.result {
                Ok((projection_slot, projection_unix_s, accounts, units)) => {
                    let canonical = self.st.get(&done.account);
                    let base_matches = canonical
                        .map(|r| {
                            r.slot == done.base_slot && hash(&r.data) == done.base_account_hash
                        })
                        .unwrap_or(false);
                    // E9: a dependency revised while the worker projected makes it stale too.
                    let dependencies_match = dependency_fingerprint(&self.st, &done.dependencies)
                        == done.dependency_fingerprint;
                    if base_matches && !dependencies_match {
                        owner_log!(
                            self,
                            "projection_dependencies_changed: account={} at=result",
                            done.account
                        );
                    }
                    let canonical_matches = base_matches && dependencies_match;

                    // [FIX-116b] The candidate MarginfiAccount is readonly during the external
                    // refresh prefix and is therefore intentionally not requested as a post-account.
                    // Freshness is proven against canonical StateCore via exact slot+hash identity.
                    if canonical_matches {
                        if targeted_probe_result {
                            targeted_status = Some("ready");
                        }
                        self.projection_cache.insert(
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
                        self.detect_queue.push_front((
                            done.account,
                            done.t_rx_ns,
                            done.base_slot,
                            "integration_projection",
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
                        self.projection_cache.remove(&done.account);
                        let line = RecordEvent::ProjectionStale {
                            account: done.account,
                            base_slot: done.base_slot,
                            projection_slot,
                            canonical_matches,
                        };
                        self.evidence.emit(OwnerEvidence::Record(line))?;
                    }
                }
                Err(error) => {
                    if targeted_probe_result {
                        targeted_status = Some("failed");
                    }
                    self.projection_cache.remove(&done.account);
                    let line = RecordEvent::ProjectionFailed {
                        account: done.account,
                        base_slot: done.base_slot,
                        error,
                    };
                    self.evidence.emit(OwnerEvidence::Record(line))?;
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
