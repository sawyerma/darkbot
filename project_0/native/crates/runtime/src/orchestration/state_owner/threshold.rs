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
}

impl ThresholdControl {
    pub(super) fn new(requested: Requested, min_ticks: Option<u64>) -> Self {
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
