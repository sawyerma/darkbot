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
                            trigger,
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

                    let cached = if trigger == "integration_projection" {
                        self.projection_cache.remove(&acct).filter(|c| {
                            c.base_account_hash == base_account_hash && base_record.slot == slot
                        })
                    } else {
                        self.projection_cache.remove(&acct);
                        None
                    };

                    if let Some(cached) = cached {
                        match darkbot_project0::projection::ProjectedState::new(
                            &view,
                            cached.projection_slot,
                            cached.projection_unix_s,
                            cached.accounts,
                        ) {
                            Ok(p) => Some(p),
                            Err(e) => {
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
                        if !self.projection_pending.contains(&acct) {
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

                            let Some(bh) =
                                self.blockhash.fresh(self.clock.now_ns(), 30_000_000_000)
                            else {
                                self.evidence.emit(OwnerEvidence::Record(
                                    RecordEvent::IntegrationSkip {
                                        account: acct,
                                        reason: "projection blockhash stale".into(),
                                    },
                                ))?;
                                return Ok(false);
                            };

                            let projection_plan = tx::TxPlan {
                                version: tx::BuildVersion::V0,
                                payer: self.liquidator,
                                instructions: refreshes,
                                lookup_tables: vec![],
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
                            };
                            match self.remote.projection_jobs.try_push(work) {
                                Ok(()) => {
                                    self.projection_pending.insert(acct);
                                }
                                Err(ChannelError::Full(_)) => {
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
                                trigger,
                                slot,
                                integration_kinds,
                                pricing_integration_kinds,
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
            let inventory = |bank_key: &Pubkey| -> u64 {
                let Some(bank) = bank_from_state(eval_state, &self.program, bank_key) else {
                    return 0;
                };
                validated_liquidator_ata(eval_state, &bank.mint, &self.liquidator)
                    .map(|(_, amount)| amount)
                    .unwrap_or(0)
            };
            #[cfg(feature = "crate_health")]
            let ata = |mint: &Pubkey| -> Option<Pubkey> {
                validated_liquidator_ata(eval_state, mint, &self.liquidator).map(|(pk, _)| pk)
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
            let builder = P0ReceivershipBuilder {
                state: eval_state,
                program: self.program,
                group: self.group,
                liquidator: self.liquidator,
                global_fee_wallet: match sizer_for_cost.fee_snapshot() {
                    Ok(f) => f.global_fee_wallet,
                    Err(_) => Pubkey::default(),
                },
                sizing: &sizing,
                routes: &self.routes,
                inventory: &inventory,
                ata: &ata,
                lamports_to_usd_micro: &lamports_to_usd_micro,
                jito_tip_usd_micro: 0,
                jito_tip_lamports: 0,
                zero_prefund_qualified: self.zero_prefund_enabled,
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
                    self.ledger
                        .on_health(false, matches!(e, HealthError::Unsupported(_)));
                    HealthEvidence::Error(e.to_string())
                }
            };
            self.evidence
                .emit(OwnerEvidence::Record(RecordEvent::Health {
                    account: acct,
                    trigger,
                    t_rx_ns: t_rx,
                    slot,
                    evaluation_slot: protocol_slot,
                    evaluation_unix_s: protocol_now,
                    result: health_evidence,
                }))?;
            self.evidence.check()?;
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
                blockhash_max_age_ns: 30_000_000_000,
                p7_passed: false,
                freshness: self.freshness,
                tip_account: None,
                global_allowed: self.global_allowed.clone(),
                prereq_allowed: self.prereq_allowed.clone(),
                slot_watch: &self.slot_watch,
            };
            #[cfg(feature = "crate_health")]
            let pipeline_started_ns = self.clock.now_ns();
            darkbot_common::mt0::MT0.add(darkbot_common::mt0::Counter::PipelineEvaluations, 1); // [MT0]
            let mut rec = pipe.evaluate(
                acct,
                trigger,
                t_rx,
                protocol_now,
                protocol_slot,
                self.st.run_state().is_tradable(),
            );
            rec.rx_slot = Some(slot); // trigger provenance differs from projected evaluation slot
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
                // [FIX-91] Bankruptcy/no-collateral is a protocol state, not a routing/sizing
                // failure. Reclassify independently of the stage that first reported "no plan".
                if h.is_bad_debt && h.equity_assets <= 0 {
                    rec.loss = Some(crate::analytics::record::LossReason::Bankruptcy);
                    rec.reason = Some("bad debt with zero equity assets: bankruptcy/no-collateral case, not a normal receivership opportunity".into());
                }
            }
            if rec
                .reason
                .as_deref()
                .map(|s| s.contains("ReserveStale"))
                .unwrap_or(false)
            {
                rec.loss = Some(crate::analytics::record::LossReason::IntegrationRefresh);
                rec.reason = Some("integration reserve stale: required reserve refresh prerequisite missing before exact sizing".into());
            }
            if let Some(h) = &self.econ {
                h.emit(crate::analytics::econ::Evidence::Shadow(Box::new(
                    rec.clone(),
                )));
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
            // [FIX-18][FIX-23] Submitted und AmbiguousSubmission landen beide in der Reconciliation.
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
