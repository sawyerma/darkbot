//! Coalesced exact-health fanout and candidate scheduling.
use super::*;

impl ShadowOwner {
    /// An R127 sweep batch is open: it flushes once full or older than its age bound. V2-03: in
    /// the threshold mode only a due time wake-up opens it (that mode fills no dirty banks and
    /// arms no census), so time alone re-evaluates exactly in both modes.
    #[cfg(feature = "crate_health")]
    pub(super) fn sweep_batch_open(&self) -> bool {
        self.st.run_state().is_tradable()
            && if self.threshold_authority() {
                self.wake_due()
            } else {
                !self.sweep_dirty_banks.is_empty()
                    || self.c0_census.as_ref().is_some_and(|c| c.armed.is_some())
                    || self.wake_due()
            }
    }

    /// P2-04/P2-05: a wake-up has passed on the chain clock.
    #[cfg(feature = "crate_health")]
    fn wake_due(&self) -> bool {
        chain_clock(&self.st).is_ok_and(|c| {
            self.time_schedule
                .earliest()
                .is_some_and(|at| at <= c.unix_s())
        })
    }

    pub(super) fn sweep(&mut self) {
        self.mt0_owner.enter(darkbot_common::mt0::OwnerCat::Sweep);
        // [R127c] Flush the accumulated Bank set only after an ingress burst is drained,
        // or after a bounded number of dependency events. This mirrors eva01's "drain updates,
        // dedupe affected accounts, then evaluate" behavior and prevents per-update fanout.
        #[cfg(feature = "crate_health")]
        if self.sweep_batch_open() {
            let batch_age_ns = self
                .sweep_batch_started_ns
                .map(|t| self.clock.now_ns().saturating_sub(t))
                .unwrap_or(0);
            if self.sweep_batch_events >= self.sweep_batch_max_events
                || batch_age_ns >= self.sweep_batch_max_age_ns
                || self.wake_due()
            {
                let dirty = std::mem::take(&mut self.sweep_dirty_banks);
                let owner_block_started = std::time::Instant::now();
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
                // [C0 Ticket 2] The armed census batch evaluates every debtor of the population
                // exactly once, including debtors outside the dirty-bank fan-out (for example on
                // an undecodable bank); the map deduplicates.
                if let Some(population) = self.c0_census.as_ref().and_then(|c| c.armed.as_ref()) {
                    let (t_rx_ns, slot) = (self.clock.now_ns(), self.slot_watch.get());
                    for debtor in &population.debtors {
                        account_work.entry(*debtor).or_insert((t_rx_ns, slot, 0));
                    }
                }

                // P2-04/P2-05 (0.1.12): an account whose wake-up passed is re-evaluated exactly
                // even without any bank or account event (time alone moved its health or its tag
                // premium); a tagged candidate is forwarded again, not held by its repeat interval.
                let mut due_accounts: HashSet<Pubkey> = HashSet::new();
                if let Ok(c) = chain_clock(&self.st) {
                    let (t_rx_ns, slot) = (self.clock.now_ns(), self.slot_watch.get());
                    let now = c.unix_s();
                    for due in self.time_schedule.pop_due(now) {
                        due_accounts.insert(due.account);
                        if due.trigger == darkbot_project0::schedule::Trigger::TagProfitable {
                            self.candidate_force_rearm.insert(due.account);
                        }
                        account_work
                            .entry(due.account)
                            .or_insert((t_rx_ns, slot, 0));
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
                // [C0-FIX-01] The armed startup batch also yields one census row per debtor.
                let census_debtors = self
                    .c0_census
                    .as_ref()
                    .and_then(|c| c.armed.as_ref())
                    .map(|p| &p.debtors);
                let census_batch = census_debtors.is_some();
                let batch_started = std::time::Instant::now();
                let program = self.program;
                let view = StateView(&self.st);

                // CPU-only and independent per account. No RPC, no state mutation.
                // P2-04: each evaluation also yields the account's premium boundary.
                type Evaluated = Vec<((Pubkey, u64, u64, u64, CandidateHealthState), Option<u64>)>;
                let (evaluated, census_rows): (Evaluated, Vec<Option<super::census::CensusRowOf>>) =
                    account_work
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
                            let member = census_debtors.is_some_and(|d| d.contains(&account));
                            // V2-14 P3-07: the premium decay once, typed, for both consumers (the
                            // wake-up boundary of a healthy account and the census row).
                            let healthy = matches!(&health, Ok(h) if !h.liquidatable);
                            let decay = (healthy || member).then(|| match &clock {
                                Ok(c) => engine.premium_decay_per_second(
                                    &account,
                                    c.unix_timestamp,
                                    c.slot,
                                ),
                                Err(e) => {
                                    Err(darkbot_project0::health::HealthError::Stale(e.to_string()))
                                }
                            });
                            // P2-04 (0.1.12): a healthy account with premium-active debt is woken
                            // when pending premium alone reaches its maintenance margin.
                            let premium_boundary = match (&health, &clock, &decay) {
                                (Ok(h), Ok(c), Some(Ok(decay))) if !h.liquidatable => {
                                    darkbot_project0::schedule::premium_boundary_unix_s(
                                        c.unix_s(),
                                        h.maintenance_health,
                                        *decay,
                                    )
                                }
                                _ => None,
                            };
                            let row = member.then(|| {
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
                                (
                                    account,
                                    view.0.get(&account).map(|r| r.slot),
                                    darkbot_project0::census::CensusRow::new(
                                        health,
                                        // A member's decay is always computed above.
                                        decay.unwrap_or_else(|| {
                                            Err(darkbot_project0::health::HealthError::Stale(
                                                "PREMIUM_DECAY_NOT_COMPUTED".into(),
                                            ))
                                        }),
                                        facts,
                                    ),
                                )
                            });
                            (
                                (
                                    (account, t_rx_ns, event_slot, input_revision, status),
                                    premium_boundary,
                                ),
                                row,
                            )
                        })
                        .unzip();
                let evaluated_ns = self.clock.now_ns();
                // P2-04: every evaluation renews or removes the account's premium wake-up.
                let mut evaluated: Vec<_> = evaluated
                    .into_iter()
                    .map(|(work, boundary)| {
                        self.time_schedule.upsert(
                            work.0,
                            darkbot_project0::schedule::Trigger::PremiumBoundary,
                            boundary,
                        );
                        work
                    })
                    .collect();
                // DEC-06 OBSERVE: this batch against the threshold trigger's wake-ups.
                self.threshold_compare(&evaluated, evaluated_ns, &due_accounts);
                if census_batch {
                    let rows = census_rows.into_iter().flatten().collect();
                    self.submit_c0_census(clock, rows, batch_started.elapsed());
                }

                // [FIX-135] Preserve bounded repeat work but remove deterministic starvation:
                // correctness work first; ordinary repeats oldest last-forwarded slot first;
                // pubkey is only the deterministic tie-breaker.
                // D12: every candidate's key exactly once (two map lookups each), not once for
                // the rotation check plus O(n log n) times inside the sort.
                let fairness_round =
                    order_by_fairness(&mut evaluated, |(account, _, _, input_revision, status)| {
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
                if fairness_round {
                    self.metrics.candidate_fairness_rotations =
                        self.metrics.candidate_fairness_rotations.saturating_add(1);
                }

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
                                self.oracle_lanes.len() < self.candidate_repeat_soft_cap;
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
                                // P6-MT.5 lane: forced re-arm (incl. a due P2-04/P2-05 wake-up)
                                // or class transition → L0; a changed canonical revision of an
                                // already relevant candidate → L1; a periodic repeat → L2.
                                let lane = if forced || transition {
                                    Lane::L0
                                } else if revision_changed {
                                    Lane::L1
                                } else {
                                    Lane::L2
                                };
                                let queue_admission = enqueue_or_coalesce_candidate(
                                    account,
                                    LaneAdmission {
                                        t_rx_ns,
                                        slot: event_slot,
                                        trigger: "oracle_health_ready",
                                        lane,
                                        revision: input_revision,
                                        last_forwarded_slot: last_forwarded,
                                    },
                                    &mut self.oracle_lanes,
                                    &mut self.metrics.oracle_enqueued,
                                    &mut self.metrics.oracle_coalesced,
                                    &mut self.metrics.oracle_max_pending,
                                );
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
                                self.oracle_lanes.len() < self.candidate_repeat_soft_cap;
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
                                // P6-MT.5 lane: an errored evaluation is Error/maintenance work
                                // → L2 (also when forced or a transition into Error: the sweep
                                // has no verified state to prioritise; the detect evaluates
                                // exactly, and L2 is starvation-free).
                                let queue_admission = enqueue_or_coalesce_candidate(
                                    account,
                                    LaneAdmission {
                                        t_rx_ns,
                                        slot: event_slot,
                                        trigger: "oracle_health_error",
                                        lane: Lane::L2,
                                        revision: input_revision,
                                        last_forwarded_slot: last_forwarded,
                                    },
                                    &mut self.oracle_lanes,
                                    &mut self.metrics.oracle_enqueued,
                                    &mut self.metrics.oracle_coalesced,
                                    &mut self.metrics.oracle_max_pending,
                                );
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
                // D3: the whole batch blocks the owner's ingress; measured, never estimated.
                let blocked_ns =
                    u64::try_from(owner_block_started.elapsed().as_nanos()).unwrap_or(u64::MAX);
                self.metrics.sweep_batch_ns_total =
                    self.metrics.sweep_batch_ns_total.saturating_add(blocked_ns);
                if blocked_ns > self.metrics.sweep_batch_ns_max {
                    self.metrics.sweep_batch_ns_max = blocked_ns;
                    self.metrics.sweep_batch_ns_max_accounts = unique;
                }
                // DEC-06: the batch is evaluated — ACTIVE's barrier is open now (under steady
                // oracle traffic a batch is always open at the loop's start).
                self.threshold_refresh("BATCH");
            }
        }
    }
}
