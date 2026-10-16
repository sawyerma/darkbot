//! Ingress application, recovery and detection selection.
use super::*;

impl ShadowOwner {
    pub(super) fn events(
        &mut self,
        fetch: &mut impl FnMut() -> Result<SnapshotData, String>,
        ingress: &IngressSessions,
    ) -> Result<OwnerStep<Option<DetectWork>>, Box<dyn std::error::Error>> {
        self.mt0_owner.enter(darkbot_common::mt0::OwnerCat::Wait);
        #[cfg(feature = "crate_health")]
        let queued_detection_available = !self.detect_queue.is_empty()
            || !self.oracle_detect_pending.is_empty()
            || (!self.integration_probe_queue.is_empty() && self.projection_pending.is_empty());
        #[cfg(feature = "crate_health")]
        let force_detection = queued_detection_available
            && self.ingress_since_detect >= self.ingress_burst_before_detect;
        #[cfg(feature = "crate_health")]
        let maybe_ev = if force_detection {
            None
        } else if queued_detection_available {
            self.consumer.try_pop()
        } else if !self.sweep_dirty_banks.is_empty() {
            self.consumer.pop_timeout(Duration::from_millis(1))
        } else {
            self.consumer.pop_timeout(Duration::from_millis(250))
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
            #[cfg(feature = "crate_health")]
            let updated_marginfi_account: Option<Pubkey> = match &ev {
                IngressEvent::Account(a)
                    if classify_account(&a.data) == AccountType::MarginfiAccount =>
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
                        eprintln!(
                        "PROGRAM_UPGRADE detected slot={} program_data={} — latched HALT until explicit P1B promotion",
                        a.slot, self.program_data
                    );
                    }
                    self.program_upgrade_latched = true;
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
            if let Some(o) = self.drv.on_event_cancellable(
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
            ) {
                eprintln!("recovery: {:?}, run_state={:?}", o, self.st.run_state());
                if matches!(o, darkbot_solana::state::recovery::RecoveryOutcome::Ready) {
                    self.halt.clear();

                    // A program upgrade can race startup/recovery before its WSS notification is
                    // observed. Re-check the canonical recovered ProgramData bytes against the
                    // startup baseline before allowing any detection work.
                    if self
                        .st
                        .get(&self.program_data)
                        .map(|r| hash(&r.data) != self.program_data_baseline_hash)
                        .unwrap_or(true)
                    {
                        self.program_upgrade_latched = true;
                    }
                    if self.program_upgrade_latched {
                        self.st
                            .halt(darkbot_common::error::HaltReason::ProgramUpgrade);
                        eprintln!(
                            "PROGRAM_UPGRADE HALT active program_data={} — no automatic resume",
                            self.program_data
                        );
                        return Ok(OwnerStep::Continue);
                    }

                    #[cfg(feature = "crate_health")]
                    {
                        let (rebuilt, indexed_banks, indexed_accounts, unsupported_banks) =
                            rebuild_reverse_index(&self.st, &self.program, &self.group);
                        self.reverse_index = rebuilt;
                        self.sweep_dirty_banks.clear();
                        self.sweep_batch_events = 0;
                        self.sweep_batch_started_ns = None;
                        self.candidate_cache.clear();
                        self.candidate_force_rearm.clear();
                        self.candidate_input_epoch = 0;
                        self.bank_input_revision.clear();
                        self.threshold_index = ThresholdIndex::new();
                        self.threshold_fallback.clear();
                        self.threshold_bootstrap.clear();
                        if self.use_threshold_trigger {
                            for (pk, record) in self.st.iter_owned_by(&self.program) {
                                if liability_account_in_group(&record.data, &self.group) {
                                    self.threshold_bootstrap.push_back(*pk);
                                }
                            }
                            self.threshold_ready = self.threshold_bootstrap.is_empty();
                        } else {
                            self.threshold_ready = true;
                        }
                        eprintln!(
                        "reverse_index_ready: banks={} accounts={} unsupported_bank_oracle_layouts={} threshold_bootstrap={} threshold_trigger={}",
                        indexed_banks, indexed_accounts, unsupported_banks, self.threshold_bootstrap.len(), self.use_threshold_trigger
                    );
                        if !self.use_threshold_trigger {
                            // [R127c] eva01 parity: one full debtor scan at startup so an account
                            // already underwater before StreamReady cannot wait forever for a future
                            // unrelated write. Reuse the same batched/parallel Health admission path
                            // by marking every current group Bank dirty exactly once.
                            let startup_t_rx_ns = self.clock.now_ns();
                            let startup_slot = self.slot_watch.get();
                            let view = StateView(&self.st);
                            let mut startup_banks = 0usize;
                            for (pk, record) in self.st.iter_owned_by(&self.program) {
                                let Some(bank) = bank_from_state(&view, &self.program, pk) else {
                                    continue;
                                };
                                if bank.group != self.group
                                    || classify_account(&record.data) != AccountType::Bank
                                {
                                    continue;
                                }
                                self.sweep_dirty_banks
                                    .insert(*pk, (startup_t_rx_ns, startup_slot));
                                startup_banks = startup_banks.saturating_add(1);
                            }
                            if startup_banks > 0 {
                                // Force one immediate startup batch; runtime batches use the normal
                                // event-count/time bounds.
                                self.sweep_batch_events = self.sweep_batch_max_events;
                                self.sweep_batch_started_ns = Some(startup_t_rx_ns);
                                self.metrics.sweep_max_dirty_banks = self
                                    .metrics
                                    .sweep_max_dirty_banks
                                    .max(self.sweep_dirty_banks.len());
                            }
                            eprintln!(
                            "r127_health_sweep_ready: reverse_accounts={} startup_banks={} threshold_trigger=false",
                            indexed_accounts, startup_banks
                        );
                        }
                    }

                    #[cfg(feature = "crate_health")]
                    if self.mode == "shadow" && !self.bootstrap_probe_done {
                        self.bootstrap_probe_done = true;
                        let mut probes = Vec::new();
                        for (pk, r) in self.st.iter_owned_by(&self.program) {
                            if bootstrap_probe_candidate(&r.data, &self.group) {
                                probes.push(darkbot_solana::ingress::AccountUpdate {
                                    pubkey: *pk,
                                    owner: r.owner,
                                    lamports: r.lamports,
                                    data: r.data.clone(),
                                    slot: r.slot,
                                    write_version: r.write_version,
                                    source: SourceKind::Snapshot,
                                    t_rx_ns: self.clock.now_ns(),
                                });
                            }
                        }
                        probes.sort_unstable_by_key(|a| a.pubkey.to_bytes());
                        let mut queued = 0usize;
                        for a in probes {
                            if self.producer.try_push(IngressEvent::Account(a)).is_ok() {
                                queued += 1;
                            } else {
                                break;
                            }
                        }
                        eprintln!(
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
                        eprintln!(
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

                            eprintln!(
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

                            eprintln!(
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
                        if !self.use_threshold_trigger {
                            // R127 Long-Tail admission uses targeted exact-health sweeps.
                        } else {
                            let now_s = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0);
                            let slot = self
                                .st
                                .get(&account_pk)
                                .map(|r| r.slot)
                                .unwrap_or(self.slot_watch.get());
                            let view = StateView(&self.st);
                            let engine = darkbot_project0::health::crate_impl::CrateHealth {
                                state: &view,
                                group: self.group,
                            };
                            match engine.thresholds(&account_pk, now_s, slot) {
                                Ok(t) => {
                                    self.threshold_index.set_account(account_pk, &t.thresholds);
                                    self.threshold_fallback.remove(&account_pk);
                                }
                                Err(e) => {
                                    self.threshold_index.remove_account(&account_pk);
                                    self.threshold_fallback.insert(account_pk);
                                    eprintln!(
                                        "threshold_account_fallback: account={} error={}",
                                        account_pk, e
                                    );
                                }
                            }
                        }
                    }
                    None => {
                        self.reverse_index.remove_account(&account_pk);
                        if self.use_threshold_trigger {
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
                if self.use_threshold_trigger {
                    for account in affected_accounts.iter().copied() {
                        self.threshold_index.remove_account(&account);
                        self.threshold_fallback.insert(account);
                        self.threshold_bootstrap.push_back(account);
                    }
                    self.threshold_ready = self.threshold_bootstrap.is_empty();
                } else if self.st.run_state().is_tradable() {
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
                            eprintln!(
                                "reverse_index_bank_unsupported: bank={} error={}",
                                bank_pk, e
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
                let required_now =
                    darkbot_project0::health::dependency_keys(&view, &self.program, &[bk]);
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
                    eprintln!(
                    "dependency set changed: +{} required +{} optional → HALT, resubscribe; Recovery follows StreamReady",
                    new_required.len(),
                    new_optional.len()
                );
                    self.st.halt(darkbot_common::error::HaltReason::StateGap);
                    self.ingress_session.request();
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
                    let mut all_keys: Vec<Pubkey> = self.subscribed.iter().copied().collect();
                    all_keys.sort_unstable_by_key(|k| k.to_bytes());
                    let sub2 = Subscription {
                        programs: vec![],
                        program_filters: vec![
                            (self.program, liability_account_filters(&self.group)),
                            (self.program, bank_filters(&self.group)),
                        ],
                        accounts: all_keys,
                        slots: true,
                        roots: true,
                        transactions_mentioning: vec![],
                        logs_mentioning: vec![],
                    };
                    self.ingress_session =
                        ingress.spawn(sub2, self.producer.clone(), self.halt.clone())?;
                }
            }
            #[cfg(feature = "crate_health")]
            if self.st.run_state().is_tradable() {
                if let Some((dependency, t_rx_ns, event_slot)) = dependency_update {
                    self.metrics.oracle_dependency_updates =
                        self.metrics.oracle_dependency_updates.saturating_add(1);
                    let now_s = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let view = StateView(&self.st);
                    let engine = darkbot_project0::health::crate_impl::CrateHealth {
                        state: &view,
                        group: self.group,
                    };
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

                    if !self.use_threshold_trigger {
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
                        let mut candidates: HashSet<Pubkey> = HashSet::new();
                        for bank in banks {
                            let all_accounts: Vec<Pubkey> = self
                                .reverse_index
                                .accounts_for_bank(&bank)
                                .copied()
                                .collect();
                            if self.threshold_ready {
                                match engine.maintenance_risk_price_bits(&bank, now_s, event_slot) {
                                    Ok((asset_price, liability_price)) => {
                                        candidates.extend(self.threshold_index.candidates(
                                            &bank,
                                            asset_price,
                                            liability_price,
                                            0,
                                        ));
                                        candidates.extend(
                                            all_accounts
                                                .iter()
                                                .copied()
                                                .filter(|a| self.threshold_fallback.contains(a)),
                                        );
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
                                            eprintln!(
                                        "oracle_bank_projection_coalesced: dependency={} bank={} error={}",
                                        dependency, bank, e
                                    );
                                        } else {
                                            let Some(bank_record) = self.st.get(&bank) else {
                                                self.metrics.oracle_price_fallbacks = self
                                                    .metrics
                                                    .oracle_price_fallbacks
                                                    .saturating_add(1);
                                                candidates.extend(all_accounts);
                                                continue;
                                            };
                                            let Some(bank_state) =
                                                bank_from_state(&view, &self.program, &bank)
                                            else {
                                                self.metrics.oracle_price_fallbacks = self
                                                    .metrics
                                                    .oracle_price_fallbacks
                                                    .saturating_add(1);
                                                candidates.extend(all_accounts);
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
                                                candidates.extend(all_accounts);
                                                continue;
                                            }
                                            Err(refresh_error) => {
                                                self.metrics.oracle_price_fallbacks =
                                                    self.metrics.oracle_price_fallbacks.saturating_add(1);
                                                eprintln!(
                                                    "oracle_bank_projection_plan_fallback: dependency={} bank={} price_error={} refresh_error={}",
                                                    dependency, bank, e, refresh_error
                                                );
                                                candidates.extend(all_accounts);
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
                                                candidates.extend(all_accounts);
                                                continue;
                                            }
                                            let Some(bh) = self
                                                .blockhash
                                                .fresh(self.clock.now_ns(), 30_000_000_000)
                                            else {
                                                self.metrics.oracle_price_fallbacks = self
                                                    .metrics
                                                    .oracle_price_fallbacks
                                                    .saturating_add(1);
                                                candidates.extend(all_accounts);
                                                continue;
                                            };
                                            let projection_plan = tx::TxPlan {
                                                version: tx::BuildVersion::V0,
                                                payer: self.liquidator,
                                                instructions: refreshes,
                                                lookup_tables: vec![],
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
                                                    eprintln!(
                                                "oracle_bank_projection_build_fallback: dependency={} bank={} error={}",
                                                dependency, bank, build_error
                                            );
                                                    candidates.extend(all_accounts);
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
                                                eprintln!(
                                            "oracle_bank_projection_sign_fallback: dependency={} bank={} error={}",
                                            dependency, bank, sign_error
                                        );
                                                candidates.extend(all_accounts);
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
                                                    eprintln!(
                                                "oracle_bank_projection_pending: dependency={} bank={} price_error={}",
                                                dependency, bank, e
                                            );
                                                }
                                                Err(ChannelError::Full(_)) => {
                                                    self.metrics.oracle_price_fallbacks = self
                                                        .metrics
                                                        .oracle_price_fallbacks
                                                        .saturating_add(1);
                                                    candidates.extend(all_accounts);
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
                                candidates.extend(all_accounts);
                            }
                        }

                        let mt0_len_before = self.oracle_detect_order.len(); // [MT0]
                        let count = enqueue_oracle_accounts(
                            candidates,
                            t_rx_ns,
                            event_slot,
                            "oracle_threshold",
                            &mut self.oracle_detect_order,
                            &mut self.oracle_detect_pending,
                            &mut self.metrics.oracle_enqueued,
                            &mut self.metrics.oracle_coalesced,
                            &mut self.metrics.oracle_max_pending,
                        );
                        for _ in mt0_len_before..self.oracle_detect_order.len() {
                            self.mt0_order_rx.push_back(t_rx_ns);
                        }
                        self.metrics.oracle_candidate_hits = self
                            .metrics
                            .oracle_candidate_hits
                            .saturating_add(count as u64);
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
                detect = self
                    .detect_queue
                    .pop_front()
                    .or_else(|| {
                        let mt0_len_before = self.oracle_detect_order.len();
                        let work = next_oracle_detect(
                            &mut self.oracle_detect_order,
                            &mut self.oracle_detect_pending,
                        );
                        let mut mt0_first = None;
                        for _ in 0..mt0_len_before.saturating_sub(self.oracle_detect_order.len()) {
                            mt0_first = self.mt0_order_rx.pop_front();
                        }
                        if let (Some(w), Some(first)) = (&work, mt0_first) {
                            darkbot_common::mt0::MT0.record_queue_wait_first(
                                darkbot_common::mt0::TriggerClass::from_trigger(w.3),
                                self.clock.now_ns().saturating_sub(first),
                            );
                        }
                        work
                    })
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
