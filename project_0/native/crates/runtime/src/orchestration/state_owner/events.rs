//! Ingress application, recovery and detection selection.
use super::*;

impl ShadowOwner {
    /// A23-01: the owner's only idle wait. Without an ingress event it returns at once when a
    /// worker result, the census outcome or a stop is waiting (applied by the next loop pass);
    /// else it sleeps on the doorbell until any of them arrives or the next deadline: the open
    /// sweep batch's age bound, otherwise `IDLE_TICK`. No poll interval decides latency.
    #[cfg(feature = "crate_health")]
    fn next_event(&mut self) -> Option<IngressEvent> {
        let token = self.wake.token();
        if let Some(ev) = self.consumer.try_pop() {
            return Some(ev);
        }
        // DEC-01: a newer blockhash for parked candidates is applied by the next pass.
        if self.remote_pending()
            || self.stop_pending()
            || self.sources.pending()
            || self.blockhash_parking.pending(self.blockhash.generation())
        {
            return None;
        }
        // Deadlines that end the wait without an event: the open sweep batch's age bound
        // (`sweep` flushes it at that age) and the econ cutoff (the loop then drains), so a
        // zero timeout never repeats; else only the housekeeping tick.
        let sweep = self
            .sweep_batch_started_ns
            .filter(|_| self.sweep_batch_open())
            .map(|started| {
                Duration::from_nanos(
                    self.sweep_batch_max_age_ns
                        .saturating_sub(self.clock.now_ns().saturating_sub(started)),
                )
            });
        let cutoff = self.econ.as_ref().and_then(|h| h.until_cutoff());
        let timeout = [sweep, cutoff]
            .into_iter()
            .flatten()
            .fold(IDLE_TICK, Duration::min);
        self.sleep_on_wake(token, timeout);
        self.consumer.try_pop()
    }

    pub(super) fn events(
        &mut self,
        fetch: &mut impl FnMut() -> Result<SnapshotData, String>,
        ingress: &IngressSessions,
    ) -> Result<OwnerStep<Option<DetectWork>>, Box<dyn std::error::Error>> {
        // S1-02: control changes and observed (never applied) source events first.
        self.sources_step(ingress)?;
        self.mt0_owner.enter(darkbot_common::mt0::OwnerCat::Wait);
        #[cfg(feature = "crate_health")]
        let queued_detection_available = !self.detect_queue.is_empty()
            || !self.oracle_lanes.is_empty()
            || !self.bootstrap_probe_queue.is_empty()
            || (!self.integration_probe_queue.is_empty() && self.projection_pending.is_empty());
        #[cfg(feature = "crate_health")]
        let force_detection = queued_detection_available
            && self.ingress_since_detect >= self.ingress_burst_before_detect;
        #[cfg(feature = "crate_health")]
        let maybe_ev = if force_detection {
            None
        } else if queued_detection_available {
            self.consumer.try_pop()
        } else {
            self.next_event()
        };
        // C6/D10: the StreamReady of a replaced session is no readiness of the current one and
        // never starts a recovery; its data events are applied as usual.
        let maybe_ev = match maybe_ev.map(|ev| self.retired_events.admit(ev)) {
            Some(None) => {
                owner_log!(
                    self,
                    "ingress: StreamReady of a replaced session dropped (total={})",
                    self.retired_events.stream_ready_dropped
                );
                return Ok(OwnerStep::Continue);
            }
            Some(Some(ev)) => Some(ev),
            None => None,
        };

        self.mt0_owner
            .enter(darkbot_common::mt0::OwnerCat::IngressApply);
        if let Some(ev) = &maybe_ev {
            darkbot_common::mt0::MT0
                .record_ingress_queue_wait(self.clock.now_ns().saturating_sub(ev.t_rx_ns()));
        }
        #[cfg(feature = "crate_health")]
        let mut detect;

        if let Some(ev) = maybe_ev {
            #[cfg(feature = "crate_health")]
            {
                self.ingress_since_detect = self.ingress_since_detect.saturating_add(1);
            }
            self.sources.measure_canonical(&ev);
            // Trigger (a) Long-Tail: jedes MarginfiAccount-Update → Detection im Shadow-Modus.
            detect = match &ev {
                IngressEvent::Account(a)
                    if self.st.run_state().is_tradable()
                        && marginfi_account_in_group(&a.data, &self.group) =>
                {
                    let trigger = if a.source == SourceKind::Snapshot {
                        "bootstrap_snapshot_probe"
                    } else {
                        "account_update"
                    };
                    Some((a.pubkey, a.t_rx_ns, a.slot, trigger))
                }
                _ => None,
            };
            // [FIX-35] Bank-Pubkey merken; die Dependency-Prüfung läuft NACH dem Anwenden des Updates.
            let updated_bank_event: Option<(Pubkey, u64, u64)> = match &ev {
                IngressEvent::Account(a) if classify_account(&a.data) == AccountType::Bank => {
                    Some((a.pubkey, a.t_rx_ns, a.slot))
                }
                _ => None,
            };
            let updated_bank: Option<Pubkey> = updated_bank_event.map(|(pk, _, _)| pk);
            // V2-10: an update of an already indexed account goes through the post-apply path as
            // well — a close (empty data, foreign owner) decodes to no banks there and is removed.
            #[cfg(feature = "crate_health")]
            let updated_marginfi_account: Option<Pubkey> = match &ev {
                IngressEvent::Account(a)
                    if classify_account(&a.data) == AccountType::MarginfiAccount
                        || self
                            .reverse_index
                            .banks_for_account(&a.pubkey)
                            .next()
                            .is_some() =>
                {
                    Some(a.pubkey)
                }
                _ => None,
            };
            #[cfg(feature = "crate_health")]
            let dependency_update: Option<(Pubkey, u64, u64)> = match &ev {
                IngressEvent::Account(a)
                    if self
                        .reverse_index
                        .banks_for_oracle(&a.pubkey)
                        .next()
                        .is_some() =>
                {
                    Some((a.pubkey, a.t_rx_ns, a.slot))
                }
                _ => None,
            };
            #[cfg(feature = "crate_health")]
            if let (Some((dependency, _, _)), IngressEvent::Account(a)) = (dependency_update, &ev) {
                if let Some(existing) = self.st.get(&dependency) {
                    let same_content = existing.owner == a.owner
                        && existing.lamports == a.lamports
                        && existing.data == a.data;
                    if same_content {
                        self.metrics.dependency_content_unchanged =
                            self.metrics.dependency_content_unchanged.saturating_add(1);
                    } else {
                        self.metrics.dependency_content_changed =
                            self.metrics.dependency_content_changed.saturating_add(1);
                    }
                } else {
                    self.metrics.dependency_content_changed =
                        self.metrics.dependency_content_changed.saturating_add(1);
                }
            }

            if let IngressEvent::Account(a) = &ev {
                if a.pubkey == self.program_data && hash(&a.data) != self.program_data_baseline_hash
                {
                    if !self.program_upgrade_latched {
                        // [P1A-04] Identify the new deployment for requalification; an
                        // undecodable account is reported, never trusted.
                        let new = darkbot_project0::protocol::decode_program_data(
                            &a.pubkey, &a.owner, false, &a.data,
                        );
                        owner_log!(self,
                        "PROGRAM_UPGRADE detected slot={} program_data={} new_deployment={:?} — latched HALT until explicit P1B promotion",
                        a.slot, self.program_data, new
                    );
                    }
                    self.latch_program_upgrade();
                }
                let i = match classify_account(&a.data) {
                    AccountType::MarginfiAccount => 0,
                    AccountType::Bank => 1,
                    AccountType::MarginfiGroup => 2,
                    AccountType::FeeState => 3,
                    AccountType::LiquidationRecord => 4,
                    AccountType::Unknown => 5,
                };
                self.counts[i] += 1;
            }
            // Review E3: a raised ingress halt not yet consumed by the lifecycle halts the state
            // first — no fork-triggered recovery runs over it; a Ready clears only the raises
            // from before this recovery (a failure during it stays raised).
            if self.halt.is_raised() {
                self.st.halt(darkbot_common::error::HaltReason::FeedGap);
            }
            let raises_before = self.halt.raises();
            let outcome = self.drv.on_event_cancellable(
                &mut self.st,
                ev,
                fetch,
                &mut |s: &mut StateCore| {
                    while let Some(ev) = self.consumer.try_pop() {
                        s.on_event(ev);
                    }
                },
                &|| {
                    self.shutdown.is_requested()
                        || self.econ.as_ref().is_some_and(|h| h.drain_expired())
                },
            );
            let (lines, beyond) = self.drv.take_diagnostics();
            for line in lines {
                owner_log!(self, "{line}");
            }
            if beyond > 0 {
                owner_log!(self, "[recovery] {beyond} further lines beyond the bound");
            }
            if let Some(o) = outcome {
                owner_log!(
                    self,
                    "recovery: {:?}, run_state={:?}",
                    o,
                    self.st.run_state()
                );
                // C1/C7: no fresh stream generation after the bounded cycles, or a terminal feed
                // failure, ends the run in order as INCOMPLETE instead of a passive HALT.
                let terminal = match o {
                    darkbot_solana::state::recovery::RecoveryOutcome::Exhausted { .. } => {
                        Some("STATE_RECOVERY_EXHAUSTED")
                    }
                    darkbot_solana::state::recovery::RecoveryOutcome::Fatal { code } => Some(code),
                    _ => None,
                };
                if let Some(code) = terminal {
                    owner_log!(
                        self,
                        "{code}: ending the run INCOMPLETE (no detection, no submission)"
                    );
                    return Err(Box::new(OwnerTerminal(code)));
                }
                if matches!(o, darkbot_solana::state::recovery::RecoveryOutcome::Ready) {
                    // X1: the first Ready is the admission. The audited identities bind to the
                    // state the runtime read itself, before the halt clears or anything runs.
                    if let Some(audit) = self.admission.take() {
                        let required: HashSet<Pubkey> =
                            self.required_dependency_keys.iter().copied().collect();
                        match audit.bind_state(
                            &self.program,
                            &self.program_data,
                            self.program_data_executable,
                            &required,
                            &StateView(&self.st),
                        ) {
                            Ok(b) => owner_log!(
                                self,
                                "BOOT_IDENTITY_BOUND bound={} outside_runtime_scope={}",
                                b.bound,
                                b.outside_scope.len()
                            ),
                            Err(e) => {
                                owner_log!(self, "{e}: ending the run before admission");
                                return Err(Box::new(OwnerTerminal("BOOT_IDENTITY_DRIFT")));
                            }
                        }
                    }
                    self.halt.clear_through(raises_before);

                    // A program upgrade can race startup/recovery before its WSS notification is
                    // observed. Re-check the canonical recovered ProgramData bytes against the
                    // startup baseline before allowing any detection work.
                    if self
                        .st
                        .get(&self.program_data)
                        .map(|r| hash(&r.data) != self.program_data_baseline_hash)
                        .unwrap_or(true)
                    {
                        self.latch_program_upgrade();
                    }
                    if self.program_upgrade_latched {
                        self.st
                            .halt(darkbot_common::error::HaltReason::ProgramUpgrade);
                        owner_log!(
                            self,
                            "PROGRAM_UPGRADE HALT active program_data={} — no automatic resume",
                            self.program_data
                        );
                        return Ok(OwnerStep::Continue);
                    }

                    #[cfg(feature = "crate_health")]
                    {
                        // [C0 Ticket 2] The census population is collected in this same scan.
                        let mut population = self
                            .c0_census
                            .as_ref()
                            .is_some_and(|c| c.wants_population())
                            .then(CensusPopulation::default);
                        let (rebuilt, indexed_banks, indexed_accounts, unsupported_banks) =
                            rebuild_reverse_index(
                                &self.st,
                                &self.program,
                                &self.group,
                                population.as_mut(),
                            );
                        self.reverse_index = rebuilt;
                        self.sweep_dirty_banks.clear();
                        self.sweep_batch_events = 0;
                        self.sweep_batch_started_ns = None;
                        self.candidate_cache.clear();
                        self.candidate_revisions.clear();
                        self.candidate_force_rearm.clear();
                        self.candidate_input_epoch = 0;
                        self.bank_input_revision.clear();
                        // DEC-06: every Ready rebuilds the threshold index; the effective mode
                        // follows it (SWEEP, INDEX_BOOTSTRAP until bootstrapped).
                        self.threshold_index_reset();
                        self.threshold_refresh("INDEX");
                        owner_log!(self,
                        "reverse_index_ready: banks={} accounts={} unsupported_bank_oracle_layouts={} threshold_bootstrap={} threshold_trigger={}",
                        indexed_banks, indexed_accounts, unsupported_banks, self.threshold_bootstrap.len(), self.threshold_authority()
                    );
                        // [C0-FIX-01] The census rides on the startup sweep batch below (setup
                        // admits a census only in R127 sweep mode); force that batch.
                        if let (Some(census), Some(population)) =
                            (self.c0_census.as_mut(), population)
                        {
                            owner_log!(
                                self,
                                "c0_census: ARMED debtors={} undecodable_members={}",
                                population.debtors.len(),
                                population.undecodable.len()
                            );
                            census.arm(population);
                            self.sweep_batch_events = self.sweep_batch_max_events;
                            self.sweep_batch_started_ns
                                .get_or_insert(self.clock.now_ns());
                        }
                        if !self.threshold_authority() {
                            // [R127c] eva01 parity: one full debtor scan at startup so an account
                            // already underwater before StreamReady cannot wait forever for a future
                            // unrelated write. Reuse the same batched/parallel Health admission path
                            // by marking every current group Bank dirty exactly once.
                            let startup_banks = self.mark_all_banks_dirty();
                            owner_log!(self,
                            "r127_health_sweep_ready: reverse_accounts={} startup_banks={} threshold_trigger=false",
                            indexed_accounts, startup_banks
                        );
                        }
                    }

                    #[cfg(feature = "crate_health")]
                    if self.mode == "shadow" && !self.bootstrap_probe_done {
                        self.bootstrap_probe_done = true;
                        // Z14: the probes are the owner's own detection work (served after the
                        // detect and oracle queues), never events in the bounded ingress channel
                        // the WSS session writes to — a channel they filled would end that session
                        // as an overflow.
                        let now_ns = self.clock.now_ns();
                        let mut probes: Vec<DetectWork> = self
                            .st
                            .iter_owned_by(&self.program)
                            .filter(|(_, r)| bootstrap_probe_candidate(&r.data, &self.group))
                            .map(|(pk, r)| (*pk, now_ns, r.slot, "bootstrap_snapshot_probe"))
                            .collect();
                        probes.sort_unstable_by_key(|w| w.0.to_bytes());
                        let queued = probes.len();
                        self.bootstrap_probe_queue.extend(probes);
                        self.metrics.bootstrap_probes_queued += queued;
                        owner_log!(
                            self,
                            "bootstrap_probe: queued={} candidates from was_liquidatable",
                            queued
                        );

                        let view = StateView(&self.st);
                        let mut kamino = 0usize;
                        let mut drift = 0usize;
                        let mut juplend = 0usize;
                        let mut pricing_kamino = 0usize;
                        let mut pricing_drift = 0usize;
                        let mut pricing_juplend = 0usize;
                        for (pk, r) in self.st.iter_owned_by(&self.program) {
                            if !bootstrap_probe_candidate(&r.data, &self.group) {
                                continue;
                            }
                            if let Ok(kinds) = darkbot_project0::integration::kinds_for_account(
                                &view,
                                &self.program,
                                pk,
                            ) {
                                for k in kinds {
                                    match k {
                                        darkbot_project0::integration::IntegrationKind::Kamino => {
                                            kamino += 1
                                        }
                                        darkbot_project0::integration::IntegrationKind::Drift => {
                                            drift += 1
                                        }
                                        darkbot_project0::integration::IntegrationKind::JupLend => {
                                            juplend += 1
                                        }
                                    }
                                }
                            }
                            if let Ok(kinds) =
                                darkbot_project0::integration::pricing_kinds_for_account(
                                    &view,
                                    &self.program,
                                    pk,
                                )
                            {
                                for k in kinds {
                                    match k {
                                        darkbot_project0::integration::IntegrationKind::Kamino => {
                                            pricing_kamino += 1
                                        }
                                        darkbot_project0::integration::IntegrationKind::Drift => {
                                            pricing_drift += 1
                                        }
                                        darkbot_project0::integration::IntegrationKind::JupLend => {
                                            pricing_juplend += 1
                                        }
                                    }
                                }
                            }
                        }
                        owner_log!(self,
                        "bootstrap_probe_integrations: withdraw_kamino={} withdraw_drift={} withdraw_juplend={} pricing_kamino={} pricing_drift={} pricing_juplend={}",
                        kamino, drift, juplend, pricing_kamino, pricing_drift, pricing_juplend
                    );

                        // [FIX-115][R6] Optional targeted qualification probe. The historical
                        // was_liquidatable set can legitimately contain zero integration accounts.
                        // Scan every liability-bearing account already present in canonical RAM,
                        // report the actual integration population, then queue a small deterministic
                        // sample per venue through the existing projection -> Health/Sizing pipeline.
                        if self.integration_probe_enabled {
                            let mut all_withdraw = [0usize; 3];
                            let mut all_pricing = [0usize; 3];
                            let mut scan_errors = 0usize;
                            let mut kamino_accounts: Vec<(Pubkey, u64)> = Vec::new();
                            let mut drift_accounts: Vec<(Pubkey, u64)> = Vec::new();
                            let mut juplend_accounts: Vec<(Pubkey, u64)> = Vec::new();

                            for (pk, r) in self.st.iter_owned_by(&self.program) {
                                if !marginfi_account_in_group(&r.data, &self.group) {
                                    continue;
                                }

                                let withdraw =
                                    match darkbot_project0::integration::kinds_for_account(
                                        &view,
                                        &self.program,
                                        pk,
                                    ) {
                                        Ok(v) => v,
                                        Err(_) => {
                                            scan_errors += 1;
                                            continue;
                                        }
                                    };
                                let pricing =
                                    match darkbot_project0::integration::pricing_kinds_for_account(
                                        &view,
                                        &self.program,
                                        pk,
                                    ) {
                                        Ok(v) => v,
                                        Err(_) => {
                                            scan_errors += 1;
                                            continue;
                                        }
                                    };

                                let mut has_kamino = false;
                                let mut has_drift = false;
                                let mut has_juplend = false;
                                for k in withdraw {
                                    match k {
                                        darkbot_project0::integration::IntegrationKind::Kamino => {
                                            all_withdraw[0] += 1;
                                            has_kamino = true;
                                        }
                                        darkbot_project0::integration::IntegrationKind::Drift => {
                                            all_withdraw[1] += 1;
                                            has_drift = true;
                                        }
                                        darkbot_project0::integration::IntegrationKind::JupLend => {
                                            all_withdraw[2] += 1;
                                            has_juplend = true;
                                        }
                                    }
                                }
                                for k in pricing {
                                    match k {
                                        darkbot_project0::integration::IntegrationKind::Kamino => {
                                            all_pricing[0] += 1;
                                            has_kamino = true;
                                        }
                                        darkbot_project0::integration::IntegrationKind::Drift => {
                                            all_pricing[1] += 1;
                                            has_drift = true;
                                        }
                                        darkbot_project0::integration::IntegrationKind::JupLend => {
                                            all_pricing[2] += 1;
                                            has_juplend = true;
                                        }
                                    }
                                }

                                if has_kamino {
                                    kamino_accounts.push((*pk, r.slot));
                                }
                                if has_drift {
                                    drift_accounts.push((*pk, r.slot));
                                }
                                if has_juplend {
                                    juplend_accounts.push((*pk, r.slot));
                                }
                            }

                            for v in [
                                &mut kamino_accounts,
                                &mut drift_accounts,
                                &mut juplend_accounts,
                            ] {
                                v.sort_unstable_by_key(|(pk, _)| pk.to_bytes());
                                v.dedup_by_key(|(pk, _)| *pk);
                            }

                            owner_log!(self,
                            "integration_target_census: withdraw_kamino={} withdraw_drift={} withdraw_juplend={} pricing_kamino={} pricing_drift={} pricing_juplend={} scan_errors={}",
                            all_withdraw[0],
                            all_withdraw[1],
                            all_withdraw[2],
                            all_pricing[0],
                            all_pricing[1],
                            all_pricing[2],
                            scan_errors
                        );

                            let mut selected = HashSet::new();
                            let mut queued_kamino = 0usize;
                            let mut queued_drift = 0usize;
                            let mut queued_juplend = 0usize;

                            for (kind_idx, accounts) in [
                                (0usize, &kamino_accounts),
                                (1usize, &drift_accounts),
                                (2usize, &juplend_accounts),
                            ] {
                                for (pk, slot) in
                                    accounts.iter().take(self.integration_probe_limit_per_kind)
                                {
                                    match kind_idx {
                                        0 => queued_kamino += 1,
                                        1 => queued_drift += 1,
                                        _ => queued_juplend += 1,
                                    }
                                    if selected.insert(*pk) {
                                        self.integration_probe_queue.push_back((*pk, *slot));
                                    }
                                }
                            }

                            self.integration_probe_expected = selected.clone();
                            self.integration_probe_terminal.clear();
                            self.integration_probe_projection_ready = 0;
                            self.integration_probe_projection_failed = 0;
                            self.integration_probe_projection_stale = 0;
                            self.integration_probe_completion_reported = false;

                            owner_log!(self,
                            "integration_target_probe: queued_unique={} kamino_sample={} drift_sample={} juplend_sample={} limit_per_kind={}",
                            selected.len(),
                            queued_kamino,
                            queued_drift,
                            queued_juplend,
                            self.integration_probe_limit_per_kind
                        );
                        }
                    }
                }
            }
            // [FIX-133a] Keep reverse indices in lockstep with the canonical post-event state.
            // This does not trigger detection yet; FIX-133b will consume it only after threshold
            // qualification. Unknown oracle layouts are removed from the trigger map fail-closed.
            #[cfg(feature = "crate_health")]
            if let Some(account_pk) = updated_marginfi_account {
                match self
                    .st
                    .get(&account_pk)
                    .and_then(|r| active_banks_in_group_account(&r.data, &self.group))
                {
                    Some(banks) => {
                        self.reverse_index.set_account_banks(account_pk, &banks);
                        // DEC-06: the threshold index is kept while a threshold mode is
                        // requested; without one the R127 sweep alone admits. Beside the
                        // sweep the upkeep is OBSERVE work within its budget (B-9): without
                        // budget the account is re-checked bank-wide until re-evaluated.
                        if self.threshold_indexing() && !self.threshold_work_allowed() {
                            self.threshold_defer(account_pk);
                        } else if self.threshold_indexing() {
                            let upkeep_started = std::time::Instant::now();
                            // [C0-FIX-03] canonical chain clock, never local or zero time
                            let clock = chain_clock(&self.st);
                            let view = StateView(&self.st);
                            let engine = darkbot_project0::health::crate_impl::CrateHealth {
                                state: &view,
                                group: self.group,
                            };
                            match clock
                                .map_err(|e| {
                                    darkbot_project0::health::HealthError::Stale(e.to_string())
                                })
                                .and_then(|c| {
                                    engine.thresholds(&account_pk, c.unix_timestamp, c.slot)
                                }) {
                                Ok(t) => {
                                    self.threshold_index.set_account(account_pk, &t.thresholds);
                                    self.threshold_fallback.remove(&account_pk);
                                    self.threshold.stats.index_evaluations =
                                        self.threshold.stats.index_evaluations.saturating_add(1);
                                }
                                Err(e) => {
                                    self.threshold_index.remove_account(&account_pk);
                                    self.threshold_fallback.insert(account_pk);
                                    owner_log!(
                                        self,
                                        "threshold_account_fallback: account={} error={}",
                                        account_pk,
                                        e
                                    );
                                }
                            }
                            if !self.threshold_authority() {
                                self.threshold_charge(upkeep_started);
                            }
                        }
                    }
                    None => {
                        self.reverse_index.remove_account(&account_pk);
                        self.time_schedule.remove_account(&account_pk);
                        if self.threshold_indexing() {
                            self.threshold_index.remove_account(&account_pk);
                            self.threshold_fallback.remove(&account_pk);
                        }
                    }
                }
            }
            #[cfg(feature = "crate_health")]
            if let Some(bank_pk) = updated_bank {
                let affected_accounts: Vec<Pubkey> = self
                    .reverse_index
                    .accounts_for_bank(&bank_pk)
                    .copied()
                    .collect();
                if self.threshold_indexing() {
                    for account in affected_accounts.iter().copied() {
                        self.threshold_index.remove_account(&account);
                        self.threshold_fallback.insert(account);
                        self.threshold_bootstrap.push_back(account);
                    }
                    self.threshold_ready = self.threshold_bootstrap.is_empty();
                }
                if !self.threshold_authority() && self.st.run_state().is_tradable() {
                    // A canonical Bank write can change weights, fees, operational state or
                    // integration configuration without changing the debtor account bytes.
                    // [FIX-134] Record a causal revision in addition to the existing force-rearm.
                    self.candidate_input_epoch = self.candidate_input_epoch.saturating_add(1);
                    self.bank_input_revision
                        .insert(bank_pk, self.candidate_input_epoch);
                    self.candidate_force_rearm
                        .extend(affected_accounts.iter().copied());
                    if let Some((_, t_rx_ns, event_slot)) = updated_bank_event {
                        if self.sweep_batch_started_ns.is_none() {
                            self.sweep_batch_started_ns = Some(self.clock.now_ns());
                        }
                        self.sweep_dirty_banks
                            .entry(bank_pk)
                            .and_modify(|w| {
                                if event_slot > w.1 || (event_slot == w.1 && t_rx_ns > w.0) {
                                    *w = (t_rx_ns, event_slot);
                                }
                            })
                            .or_insert((t_rx_ns, event_slot));
                        self.sweep_batch_events = self.sweep_batch_events.saturating_add(1);
                        self.metrics.sweep_max_dirty_banks = self
                            .metrics
                            .sweep_max_dirty_banks
                            .max(self.sweep_dirty_banks.len());
                    }
                }

                let view = StateView(&self.st);
                match bank_from_state(&view, &self.program, &bank_pk)
                    .filter(|b| b.group == self.group)
                {
                    Some(bank) => match bank_observation_keys(&bank) {
                        Ok(_) => {
                            let deps = darkbot_project0::health::dependency_keys(
                                &view,
                                &self.program,
                                &[bank_pk],
                            );
                            self.reverse_index.set_bank_oracles(bank_pk, &deps);
                        }
                        Err(e) => {
                            self.reverse_index.set_bank_oracles(bank_pk, &[]);
                            owner_log!(
                                self,
                                "reverse_index_bank_unsupported: bank={} error={}",
                                bank_pk,
                                e
                            );
                        }
                    },
                    None => self.reverse_index.set_bank_oracles(bank_pk, &[]),
                }
            }

            // [FIX-34][FIX-35] Nach dem Anwenden: zeigt die NEUE Bank-Konfiguration auf nicht abonnierte Accounts?
            #[cfg(feature = "crate_health")]
            if let Some(bk) = updated_bank {
                let view = StateView(&self.st);
                // B16: an updated bank without a complete dependency set is reported.
                let scan = darkbot_project0::health::dependency_scan(&view, &self.program, &[bk]);
                // N6: reported when new, changed or cleared — never once per bank update.
                let gap = scan.gaps.iter().find(|(b, _)| *b == bk).map(|(_, g)| g);
                if note_gap(&mut self.dependency_gaps, bk, gap) {
                    match gap {
                        Some(g) => owner_log!(
                            self,
                            "dependency_gap: bank={} code={} detail={:?}",
                            bk,
                            g.code(),
                            g
                        ),
                        None => owner_log!(self, "dependency_gap_cleared: bank={}", bk),
                    }
                }
                let required_now = scan.keys;
                let optional_now =
                    ata_candidates_for_bank(&view, &self.program, &bk, &self.liquidator);
                let new_required: Vec<Pubkey> = required_now
                    .into_iter()
                    .filter(|k| !self.required_set.contains(k))
                    .collect();
                let new_optional: Vec<Pubkey> = optional_now
                    .into_iter()
                    .filter(|k| !self.subscribed.contains(k))
                    .collect();
                if !new_required.is_empty() || !new_optional.is_empty() {
                    owner_log!(self,
                    "dependency set changed: +{} required +{} optional → HALT, resubscribe; Recovery follows StreamReady",
                    new_required.len(),
                    new_optional.len()
                );
                    self.st.halt(darkbot_common::error::HaltReason::StateGap);
                    // C6/D10: the old session ends and is joined before the new one starts; one
                    // that does not stop within the shutdown budget ends the run INCOMPLETE.
                    let budget = Duration::from_secs(
                        darkbot_solana::runtime_config::active()
                            .limits
                            .shutdown_seconds,
                    );
                    if !self.ingress_session.stop_and_join(budget) {
                        owner_log!(
                            self,
                            "INGRESS_SESSION_NOT_QUIESCENT: generation={} still running after {} s",
                            self.ingress_session.generation,
                            budget.as_secs()
                        );
                        return Err(Box::new(OwnerTerminal("INGRESS_SESSION_NOT_QUIESCENT")));
                    }
                    let queued = {
                        let s = self.consumer.stats();
                        s.sent.saturating_sub(s.received)
                    };
                    self.retired_events.retire(queued);
                    // The new generation has no StreamReady yet: StateCore keeps the gap open until
                    // its `Reconnected`, so no recovery can become Ready before it.
                    self.halt.raise();
                    self.st.on_event(IngressEvent::Gap {
                        source: SourceKind::WsRpcControl,
                        from_slot: None,
                        to_slot: None,
                        t_rx_ns: self.clock.now_ns(),
                    });
                    self.required_set.extend(new_required.iter().copied());
                    self.optional_set.extend(new_optional.iter().copied());
                    self.required_dependency_keys = self.required_set.iter().copied().collect();
                    self.optional_dependency_keys = self.optional_set.iter().copied().collect();
                    self.required_dependency_keys
                        .sort_unstable_by_key(|k| k.to_bytes());
                    self.optional_dependency_keys
                        .sort_unstable_by_key(|k| k.to_bytes());
                    self.subscribed = self
                        .required_set
                        .union(&self.optional_set)
                        .copied()
                        .collect();
                    if let Ok(mut g) = self.dep_shared.lock() {
                        *g = self.required_dependency_keys.clone();
                    }
                    if let Ok(mut g) = self.opt_shared.lock() {
                        *g = self.optional_dependency_keys.clone();
                    }
                    let generation = self.ingress_session.generation.saturating_add(1);
                    owner_log!(
                        self,
                        "ingress: session generation={} joined, {} queued events retired; starting generation={}",
                        self.ingress_session.generation,
                        queued,
                        generation
                    );
                    // S1-02: the canonical unit's next generation (WSS or an ACTIVE Geyser unit);
                    // without one the state stays halted.
                    if let Some(session) = self.spawn_canonical(ingress)? {
                        self.ingress_session = session;
                    }
                }
            }
            #[cfg(feature = "crate_health")]
            if self.st.run_state().is_tradable() {
                if let Some((dependency, t_rx_ns, event_slot)) = dependency_update {
                    self.metrics.oracle_dependency_updates =
                        self.metrics.oracle_dependency_updates.saturating_add(1);
                    // [C0-FIX-03] canonical chain clock, never local or zero time
                    let clock = chain_clock(&self.st);
                    let view = StateView(&self.st);
                    let banks: Vec<Pubkey> = self
                        .reverse_index
                        .banks_for_oracle(&dependency)
                        .copied()
                        .collect();

                    // [FIX-134] Oracle and integration source writes are causal candidate inputs.
                    // A global epoch makes multiple writes inside one slot distinguishable and
                    // lets a candidate collapse several changed Banks to a single newest revision.
                    if !banks.is_empty() {
                        self.candidate_input_epoch = self.candidate_input_epoch.saturating_add(1);
                        for bank in banks.iter().copied() {
                            self.bank_input_revision
                                .insert(bank, self.candidate_input_epoch);
                        }
                    }

                    if !self.threshold_authority() {
                        // DEC-06 OBSERVE: the threshold trigger's lookup of this tick on the
                        // same state and input revision, beside the sweep; it admits nothing.
                        if self.threshold.effective
                            == crate::orchestration::threshold_mode::Effective::Observe
                        {
                            self.threshold_observe_tick(&banks, t_rx_ns, clock);
                        }
                        if self.sweep_batch_started_ns.is_none() {
                            self.sweep_batch_started_ns = Some(self.clock.now_ns());
                        }
                        for bank in banks {
                            self.sweep_dirty_banks
                                .entry(bank)
                                .and_modify(|w| {
                                    if event_slot > w.1 || (event_slot == w.1 && t_rx_ns > w.0) {
                                        *w = (t_rx_ns, event_slot);
                                    }
                                })
                                .or_insert((t_rx_ns, event_slot));
                        }
                        self.sweep_batch_events = self.sweep_batch_events.saturating_add(1);
                        self.metrics.sweep_max_dirty_banks = self
                            .metrics
                            .sweep_max_dirty_banks
                            .max(self.sweep_dirty_banks.len());
                    } else {
                        // P6-MT.5 lanes of the threshold trigger: `candidates` are threshold-index
                        // hits (the price crossed the account's maintenance threshold: a potential
                        // transition → L0, P6-MT.14 #29); `fallback` are bank-wide or
                        // threshold-unknown re-checks without a per-account signal → L2.
                        let mut candidates: HashSet<Pubkey> = HashSet::new();
                        let mut fallback: HashSet<Pubkey> = HashSet::new();
                        for bank in banks {
                            if self.threshold_ready {
                                match self.threshold_hits(&bank, clock) {
                                    Ok((hits, rechecks)) => {
                                        candidates.extend(hits);
                                        fallback.extend(rechecks);
                                    }
                                    Err(e) => {
                                        // [R126] Integration pricing may be stale in canonical state even
                                        // though the exact pre-Start refresh is deterministic. Project one
                                        // Bank refresh once, then threshold-select accounts from that fresh
                                        // pricing state instead of waking the whole Bank immediately.
                                        if let Some(existing) =
                                            self.oracle_bank_projection_pending.get_mut(&bank)
                                        {
                                            *existing = (t_rx_ns, event_slot);
                                            owner_log!(self,
                                        "oracle_bank_projection_coalesced: dependency={} bank={} error={}",
                                        dependency, bank, e
                                    );
                                        } else {
                                            let Some(bank_record) = self.st.get(&bank) else {
                                                self.metrics.oracle_price_fallbacks = self
                                                    .metrics
                                                    .oracle_price_fallbacks
                                                    .saturating_add(1);
                                                fallback.extend(
                                                    self.reverse_index
                                                        .accounts_for_bank(&bank)
                                                        .copied(),
                                                );
                                                continue;
                                            };
                                            let Some(bank_state) =
                                                bank_from_state(&view, &self.program, &bank)
                                            else {
                                                self.metrics.oracle_price_fallbacks = self
                                                    .metrics
                                                    .oracle_price_fallbacks
                                                    .saturating_add(1);
                                                fallback.extend(
                                                    self.reverse_index
                                                        .accounts_for_bank(&bank)
                                                        .copied(),
                                                );
                                                continue;
                                            };
                                            let refreshes =
                                        match darkbot_project0::integration::refresh_plan_for_bank(
                                            &view,
                                            &bank_state,
                                        ) {
                                            Ok(v) if !v.is_empty() => v,
                                            Ok(_) => {
                                                self.metrics.oracle_price_fallbacks =
                                                    self.metrics.oracle_price_fallbacks.saturating_add(1);
                                                fallback.extend(self.reverse_index.accounts_for_bank(&bank).copied());
                                                continue;
                                            }
                                            Err(refresh_error) => {
                                                self.metrics.oracle_price_fallbacks =
                                                    self.metrics.oracle_price_fallbacks.saturating_add(1);
                                                owner_log!(self,
                                                    "oracle_bank_projection_plan_fallback: dependency={} bank={} price_error={} refresh_error={}",
                                                    dependency, bank, e, refresh_error
                                                );
                                                fallback.extend(self.reverse_index.accounts_for_bank(&bank).copied());
                                                continue;
                                            }
                                        };
                                            let targets =
                                                darkbot_project0::projection::projection_targets(
                                                    &refreshes,
                                                );
                                            if targets.is_empty() {
                                                self.metrics.oracle_price_fallbacks = self
                                                    .metrics
                                                    .oracle_price_fallbacks
                                                    .saturating_add(1);
                                                fallback.extend(
                                                    self.reverse_index
                                                        .accounts_for_bank(&bank)
                                                        .copied(),
                                                );
                                                continue;
                                            }
                                            let Some(bh) = self.blockhash.fresh(
                                                self.clock.now_ns(),
                                                self.blockhash_max_age_ns,
                                            ) else {
                                                self.metrics.oracle_price_fallbacks = self
                                                    .metrics
                                                    .oracle_price_fallbacks
                                                    .saturating_add(1);
                                                fallback.extend(
                                                    self.reverse_index
                                                        .accounts_for_bank(&bank)
                                                        .copied(),
                                                );
                                                continue;
                                            };
                                            let projection_plan = tx::TxPlan {
                                                version: tx::BuildVersion::V0,
                                                payer: self.liquidator,
                                                instructions: refreshes,
                                                lookup_tables: vec![].into(),
                                                resources: self.resources,
                                            };
                                            let mut built = match tx::build(
                                                &projection_plan,
                                                bh.hash,
                                                &TxLimits::default(),
                                            ) {
                                                Ok(v) => v,
                                                Err(build_error) => {
                                                    self.metrics.oracle_price_fallbacks = self
                                                        .metrics
                                                        .oracle_price_fallbacks
                                                        .saturating_add(1);
                                                    owner_log!(self,
                                                "oracle_bank_projection_build_fallback: dependency={} bank={} error={}",
                                                dependency, bank, build_error
                                            );
                                                    fallback.extend(
                                                        self.reverse_index
                                                            .accounts_for_bank(&bank)
                                                            .copied(),
                                                    );
                                                    continue;
                                                }
                                            };
                                            if let Err(sign_error) =
                                                tx::sign(&mut built, &[&self.payer])
                                            {
                                                self.metrics.oracle_price_fallbacks = self
                                                    .metrics
                                                    .oracle_price_fallbacks
                                                    .saturating_add(1);
                                                owner_log!(self,
                                            "oracle_bank_projection_sign_fallback: dependency={} bank={} error={}",
                                            dependency, bank, sign_error
                                        );
                                                fallback.extend(
                                                    self.reverse_index
                                                        .accounts_for_bank(&bank)
                                                        .copied(),
                                                );
                                                continue;
                                            }
                                            let work = OraclePriceProjectionWork {
                                                bank,
                                                base_slot: bank_record.slot,
                                                base_bank_hash: hash(&bank_record.data),
                                                tx: built.tx,
                                                targets,
                                            };
                                            match self
                                                .remote
                                                .oracle_price_projection_jobs
                                                .try_push(work)
                                            {
                                                Ok(()) => {
                                                    self.oracle_bank_projection_pending
                                                        .insert(bank, (t_rx_ns, event_slot));
                                                    self.metrics.oracle_bank_projection_submitted =
                                                        self.metrics
                                                            .oracle_bank_projection_submitted
                                                            .saturating_add(1);
                                                    owner_log!(self,
                                                "oracle_bank_projection_pending: dependency={} bank={} price_error={}",
                                                dependency, bank, e
                                            );
                                                }
                                                Err(ChannelError::Full(_)) => {
                                                    self.remote
                                                        .oracle_price_projection_jobs
                                                        .give_up();
                                                    self.metrics.oracle_price_fallbacks = self
                                                        .metrics
                                                        .oracle_price_fallbacks
                                                        .saturating_add(1);
                                                    fallback.extend(
                                                        self.reverse_index
                                                            .accounts_for_bank(&bank)
                                                            .copied(),
                                                    );
                                                }
                                                Err(ChannelError::Disconnected(_)) => {
                                                    return Err(
                                                "oracle price projection RPC worker disconnected"
                                                    .into(),
                                            );
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                fallback
                                    .extend(self.reverse_index.accounts_for_bank(&bank).copied());
                            }
                        }

                        // This tick's input revision (FIX-134): a new cause for every hit.
                        self.admit_oracle_accounts(
                            candidates,
                            fallback,
                            (t_rx_ns, event_slot),
                            self.candidate_input_epoch,
                        );
                    }
                }
            }

            if self.program_upgrade_latched {
                self.st
                    .halt(darkbot_common::error::HaltReason::ProgramUpgrade);
                detect = None;
            }
        } else {
            #[cfg(feature = "crate_health")]
            {
                // E9: parked projection work whose slot is now processed.
                self.release_parked_projections();
                // DEC-01: candidates parked on a stale blockhash return to their lanes once a
                // newer blockhash is stored (nothing is scanned before).
                self.rearm_blockhash_parked()?;
                self.detect_origin = None;
                detect = self
                    .detect_queue
                    .pop_front()
                    .or_else(|| {
                        // P6-MT.5: the lanes' weighted deficit round robin picks the entry; its
                        // `work` carries the newest coalesced `t_rx` (FIX-21 freshness origin,
                        // unchanged by MT3); `t_rx_first` feeds the wait telemetry and, with
                        // the lane, the candidate identity (MT5a).
                        let entry = self.oracle_lanes.next()?;
                        let wait_first = self.clock.now_ns().saturating_sub(entry.t_rx_first);
                        darkbot_common::mt0::MT0.record_queue_wait_first(
                            darkbot_common::mt0::TriggerClass::from_trigger(entry.work.3),
                            wait_first,
                        );
                        self.metrics.lane_wait_ns[entry.lane as usize].record(wait_first);
                        self.detect_origin = Some(generations::DetectOrigin {
                            lane: entry.lane,
                            t_rx_first_ns: entry.t_rx_first,
                            revision: entry.revision,
                            last_forwarded_slot: entry.last_forwarded_slot,
                        });
                        Some(entry.work)
                    })
                    // Z14: bootstrap probes after the oracle-triggered candidates.
                    .or_else(|| self.bootstrap_probe_queue.pop_front())
                    .or_else(|| {
                        next_integration_probe(
                            &mut self.integration_probe_queue,
                            !self.projection_pending.is_empty(),
                            self.clock.now_ns(),
                        )
                    });
                if detect.is_some() {
                    self.ingress_since_detect = 0;
                }
            }
            if detect.is_none() {
                return Ok(OwnerStep::Continue);
            }
        } // normal ingress event path; projected rechecks skip state mutation

        Ok(OwnerStep::Next(detect))
    }
}
