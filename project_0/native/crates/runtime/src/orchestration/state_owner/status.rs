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
            wallet: self.pilot().liquidator.to_string(),
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
                threshold: self.threshold_status(),
            },
            pipeline: self.pipeline_status(),
            // DEC-17b: the pilot's Safety, journal, approval and admission; every wallet's in
            // the Wallets rows.
            safety: self.safety_status(0),
            capabilities: self.capabilities(),
            wallets: self.wallets_status(0),
            approval: self.approval_status(0),
            admission: self.admission_status(0),
            finance: self.finance_status(),
            rebalance: self.rebalance.projection(self.st.highest_slot()),
            sources: self.sources_status(),
            last_source_decision: self.sources.last_decision.clone(),
            source_config_version: self.sources.config_version,
            capability_config_version: self.sources.capabilities.config_version,
            last_capability_decision: self.sources.capabilities.last_decision.clone(),
            capability_epoch: self.sources.capabilities.epoch,
            fe06: self.fe06_status(),
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

    fn safety_status(&self, i: usize) -> OwnerSafety {
        let limits = self.wallets[i].safety.limits();
        let day = self.wallets[i].safety.day();
        OwnerSafety {
            capital_in_use_usd_micro: self.wallets[i].safety.capital_in_use(),
            max_total_capital_usd_micro: limits.max_total_capital,
            max_daily_loss_usd_micro: limits.max_daily_loss,
            max_exposure_per_attempt_usd_micro: limits.max_exposure_per_attempt,
            max_concurrent_positions: limits.max_concurrent_positions,
            max_outstanding_attempts: limits.max_outstanding_attempts,
            max_failed_attempts_per_day: limits.max_failed_attempts_per_day,
            halt_reason: self.wallets[i].safety.halted().map(|r| format!("{r:?}")),
            day_realized_usd_micro: day.realized_pnl,
            day_failed_attempts: day.failed_attempts,
            day_attempts: day.attempts,
        }
    }

    /// P0-PROD-CTRL-03: requested vs effective of every switchable capability; a gate that
    /// blocks it names itself (never a fake ACTIVE).
    fn capabilities(&self) -> Vec<OwnerCapability> {
        let cap = |name: &'static str,
                   requested: &'static str,
                   effective: &'static str,
                   block_reason: &'static str| OwnerCapability {
            name: name.into(),
            requested,
            effective,
            block_reason: block_reason.into(),
            roles: Vec::new(),
        };
        let on = |b: bool| if b { "ACTIVE" } else { "OFF" };
        let jit = self.jit_funding_bank.is_some();
        let bundles = {
            use darkbot_solana::submit::SubmissionAdapter;
            self.submitter.supports_bundle()
        };
        let rebalancing = !self.inventory_targets.is_empty();
        let observing = self.rebalance.mode == crate::execution::inventory::RebalanceMode::Observe;
        let mut all = vec![
            {
                // DEC-13/P8-CTRL.2: the operator's request and the one effective decision — every
                // economic switch, every wallet and FE-06; the first missing gate, then the rest.
                let c = &self.sources.capabilities;
                let (effective, block_reason) =
                    crate::orchestration::capability_control::live_decision(&c.live_gates());
                OwnerCapability {
                    name: crate::orchestration::capability_control::LIVE.into(),
                    requested: c.live_requested,
                    effective,
                    block_reason: block_reason.into(),
                    roles: Vec::new(),
                }
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
            // B01: the operator's request (INVENTORY_REBALANCING); OBSERVE plans with real
            // balances, quotes and costs; a trade needs its own authorization (never effective).
            cap(
                crate::execution::inventory::INVENTORY_REBALANCING,
                self.rebalance.requested,
                match (rebalancing, observing) {
                    (true, true) => "OBSERVE",
                    _ => "OFF",
                },
                match self.rebalance.block() {
                    "" if observing => "Shadow: rebalancing is planned, never traded",
                    "" if rebalancing => "bands reserve inventory; rebalancing OFF",
                    block => block,
                },
            ),
            // DEC-06: requested OFF|OBSERVE|ACTIVE, effective OFF|SWEEP|OBSERVE|ACTIVE.
            cap(
                crate::orchestration::threshold_mode::THRESHOLD_TRIGGER,
                self.threshold.requested.code(),
                self.threshold.effective.code(),
                self.threshold.block,
            ),
            // P6-MT.10: the local fee model is observed against every RPC quote; this run's parity
            // per class (the RPC quote decides in this build).
            self.fee_parity_capability(),
        ];
        // P8-CTRL.1: the economic switches and one switch per configured signer — partial
        // permissions, effective as requested; LIVE above needs all of them.
        use crate::orchestration::capability_control as c;
        let switch = |on: bool| if on { c::ENABLED } else { c::DISABLED };
        let caps = &self.sources.capabilities;
        all.extend(c::ECON.iter().map(|e| {
            let on = switch(caps.econ.get(e) == Some(true));
            OwnerCapability {
                name: (*e).into(),
                requested: on,
                effective: on,
                block_reason: "".into(),
                roles: Vec::new(),
            }
        }));
        all.extend(caps.wallets.iter().map(|w| {
            let on = switch(w.enabled);
            OwnerCapability {
                name: w.capability().into(),
                requested: on,
                effective: on,
                block_reason: "".into(),
                roles: w.roles.clone(),
            }
        }));
        all
    }

    /// P8-CTRL.3/.5: the FE-06 grant in force with its scope's limits and the attempts booked
    /// against them (from the FE-04 ledger view; unknown while it is unreadable), and the scopes
    /// previewed for this owner epoch.
    fn fe06_status(&self) -> crate::status_tap::OwnerFe06 {
        use crate::orchestration::capability_control as cap;
        let c = &self.sources.capabilities;
        let mut s = crate::status_tap::OwnerFe06::default();
        match &c.fe06 {
            Some(grant) => {
                s.grant_digest = Some(grant.digest().to_string());
                let now = super::accounts::wall_unix_s().ok();
                s.in_force = now.is_some_and(|now| grant.in_force(now));
                if let Some(scope) = grant.scope() {
                    s.scope_mode = scope.mode.code();
                    s.valid_from_unix_s = Some(scope.not_before_unix_s);
                    s.valid_until_unix_s = Some(scope.expires_unix_s);
                    s.campaign_id = scope.campaign_id.clone();
                    s.max_attempts_per_utc_day = scope.max_attempts_per_utc_day;
                    s.max_attempts_campaign = scope.max_attempts_campaign;
                    s.max_attempts_grant = scope.max_attempts_grant;
                    s.max_exits_grant = scope.max_exits_grant;
                    // DEC-17b: the bookings of every wallet's journals.
                    if let Some(views) = self.ledger_views() {
                        use crate::execution::one_shot::Fe06Quota;
                        let bookings = || views.iter().flat_map(|v| &v.bookings);
                        let exits = || views.iter().flat_map(|v| &v.exit_bookings);
                        let day = super::accounts::wall_unix_s().ok().map(|n| n / 86_400);
                        s.attempts_today = day.map(|d| Fe06Quota::UtcDay(d).used(bookings()));
                        s.attempts_campaign =
                            Some(Fe06Quota::Campaign(&scope.campaign_id).used(bookings()));
                        s.attempts_grant = Some(Fe06Quota::Grant(grant.digest()).used(bookings()));
                        s.exits_grant = Some(Fe06Quota::Grant(grant.digest()).used(exits()));
                    }
                }
                if !s.in_force {
                    s.block_reason = cap::grant_not_in_force(grant.digest());
                } else if let Some(e) = self.fe06_unbound(grant, now) {
                    // DEC-17a (review L3): a grant that no longer binds what runs, or whose
                    // evidence changed, is not in force — as the LIVE decision sees it.
                    s.in_force = false;
                    s.block_reason = e;
                }
            }
            None => s.block_reason = cap::FE06_NO_GRANT.into(),
        }
        // The scopes previewed for this epoch, as the lifecycle's refresh cached them.
        s.previews = c.previews.current();
        s
    }

    /// DEC-17a (review L3): why `grant` opens nothing now although within its validity — its
    /// scope does not bind what runs, or its evidence (as last checked) changed.
    fn fe06_unbound(
        &self,
        grant: &crate::orchestration::capability_control::Fe06Authorization,
        now: Option<u64>,
    ) -> Option<String> {
        let c = &self.sources.capabilities;
        if let (Some(scope), Some(now)) = (grant.scope(), now) {
            let binds = match &c.running {
                Some(running) => crate::orchestration::fe06::check_running(scope, running, now),
                None => Err("FE06_SCOPE_BINDING: what the owner runs is unknown".into()),
            };
            if let Err(e) = binds {
                return Some(e);
            }
        }
        c.fe06_evidence.clone()
    }

    fn wallets_status(&self, i: usize) -> OwnerWallets {
        // DEC-17b B4 (review L4): what is held on the wallet itself (another wallet's SOL is that
        // one's), as its row shows it.
        let holds = self.wallets[i].reservations.held_by_others(&[]);
        let (journal_read, verdict, unknown_reasons, totals) = match &self.wallets[i].ledger_view {
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
            held_attempt: self.wallets[i]
                .fe04_held
                .as_ref()
                .and_then(|j| j.step_state())
                .map_or("NONE", |s| s.name()),
            non_landing: self.non_landing_projection(i),
            hold_b: Some(self.hold_b_projection(i)),
            rows: (0..self.wallets.len())
                .map(|j| self.wallet_row(j))
                .collect(),
            selections: self.wallet_selection.selections,
            none_eligible: self.wallet_selection.none_eligible,
            last_none_eligible: self.wallet_selection.last_none_eligible.clone(),
        }
    }

    /// DEC-17b: wallet `i`'s row — roles, switch, why it may not run a candidate now (in the
    /// effective mode), its SOL, holds, held attempt, Safety, journal and this run's choices.
    /// DEC-17b B4: its holds are what is held on it (the SOL another wallet pays is that one's),
    /// its fee payer the wallet paying its candidates now and the SOL held on it for others.
    fn wallet_row(&self, i: usize) -> crate::status_tap::OwnerWalletRow {
        let w = &self.wallets[i];
        let holds = w.reservations.held_by_others(&[]);
        let live = self.live_effective();
        let payer = self.choose_payer(live);
        let eligible = if w.pays_others() {
            self.payer_eligible(i, live).map(|_| None)
        } else {
            self.wallet_eligible(i, live, &payer).map(|(_, p)| p)
        };
        let fee_payer = match (&eligible, w.runs_candidates(), w.pays_itself()) {
            (_, true, true) => Some(w.liquidator.to_string()),
            (Ok(Some(p)), true, false) => Some(self.wallets[*p].liquidator.to_string()),
            (_, true, false) => payer
                .as_ref()
                .ok()
                .map(|(p, _)| self.wallets[*p].liquidator.to_string()),
            _ => None,
        };
        let key = w.liquidator.to_string();
        let sol_held_for_others = self
            .wallets
            .iter()
            .filter(|o| !o.reservations.is_unreadable())
            .fold(0u64, |s, o| {
                s.saturating_add(o.reservations.held_for(&key, &[]))
            });
        let view = w.ledger_view.as_ref().and_then(|v| v.as_ref().ok());
        crate::status_tap::OwnerWalletRow {
            wallet: w.liquidator.to_string(),
            roles: w.roles.clone(),
            enabled: self
                .sources
                .capabilities
                .wallets
                .iter()
                .any(|s| s.pubkey == w.liquidator && s.enabled),
            runs_candidates: w.runs_candidates(),
            block_reason: eligible.err().unwrap_or_default(),
            sol_lamports: self.st.get(&w.liquidator).map(|r| r.lamports),
            free_sol_lamports: self.wallet_free_sol(i),
            holds,
            held_attempt: w
                .fe04_held
                .as_ref()
                .and_then(|j| j.step_state())
                .map_or("NONE", |s| s.name()),
            halt_reason: w.safety.halted().map(|r| format!("{r:?}")),
            capital_in_use_usd_micro: w.safety.capital_in_use(),
            day_attempts: w.safety.day().attempts,
            journal_read: view.is_some(),
            realized_net_usd_micro: view.and_then(|v| v.totals.realized_net_usd_micro),
            selected: w.selected,
            fee_payer,
            sol_held_for_others,
        }
    }

    /// DEC-03: the Wallets non-landing status from the held journal and this run's latest read
    /// of the same subject; without a read in this run, a released subject shows its persisted
    /// release (its usable B operators, C negative).
    fn non_landing_projection(&self, i: usize) -> Option<crate::status_tap::OwnerNonLanding> {
        use crate::execution::one_shot::non_landing::Verdict;
        let s = self.fe04_non_landing_status(i)?;
        let read = self.wallets[i]
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
                self.wallets[i].fe04_non_landing.reads
            } else {
                0
            },
        })
    }

    /// DEC-04: the B hold mode with its evidence, Safety's reserved inventory and capital, and
    /// the held attempt's A, B and finalized flows (every asset of a prediction or of the deltas).
    fn hold_b_projection(&self, i: usize) -> crate::status_tap::OwnerHoldB {
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
        let attempt = self.wallets[i]
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
            inventory_reserved_usd_micro: self.wallets[i].safety.inventory_reserved(),
            capital_in_use_usd_micro: self.wallets[i].safety.capital_in_use(),
            attempt,
        }
    }

    /// DEC-10: the configured approval at this instant (expiry moves with the clock); without an
    /// FE-04 journal its single use is unchecked: never VALID then.
    /// DEC-13b/DEC-02: the admission state — OFF without an effective LIVE (its block reason);
    /// BLOCKED by a held attempt, a Safety halt or the last LIVE candidate's refusal; READY with
    /// an open admission (S4-04: opened ahead of the first LIVE candidate); WAITING while none is
    /// open yet (the candidate's own admission reports why it cannot be) — and this run's counters.
    fn admission_status(&self, i: usize) -> crate::status_tap::OwnerAdmission {
        let a = &self.wallets[i].fe04_admissions;
        let (state, block_reason) = if !self.live_effective() {
            let (_, reason) =
                crate::orchestration::capability_control::live_decision(&self.sources.live_gates());
            ("OFF", reason)
        } else if let Some(state) = self.wallets[i]
            .fe04_held
            .as_ref()
            .and_then(|j| j.step_state())
        {
            // Single flight: the held attempt blocks every admission.
            ("BLOCKED", format!("FE04_ATTEMPT_HELD:{}", state.name()))
        } else if let Some(halt) = self.wallets[i].safety.halted() {
            ("BLOCKED", format!("SAFETY_HALTED:{}", halt.as_str()))
        } else if !a.block_reason.is_empty() {
            // The last LIVE candidate's refusal.
            ("BLOCKED", a.block_reason.clone())
        } else if self.wallets[i].fe04_admission.is_some() {
            ("READY", String::new())
        } else {
            ("WAITING", String::new())
        };
        crate::status_tap::OwnerAdmission {
            state,
            block_reason,
            submitted: a.submitted,
            refused: a.refused,
            unknown: a.unknown,
            last_refusal: a.last_refusal.clone(),
            simulation_reused: a.simulation.reused,
            simulation_resimulated: a.simulation.resimulated,
            simulation_refused: a.simulation.refused,
            last_reuse_refusal: a.simulation.last_reuse_refusal.unwrap_or("").to_string(),
            exits_submitted: a.exits_submitted,
            exits_refused: a.exits_refused,
            exits_unknown: a.exits_unknown,
            last_exit_refusal: a.last_exit_refusal.clone(),
        }
    }

    pub(super) fn approval_status(
        &self,
        i: usize,
    ) -> crate::execution::one_shot::approval_file::ApprovalStatus {
        use crate::execution::one_shot::approval_file::{self, Usage};
        let Some(source) = &self.wallets[i].fe04_approval else {
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

    /// DEC-17b: the FE-04 accounting of every wallet that runs candidates (`None`: one was not
    /// read or is unreadable — a sum over the others would be smaller than the truth).
    fn ledger_views(&self) -> Option<Vec<&status_view::LedgerView>> {
        self.wallets
            .iter()
            .filter(|w| w.runs_candidates())
            .map(|w| w.ledger_view.as_ref().and_then(|v| v.as_ref().ok()))
            .collect()
    }

    /// The finance summary over every wallet (DEC-17b): reserved capital from their Safety,
    /// realized net from their settled steps; totals and withdrawable profit need reconciled
    /// wallet balances and stay unknown. VERIFIED only with every field known and every step
    /// settled; PARTIAL once every journal was read.
    fn finance_status(&self) -> OwnerFinance {
        let views = self.ledger_views();
        let reserved = self
            .wallets
            .iter()
            .try_fold(0u64, |sum, w| sum.checked_add(w.safety.capital_in_use()));
        let realized = views.as_ref().and_then(|vs| {
            vs.iter().try_fold(0i128, |sum, v| {
                sum.checked_add(v.totals.realized_net_usd_micro?)
            })
        });
        let mut f = OwnerFinance {
            as_of_slot: self.st.highest_slot(),
            total_capital_usd_micro: None,
            available_capital_usd_micro: None,
            reserved_capital_usd_micro: reserved.map(i128::from),
            realized_net_pnl_usd_micro: realized,
            withdrawable_profit_usd_micro: None,
            transferred_to_treasury_usd_micro: views.as_ref().map(|_| 0),
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
        f.status = match &views {
            None => "UNKNOWN",
            Some(vs) if vs.iter().any(|v| matches!(v.verdict, Verdict::Unknown(_))) => "PARTIAL",
            Some(_) if complete => "VERIFIED",
            Some(_) => "PARTIAL",
        };
        f
    }
}

impl ShadowOwner {
    /// P6-MT.10: the local fee model's status row — the controller's request, what is effective
    /// (ACTIVE only in a sealed class no mismatch of this run failed) and this run's parity (the
    /// saved runs' sum: `p0ctl fee-parity`).
    fn fee_parity_capability(&self) -> OwnerCapability {
        let (requested, effective, block_reason) = self.local_fee_state();
        OwnerCapability {
            name: crate::execution::fee_parity::LOCAL_FEE_MODEL.into(),
            requested,
            effective,
            block_reason: block_reason.into(),
            roles: Vec::new(),
        }
    }
}
