//! P0-PROD-CTRL-04: the state owner's status projection (topics project0.liquidator.state,
//! project0.wallets.state, project0.finance.summary). Copies what the owner holds into the
//! installed status tap, at most every `owner_status_ms`; no protobuf, socket or file I/O here.
//! Unknown values stay `None` (never 0); gated capabilities show why they are blocked.
use super::*;
use crate::execution::accounting::Verdict;
use crate::execution::identity::Lane;
use crate::status_tap::{
    OwnerCapability, OwnerEngine, OwnerFinance, OwnerPipeline, OwnerSafety, OwnerSnapshot,
    OwnerTrigger, OwnerWallets,
};
use darkbot_common::lifecycle::RunState;

impl ShadowOwner {
    /// Publishes the projection into the installed status tap, at most every `owner_status_ms`;
    /// without a tap or a wall clock (D08) nothing is built.
    pub(super) fn publish_status(&mut self) {
        let Some(tap) = crate::status_tap::installed() else {
            return;
        };
        if self
            .status_published
            .is_some_and(|at| at.elapsed() < self.status_interval)
        {
            return;
        }
        let Some(at_unix_ms) = crate::status_tap::unix_ms_now() else {
            return;
        };
        self.status_published = Some(std::time::Instant::now());
        self.status_seq = self.status_seq.saturating_add(1);
        tap.publish_owner(self.owner_snapshot(at_unix_ms));
    }

    pub(super) fn owner_snapshot(&self, at_unix_ms: u64) -> OwnerSnapshot {
        OwnerSnapshot {
            at_unix_ms,
            run_id: self.run_id.clone(),
            mode: self.mode.clone(),
            wallet: self.liquidator.to_string(),
            program_id: self.program.to_string(),
            end_result: self.end_result.map(String::from),
            engine: self.engine_status(),
            trigger: OwnerTrigger {
                mode: if self.use_threshold_trigger {
                    "threshold"
                } else {
                    "sweep"
                },
                threshold_ready: self.threshold_ready,
                lanes: [Lane::L0, Lane::L1, Lane::L2]
                    .map(|lane| self.oracle_lanes.lane_len(lane) as u64),
                detect_queue: self.detect_queue.len() as u64,
                indexed_accounts: self.reverse_index.account_count() as u64,
                suppressed: self.metrics.candidate_cache_suppressed,
                softcap_suppressed: self.metrics.candidate_cache_softcap_suppressed,
            },
            pipeline: self.pipeline_status(),
            safety: self.safety_status(),
            capabilities: self.capabilities(),
            wallets: self.wallets_status(),
            finance: self.finance_status(),
            rebalance: self.rebalance.projection(self.st.highest_slot()),
            sources: self.sources_status(),
            last_source_decision: self.sources.last_decision.clone(),
            source_stats: crate::status_tap::OwnerSourceStats {
                observed: self.sources.stats.observed,
                rejected: self.sources.stats.rejected,
                applied: self.sources.stats.controls_applied,
                refused: self.sources.stats.controls_refused,
            },
        }
    }

    /// S1-02: every unit of the arbiter with its applied control.
    fn sources_status(&self) -> Vec<crate::status_tap::OwnerSource> {
        use darkbot_solana::ingress::sources::{Qualification, SourceRole};
        let canonical = self.sources.arbiter.canonical();
        self.sources
            .arbiter
            .controls()
            .into_iter()
            .map(|(c, generation)| crate::status_tap::OwnerSource {
                unit: super::sources::unit_name(&c.unit),
                role: match c.unit.role() {
                    SourceRole::Authoritative => "AUTHORITATIVE",
                    SourceRole::PreExecutionSensor => "PRE_EXECUTION_SENSOR",
                },
                mode: crate::orchestration::source_control::mode_name(c.mode),
                qualification: match &c.qualification {
                    Qualification::NotRun => "NOT_RUN".into(),
                    Qualification::LocalPass => "LOCAL_PASS".into(),
                    Qualification::Qualified { evidence } => format!("QUALIFIED:{evidence}"),
                    Qualification::Blocked(reason) => format!("BLOCKED:{reason}"),
                },
                generation,
                canonical: canonical == Some(&c.unit),
            })
            .collect()
    }

    fn engine_status(&self) -> OwnerEngine {
        let (run_state, halt_reason) = match self.st.run_state() {
            RunState::Bootstrapping => ("BOOTSTRAPPING", None),
            RunState::Reconciling => ("RECONCILING", None),
            RunState::Ready => ("READY", None),
            RunState::Halted(reason) => ("HALTED", Some(format!("{reason:?}"))),
        };
        OwnerEngine {
            run_state,
            halt_reason,
            ingress_generation: self.ingress_session.generation,
            reconnects: self.st.reconnects_applied(),
            gap_open: self.st.gap_open(),
            highest_slot: self.st.highest_slot(),
            ingress_halt_raised: self.halt.is_raised(),
            program_upgrade_latched: self.program_upgrade_latched,
            counters: self.st.counters(),
            program_pin: self.program_pin.clone(),
        }
    }

    fn pipeline_status(&self) -> OwnerPipeline {
        use darkbot_common::mt0::{Counter, RemoteClass, MT0};
        OwnerPipeline {
            evaluations: MT0.get(Counter::PipelineEvaluations),
            day: self.ledger.day().clone(),
            decision_p95_ns: self.ledger.rx_ready_p95_ns(),
            fee_quote_p95_ns: MT0.remote_latency(RemoteClass::FeeRpc).p95,
            simulation_p95_ns: MT0.remote_latency(RemoteClass::Simulation).p95,
        }
    }

    fn safety_status(&self) -> OwnerSafety {
        let limits = self.safety.limits();
        let day = self.safety.day();
        OwnerSafety {
            capital_in_use_usd_micro: self.safety.capital_in_use(),
            max_total_capital_usd_micro: limits.max_total_capital,
            max_daily_loss_usd_micro: limits.max_daily_loss,
            max_exposure_per_attempt_usd_micro: limits.max_exposure_per_attempt,
            max_concurrent_positions: limits.max_concurrent_positions,
            max_outstanding_attempts: limits.max_outstanding_attempts,
            max_failed_attempts_per_day: limits.max_failed_attempts_per_day,
            halt_reason: self.safety.halted().map(|r| format!("{r:?}")),
            day_realized_usd_micro: day.realized_pnl,
            day_failed_attempts: day.failed_attempts,
            day_attempts: day.attempts,
        }
    }

    /// P0-PROD-CTRL-03: requested vs effective of every switchable capability; a gate that
    /// blocks it names itself (never a fake ACTIVE).
    fn capabilities(&self) -> Vec<OwnerCapability> {
        let cap = |name, requested, effective, block_reason| OwnerCapability {
            name,
            requested,
            effective,
            block_reason,
        };
        let on = |b: bool| if b { "ACTIVE" } else { "OFF" };
        let jit = self.jit_funding_bank.is_some();
        let bundles = {
            use darkbot_solana::submit::SubmissionAdapter;
            self.submitter.supports_bundle()
        };
        let rebalancing = !self.inventory_targets.is_empty();
        let observing = self.rebalance.mode == crate::execution::inventory::RebalanceMode::Observe;
        vec![
            cap(
                "LIVE",
                "OFF",
                "BLOCKED",
                "FE-06/P8-LT: LIVE is not authorized (B10)",
            ),
            cap(
                "SUBMISSION",
                "OFF",
                "NOT_AUTHORIZED",
                "observe-only adapter: no value-bearing approval",
            ),
            cap(
                "JITO_BUNDLE",
                "OFF",
                "BLOCKED",
                "no qualified Jito adapter and bundle simulation (B02/B05/F-20)",
            ),
            if cfg!(feature = "txv1") {
                cap("V1_MESSAGES", "ACTIVE", "ACTIVE", "")
            } else {
                cap("V1_MESSAGES", "OFF", "UNSUPPORTED", "built without txv1")
            },
            // A JIT plan is a bundle (buy + liquidation): only a bundle path simulates it.
            cap(
                "JIT_EXACT_IN",
                on(jit),
                match (jit, bundles) {
                    (false, _) => "OFF",
                    (true, true) => "OBSERVE",
                    (true, false) => "BLOCKED",
                },
                match (jit, bundles) {
                    (false, _) => "",
                    (true, true) => "Shadow: JIT bundles are simulated, never sent",
                    (true, false) => {
                        "JIT plans are bundles; no qualified Jito bundle simulator (B02/B05/F-20), every JIT plan is refused before simulation"
                    }
                },
            ),
            cap(
                "ZERO_PREFUND",
                on(self.zero_prefund_enabled),
                if self.zero_prefund_enabled {
                    "OBSERVE"
                } else {
                    "OFF"
                },
                if self.zero_prefund_enabled {
                    "Shadow: plans are simulated, never sent"
                } else {
                    ""
                },
            ),
            // B01: OBSERVE plans with real balances, quotes and costs; trading needs LIVE (B10).
            cap(
                "INVENTORY_REBALANCING",
                self.rebalance.mode.name(),
                match (rebalancing, observing) {
                    (true, true) => "OBSERVE",
                    _ => "OFF",
                },
                if observing {
                    "Shadow: rebalancing is planned, never traded (ACTIVE needs LIVE, B10)"
                } else if rebalancing {
                    "bands reserve inventory; rebalancing OFF (P0_REBALANCE_MODE)"
                } else {
                    ""
                },
            ),
            cap(
                "THRESHOLD_TRIGGER",
                on(self.use_threshold_trigger),
                on(self.use_threshold_trigger),
                "",
            ),
        ]
    }

    fn wallets_status(&self) -> OwnerWallets {
        let mut holds: std::collections::BTreeMap<String, u64> = Default::default();
        for (_, attempt) in self.reservations.attempts() {
            for (asset, units) in attempt {
                let held = holds.entry(asset.clone()).or_default();
                *held = held.saturating_add(*units);
            }
        }
        let (journal_read, verdict, unknown_reasons, totals) = match &self.ledger_view {
            Some(Ok(view)) => (
                true,
                view.verdict.name().to_string(),
                match &view.verdict {
                    Verdict::Unknown(reasons) => reasons.clone(),
                    _ => vec![],
                },
                view.totals.clone(),
            ),
            Some(Err(e)) => (false, "UNKNOWN".into(), vec![e.clone()], Default::default()),
            None => (
                false,
                "UNKNOWN".into(),
                vec![if self.fe04_journal.is_some() {
                    "FE04_JOURNAL_NOT_READ_YET".into()
                } else {
                    "FE04_JOURNAL_NOT_CONFIGURED".into()
                }],
                Default::default(),
            ),
        };
        OwnerWallets {
            finance_snapshot_id: self.finance_snapshot_id(),
            journal_read,
            verdict,
            unknown_reasons,
            totals,
            holds,
            policy_reserves: crate::execution::inventory::reserves(&self.inventory_targets),
            held_attempt: self
                .fe04_held
                .as_ref()
                .and_then(|j| j.step_state())
                .map_or("NONE", |s| s.name()),
        }
    }

    /// The id both wallet topics carry for one publication.
    fn finance_snapshot_id(&self) -> String {
        format!("{}-{}", self.run_id, self.status_seq)
    }

    /// The finance summary: reserved capital from Safety, realized net from the settled steps;
    /// totals and withdrawable profit need reconciled wallet balances and stay unknown. VERIFIED
    /// only with every field known and every step settled; PARTIAL once the journal was read.
    fn finance_status(&self) -> OwnerFinance {
        let view = self.ledger_view.as_ref().and_then(|v| v.as_ref().ok());
        let mut f = OwnerFinance {
            as_of_slot: self.st.highest_slot(),
            total_capital_usd_micro: None,
            available_capital_usd_micro: None,
            reserved_capital_usd_micro: Some(i128::from(self.safety.capital_in_use())),
            realized_net_pnl_usd_micro: view.and_then(|v| v.totals.realized_net_usd_micro),
            withdrawable_profit_usd_micro: None,
            transferred_to_treasury_usd_micro: view.map(|_| 0),
            status: "UNKNOWN",
        };
        let complete = [
            f.total_capital_usd_micro,
            f.available_capital_usd_micro,
            f.reserved_capital_usd_micro,
            f.realized_net_pnl_usd_micro,
            f.withdrawable_profit_usd_micro,
            f.transferred_to_treasury_usd_micro,
        ]
        .iter()
        .all(Option::is_some);
        f.status = match view.map(|v| &v.verdict) {
            None => "UNKNOWN",
            Some(Verdict::Unknown(_)) => "PARTIAL",
            Some(_) if complete => "VERIFIED",
            Some(_) => "PARTIAL",
        };
        f
    }
}
