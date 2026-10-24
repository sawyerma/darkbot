//! Shared health, planning and execution pipeline admission.
use super::*;

impl ShadowOwner {
    pub(super) fn detect(
        &mut self,
        detect: Option<DetectWork>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        self.mt0_owner.enter(darkbot_common::mt0::OwnerCat::Detect);
        if let Some((acct, t_rx, slot, trigger)) = detect {
            darkbot_common::mt0::MT0.record_queue_wait(
                darkbot_common::mt0::TriggerClass::from_trigger(trigger),
                self.clock.now_ns().saturating_sub(t_rx),
                None,
            );
            // Health über das marginfi-Crate (Feature crate_health ist für dieses Binary Pflicht).
            // Bookkeeping time only; protocol evaluation uses the canonical chain clock below.
            let now = wall_unix_s()?;
            let view = StateView(&self.st);

            // [FIX-111][R6] Integration refreshes are projected by an isolated RPC worker first.
            // The returned account set is one-shot: only the exact candidate account state that
            // requested it may consume it, and canonical StateCore is never mutated.
            #[cfg(feature = "crate_health")]
            let projected_owned = {
                let refreshes = match darkbot_project0::integration::refresh_plan_for_account(
                    &view,
                    &self.program,
                    &acct,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        let line = RecordEvent::IntegrationPlan {
                            account: acct,
                            trigger: trigger.into(),
                            slot,
                            reason: format!("integration refresh plan: {e}"),
                        };
                        self.evidence.emit(OwnerEvidence::Record(line))?;
                        return Ok(false);
                    }
                };

                if refreshes.is_empty() {
                    None
                } else {
                    let Some(base_record) = self.st.get(&acct) else {
                        self.evidence.emit(OwnerEvidence::Record(
                            RecordEvent::IntegrationSkip {
                                account: acct,
                                reason: "candidate account disappeared before projection".into(),
                            },
                        ))?;
                        return Ok(false);
                    };
                    let base_account_hash = hash(&base_record.data);

                    // E9: a cached projection newer than the processed account notifications
                    // waits (parked); a new external trigger supersedes such a wait.
                    if !projection_consume_gate(
                        &self.st,
                        &self.projection_cache,
                        &mut self.projection_parked,
                        &mut self.projection_stats,
                        (acct, t_rx, slot, trigger),
                    ) {
                        return Ok(false);
                    }
                    let cached = if trigger == TRIGGER_INTEGRATION_PROJECTION {
                        self.projection_cache.remove(&acct).and_then(|c| {
                            let base_matches = c.base_account_hash == base_account_hash
                                && base_record.slot == slot;
                            // E9: a projection is reused only while every dependency it was
                            // requested with is still at the same canonical revision
                            // (N4: counted, no line per discard).
                            let unchanged = base_matches
                                && dependency_fingerprint(&self.st, &c.dependencies)
                                    == c.dependency_fingerprint;
                            self.projection_stats
                                .consume(base_matches, unchanged)
                                .then_some(c)
                        })
                    } else {
                        let removed = self.projection_cache.remove(&acct);
                        self.projection_stats.dropped_if(removed);
                        None
                    };

                    if let Some(cached) = cached {
                        match darkbot_project0::projection::ProjectedState::new(
                            &view,
                            cached.projection_slot,
                            cached.projection_unix_s,
                            cached.accounts,
                        ) {
                            Ok(p) => {
                                self.projection_stats.used(true);
                                Some(p)
                            }
                            Err(e) => {
                                self.projection_stats.used(false);
                                self.evidence.emit(OwnerEvidence::Record(
                                    RecordEvent::IntegrationSkip {
                                        account: acct,
                                        reason: format!("projected state invalid: {e}"),
                                    },
                                ))?;
                                return Ok(false);
                            }
                        }
                    } else {
                        // E9: bounded re-requests (`projection_request_allowed`).
                        if !self.projection_pending.contains(&acct)
                            && projection_request_allowed(
                                &mut self.projection_requeues,
                                &mut self.projection_stats,
                                acct,
                                trigger,
                            )
                        {
                            let targets =
                                match darkbot_project0::integration::projection_targets_for_account(
                                    &view,
                                    &self.program,
                                    &acct,
                                    &refreshes,
                                ) {
                                    Ok(v) if !v.is_empty() => v,
                                    Ok(_) => {
                                        self.evidence.emit(OwnerEvidence::Record(
                                            RecordEvent::IntegrationSkip {
                                                account: acct,
                                                reason:
                                                    "integration projection produced zero targets"
                                                        .into(),
                                            },
                                        ))?;
                                        return Ok(false);
                                    }
                                    Err(e) => {
                                        self.evidence.emit(OwnerEvidence::Record(
                                            RecordEvent::IntegrationSkip {
                                                account: acct,
                                                reason: format!("projection targets: {e}"),
                                            },
                                        ))?;
                                        return Ok(false);
                                    }
                                };

                            let Some(bh) = self
                                .blockhash
                                .fresh(self.clock.now_ns(), BLOCKHASH_MAX_AGE_NS)
                            else {
                                self.evidence.emit(OwnerEvidence::Record(
                                    RecordEvent::IntegrationSkip {
                                        account: acct,
                                        reason: "projection blockhash stale".into(),
                                    },
                                ))?;
                                return Ok(false);
                            };

                            // E9: the dependency set of this projection and its revisions now.
                            // N3: an unknown observation set means no projection (fail-closed),
                            // recorded and counted.
                            let dependencies = match projection_dependency_set(
                                &view,
                                &self.program,
                                &self.program_data,
                                &self.group,
                                &acct,
                                &refreshes,
                            ) {
                                Ok(d) => d,
                                Err(e) => {
                                    self.projection_stats.observation_unknown += 1;
                                    self.evidence.emit(OwnerEvidence::Record(
                                        RecordEvent::IntegrationSkip {
                                            account: acct,
                                            reason: format!("projection observation unknown: {e}"),
                                        },
                                    ))?;
                                    return Ok(false);
                                }
                            };
                            let fingerprint = dependency_fingerprint(&self.st, &dependencies);
                            let min_context_slot =
                                required_context_slot(&self.st, base_record.slot, &dependencies);
                            let projection_plan = tx::TxPlan {
                                version: tx::BuildVersion::V0,
                                payer: self.liquidator,
                                instructions: refreshes,
                                lookup_tables: vec![].into(),
                                resources: self.resources,
                            };
                            let mut built =
                                match tx::build(&projection_plan, bh.hash, &TxLimits::default()) {
                                    Ok(v) => v,
                                    Err(e) => {
                                        self.evidence.emit(OwnerEvidence::Record(
                                            RecordEvent::IntegrationSkip {
                                                account: acct,
                                                reason: format!("projection tx build: {e}"),
                                            },
                                        ))?;
                                        return Ok(false);
                                    }
                                };
                            if let Err(e) = tx::sign(&mut built, &[&self.payer]) {
                                self.evidence.emit(OwnerEvidence::Record(
                                    RecordEvent::IntegrationSkip {
                                        account: acct,
                                        reason: format!("projection tx sign: {e}"),
                                    },
                                ))?;
                                return Ok(false);
                            }

                            let work = ProjectionWork {
                                account: acct,
                                base_slot: base_record.slot,
                                base_account_hash,
                                t_rx_ns: t_rx,
                                tx: built.tx,
                                targets,
                                dependencies,
                                dependency_fingerprint: fingerprint,
                                min_context_slot,
                            };
                            match self.remote.projection_jobs.try_push(work) {
                                Ok(()) => {
                                    self.projection_stats.requested += 1;
                                    self.projection_pending.insert(acct);
                                }
                                Err(ChannelError::Full(_)) => {
                                    self.projection_stats.queue_full += 1;
                                    self.remote.projection_jobs.give_up();
                                    self.evidence.emit(OwnerEvidence::Record(
                                        RecordEvent::IntegrationSkip {
                                            account: acct,
                                            reason: "projection worker queue full".into(),
                                        },
                                    ))?;
                                }
                                Err(ChannelError::Disconnected(_)) => {
                                    return Err("projection RPC worker disconnected".into());
                                }
                            }
                        }
                        let integration_kinds = darkbot_project0::integration::kinds_for_account(
                            &view,
                            &self.program,
                            &acct,
                        )
                        .unwrap_or_default()
                        .into_iter()
                        .map(darkbot_project0::integration::kind_name)
                        .collect::<Vec<_>>();
                        let pricing_integration_kinds =
                            darkbot_project0::integration::pricing_kinds_for_account(
                                &view,
                                &self.program,
                                &acct,
                            )
                            .unwrap_or_default()
                            .into_iter()
                            .map(darkbot_project0::integration::kind_name)
                            .collect::<Vec<_>>();
                        self.evidence.emit(OwnerEvidence::Record(
                            RecordEvent::IntegrationPending {
                                account: acct,
                                trigger: trigger.into(),
                                slot,
                                integration_kinds: integration_kinds
                                    .into_iter()
                                    .map(Into::into)
                                    .collect(),
                                pricing_integration_kinds: pricing_integration_kinds
                                    .into_iter()
                                    .map(Into::into)
                                    .collect(),
                            },
                        ))?;
                        return Ok(false);
                    }
                }
            };

            #[cfg(feature = "crate_health")]
            let eval_state: &dyn AccountSource = projected_owned
                .as_ref()
                .map(|p| p as &dyn AccountSource)
                .unwrap_or(&view as &dyn AccountSource);

            // [FIX-119][R6] Use both coordinates of the exact simulation Clock for every
            // projected protocol calculation. The original slot remains trigger provenance.
            #[cfg(feature = "crate_health")]
            let (protocol_now, protocol_slot) =
                match projected_owned.as_ref().map(|p| p.evaluation_time()) {
                    Some(t) => t,
                    // [C0-FIX-03] canonical chain clock; without it nothing is evaluated.
                    None => match chain_clock(&self.st) {
                        Ok(c) => (c.unix_s(), c.slot),
                        Err(e) => {
                            owner_log!(
                                self,
                                "detect: SKIP account={acct} trigger_slot={slot} reason={e}"
                            );
                            return Ok(false);
                        }
                    },
                };

            #[cfg(feature = "crate_health")]
            let engine = darkbot_project0::health::crate_impl::CrateHealth {
                state: eval_state,
                group: self.group,
            };
            #[cfg(feature = "crate_health")]
            // P8-RES-01: offers see only what no attempt holds (C11).
            let inventory = |bank_key: &Pubkey| -> u64 {
                let Some(bank) = bank_from_state(eval_state, &self.program, bank_key) else {
                    return 0;
                };
                // The FE-04 keys are base58 strings: encoded only when a policy exists.
                let reserve = if self.inventory_targets.is_empty() {
                    0
                } else {
                    self.inventory_targets
                        .get(&bank.mint.to_string())
                        .map_or(0, |t| t.reserve)
                };
                available_liquidator_inventory(
                    eval_state,
                    &bank.mint,
                    &self.atas,
                    &self.reservations,
                    reserve,
                )
            };
            #[cfg(feature = "crate_health")]
            let ata = |mint: &Pubkey| -> Option<Pubkey> {
                validated_liquidator_ata(eval_state, mint, &self.atas).map(|(pk, _)| pk)
            };
            #[cfg(feature = "crate_health")]
            let sizing = ProtocolSizingProvider {
                state: eval_state,
                program: self.program,
                group: self.group,
                sol_bank: self.sol_bank,
                inventory: &inventory,
                slippage_bps: self.eval_slippage_bps,
            };
            #[cfg(feature = "crate_health")]
            let sizer_for_cost = darkbot_project0::sizing::ProtocolSizer {
                state: eval_state,
                program: self.program,
                group: self.group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            };
            #[cfg(feature = "crate_health")]
            let lamports_to_usd_micro = |lamports| {
                sizer_for_cost
                    .native_amount_cost_usd_micro(
                        &self.sol_bank,
                        lamports,
                        protocol_now as i64,
                        protocol_slot,
                    )
                    .ok()
            };
            #[cfg(feature = "crate_health")]
            let rent_lamports_to_usd_micro = |lamports| {
                sizer_for_cost
                    .native_amount_conservative_cost_usd_micro(
                        &self.sol_bank,
                        lamports,
                        protocol_now as i64,
                        protocol_slot,
                    )
                    .ok()
            };
            #[cfg(feature = "crate_health")]
            let builder = P0ReceivershipBuilder {
                state: eval_state,
                program: self.program,
                group: self.group,
                liquidator: self.liquidator,
                global_fee_wallet: sizer_for_cost.fee_snapshot().map(|f| f.global_fee_wallet),
                sizing: &sizing,
                routes: &self.routes,
                inventory: &inventory,
                ata: &ata,
                lamports_to_usd_micro: &lamports_to_usd_micro,
                rent_lamports_to_usd_micro: &rent_lamports_to_usd_micro,
                jito_tip_usd_micro: 0,
                jito_tip_lamports: 0,
                zero_prefund_qualified: self.zero_prefund_enabled,
                instructions: &self.instruction_table,
                v1_plans: darkbot_solana::tx::v1_build_capability()
                    && darkbot_solana::submit::SubmissionAdapter::supports_tx_version(
                        &self.submitter,
                        darkbot_solana::tx::BuildVersion::V1,
                    ),
                prepared: Some((&self.prepared, self.ingress_session.generation)),
                path: Path::Rpc,
                resources: self.resources,
                limits: TxLimits::default(),
            };
            let _ = &view;
            if let Some(day) = self.ledger.on_account_update(now) {
                self.evidence.emit(OwnerEvidence::Summary(day))?;
            }
            #[cfg(feature = "crate_health")]
            let health_started_ns = self.clock.now_ns();
            darkbot_common::mt0::MT0.count_health(darkbot_common::mt0::HealthSite::DetectConsumer); // [MT0]
            let health_result = engine.health(&acct, protocol_now as i64, protocol_slot);
            #[cfg(feature = "crate_health")]
            {
                let elapsed = self.clock.now_ns().saturating_sub(health_started_ns);
                self.metrics.detect_health_calls =
                    self.metrics.detect_health_calls.saturating_add(1);
                self.metrics.detect_health_ns_total =
                    self.metrics.detect_health_ns_total.saturating_add(elapsed);
                self.metrics.detect_health_ns_max = self.metrics.detect_health_ns_max.max(elapsed);
            }
            // Typed snapshot only; the evidence writer owns all JSON encoding.
            let health_evidence = match &health_result {
                Ok(h) => {
                    self.ledger.on_health(h.liquidatable, false);
                    HealthEvidence::Known {
                        liquidatable: h.liquidatable,
                        maintenance_health: h.maintenance_health,
                        equity_assets: h.equity_assets,
                        bad_debt: h.is_bad_debt,
                    }
                }
                Err(HealthError::Unavailable) => {
                    self.ledger.on_health(false, false);
                    HealthEvidence::Unavailable
                }
                Err(e) => {
                    self.ledger.on_health(false, health_fails_closed(e));
                    HealthEvidence::Error(e.to_string())
                }
            };
            self.evidence
                .emit(OwnerEvidence::Record(RecordEvent::Health {
                    account: acct,
                    trigger: trigger.into(),
                    t_rx_ns: t_rx,
                    slot,
                    evaluation_slot: protocol_slot,
                    evaluation_unix_s: protocol_now,
                    result: health_evidence,
                }))?;
            self.evidence.check()?;
            // B6: an empty ingress queue opens a new drain window of the network budget.
            let queue = self.consumer.stats();
            if queue.sent <= queue.received {
                self.network.drained();
            }
            // [FIX-1] Pipeline (Shadow): Health → Pläne → Economics → Build → Sign → Send-Boundary.
            let mut pipe = Pipeline {
                mode: Mode::Shadow,
                clock: &self.clock,
                health: &engine,
                builder: &builder,
                submitter: &self.submitter,
                safety: &mut self.safety,
                blockhash: &self.blockhash,
                payer: &self.payer,
                allowlist: &self.allowlist,
                failed_costs: &self.failed_costs,
                limits: TxLimits::default(),
                resources: self.resources,
                min_profit: self.min_profit,
                blockhash_max_age_ns: BLOCKHASH_MAX_AGE_NS,
                p7_passed: false,
                freshness: self.freshness,
                tip_account: None,
                global_allowed: &self.global_allowed,
                prereq_allowed: &self.prereq_allowed,
                slot_watch: &self.slot_watch,
                network: &mut self.network,
                simulation_failure: None,
                fe04: None,
            };
            #[cfg(feature = "crate_health")]
            let pipeline_started_ns = self.clock.now_ns();
            // MT2: the one Health computation of this detection (above) is the pipeline's and
            // the sizer's Health; none is computed again on the same state and clock.
            darkbot_common::mt0::MT0.add(darkbot_common::mt0::Counter::PipelineEvaluations, 1); // [MT0]
            let mut rec = pipe.evaluate_prepared(
                acct,
                trigger,
                t_rx,
                protocol_now,
                protocol_slot,
                self.st.run_state().is_tradable(),
                health_result.as_ref(),
            );
            let simulation_failure = pipe.simulation_failure.take();
            rec.rx_slot = Some(slot); // trigger provenance differs from projected evaluation slot
                                      // P2-05: a tagged candidate is woken when its growing premium could make it
                                      // actionable; any other outcome drops the wake-up.
            self.time_schedule.upsert(
                acct,
                darkbot_project0::schedule::Trigger::TagProfitable,
                rec.t_profitable_unix_s,
            );
            #[cfg(feature = "crate_health")]
            {
                let (lines, beyond) = crate::execution::builder::p0::take_route_diagnostics();
                for line in lines {
                    owner_log!(self, "{line}");
                }
                if beyond > 0 {
                    owner_log!(
                        self,
                        "route_budget: {beyond} further lines beyond the bound"
                    );
                }
            }
            #[cfg(feature = "crate_health")]
            {
                let elapsed = self.clock.now_ns().saturating_sub(pipeline_started_ns);
                self.metrics.detect_pipeline_calls =
                    self.metrics.detect_pipeline_calls.saturating_add(1);
                self.metrics.detect_pipeline_ns_total = self
                    .metrics
                    .detect_pipeline_ns_total
                    .saturating_add(elapsed);
                self.metrics.detect_pipeline_ns_max =
                    self.metrics.detect_pipeline_ns_max.max(elapsed);
            }
            if let Ok(h) = &health_result {
                (rec.loss, rec.reason) = bankruptcy_classification(
                    rec.loss,
                    rec.reason.take(),
                    h.is_bad_debt,
                    h.equity_assets,
                );
            }
            // C10/Q5: a stale integration reserve is classified at its source (pinned error code ⇒
            // HealthError::IntegrationStale / typed PlanErrorKind::IntegrationStale), never from text.
            if let Some(h) = &self.econ {
                h.emit(crate::analytics::econ::Evidence::Shadow(Box::new(
                    rec.clone(),
                )));
            }
            if let Some(failure) = simulation_failure {
                self.evidence
                    .emit(OwnerEvidence::SimulationFailure(Box::new(
                        crate::analytics::record::SimulationFailureRecord {
                            account: rec.account.clone(),
                            rx_slot: rec.rx_slot,
                            failure,
                        },
                    )))?;
            }
            self.evidence
                .emit(OwnerEvidence::Opportunity(Box::new(rec.clone())))?;
            // [M3] Ledger + [M2] Watchlist (nur Shadow-relevante Entscheidungen).
            let sim_ok = match rec.decision {
                Decision::ShadowReady | Decision::ShadowTechnicalReady => Some(true),
                Decision::ShadowSimFailed | Decision::ShadowTechnicalSimFailed => Some(false),
                _ => None,
            };
            self.ledger.on_record(&rec, sim_ok);
            // [FIX-43][FIX-71] Nur wirtschaftlich bekannter ShadowReady darf als virtueller Win/PnL zählen.
            // Technische Probes liefern ausschließlich Build/Sign/Simulation-Evidence.
            // [F1] Only exact-Health-liquidatable candidates are winner-relevant; one watch per account.
            let health_liquidatable = matches!(&health_result, Ok(h) if h.liquidatable);
            if is_winner_relevant(health_liquidatable, rec.decision) {
                let shadow_ready = matches!(rec.decision, Decision::ShadowReady);
                let our_loss = if shadow_ready {
                    None
                } else {
                    Some(
                        rec.loss
                            .unwrap_or(crate::analytics::record::LossReason::Build),
                    )
                };
                self.winner_tracker.observe(
                    WinnerWatch {
                        revision: 0, // assigned by the tracker
                        account: acct,
                        rx_slot: slot,
                        detect_slot: slot,
                        sign_slot: rec.sign_slot,
                        expected_net: rec.net_profit,
                        shadow_ready,
                        our_loss,
                        created_unix_s: now,
                        next_check_unix_s: now,
                    },
                    now,
                );
            }
            // [FIX-18][FIX-23] Submitted und AmbiguousSubmission landen beide in der Reconciliation
            // (Telemetrie; finanzieller Abschluss: FE-04-Journal-Settlement).
            if let (Some(sig), Some(exp)) = (rec.signature.as_deref(), rec.exposure) {
                if let Ok(s) = sig.parse() {
                    let sigs = rec
                        .all_signatures
                        .iter()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    // [FIX-33] tatsächlich verwendeter Pfad aus dem Record, nicht die Adapter-Default.
                    let path = match rec.path.as_deref() {
                        Some("JITO_BUNDLE") => darkbot_solana::submit::SubmissionPath::JitoBundle,
                        Some("JITO_SINGLE") => darkbot_solana::submit::SubmissionPath::JitoSingle,
                        _ => darkbot_solana::submit::SubmissionPath::Rpc,
                    };
                    self.pending.push(ReconcileWork {
                        submission: PendingSubmission {
                            signature: s,
                            signatures: sigs,
                            bundle_id: rec.bundle_id.clone(),
                            path,
                            exposure: exp,
                            submitted_unix_s: now,
                            tip: rec.submitted_tip.clone(),
                        },
                        next_check_unix_s: now,
                    });
                }
            }
        }
        Ok(true)
    }
}
