//! Coalesced exact-health fanout and candidate scheduling.
use super::*;

impl ShadowOwner {
    pub(super) fn sweep(&mut self) {
        self.mt0_owner.enter(darkbot_common::mt0::OwnerCat::Sweep);
        // [R127c] Flush the accumulated Bank set only after an ingress burst is drained,
        // or after a bounded number of dependency events. This mirrors eva01's "drain updates,
        // dedupe affected accounts, then evaluate" behavior and prevents per-update fanout.
        #[cfg(feature = "crate_health")]
        if !self.use_threshold_trigger
            && self.st.run_state().is_tradable()
            && !self.sweep_dirty_banks.is_empty()
        {
            let batch_age_ns = self
                .sweep_batch_started_ns
                .map(|t| self.clock.now_ns().saturating_sub(t))
                .unwrap_or(0);
            if self.sweep_batch_events >= self.sweep_batch_max_events
                || batch_age_ns >= self.sweep_batch_max_age_ns
            {
                let dirty = std::mem::take(&mut self.sweep_dirty_banks);
                self.sweep_batch_events = 0;
                self.sweep_batch_started_ns = None;
                self.metrics.sweep_batches = self.metrics.sweep_batches.saturating_add(1);

                // [FIX-134] Carry the newest causal Bank/dependency revision into
                // each deduped Account evaluation. Multiple dirty Banks collapse to one Account
                // while retaining the newest revision and newest event timing.
                let mut account_work: HashMap<Pubkey, (u64, u64, u64)> = HashMap::new();
                let mut refs = 0usize;
                for (bank, (t_rx_ns, event_slot)) in dirty {
                    let input_revision = self.bank_input_revision.get(&bank).copied().unwrap_or(0);
                    for account in self.reverse_index.accounts_for_bank(&bank).copied() {
                        refs = refs.saturating_add(1);
                        account_work
                            .entry(account)
                            .and_modify(|w| {
                                if event_slot > w.1 || (event_slot == w.1 && t_rx_ns > w.0) {
                                    w.0 = t_rx_ns;
                                    w.1 = event_slot;
                                }
                                w.2 = w.2.max(input_revision);
                            })
                            .or_insert((t_rx_ns, event_slot, input_revision));
                    }
                }

                let unique = account_work.len();
                self.metrics.sweep_account_refs =
                    self.metrics.sweep_account_refs.saturating_add(refs as u64);
                self.metrics.sweep_unique_accounts = self
                    .metrics
                    .sweep_unique_accounts
                    .saturating_add(unique as u64);
                self.metrics.sweep_max_unique_batch =
                    self.metrics.sweep_max_unique_batch.max(unique);
                self.metrics.oracle_candidate_hits = self
                    .metrics
                    .oracle_candidate_hits
                    .saturating_add(unique as u64);
                self.metrics.health_sweep_checks = self
                    .metrics
                    .health_sweep_checks
                    .saturating_add(unique as u64);

                // [C0-FIX-03] One canonical chain clock for the whole batch; without it every
                // account is an evaluation error, never a local-time or zero-time result.
                let clock = chain_clock(&self.st);
                let evaluation_slot = clock.map_or_else(|_| self.slot_watch.get(), |c| c.slot);
                // [C0-FIX-01] The armed startup batch also yields one census row per account.
                let census_batch = self.c0_census.as_ref().is_some_and(|c| c.armed.is_some());
                let batch_started = std::time::Instant::now();
                let program = self.program;
                let view = StateView(&self.st);

                // CPU-only and independent per account. No RPC, no state mutation.
                let (mut evaluated, census_rows): (
                    Vec<(Pubkey, u64, u64, u64, CandidateHealthState)>,
                    Vec<Option<darkbot_project0::census::CensusRow>>,
                ) = account_work
                    .into_par_iter()
                    .map(|(account, (t_rx_ns, event_slot, input_revision))| {
                        let engine = darkbot_project0::health::crate_impl::CrateHealth {
                            state: &view,
                            group: self.group,
                        };
                        darkbot_common::mt0::MT0
                            .count_health(darkbot_common::mt0::HealthSite::Sweep); // [MT0]
                        let health = match &clock {
                            Ok(c) => engine.health(&account, c.unix_timestamp, c.slot),
                            Err(e) => {
                                Err(darkbot_project0::health::HealthError::Stale(e.to_string()))
                            }
                        };
                        let status = match &health {
                            Ok(h) if !h.liquidatable => CandidateHealthState::Healthy,
                            Ok(_) => CandidateHealthState::Liquidatable,
                            Err(_) => CandidateHealthState::Error,
                        };
                        let row = census_batch.then(|| {
                            let facts = engine
                                .observation_accounts(&account)
                                .map_err(|e| e.to_string())
                                .and_then(|o| {
                                    darkbot_project0::census::account_facts(
                                        &view,
                                        &program,
                                        &account,
                                        o.len(),
                                    )
                                });
                            darkbot_project0::census::CensusRow::new(
                                health,
                                engine.premium_decay_per_second(&account),
                                facts,
                            )
                        });
                        ((account, t_rx_ns, event_slot, input_revision, status), row)
                    })
                    .unzip();
                if census_batch {
                    let rows = census_rows.into_iter().flatten().collect();
                    self.submit_c0_census(clock, rows, batch_started.elapsed());
                }

                // [FIX-135] Preserve bounded repeat work but remove deterministic starvation:
                // correctness work first; ordinary repeats oldest last-forwarded slot first;
                // pubkey is only the deterministic tie-breaker.
                let fairness_round =
                    evaluated
                        .iter()
                        .any(|(account, _, _, input_revision, status)| {
                            let previous = self.candidate_cache.get(account).copied();
                            let forced = self.candidate_force_rearm.contains(account);
                            let recheck_slots = match status {
                                CandidateHealthState::Liquidatable => self.candidate_recheck_slots,
                                CandidateHealthState::Error => self.candidate_error_recheck_slots,
                                CandidateHealthState::Healthy => u64::MAX,
                            };
                            candidate_fairness_key(
                                *account,
                                previous,
                                *status,
                                evaluation_slot,
                                *input_revision,
                                forced,
                                recheck_slots,
                            )
                            .0 == 1
                        });
                if fairness_round {
                    self.metrics.candidate_fairness_rotations =
                        self.metrics.candidate_fairness_rotations.saturating_add(1);
                }
                evaluated.sort_unstable_by_key(|(account, _, _, input_revision, status)| {
                    let previous = self.candidate_cache.get(account).copied();
                    let forced = self.candidate_force_rearm.contains(account);
                    let recheck_slots = match status {
                        CandidateHealthState::Liquidatable => self.candidate_recheck_slots,
                        CandidateHealthState::Error => self.candidate_error_recheck_slots,
                        CandidateHealthState::Healthy => u64::MAX,
                    };
                    candidate_fairness_key(
                        *account,
                        previous,
                        *status,
                        evaluation_slot,
                        *input_revision,
                        forced,
                        recheck_slots,
                    )
                });

                for (account, t_rx_ns, event_slot, input_revision, status) in evaluated {
                    let previous = self.candidate_cache.get(&account).copied();
                    let forced = self.candidate_force_rearm.remove(&account);

                    match status {
                        CandidateHealthState::Healthy => {
                            self.metrics.health_sweep_healthy =
                                self.metrics.health_sweep_healthy.saturating_add(1);
                            self.candidate_cache.insert(
                                account,
                                CandidateCacheEntry {
                                    state: CandidateHealthState::Healthy,
                                    last_forwarded_slot: previous
                                        .map(|e| e.last_forwarded_slot)
                                        .unwrap_or(0),
                                    last_forwarded_revision: previous
                                        .map(|e| e.last_forwarded_revision)
                                        .unwrap_or(0),
                                },
                            );
                        }
                        CandidateHealthState::Liquidatable => {
                            self.metrics.health_sweep_liquidatable =
                                self.metrics.health_sweep_liquidatable.saturating_add(1);

                            let transition = !matches!(
                                previous,
                                Some(CandidateCacheEntry {
                                    state: CandidateHealthState::Liquidatable,
                                    ..
                                })
                            );
                            let last_forwarded =
                                previous.map(|e| e.last_forwarded_slot).unwrap_or(0);
                            let wait_slots = evaluation_slot.saturating_sub(last_forwarded);
                            let periodic_due = wait_slots >= self.candidate_recheck_slots;
                            let revision_changed = input_revision
                                > previous.map(|e| e.last_forwarded_revision).unwrap_or(0);
                            if periodic_due && !transition && !forced && !revision_changed {
                                self.metrics.candidate_repeat_wait_slots_max =
                                    self.metrics.candidate_repeat_wait_slots_max.max(wait_slots);
                                match wait_slots {
                                    0 => {}
                                    1 => {
                                        self.metrics.candidate_repeat_wait_slots_1 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_1
                                            .saturating_add(1)
                                    }
                                    2..=3 => {
                                        self.metrics.candidate_repeat_wait_slots_2_3 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_2_3
                                            .saturating_add(1)
                                    }
                                    4..=7 => {
                                        self.metrics.candidate_repeat_wait_slots_4_7 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_4_7
                                            .saturating_add(1)
                                    }
                                    8..=15 => {
                                        self.metrics.candidate_repeat_wait_slots_8_15 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_8_15
                                            .saturating_add(1)
                                    }
                                    _ => {
                                        self.metrics.candidate_repeat_wait_slots_ge_16 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_ge_16
                                            .saturating_add(1)
                                    }
                                }
                            }
                            let repeat_capacity =
                                self.oracle_detect_pending.len() < self.candidate_repeat_soft_cap;
                            let should_forward = candidate_should_forward(
                                previous,
                                CandidateHealthState::Liquidatable,
                                evaluation_slot,
                                input_revision,
                                self.candidate_recheck_slots,
                                forced,
                                repeat_capacity,
                            );

                            if should_forward {
                                self.metrics.sweep_forwarded =
                                    self.metrics.sweep_forwarded.saturating_add(1);
                                if forced {
                                    self.metrics.candidate_cache_forced_forwards = self
                                        .metrics
                                        .candidate_cache_forced_forwards
                                        .saturating_add(1);
                                } else if transition {
                                    self.metrics.candidate_cache_transition_forwards = self
                                        .metrics
                                        .candidate_cache_transition_forwards
                                        .saturating_add(1);
                                } else if revision_changed {
                                    self.metrics.candidate_revision_forwards =
                                        self.metrics.candidate_revision_forwards.saturating_add(1);
                                } else {
                                    self.metrics.candidate_cache_periodic_forwards = self
                                        .metrics
                                        .candidate_cache_periodic_forwards
                                        .saturating_add(1);
                                }
                                let mt0_len_before = self.oracle_detect_order.len(); // [MT0]
                                let queue_admission = enqueue_or_coalesce_candidate(
                                    account,
                                    t_rx_ns,
                                    event_slot,
                                    "oracle_health_ready",
                                    &mut self.oracle_detect_order,
                                    &mut self.oracle_detect_pending,
                                    &mut self.metrics.oracle_enqueued,
                                    &mut self.metrics.oracle_coalesced,
                                    &mut self.metrics.oracle_max_pending,
                                );
                                for _ in mt0_len_before..self.oracle_detect_order.len() {
                                    self.mt0_order_rx.push_back(t_rx_ns);
                                }
                                match queue_admission {
                                    CandidateQueueAdmission::Enqueued => {
                                        self.metrics.candidate_physical_enqueues = self
                                            .metrics
                                            .candidate_physical_enqueues
                                            .saturating_add(1);
                                    }
                                    CandidateQueueAdmission::Coalesced => {
                                        self.metrics.candidate_pending_coalesced = self
                                            .metrics
                                            .candidate_pending_coalesced
                                            .saturating_add(1);
                                        if revision_changed {
                                            self.metrics.candidate_revision_coalesced = self
                                                .metrics
                                                .candidate_revision_coalesced
                                                .saturating_add(1);
                                        }
                                    }
                                }
                                self.candidate_cache.insert(
                                    account,
                                    CandidateCacheEntry {
                                        state: CandidateHealthState::Liquidatable,
                                        last_forwarded_slot: evaluation_slot,
                                        last_forwarded_revision: input_revision,
                                    },
                                );
                            } else {
                                self.metrics.candidate_cache_suppressed =
                                    self.metrics.candidate_cache_suppressed.saturating_add(1);
                                if periodic_due && !repeat_capacity {
                                    self.metrics.candidate_cache_softcap_suppressed = self
                                        .metrics
                                        .candidate_cache_softcap_suppressed
                                        .saturating_add(1);
                                    self.metrics.candidate_periodic_due_but_deferred = self
                                        .metrics
                                        .candidate_periodic_due_but_deferred
                                        .saturating_add(1);
                                }
                                self.candidate_cache.insert(
                                    account,
                                    CandidateCacheEntry {
                                        state: CandidateHealthState::Liquidatable,
                                        last_forwarded_slot: last_forwarded,
                                        last_forwarded_revision: previous
                                            .map(|e| e.last_forwarded_revision)
                                            .unwrap_or(0),
                                    },
                                );
                            }
                        }
                        CandidateHealthState::Error => {
                            // Fail closed, but do not hammer the exact same stale/error Account
                            // through refresh/projection every 5 ms. Error state gets a shorter
                            // bounded recheck cadence than a stable liquidatable candidate.
                            self.metrics.health_sweep_errors =
                                self.metrics.health_sweep_errors.saturating_add(1);

                            let transition = !matches!(
                                previous,
                                Some(CandidateCacheEntry {
                                    state: CandidateHealthState::Error,
                                    ..
                                })
                            );
                            let last_forwarded =
                                previous.map(|e| e.last_forwarded_slot).unwrap_or(0);
                            let wait_slots = evaluation_slot.saturating_sub(last_forwarded);
                            let periodic_due = wait_slots >= self.candidate_error_recheck_slots;
                            let revision_changed = input_revision
                                > previous.map(|e| e.last_forwarded_revision).unwrap_or(0);
                            if periodic_due && !transition && !forced && !revision_changed {
                                self.metrics.candidate_repeat_wait_slots_max =
                                    self.metrics.candidate_repeat_wait_slots_max.max(wait_slots);
                                match wait_slots {
                                    0 => {}
                                    1 => {
                                        self.metrics.candidate_repeat_wait_slots_1 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_1
                                            .saturating_add(1)
                                    }
                                    2..=3 => {
                                        self.metrics.candidate_repeat_wait_slots_2_3 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_2_3
                                            .saturating_add(1)
                                    }
                                    4..=7 => {
                                        self.metrics.candidate_repeat_wait_slots_4_7 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_4_7
                                            .saturating_add(1)
                                    }
                                    8..=15 => {
                                        self.metrics.candidate_repeat_wait_slots_8_15 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_8_15
                                            .saturating_add(1)
                                    }
                                    _ => {
                                        self.metrics.candidate_repeat_wait_slots_ge_16 = self
                                            .metrics
                                            .candidate_repeat_wait_slots_ge_16
                                            .saturating_add(1)
                                    }
                                }
                            }
                            let repeat_capacity =
                                self.oracle_detect_pending.len() < self.candidate_repeat_soft_cap;
                            let should_forward = candidate_should_forward(
                                previous,
                                CandidateHealthState::Error,
                                evaluation_slot,
                                input_revision,
                                self.candidate_error_recheck_slots,
                                forced,
                                repeat_capacity,
                            );

                            if should_forward {
                                self.metrics.sweep_forwarded =
                                    self.metrics.sweep_forwarded.saturating_add(1);
                                if forced {
                                    self.metrics.candidate_cache_forced_forwards = self
                                        .metrics
                                        .candidate_cache_forced_forwards
                                        .saturating_add(1);
                                } else if transition {
                                    self.metrics.candidate_cache_transition_forwards = self
                                        .metrics
                                        .candidate_cache_transition_forwards
                                        .saturating_add(1);
                                } else if revision_changed {
                                    self.metrics.candidate_revision_forwards =
                                        self.metrics.candidate_revision_forwards.saturating_add(1);
                                } else {
                                    self.metrics.candidate_cache_periodic_forwards = self
                                        .metrics
                                        .candidate_cache_periodic_forwards
                                        .saturating_add(1);
                                }
                                let mt0_len_before = self.oracle_detect_order.len(); // [MT0]
                                let queue_admission = enqueue_or_coalesce_candidate(
                                    account,
                                    t_rx_ns,
                                    event_slot,
                                    "oracle_health_error",
                                    &mut self.oracle_detect_order,
                                    &mut self.oracle_detect_pending,
                                    &mut self.metrics.oracle_enqueued,
                                    &mut self.metrics.oracle_coalesced,
                                    &mut self.metrics.oracle_max_pending,
                                );
                                for _ in mt0_len_before..self.oracle_detect_order.len() {
                                    self.mt0_order_rx.push_back(t_rx_ns);
                                }
                                match queue_admission {
                                    CandidateQueueAdmission::Enqueued => {
                                        self.metrics.candidate_physical_enqueues = self
                                            .metrics
                                            .candidate_physical_enqueues
                                            .saturating_add(1);
                                    }
                                    CandidateQueueAdmission::Coalesced => {
                                        self.metrics.candidate_pending_coalesced = self
                                            .metrics
                                            .candidate_pending_coalesced
                                            .saturating_add(1);
                                        if revision_changed {
                                            self.metrics.candidate_revision_coalesced = self
                                                .metrics
                                                .candidate_revision_coalesced
                                                .saturating_add(1);
                                        }
                                    }
                                }
                                self.candidate_cache.insert(
                                    account,
                                    CandidateCacheEntry {
                                        state: CandidateHealthState::Error,
                                        last_forwarded_slot: evaluation_slot,
                                        last_forwarded_revision: input_revision,
                                    },
                                );
                            } else {
                                self.metrics.candidate_cache_suppressed =
                                    self.metrics.candidate_cache_suppressed.saturating_add(1);
                                if periodic_due && !repeat_capacity {
                                    self.metrics.candidate_cache_softcap_suppressed = self
                                        .metrics
                                        .candidate_cache_softcap_suppressed
                                        .saturating_add(1);
                                    self.metrics.candidate_periodic_due_but_deferred = self
                                        .metrics
                                        .candidate_periodic_due_but_deferred
                                        .saturating_add(1);
                                }
                                self.candidate_cache.insert(
                                    account,
                                    CandidateCacheEntry {
                                        state: CandidateHealthState::Error,
                                        last_forwarded_slot: last_forwarded,
                                        last_forwarded_revision: previous
                                            .map(|e| e.last_forwarded_revision)
                                            .unwrap_or(0),
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
