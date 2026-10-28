//! DEC-06: the owner's threshold mode — its requested and effective mode, generation, block
//! reason and the transitions between them (`orchestration::threshold_mode` decides).
use super::*;
use crate::orchestration::threshold_mode::{self as tm, Effective, Requested};

/// The owner's threshold mode and its OBSERVE evidence.
#[derive(Debug)]
pub(super) struct ThresholdControl {
    pub(super) requested: Requested,
    pub(super) effective: Effective,
    pub(super) block: &'static str,
    /// Bumped by every change of the effective mode or its block reason.
    pub(super) generation: u64,
    /// N (B-8, `threshold_min_ticks`); `None`: undecided, ACTIVE stays blocked.
    pub(super) min_ticks: Option<u64>,
    /// A miss in ACTIVE took ACTIVE back; only a new ACTIVE request lifts it.
    pub(super) latched: bool,
    /// The index was bootstrapped after its last (re)build.
    pub(super) bootstrapped: bool,
    pub(super) stats: tm::ThresholdStats,
    /// B-9: the owner time the threshold work beside the sweep may use.
    pub(super) budget: tm::ObserveBudget,
    /// OBSERVE: the threshold trigger's wake-ups of the open sweep batch.
    pub(super) batch: ObserveBatch,
}

/// OBSERVE: what the threshold trigger woke during the open sweep batch, on the same state and
/// input revisions the batch's flush evaluates.
#[derive(Debug, Default)]
pub(super) struct ObserveBatch {
    /// Woken accounts: t_rx of the waking oracle tick and the wake-up's time.
    pub(super) woken: HashMap<Pubkey, (u64, u64)>,
    /// The banks of the batch's oracle ticks.
    pub(super) oracle_banks: HashSet<Pubkey>,
    pub(super) ticks: u64,
    /// A tick was sampled out or could not be looked up (a price ACTIVE projects first).
    pub(super) uncomparable: bool,
    /// The generation of the batch's first tick.
    pub(super) generation: Option<u64>,
}

impl ThresholdControl {
    pub(super) fn new(
        requested: Requested,
        min_ticks: Option<u64>,
        budget_ppm: u64,
        now_ns: u64,
    ) -> Self {
        Self {
            requested,
            effective: if requested == Requested::Off {
                Effective::Off
            } else {
                Effective::Sweep
            },
            block: if requested == Requested::Off {
                ""
            } else {
                tm::STATE_NOT_TRADABLE
            },
            generation: 0,
            min_ticks,
            latched: false,
            bootstrapped: false,
            stats: Default::default(),
            budget: tm::ObserveBudget::new(budget_ppm, now_ns),
            batch: Default::default(),
        }
    }
}

impl ShadowOwner {
    /// The threshold trigger admits (effective ACTIVE); otherwise the sweep is authoritative.
    pub(super) fn threshold_authority(&self) -> bool {
        self.threshold.effective == Effective::Active
    }

    /// A threshold mode is requested: the owner keeps the threshold index.
    pub(super) fn threshold_indexing(&self) -> bool {
        self.threshold.requested != Requested::Off
    }

    /// A sweep batch is open: dirty banks wait for their flush (ACTIVE's barrier).
    pub(super) fn sweep_batch_pending(&self) -> bool {
        !self.sweep_dirty_banks.is_empty() || self.sweep_batch_started_ns.is_some()
    }

    /// Re-decides the effective mode; a change is a new generation with its record. Leaving
    /// ACTIVE re-sweeps every bank (nothing the threshold trigger did not wake is lost).
    pub(super) fn threshold_refresh(&mut self, cause: &'static str) {
        let t = &self.threshold;
        let (effective, block) = tm::decide(&tm::Inputs {
            requested: t.requested,
            tradable: self.st.run_state().is_tradable(),
            bootstrapped: t.bootstrapped,
            min_ticks: t.min_ticks,
            streak: t.stats.streak,
            latched: t.latched,
            // The barrier guards the way into ACTIVE only: in ACTIVE the sweep fills no batch.
            batch_open: t.effective != Effective::Active && self.sweep_batch_pending(),
        });
        if (effective, block) == (t.effective, t.block) {
            return;
        }
        let left_active = t.effective == Effective::Active && effective != Effective::Active;
        self.threshold.effective = effective;
        self.threshold.block = block;
        self.threshold.generation = self.threshold.generation.saturating_add(1);
        if left_active {
            self.threshold.stats.fallbacks = self.threshold.stats.fallbacks.saturating_add(1);
            let banks = self.mark_all_banks_dirty();
            owner_log!(
                self,
                "threshold_mode: ACTIVE left ({}), full sweep of {banks} banks",
                if block.is_empty() { cause } else { block }
            );
        }
        self.threshold_record(cause);
    }

    /// Journals the current threshold mode (`THRESHOLD_TRANSITION`).
    pub(super) fn threshold_record(&mut self, cause: &str) {
        let t = &self.threshold;
        let record = crate::analytics::evidence::ThresholdTransition {
            generation: t.generation,
            requested: t.requested.code().into(),
            effective: t.effective.code().into(),
            block_reason: t.block.into(),
            cause: cause.into(),
            ticks: t.stats.ticks,
            streak: t.stats.streak,
            latched: t.latched,
            at_unix_ms: crate::status_tap::unix_ms_now(),
        };
        owner_log!(
            self,
            "threshold_mode: generation={} requested={} effective={} block={} cause={cause}",
            record.generation,
            record.requested,
            record.effective,
            if record.block_reason.is_empty() {
                "-"
            } else {
                record.block_reason.as_str()
            }
        );
        let _ = self
            .evidence
            .emit(OwnerEvidence::ThresholdTransition(record));
    }

    /// Rebuilds the threshold index from nothing: with a requested mode every liability account
    /// of the group is queued for its exact thresholds (INDEX_BOOTSTRAP until done), without one
    /// the index stays empty.
    pub(super) fn threshold_index_reset(&mut self) {
        self.threshold_index = ThresholdIndex::new();
        self.threshold_fallback.clear();
        self.threshold_bootstrap.clear();
        if self.threshold_indexing() {
            for (pk, record) in self.st.iter_owned_by(&self.program) {
                if liability_account_in_group(&record.data, &self.group) {
                    self.threshold_bootstrap.push_back(*pk);
                }
            }
        }
        self.threshold_ready = self.threshold_bootstrap.is_empty();
        self.threshold.bootstrapped = self.threshold_ready && self.threshold_indexing();
    }

    /// [R127c] Marks every bank of the group dirty and forces the next sweep batch: one exact
    /// evaluation of every debtor (the startup sweep, and the full sweep after ACTIVE). Returns the
    /// banks marked.
    pub(super) fn mark_all_banks_dirty(&mut self) -> usize {
        let t_rx_ns = self.clock.now_ns();
        let slot = self.slot_watch.get();
        let view = StateView(&self.st);
        let mut banks = 0usize;
        for (pk, record) in self.st.iter_owned_by(&self.program) {
            let Some(bank) = bank_from_state(&view, &self.program, pk) else {
                continue;
            };
            if bank.group != self.group || classify_account(&record.data) != AccountType::Bank {
                continue;
            }
            self.sweep_dirty_banks
                .entry(*pk)
                .and_modify(|w| {
                    if slot > w.1 || (slot == w.1 && t_rx_ns > w.0) {
                        *w = (t_rx_ns, slot);
                    }
                })
                .or_insert((t_rx_ns, slot));
            banks = banks.saturating_add(1);
        }
        if banks > 0 {
            // Force one immediate batch; runtime batches use the event-count/time bounds.
            self.sweep_batch_events = self.sweep_batch_max_events;
            self.sweep_batch_started_ns.get_or_insert(t_rx_ns);
            self.metrics.sweep_max_dirty_banks = self
                .metrics
                .sweep_max_dirty_banks
                .max(self.sweep_dirty_banks.len());
        }
        banks
    }

    /// The threshold trigger's wake-ups on `bank` at the current state: the index's hits at the
    /// bank's maintenance-risk prices and its unknown-threshold accounts (V2-13: from the fallback
    /// set; the bank's list is never materialized). The one lookup of ACTIVE and OBSERVE.
    pub(super) fn threshold_hits(
        &self,
        bank: &Pubkey,
        clock: Result<ChainClock, &'static str>,
    ) -> Result<(Vec<Pubkey>, Vec<Pubkey>), darkbot_project0::health::HealthError> {
        let view = StateView(&self.st);
        let engine = darkbot_project0::health::crate_impl::CrateHealth {
            state: &view,
            group: self.group,
        };
        let (asset_price, liability_price) = clock
            .map_err(|e| darkbot_project0::health::HealthError::Stale(e.to_string()))
            .and_then(|c| engine.maintenance_risk_price_bits(bank, c.unix_timestamp, c.slot))?;
        Ok((
            self.threshold_index
                .candidates(bank, asset_price, liability_price, 0),
            self.threshold_fallback
                .iter()
                .copied()
                .filter(|a| self.reverse_index.bank_has_account(bank, a))
                .collect(),
        ))
    }

    /// Charges one unit of threshold work beside the sweep to the OBSERVE budget (B-9).
    pub(super) fn threshold_charge(&mut self, started: std::time::Instant) {
        let ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let s = &mut self.threshold.stats;
        self.threshold.budget.charge(ns);
        s.observe_owner_ns.record(ns);
        s.observe_owner_ns_total = s.observe_owner_ns_total.saturating_add(ns);
    }

    /// Threshold work may run now: ACTIVE's always (it is the authoritative path), the work
    /// beside the sweep only within its budget — a refusal is counted sampled.
    pub(super) fn threshold_work_allowed(&mut self) -> bool {
        if self.threshold_authority() || self.threshold.budget.allows(self.clock.now_ns()) {
            return true;
        }
        self.threshold.stats.sampled_out = self.threshold.stats.sampled_out.saturating_add(1);
        false
    }

    /// An account's thresholds could not be kept up to date for want of budget: it is
    /// re-checked bank-wide until the bootstrap queue evaluates it (conservative, never stale).
    pub(super) fn threshold_defer(&mut self, account: Pubkey) {
        self.threshold_index.remove_account(&account);
        self.threshold_fallback.insert(account);
        self.threshold_bootstrap.push_back(account);
        self.threshold_ready = false;
    }

    /// OBSERVE: one oracle tick of the threshold trigger beside the sweep — the ACTIVE lookup
    /// on the same state, recorded as wake-ups of the open batch, never admitted.
    pub(super) fn threshold_observe_tick(
        &mut self,
        banks: &[Pubkey],
        t_rx_ns: u64,
        clock: Result<ChainClock, &'static str>,
    ) {
        let generation = self.threshold.generation;
        let b = &mut self.threshold.batch;
        b.ticks = b.ticks.saturating_add(1);
        b.oracle_banks.extend(banks.iter().copied());
        if *b.generation.get_or_insert(generation) != generation {
            b.uncomparable = true;
        }
        if !self.threshold_work_allowed() {
            self.threshold.batch.uncomparable = true;
            return;
        }
        let started = std::time::Instant::now();
        let mut woken: Vec<Pubkey> = Vec::new();
        let mut uncomparable = false;
        for bank in banks {
            if !self.threshold_ready {
                // As ACTIVE: an index still bootstrapping re-checks the whole bank.
                woken.extend(self.reverse_index.accounts_for_bank(bank).copied());
                continue;
            }
            match self.threshold_hits(bank, clock) {
                Ok((hits, rechecks)) => {
                    woken.extend(hits);
                    woken.extend(rechecks);
                }
                // ACTIVE would project the bank's price first (R126): not comparable.
                Err(_) => uncomparable = true,
            }
        }
        let woke_ns = self.clock.now_ns();
        let b = &mut self.threshold.batch;
        b.uncomparable |= uncomparable;
        for account in woken {
            b.woken.entry(account).or_insert((t_rx_ns, woke_ns));
        }
        self.threshold_charge(started);
    }

    /// OBSERVE: the flushed sweep batch against the threshold trigger's wake-ups of the same
    /// ticks. Of the accounts on the ticks' banks (time wake-ups excluded: both paths consume
    /// them), a liquidatable one is a hit when woken, a miss (recorded) when not; a woken
    /// healthy one is extra. Only a comparable batch counts: every tick looked up, one
    /// generation.
    pub(super) fn threshold_compare(
        &mut self,
        evaluated: &[(Pubkey, u64, u64, u64, CandidateHealthState)],
        evaluated_ns: u64,
        due: &HashSet<Pubkey>,
    ) {
        let batch = std::mem::take(&mut self.threshold.batch);
        if batch.ticks == 0 {
            return;
        }
        let comparable = !batch.uncomparable && batch.generation == Some(self.threshold.generation);
        if !comparable || !self.threshold_work_allowed() {
            self.threshold.stats.batch(batch.ticks, false, 0);
            return;
        }
        let started = std::time::Instant::now();
        let (mut hits, mut extra, mut misses) = (0u64, 0u64, Vec::new());
        for (account, t_rx_ns, slot, revision, status) in evaluated {
            if due.contains(account)
                || !self
                    .reverse_index
                    .banks_for_account(account)
                    .any(|b| batch.oracle_banks.contains(b))
            {
                continue;
            }
            let woken = batch.woken.get(account);
            match status {
                CandidateHealthState::Liquidatable => {
                    let s = &mut self.threshold.stats;
                    s.sweep_detect_ns
                        .record(evaluated_ns.saturating_sub(*t_rx_ns));
                    match woken {
                        Some((woke_t_rx, woke_ns)) => {
                            hits += 1;
                            s.threshold_detect_ns
                                .record(woke_ns.saturating_sub(*woke_t_rx));
                        }
                        None => misses.push((*account, *slot, *revision)),
                    }
                }
                CandidateHealthState::Healthy if woken.is_some() => extra += 1,
                _ => {}
            }
        }
        let s = &mut self.threshold.stats;
        s.hits = s.hits.saturating_add(hits);
        s.extra = s.extra.saturating_add(extra);
        s.misses = s.misses.saturating_add(misses.len() as u64);
        s.batch(batch.ticks, true, misses.len() as u64);
        for (account, slot, input_revision) in misses {
            self.threshold_miss(account, slot, input_revision, false);
        }
        self.threshold_charge(started);
    }

    /// Journals a liquidatable account the threshold trigger did not wake (`THRESHOLD_MISS`);
    /// `alarm`: found by ACTIVE's cross-check.
    pub(super) fn threshold_miss(
        &mut self,
        account: Pubkey,
        slot: u64,
        input_revision: u64,
        alarm: bool,
    ) {
        let phase = if alarm { "ACTIVE" } else { "OBSERVE" };
        owner_log!(
            self,
            "threshold_miss: phase={phase} account={account} slot={slot} revision={input_revision} generation={}",
            self.threshold.generation
        );
        let _ = self.evidence.emit(OwnerEvidence::ThresholdMiss(
            crate::analytics::evidence::ThresholdMiss {
                generation: self.threshold.generation,
                phase: phase.into(),
                account,
                slot,
                input_revision,
                alarm,
                at_unix_ms: crate::status_tap::unix_ms_now(),
            },
        ));
    }

    /// Journals the threshold mode's cumulative counters (`THRESHOLD_STATS`).
    pub(super) fn threshold_stats_record(&mut self) {
        use crate::analytics::evidence::LatencyNs;
        let t = &self.threshold;
        let s = &t.stats;
        let record = crate::analytics::evidence::ThresholdStatsRecord {
            generation: t.generation,
            requested: t.requested.code().into(),
            effective: t.effective.code().into(),
            block_reason: t.block.into(),
            ticks: s.ticks,
            streak: s.streak,
            uncomparable_ticks: s.uncomparable_ticks,
            hits: s.hits,
            misses: s.misses,
            extra: s.extra,
            sampled_out: s.sampled_out,
            fallbacks: s.fallbacks,
            index_evaluations: s.index_evaluations,
            crosscheck_samples: s.crosscheck_samples,
            crosscheck_skipped: s.crosscheck_skipped,
            crosscheck_stale: s.crosscheck_stale,
            crosscheck_misses: s.crosscheck_misses,
            budget_ppm: t.budget.ppm(),
            sweep_detect_ns: LatencyNs::of(&s.sweep_detect_ns),
            threshold_detect_ns: LatencyNs::of(&s.threshold_detect_ns),
            observe_owner_ns: LatencyNs::of(&s.observe_owner_ns),
            observe_owner_ns_total: s.observe_owner_ns_total,
        };
        let _ = self
            .evidence
            .emit(OwnerEvidence::ThresholdStats(Box::new(record)));
    }

    /// Test seam: the threshold trigger effective ACTIVE on a bootstrapped index, qualified by
    /// an N of 1 (it stays ACTIVE under every refresh of a tradable state).
    #[cfg(test)]
    pub(super) fn threshold_test_active(&mut self) {
        self.threshold.requested = Requested::Active;
        self.threshold.min_ticks = Some(1);
        self.threshold.stats.streak = 1;
        self.threshold.effective = Effective::Active;
        self.threshold.block = "";
        self.threshold.bootstrapped = true;
        self.threshold_ready = true;
    }
}
