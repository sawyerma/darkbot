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
                mode: if self.threshold_authority() {
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
                blockhash_refused: self.blockhash_parking.stats.refused,
                blockhash_parked: self.blockhash_parking.len() as u64,
                blockhash_rearmed: self.blockhash_parking.stats.rearmed,
                blockhash_superseded: self.blockhash_parking.stats.superseded,
                blockhash_refresh_requests: self.blockhash_parking.stats.refresh_requests,
            },
            pipeline: self.pipeline_status(),
            safety: self.safety_status(),
            capabilities: self.capabilities(),
            wallets: self.wallets_status(),
            approval: self.approval_status(),
            finance: self.finance_status(),
            rebalance: self.rebalance.projection(self.st.highest_slot()),
            sources: self.sources_status(),
            last_source_decision: self.sources.last_decision.clone(),
            source_config_version: self.sources.config_version,
            capability_config_version: self.sources.capabilities.config_version,
            last_capability_decision: self.sources.capabilities.last_decision.clone(),
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
            {
                // DEC-13: the operator's request and the one effective decision (FE-06).
                let requested = self.sources.capabilities.live_requested;
                let (effective, block_reason) =
                    crate::orchestration::capability_control::live_effective(requested);
                cap("LIVE", requested, effective, block_reason)
            },
            // DEC-04: the B hold mode, requested and effective (ACTIVE gated by N and evidence).
            cap(
                crate::execution::hold_b::HOLD_B,
                self.sources.capabilities.hold_b_requested.code(),
                self.hold_gate.mode.code(),
                self.sources.capabilities.hold_b_block,
            ),
            cap(
                "SUBMISSION",
                "OFF",
                "NOT_AUTHORIZED",
                "observe-only adapter: no value-bearing approval",
            ),
            // DEC-11: the bundle path, requested and effective (never ACTIVE without a qualified
            // bundle simulator).
            cap(
                crate::execution::bundle_path::JITO_BUNDLE,
                self.sources.capabilities.jito_requested,
                self.sources.capabilities.jito_effective,
                self.sources.capabilities.jito_block,
            ),
            if cfg!(feature = "txv1") {
                cap("V1_MESSAGES", "ACTIVE", "ACTIVE", "")
            } else {
                cap("V1_MESSAGES", "OFF", "UNSUPPORTED", "built without txv1")
            },
            // A JIT plan is a bundle (buy + liquidation): only a bundle path builds and quotes it
            // (DEC-11); none is simulated without a qualified bundle simulator.
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
                    (true, true) => {
                        "JIT bundles are built and fee-quoted; no qualified bundle simulator (B02/B05), none is simulated or sent"
                    }
                    (true, false) => {
                        "JIT plans are bundles; the bundle path (JITO_BUNDLE) is OFF, every JIT plan is refused before simulation"
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
            // DEC-06: requested OFF|OBSERVE|ACTIVE, effective OFF|SWEEP|OBSERVE|ACTIVE.
            cap(
                crate::orchestration::threshold_mode::THRESHOLD_TRIGGER,
                self.threshold.requested.code(),
                self.threshold.effective.code(),
                self.threshold.block,
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
            non_landing: self.non_landing_projection(),
            hold_b: Some(self.hold_b_projection()),
        }
    }

    /// DEC-03: the Wallets non-landing status from the held journal and this run's latest read
    /// of the same subject; without a read in this run, a released subject shows its persisted
    /// release (its usable B operators, C negative).
    fn non_landing_projection(&self) -> Option<crate::status_tap::OwnerNonLanding> {
        use crate::execution::one_shot::non_landing::Verdict;
        let s = self.fe04_non_landing_status()?;
        let read = self
            .fe04_non_landing
            .latest
            .as_ref()
            .filter(|(exit, _)| *exit == s.exit)
            .map(|(_, r)| r);
        let now_ms = (self.clock.unix_ns(self.clock.now_ns()) / 1_000_000) as u64;
        Some(crate::status_tap::OwnerNonLanding {
            subject: if s.exit.is_some() { "EXIT" } else { "ATTEMPT" },
            result_class: s.class.map_or("NONE", Verdict::code),
            alarm: s.class == Some(Verdict::VetoContradiction),
            sources: match (read, &s.release) {
                (Some(r), _) => {
                    r.b.iter()
                        .map(|b| (b.operator.clone(), b.class.code()))
                        .collect()
                }
                (None, Some(release)) => release
                    .usable_operators
                    .iter()
                    .map(|op| (op.clone(), "USABLE".to_string()))
                    .collect(),
                (None, None) => vec![],
            },
            c_result: match (read, &s.release) {
                (Some(r), _) => r.c.code(),
                (None, Some(_)) => "NEGATIVE",
                (None, None) => "",
            },
            hold_ms: s
                .held_since_unix_ms
                .map(|since| now_ms.saturating_sub(since)),
            calls: read.map(|r| r.calls),
            bytes: read.map(|r| r.bytes),
            reads: if read.is_some() {
                self.fe04_non_landing.reads
            } else {
                0
            },
        })
    }

    /// DEC-04: the B hold mode with its evidence, Safety's reserved inventory and capital, and
    /// the held attempt's A, B and finalized flows (every asset of a prediction or of the deltas).
    fn hold_b_projection(&self) -> crate::status_tap::OwnerHoldB {
        use crate::execution::hold_b::{a_flows, compare, AssetFlow, BHold};
        use crate::status_tap::{OwnerHoldAttempt, OwnerHoldFlow};
        let rows = |predicted: &std::collections::BTreeMap<String, AssetFlow>,
                    deltas: Option<&std::collections::BTreeMap<String, i128>>|
         -> Vec<OwnerHoldFlow> {
            match deltas {
                Some(d) => compare(predicted, d)
                    .into_iter()
                    .map(|c| OwnerHoldFlow {
                        asset: c.asset,
                        reserve: c.reserve,
                        residual_max: c.residual_max,
                        actual: Some(c.actual),
                        underestimated: c.underestimated,
                    })
                    .collect(),
                None => predicted
                    .iter()
                    .map(|(asset, f)| OwnerHoldFlow {
                        asset: asset.clone(),
                        reserve: f.reserve,
                        residual_max: f.residual_max,
                        actual: None,
                        underestimated: false,
                    })
                    .collect(),
            }
        };
        let e = &self.hold_gate.evidence;
        let attempt = self
            .fe04_held
            .as_ref()
            .and_then(|j| j.attempt_holds())
            .and_then(|held| {
                let hold = held.hold?;
                let deltas = held.settled.map(|s| &s.wallet_deltas);
                let (b_class, b_reason, b_kind, b_inventory, flows) = match &hold.b {
                    BHold::Exact(f) => (
                        "EXACT",
                        String::new(),
                        f.kind.clone(),
                        Some(f.inventory_usd_micro),
                        rows(&f.flows, deltas),
                    ),
                    BHold::Unqualified(reason) => {
                        ("UNQUALIFIED", reason.clone(), String::new(), None, vec![])
                    }
                };
                Some(OwnerHoldAttempt {
                    mode: hold.mode.code(),
                    a_wallet: held.a_wallet.iter().map(|(a, u)| (a.clone(), *u)).collect(),
                    a_inventory_usd_micro: hold.a_inventory_usd_micro,
                    reserved_inventory_usd_micro: hold.reserved_inventory_usd_micro,
                    b_class,
                    b_reason,
                    b_kind,
                    b_inventory_usd_micro: b_inventory,
                    flows,
                    a_flows: rows(&a_flows(held.a_wallet), deltas),
                    settled: held.settled.is_some(),
                    actual_inventory_usd_micro: held
                        .settled
                        .map(|s| s.unrealized_inventory_usd_micro),
                })
            });
        crate::status_tap::OwnerHoldB {
            requested: self.sources.capabilities.hold_b_requested.code(),
            effective: self.hold_gate.mode.code(),
            block_reason: self.sources.capabilities.hold_b_block,
            n: self.hold_gate.n,
            evidence_exact: e.exact,
            evidence_underestimated: e.underestimated,
            evidence_unqualified: e.unqualified,
            inventory_reserved_usd_micro: self.safety.inventory_reserved(),
            capital_in_use_usd_micro: self.safety.capital_in_use(),
            attempt,
        }
    }

    /// DEC-10: the configured approval at this instant (expiry moves with the clock); without an
    /// FE-04 journal its single use is unchecked: never VALID then.
    fn approval_status(&self) -> crate::execution::one_shot::approval_file::ApprovalStatus {
        use crate::execution::one_shot::approval_file::{self, Usage};
        let Some(source) = &self.fe04_approval else {
            return approval_file::status(None, &Default::default(), 0, &Usage::Free);
        };
        let now_unix_s = (self.clock.unix_ns(self.clock.now_ns()) / 1_000_000_000) as u64;
        approval_file::status(
            Some(&source.read),
            &source.binding,
            now_unix_s,
            &source.usage,
        )
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
