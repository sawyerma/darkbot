//! Production bootstrap and preparation of the single owner.
use super::*;

type Prepared<F> = Result<(ShadowOwner, F, IngressSessions), Box<dyn std::error::Error>>;

/// S1-02: the controlled sources. The long-tail WSS feed is the canonical baseline; one Geyser
/// unit (`source_geyser` = `P0_SOURCE_GEYSER` off|observe, provider `source_geyser_provider`, both
/// of the operational config, DEC-05) observes only: ACTIVE needs a qualification this build has none of. OFF
/// constructs and connects nothing. The provider token is read once here, never logged. With
/// `P0_PROJECT_DIR` the control thread serves `p0ctl source set` at runtime.
fn configured_sources(
    econ: Option<&crate::analytics::econ::Handle>,
    clock: Clock,
    wake: &darkbot_common::channel::Wakeup,
    shutdown: &Shutdown,
) -> Result<super::sources::OwnerSources, Box<dyn std::error::Error>> {
    use crate::orchestration::source_control;
    use darkbot_solana::ingress::sources::{
        geyser_source, OperatingMode, ProviderConfig, SourceArbiter, SourceUnit,
    };
    let ops = darkbot_solana::runtime_config::active();
    let mode: OperatingMode = ops.source_geyser.parse()?;
    let provider_id = ops.source_geyser_provider.clone();
    let provider = ProviderConfig::publicnode();
    if provider_id != provider.id {
        return Err(format!("SOURCE_PROVIDER_UNKNOWN: {provider_id}").into());
    }
    let unit = SourceUnit::BinaryGeyser {
        provider: provider.id.clone(),
    };
    let arbiter = SourceArbiter::new(&[
        source_control::control_of(SourceUnit::WsRpcControl, OperatingMode::Active)?,
        source_control::control_of(unit.clone(), mode)?,
    ])?;
    let token_env = provider.auth.map(|a| a.token_env);
    let token = token_env.and_then(|k| std::env::var(k).ok());
    let failure = econ.map(|h| h.shared.clone());
    let spawn: super::sources::GeyserSpawner =
        Box::new(move |control, sub, sink, stop, halt, slots, canonical| {
            let env = |k: &str| (Some(k) == token_env).then(|| token.clone()).flatten();
            let mut src = geyser_source(control, &provider, sub, &env)
                .map_err(|reasons| reasons.join(","))?
                .ok_or_else(|| "SOURCE_OFF".to_string())?;
            // One clock with the owner: T_rx stays comparable to its queue-wait measurement.
            src.clock = clock;
            // Only a canonical session's failure ends an econ run; an observer's is measured.
            let failure = if canonical { failure.clone() } else { None };
            std::thread::Builder::new()
                .name(
                    if canonical {
                        "ingress-geyser"
                    } else {
                        "observe-geyser"
                    }
                    .into(),
                )
                .spawn(move || super::ingress::run_ingress(src, sink, stop, halt, slots, failure))
                .map_err(|e| format!("SOURCE_THREAD: {e}"))
        });
    let controls = match std::env::var_os("P0_PROJECT_DIR") {
        Some(project) => {
            let project = std::path::Path::new(&project);
            let dir = source_control::request_dir(project);
            let (tx, rx) = bounded::<source_control::ControlMessage>("source-control", 8);
            // CTRL-03: the configuration version the owner starts from (the request log's head).
            let baseline = source_control::head(&dir)?;
            // DEC-13: the capability log, decided from its head at the start (a run never
            // inherits an earlier LIVE request).
            let capabilities = crate::orchestration::capability_control::request_dir(project);
            let capability_baseline =
                crate::orchestration::capability_control::head(&capabilities)?;
            source_control::spawn(
                dir.clone(),
                tx.with_wakeup(wake),
                shutdown.clone(),
                baseline,
                Some((capabilities.clone(), capability_baseline)),
            )?;
            Some((rx, (dir, baseline, capabilities, capability_baseline)))
        }
        None => None,
    };
    let (controls, decisions) = controls.unzip();
    let sources =
        super::sources::OwnerSources::new(arbiter, Some(unit), spawn, 4_096, wake, controls);
    Ok(match decisions {
        Some((dir, baseline, capabilities, capability_baseline)) => sources
            .record_decisions_in(dir, baseline)
            .capability_control_in(capabilities, capability_baseline),
        None => sources,
    })
}

/// DEC-16: an unbounded Shadow run runs only admitted with its rolling windows — never measured
/// as one window (`get`: the run's environment; an econ run's windows are its supervisor's).
pub(super) fn refuse_unadmitted_unbounded(
    econ: bool,
    rolling: bool,
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<(), String> {
    if !econ && !rolling && crate::orchestration::until_stopped::unbounded_duration(get) {
        return Err(format!(
            "{}: an unbounded Shadow run starts only admitted with its rolling windows",
            crate::analytics::measurement::UNTIL_STOPPED_GATED
        ));
    }
    Ok(())
}

#[cfg(test)]
mod dec16_admission_tests {
    /// DEC-16: an unbounded Shadow start without its admitted rolling windows is refused before
    /// anything is read; a bounded one, an admitted one and an econ run's owner are not.
    #[test]
    fn dec16_an_unadmitted_unbounded_shadow_start_is_refused() {
        let unbounded =
            |k: &str| (k == "P0_MEASUREMENT_DURATION").then(|| "until-stopped:1h:1d".to_string());
        let bounded = |k: &str| (k == "P0_MEASUREMENT_DURATION").then(|| "4h".to_string());
        let err = super::refuse_unadmitted_unbounded(false, false, &unbounded).unwrap_err();
        assert!(
            err.starts_with(crate::analytics::measurement::UNTIL_STOPPED_GATED),
            "{err}"
        );
        assert!(super::refuse_unadmitted_unbounded(false, true, &unbounded).is_ok());
        assert!(super::refuse_unadmitted_unbounded(true, false, &unbounded).is_ok());
        assert!(super::refuse_unadmitted_unbounded(false, false, &bounded).is_ok());
    }
}

pub(super) fn prepare(
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
    admission: Option<crate::orchestration::boot_audit::NetworkAudit>,
    rolling: Option<crate::orchestration::until_stopped::Rolling>,
) -> Prepared<impl FnMut() -> Result<SnapshotData, String>> {
    let operational_start = clock.now_ns();
    refuse_unadmitted_unbounded(econ.is_some(), rolling.is_some(), &|k| {
        std::env::var(k).ok()
    })?;
    // DEC-16 (review M-1): a continued econ4h run's owner reads nothing before its capture held
    // the window the crash interrupted (bounded by the plan's bootstrap budget).
    if let Some(h) = econ
        .as_ref()
        .filter(|h| h.cfg.rolling.as_ref().is_some_and(|r| r.resume.is_some()))
    {
        h.await_resume_check(Duration::from_secs(h.cfg.plan.budgets.bootstrap_seconds))?;
    }
    // DEC-16: a crashed unbounded Shadow run continues only as itself — the window the crash
    // interrupted re-checked before anything is read (its HALT ends the run here).
    let resume_ns = clock.now_ns();
    let resume_s = (clock.unix_ns(resume_ns) / 1_000_000_000) as u64;
    let resumed = match rolling.as_ref().and_then(|r| Some((r, r.resume.as_ref()?))) {
        Some((r, run)) => {
            let record = crate::analytics::journal::binary_path(std::path::Path::new(
                &config::require("P0_RECORD_PATH")?,
            ));
            let dir = super::windows::resume_dir(&record, run)?;
            Some(super::windows::resume_windows(
                r,
                &dir,
                resume_s,
                crate::storage::preflight::environment,
                &darkbot_solana::provider::shared(),
            )?)
        }
        None => None,
    };
    let stop_poll: fn() -> bool = if econ.is_none() {
        crate::orchestration::control::stop_requested
    } else {
        || false
    };
    let operational_window = if econ.is_none() && darkbot_solana::runtime_config::configured() {
        Some(crate::analytics::measurement::ReadyWindow::new(
            crate::analytics::measurement::MeasurementPlan::from_lookup(&|k| {
                std::env::var(k).ok()
            })?,
        ))
    } else {
        None
    };
    let operational_failure = None;
    let admission = admission.map(Box::new);
    if admission.is_some() {
        crate::console::line(
            "admission: approved network audit verified; identities bind at the first Ready".into(),
        );
    }
    let ws_url = config::require("P0_WS_URL")?;
    let ws_url_for_spawn = ws_url.clone();
    let rpc_url = config::require("P0_RPC_URL")?;
    let program = Pubkey::from_str(&config::require("P0_PROGRAM_ID")?)?;
    let mode = config::require("P0_MODE")?;
    if mode != "shadow" && mode != "live" && !(mode == "econ4h" && econ.is_some()) {
        return Err("P0_MODE must be shadow|live".into());
    }
    if mode == "live" {
        return Err(
            "live mode requires P8-LT qualification in MEASUREMENT_LEDGER; not enabled".into(),
        );
    }
    // D01/D11: every operational bound from the typed limits (one default authority).
    let operational = darkbot_solana::runtime_config::active().limits.clone();
    let buffer_cap = usize::try_from(operational.ingress_queue)?;

    let group = Pubkey::from_str(&config::require("P0_GROUP_ID")?)?;
    let payer = tx::load_keypair_json(&config::require("P0_KEYPAIR_PATH")?)?;
    let liquidator = tx::payer_of(&payer);
    let shutdown = Shutdown::new();
    let halt = IngressHalt::new();
    let slot_watch = SlotWatch::new(); // [FIX-40]
                                       // A23-01: one doorbell for ingress, worker results, census and stop; an econ run's abort
                                       // rings the same one.
    let wake = econ
        .as_ref()
        .map_or_else(Default::default, |h| h.shared.owner_wake.clone());
    shutdown.ring_on_request(&wake);
    let (producer, consumer) = bounded::<IngressEvent>("ingress", buffer_cap);
    let producer = producer.with_wakeup(&wake);

    // [FIX-78] Bootstrap only the configured group's Banks first. Full-program GPA was ~387 MB
    // in C0 and is unnecessary for dependency discovery.
    let dep_rpc = darkbot_solana::provider::rpc_client(
        rpc_url.clone(),
        Duration::from_millis(operational.rpc_timeout_ms),
        Default::default(),
        darkbot_solana::provider::Workload::Bootstrap,
    );

    // [FIX-131] Loader-v3 upgrades mutate the ProgramData account, not the executable
    // Program account. Pin its current content hash at startup and subscribe it as a required
    // dependency. Any later mismatch is a latched PROGRAM_UPGRADE halt; reconnect/recovery
    // must never auto-promote new protocol code.
    // [P1A-04] The baseline must also be the qualified, source-pinned deployment: an upgrade
    // between qualification and startup latches the same HALT as one observed at runtime.
    // BOOT-12: cluster identity first (genesis pin), then the program identity below (ProgramData
    // against the qualified deployment pin, latched PROGRAM_UPGRADE), both before admission.
    let genesis = dep_rpc.get_genesis_hash()?;
    check_cluster(&genesis)?;
    crate::console::line(format!("cluster: genesis={genesis}"));
    let program_data = darkbot_project0::protocol::program_data_address(&program);
    let program_data_account = dep_rpc
        .get_account_with_commitment(&program_data, CommitmentConfig::confirmed())?
        .value
        .ok_or_else(|| format!("PROGRAM_DATA_NOT_FOUND: {}", program_data))?;
    let deployment = darkbot_project0::protocol::decode_program_data(
        &program_data,
        &program_data_account.owner,
        program_data_account.executable,
        &program_data_account.data,
    )
    .map_err(|e| format!("PROGRAM_DATA_INVALID: {} {:?}", program_data, e))?;
    let program_data_baseline_hash = deployment.hash;
    let deployment_pin = darkbot_project0::protocol::deployment_pin(&program, &deployment);
    crate::console::line(format!(
        "program_upgrade_watch: program={} program_data={} baseline_hash={} deployment_slot={} upgrade_authority={} pin={:?}",
        program,
        program_data,
        program_data_baseline_hash,
        deployment.deployment_slot,
        deployment.upgrade_authority.map_or_else(|| "NONE".to_string(), |a| a.to_string()),
        deployment_pin
    ));

    // [FIX-98] A signed Shadow simulation uses the real fee payer. If that pubkey has never
    // been funded on-chain, Solana rejects the transaction during account loading with
    // TransactionError::AccountNotFound before any program executes (empty logs). Fail here
    // with the exact payer instead of misclassifying the first real Build/Sign/Sim candidate.
    let payer_account = dep_rpc
        .get_account_with_commitment(&liquidator, CommitmentConfig::confirmed())?
        .value;
    let Some(payer_account) = payer_account else {
        return Err(format!(
            "PAYER_ACCOUNT_NOT_FOUND: liquidator={} has no confirmed on-chain account; fund this exact fee-payer pubkey with SOL before Shadow Build/Sign/Simulation",
            liquidator
        ).into());
    };
    crate::console::line(format!(
        "payer_preflight: pubkey={} lamports={} owner={} executable={}",
        liquidator, payer_account.lamports, payer_account.owner, payer_account.executable
    ));
    #[cfg(feature = "crate_health")]
    let pre =
        snapshot::program_accounts_filtered(&dep_rpc, &program, bank_filters(&group), &clock)?;
    let mut pre_state = StateCore::new(1);
    pre_state.install_snapshot(pre.slot, pre.accounts);
    let bank_keys: Vec<Pubkey> = pre_state
        .iter_owned_by(&program)
        .filter_map(|(k, r)| {
            if classify_account(&r.data) != AccountType::Bank {
                return None;
            }
            #[cfg(feature = "crate_health")]
            {
                let view = StateView(&pre_state);
                bank_from_state(&view, &program, k)
                    .filter(|b| b.group == group)
                    .map(|_| *k)
            }
        })
        .collect();
    #[cfg(feature = "crate_health")]
    let mut dependency_gaps = HashMap::new();
    #[cfg(feature = "crate_health")]
    let mut required_dependency_keys = {
        // B16: banks without a complete dependency set are reported, never silently skipped.
        let scan =
            darkbot_project0::health::dependency_scan(&StateView(&pre_state), &program, &bank_keys);
        for (bank, gap) in scan.gaps {
            crate::console::line(format!(
                "dependency_gap: bank={bank} code={} detail={gap:?}",
                gap.code()
            ));
            // N6: the owner starts with these gaps known; it reports only their changes.
            dependency_gaps.insert(bank, gap);
        }
        let mut v = scan.keys;
        v.push(group);
        v.push(protocol::fee_state_pda(&program));
        v.sort_unstable_by_key(|k| k.to_bytes());
        v.dedup();
        v
    };

    // ProgramData is required in every build: protocol upgrades are a lifecycle concern,
    // independent of whether crate_health is enabled.
    required_dependency_keys.push(program_data);
    // [C0-FIX-03] The Clock sysvar is canonical state: every protocol evaluation uses its
    // slot/unix timestamp from the same state generation instead of local wall time.
    required_dependency_keys.push(solana_sdk::sysvar::clock::id());
    // B4: the Rent sysvar is canonical state too: the rent of a LiquidationRecord a plan creates
    // comes from it, never from a constant.
    required_dependency_keys.push(solana_sdk::sysvar::rent::id());
    required_dependency_keys.sort_unstable_by_key(|k| k.to_bytes());
    required_dependency_keys.dedup();

    #[cfg(feature = "crate_health")]
    let mut optional_dependency_keys: Vec<Pubkey> = bank_keys
        .iter()
        .flat_map(|bk| ata_candidates_for_bank(&StateView(&pre_state), &program, bk, &liquidator))
        .collect();
    #[cfg(feature = "crate_health")]
    optional_dependency_keys.extend(crate::execution::lookup_tables::published_keys(&group));
    optional_dependency_keys.sort_unstable_by_key(|k| k.to_bytes());
    optional_dependency_keys.dedup();

    #[cfg(feature = "crate_health")]
    let native_sol_mint = Pubkey::from_str(protocol::NATIVE_SOL_MINT)?;
    #[cfg(feature = "crate_health")]
    let (sol_bank, sol_cost_bank) =
        find_supported_cost_bank_by_mint(&StateView(&pre_state), &program, &bank_keys, &native_sol_mint)
            .ok_or("no supported generic Marginfi WSOL bank found; cannot value SOL-denominated liquidation costs")?;
    #[cfg(feature = "crate_health")]
    crate::console::line(format!(
        "sol_cost_bank: {} asset_tag={} oracle_setup={:?} operational_state={:?}",
        sol_bank,
        sol_cost_bank.config.asset_tag,
        sol_cost_bank.config.oracle_setup,
        sol_cost_bank.config.operational_state
    ));

    let mut subscription_keys = required_dependency_keys.clone();
    subscription_keys.extend(optional_dependency_keys.iter().copied());
    subscription_keys.sort_unstable_by_key(|k| k.to_bytes());
    subscription_keys.dedup();

    crate::console::line(format!(
        "dependencies: banks={} required={} optional_atas={}",
        bank_keys.len(),
        required_dependency_keys.len(),
        optional_dependency_keys.len()
    ));
    drop(pre_state);
    // Ingress-Thread (WSS). Provider nur als URL.
    let sub = Subscription {
        programs: vec![],
        program_filters: vec![
            (program, liability_account_filters(&group)),
            (program, bank_filters(&group)),
        ],
        accounts: subscription_keys.clone(),
        slots: true,
        roots: true,
        transactions_mentioning: vec![],
        logs_mentioning: vec![],
    };
    // [FIX-34] Ingress als neu startbare Session: bei Änderung der Dependency-Menge wird die alte
    // Session beendet, eine neue mit erweiterter Subscription gestartet, dann HALT → Recovery → READY.
    let slot_watch_for_spawn = slot_watch.clone();
    let ingress = IngressSessions {
        ws_url: ws_url_for_spawn,
        slot_watch: slot_watch_for_spawn,
        clock,
        econ: econ.clone(),
    };
    let subscribed: HashSet<Pubkey> = subscription_keys.iter().copied().collect();
    let required_set: HashSet<Pubkey> = required_dependency_keys.iter().copied().collect();
    let optional_set: HashSet<Pubkey> = optional_dependency_keys.iter().copied().collect();
    // Refused source controls fail the start before any session runs.
    let sources = configured_sources(econ.as_ref(), clock, &wake, &shutdown)?;
    let ingress_session = ingress.spawn(sub.clone(), producer.clone(), halt.clone(), 1)?;

    // State + Recovery (Snapshot via RPC, außerhalb des Decision-Loops aufgerufen).
    // Large, filtered Marginfi getProgramAccounts payloads regularly approach the
    // RpcClient::new() 30-second default. Keep a bounded, independently configurable
    // timeout for snapshot/dependency fetches; never weaken recovery/reconcile gates.
    let snapshot_rpc_timeout_secs = operational.snapshot_timeout_seconds;
    crate::console::line(format!(
        "snapshot: rpc_timeout_s={snapshot_rpc_timeout_secs}"
    ));
    let rpc = darkbot_solana::provider::rpc_client(
        rpc_url.clone(),
        Duration::from_secs(snapshot_rpc_timeout_secs),
        Default::default(),
        darkbot_solana::provider::Workload::Bootstrap,
    );
    let st = StateCore::new(buffer_cap);
    // [FIX-96] Production recovery waits for confirmed WSS/root catch-up before
    // Snapshot-B reconciliation. This prevents false dirty reconciles when RPC Snapshot B
    // is ahead of account notifications still in flight on the same confirmed stream.
    // Adaptive root-only stall extension. All limits are bounded (the registry's ranges, the
    // hard deadline at least the catch-up) and the hard deadline is independent of progress,
    // with no impact on trading hot paths.
    let root_catchup_secs = operational.recovery_root_catchup_seconds;
    let root_stall_secs = operational.recovery_root_stall_seconds;
    let root_hard_secs = operational.recovery_root_hard_max_seconds;
    crate::console::line(format!(
        "recovery: root_soft_s={} root_stall_s={} root_hard_s={}",
        root_catchup_secs, root_stall_secs, root_hard_secs
    ));
    let drv = RecoveryDriver::new(3)
        .with_root_catchup(Duration::from_secs(root_catchup_secs))
        .with_root_progress_limits(
            Duration::from_secs(root_stall_secs),
            Duration::from_secs(root_hard_secs),
        );
    let dep_shared = std::sync::Arc::new(std::sync::Mutex::new(required_dependency_keys.clone()));
    let opt_shared = std::sync::Arc::new(std::sync::Mutex::new(optional_dependency_keys.clone()));
    let dep_for_fetch = dep_shared.clone();
    let opt_for_fetch = opt_shared.clone();
    let fetch = move || -> Result<SnapshotData, String> {
        let t0 = std::time::Instant::now();
        crate::console::line("snapshot: start".into());
        // [FIX-78] Recovery snapshot = target-group liability accounts + target-group banks +
        // explicit required/optional dependencies. Lending-only accounts are excluded.
        #[cfg(feature = "crate_health")]
        let a = snapshot::program_accounts_filtered(
            &rpc,
            &program,
            liability_account_filters(&group),
            &clock,
        )
        .map_err(|e| {
            // Log a duration without provider URL, response body or API credentials.
            crate::console::line(format!(
                "snapshot: failed stage=liability_gpa elapsed_ms={}",
                t0.elapsed().as_millis()
            ));
            format!("stage=liability_gpa {e}")
        })?;
        #[cfg(feature = "crate_health")]
        let b = snapshot::program_accounts_filtered(&rpc, &program, bank_filters(&group), &clock)
            .map_err(|e| format!("stage=bank_gpa {e}"))?;
        let required = dep_for_fetch
            .lock()
            .map_err(|_| "dep lock".to_string())?
            .clone();
        let optional = opt_for_fetch
            .lock()
            .map_err(|_| "opt dep lock".to_string())?
            .clone();
        let d = snapshot::multiple_accounts(&rpc, &required, &clock)
            .map_err(|e| format!("stage=required_accounts {e}"))?;
        let o = snapshot::multiple_accounts(&rpc, &optional, &clock)
            .map_err(|e| format!("stage=optional_accounts {e}"))?;
        let liability_count = a.accounts.len();
        let bank_count = b.accounts.len();
        let required_count = d.accounts.len();
        let optional_count = o.accounts.len();
        let missing_required_count = d.missing.len();
        let mut accounts = a.accounts;
        accounts.extend(b.accounts);
        accounts.extend(d.accounts);
        accounts.extend(o.accounts);
        // Required dependencies stay fail-closed; missing optional ATAs only disable that candidate.
        let mut slot = a.slot;
        if b.slot != 0 {
            slot = slot.min(b.slot);
        }
        if d.slot != 0 {
            slot = slot.min(d.slot);
        }
        if o.slot != 0 {
            slot = slot.min(o.slot);
        }
        crate::console::line(format!(
            "snapshot: done slot={} liability_accounts={} banks={} required={} optional={} missing_required={} total={} elapsed_ms={}",
            slot,
            liability_count,
            bank_count,
            required_count,
            optional_count,
            missing_required_count,
            accounts.len(),
            t0.elapsed().as_millis()
        ));
        Ok(SnapshotData {
            slot,
            accounts,
            missing_required: d.missing,
        })
    };
    // [FIX-35] Bootstrap läuft über die StreamReady-Barriere: das erste `Reconnected` (alle ACKs)
    // löst im Event-Loop die Recovery aus. Kein sleep, kein vorzeitiger Snapshot.
    crate::console::line("bootstrap: waiting for StreamReady (all subscription ACKs)".into());
    // [FIX-1] Pipeline-Bausteine: Keypair (nur signieren, Shadow sendet nicht), Blockhash-Cache,
    // Safety aus Env (Limits sind Konfiguration), Allowlist aus Source, Submitter nur für Simulation.
    let blockhash = BlockhashCache::new();
    let bh_rpc = darkbot_solana::provider::rpc_client(
        rpc_url.clone(),
        Duration::from_millis(operational.rpc_timeout_ms),
        Default::default(),
        darkbot_solana::provider::Workload::LiveRead,
    );
    let bh_cache = blockhash.clone();
    let bh_requests = blockhash.clone();
    let bh_retry = Duration::from_millis(operational.rpc_backoff_ms);
    let bh_clock = clock;
    let bh_econ = econ.clone();
    let bh_shutdown = shutdown.clone();
    let bh_interval = Duration::from_millis(operational.blockhash_refresh_ms);
    let bh_wake = wake.clone();
    let blockhash_thread =
        std::thread::Builder::new()
            .name("blockhash".into())
            .spawn(move || {
                let admission_closed =
                    move || bh_econ.as_ref().is_some_and(|h| h.admission_closed());
                run_blockhash_refresh(
                    &bh_shutdown,
                    &admission_closed,
                    || bh_cache.refresh(&bh_rpc, bh_clock.now_ns()).map(|_| ()),
                    bh_interval,
                    &bh_requests,
                    bh_retry,
                    &|| bh_wake.notify(),
                )
            })?;
    // DEC-05: the Safety limits are parameters of the operational config (validated ranges).
    let limits = Limits {
        max_total_capital: operational.max_total_capital,
        // USD-micro, conservative hold values (B15).
        max_outstanding_inventory: operational.max_outstanding_inventory,
        max_concurrent_positions: u32::try_from(operational.max_concurrent_positions)?,
        max_exposure_per_attempt: operational.max_exposure_per_attempt,
        max_daily_loss: operational.max_daily_loss,
        max_failed_attempts_per_day: u32::try_from(operational.max_failed_attempts_per_day)?,
        max_tip: operational.max_tip,
        max_slippage_bps: operational.max_slippage_bps,
        max_outstanding_attempts: u32::try_from(operational.max_outstanding_attempts)?,
    };
    // L4: one strict parse; the evaluation uses exactly the configured safety value.
    let eval_slippage_bps = limits.max_slippage_bps;
    let mut allow = Allow::default();
    allow.execution_classes.insert("longtail");
    let mut safety = Safety::new(limits, allow, false);
    // [FIX-25] Semantische Kategorien getrennt; die globale Top-Level-Allowlist prüft die Pipeline separat.
    let mut allowlist = Allowlist::default();
    for s in [
        protocol::JUP_SWAP_PROGRAM_ID,
        protocol::TITAN_SWAP_PROGRAM_ID,
    ] {
        allowlist.swap_programs.insert(Pubkey::from_str(s)?);
    }
    for s in [
        protocol::KAMINO_PROGRAM_ID,
        protocol::DRIFT_PROGRAM_ID,
        protocol::JUPLEND_PROGRAM_ID,
    ] {
        let p = Pubkey::from_str(s)?;
        allowlist.integration_programs.insert(p);
        allowlist.refresh_programs.insert(p); // Pre-Start-Refreshes nur dieser Programme (liquidate_start.rs)
    }
    // E2: init_liquidation_record is its own planner step (only before Start), not a refresh.
    #[cfg(feature = "crate_health")]
    let instruction_table = darkbot_project0::receivership::builders::instruction_table(&program)?;
    let global_allowed = protocol::receivership_allowed_programs();
    // [FIX-30] Prerequisite-TXs (JIT-Buy): Swap-Programme + System/Token/Token-2022/ATA/ComputeBudget.
    let prereq_allowed: Vec<Pubkey> = [
        protocol::JUP_SWAP_PROGRAM_ID,
        protocol::TITAN_SWAP_PROGRAM_ID,
        protocol::ASSOCIATED_TOKEN_PROGRAM_ID,
        protocol::COMPUTE_BUDGET_PROGRAM_ID,
        darkbot_project0::receivership::SYSTEM_PROGRAM,
        protocol::TOKEN_PROGRAM_ID,
        protocol::TOKEN_2022_PROGRAM_ID,
    ]
    .iter()
    .map(|s| Pubkey::from_str(s))
    .collect::<Result<_, _>>()?;
    // N2: the owner's candidate path quotes and simulates through the deadline-bound transport.
    let submitter = crate::execution::observe_only::ObserveOnly::new(
        crate::execution::bundle_path::OwnerSubmitter::new(RpcSubmitter::for_candidates(
            rpc_url.clone(),
            true,
        )?),
    );
    let failed_costs = FailedCostMatrix::default(); // alle None → Unknown → nicht handelbar, bis gemessen
    #[cfg(feature = "crate_health")]
    let routes = JupiterV2RouteProvider::new(
        liquidator,
        std::env::var("P0_JUPITER_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        operational.max_slippage_bps,
        u8::try_from(operational.jupiter_max_accounts)?,
        true,
    )
    .map_err(|e| format!("route provider: {e}"))?;
    let resources = Resources {
        compute_unit_limit: u32::try_from(operational.compute_unit_limit)?,
        priority_micro_lamports_per_cu: operational.priority_micro_lamports_per_cu,
        loaded_accounts_data_size: Some(u32::try_from(operational.loaded_accounts_data_size)?),
    };
    let freshness = Freshness {
        max_opportunity_age_ns: operational.max_opportunity_age_ns,
        execution_deadline_ns: operational.execution_deadline_ns,
    };
    let min_profit = i128::from(operational.min_profit_usd_micro);

    // The integration probe is `diagnostics` (`P0_INTEGRATION_PROBE`); its limit is > 0 with
    // diagnostics on (validated).
    #[cfg(feature = "crate_health")]
    let integration_probe_enabled = darkbot_solana::runtime_config::active().diagnostics;
    #[cfg(feature = "crate_health")]
    let integration_probe_limit_per_kind =
        usize::try_from(operational.integration_probe_limit_per_kind)?;

    // [FIX-108][R4] All remote winner/reconciliation RPC is owned by bounded workers.
    let remote_worker_capacity = usize::try_from(operational.worker_queue)?;
    // B01: OFF or OBSERVE (ACTIVE needs LIVE and is refused); OBSERVE quotes on its own worker.
    #[cfg(feature = "crate_health")]
    let rebalance_mode = crate::execution::inventory::parse_mode(
        &std::env::var("P0_REBALANCE_MODE").unwrap_or_default(),
    )?;
    let remote = {
        #[cfg_attr(not(feature = "crate_health"), allow(unused_mut))]
        let mut remote = RemoteWorkers::start(
            rpc_url.clone(),
            program,
            &shutdown,
            remote_worker_capacity,
            &wake,
        )?;
        #[cfg(feature = "crate_health")]
        if rebalance_mode == crate::execution::inventory::RebalanceMode::Observe {
            remote.start_rebalance_quoter(routes.clone(), &shutdown)?;
        }
        remote
    };

    #[cfg(feature = "crate_health")]
    let projection_cache: HashMap<Pubkey, CachedProjection> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let projection_pending: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let detect_queue: VecDeque<DetectWork> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let oracle_dependency_updates: u64 = 0;
    #[cfg(feature = "crate_health")]
    let oracle_candidate_hits: u64 = 0;
    #[cfg(feature = "crate_health")]
    let oracle_enqueued: u64 = 0;
    #[cfg(feature = "crate_health")]
    let oracle_coalesced: u64 = 0;
    #[cfg(feature = "crate_health")]
    let oracle_price_fallbacks: u64 = 0;
    #[cfg(feature = "crate_health")]
    let oracle_max_pending: usize = 0;
    #[cfg(feature = "crate_health")]
    let oracle_bank_projection_pending: HashMap<Pubkey, (u64, u64)> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let oracle_bank_projection_submitted: u64 = 0;
    #[cfg(feature = "crate_health")]
    let oracle_bank_projection_ready: u64 = 0;
    #[cfg(feature = "crate_health")]
    let oracle_bank_projection_failed: u64 = 0;
    #[cfg(feature = "crate_health")]
    let oracle_bank_projection_stale: u64 = 0;
    #[cfg(feature = "crate_health")]
    let integration_probe_expected: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let integration_probe_queue: VecDeque<(Pubkey, u64)> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let integration_probe_terminal: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let integration_probe_projection_ready = 0usize;
    #[cfg(feature = "crate_health")]
    let integration_probe_projection_failed = 0usize;
    #[cfg(feature = "crate_health")]
    let integration_probe_projection_stale = 0usize;
    #[cfg(feature = "crate_health")]
    let integration_probe_completion_reported = false;

    // [M2][M3] Shadow-Evidence: Kandidaten-Watchlist für den Winner-Vergleich und Tages-Ledger.
    let ledger = ShadowLedger::new();
    // [F1] Bounded, deduplicated winner watches with TTL for Ok(None) and Err. PROVISIONAL defaults.
    let winner_watch_cfg = WinnerTrackerConfig {
        capacity: usize::try_from(operational.winner_watch_capacity)?,
        ttl_s: operational.winner_watch_ttl_seconds,
        initial_delay_s: operational.winner_watch_initial_delay_seconds,
        recheck_interval_s: operational.winner_watch_recheck_seconds,
        full_queue_backoff_s: operational.winner_watch_full_backoff_seconds,
    }
    .validate()?;
    crate::console::line(format!(
        "winner_watch_config: capacity={} ttl_s={} initial_delay_s={} recheck_s={} full_backoff_s={} provisional=true",
        winner_watch_cfg.capacity,
        winner_watch_cfg.ttl_s,
        winner_watch_cfg.initial_delay_s,
        winner_watch_cfg.recheck_interval_s,
        winner_watch_cfg.full_queue_backoff_s
    ));
    let winner_tracker = WinnerTracker::new(winner_watch_cfg);
    // Every launcher names the run's own record (one directory per run, never a shared
    // default journal that a second run would append to).
    let record_path = config::require("P0_RECORD_PATH")?;
    let journal_path = crate::analytics::journal::binary_path(std::path::Path::new(&record_path));
    std::fs::create_dir_all(journal_path.parent().unwrap_or(std::path::Path::new(".")))?;
    if let Some(effective) = darkbot_solana::runtime_config::effective() {
        // JFI-04: the effective configuration is kept as a typed, secret-free journal record.
        let parent = std::path::Path::new(&record_path)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        let name = format!("operational_config_{}.p0j", clock.unix_ns(clock.now_ns()));
        crate::analytics::journal::write_typed_value(
            &parent.join(name),
            crate::analytics::journal::source::OWNER,
            effective,
        )?;
    }

    let recovery_path = journal_path.with_extension(format!(
        "recovery-{}-{}.p0j",
        std::process::id(),
        clock.now_ns()
    ));
    // Without P0_MT0_PATH the MT0 journal belongs to the same run as the opportunity journal
    // (`<record>.mt0.p0j`), so runs sharing one directory never collide.
    let mt0_default = journal_path.with_extension("mt0.p0j");
    let census_journal = journal_path.clone();
    let run_dir = journal_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    // DEC-16: an unbounded run's owner journals are segment chains (sealed per window and at
    // the segment size); the MT0 journal follows the window the owner opened. An econ4h run's
    // windows are its econ grid.
    let econ_rolling = econ.as_ref().is_some_and(|h| h.cfg.unbounded);
    // DEC-16: a continued econ4h run's owner continues its journals' chains.
    let econ_resume = econ
        .as_ref()
        .and_then(|h| h.cfg.rolling.as_ref())
        .is_some_and(|r| r.resume.is_some());
    let segmented = rolling.is_some() || econ_rolling;
    let segment_bytes = operational.econ_segment_mib * crate::storage::budget::MIB;
    let mt0_window = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(
        resumed.as_ref().map_or(0, |(w, _)| w.window()),
    ));
    let evidence_sync = Duration::from_millis(
        darkbot_solana::runtime_config::active()
            .limits
            .owner_evidence_sync_ms,
    );
    let mut evidence = match segmented {
        false => OwnerEvidenceWriter::start(
            journal_path,
            recovery_path,
            crate::analytics::evidence::OWNER_EVIDENCE_CAPACITY,
            evidence_sync,
        )?,
        true => {
            OwnerEvidenceWriter::start_rolling(
                journal_path,
                recovery_path,
                crate::analytics::evidence::OWNER_EVIDENCE_CAPACITY,
                evidence_sync,
                segment_bytes,
                resumed.is_some() || econ_resume,
            )?
            .0
        }
    };
    // The window the crash interrupted, in the continued evidence chain (its counters ended
    // with the crashed process).
    if let Some((_, check)) = &resumed {
        evidence.emit(crate::analytics::evidence::OwnerEvidence::Window(
            crate::analytics::evidence::OwnerWindow {
                window: check.window,
                started_unix_s: check.started_unix_s,
                ended_unix_s: check.ended_unix_s,
                ready_ns: 0,
                end: Some(crate::orchestration::until_stopped::CRASH_RECOVERED.into()),
                counters: Default::default(),
            },
        ))?;
    }

    let counts = [0u64; 6];
    let last_reported_update_total = 0u64; // [FIX-81]
    let bootstrap_probe_done = false; // [FIX-84]
                                      // [FIX-131][P1A-04]
    let program_upgrade_latched =
        deployment_pin != darkbot_project0::protocol::DeploymentPin::Reference;
    let program_pin = Some(crate::status_tap::OwnerProgramPin {
        verdict: deployment_pin.name(),
        observed_hash: deployment.hash.to_string(),
        deployment_slot: deployment.deployment_slot,
        upgrade_authority: deployment.upgrade_authority.map(|a| a.to_string()),
        source_tag: darkbot_project0::protocol::SOURCE_TAG,
        reference_source_tag: darkbot_project0::protocol::PROGRAM_DATA_REFERENCE_SOURCE_TAG,
        reference_hash: darkbot_project0::protocol::PROGRAM_DATA_REFERENCE_HASH,
    });
    #[cfg(feature = "crate_health")]
    let reverse_index = ReverseIndex::new(); // [FIX-133a]
    #[cfg(feature = "crate_health")]
    let threshold_index = ThresholdIndex::new(); // [FIX-133d]
    #[cfg(feature = "crate_health")]
    let threshold_fallback: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let threshold_bootstrap: VecDeque<Pubkey> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let threshold_bootstrap_batch = usize::try_from(operational.threshold_bootstrap_batch)?;
    // DEC-06: the start request of the threshold mode; effective only through OBSERVE.
    let threshold = super::threshold::ThresholdControl::new(
        crate::orchestration::threshold_mode::Requested::from_config(
            darkbot_solana::runtime_config::active().threshold_trigger,
        ),
        darkbot_solana::runtime_config::active().threshold_min_ticks,
    );
    let threshold_requested =
        threshold.requested != crate::orchestration::threshold_mode::Requested::Off;
    let threshold_ready = !threshold_requested;
    #[cfg(feature = "crate_health")]
    let health_sweep_checks: u64 = 0;
    #[cfg(feature = "crate_health")]
    let health_sweep_healthy: u64 = 0;
    #[cfg(feature = "crate_health")]
    let health_sweep_liquidatable: u64 = 0;
    #[cfg(feature = "crate_health")]
    let health_sweep_errors: u64 = 0;
    // [R127c] Dependency/Bank updates are first coalesced at Bank level. Only after the
    // current ingress burst is drained (or this hard event cap is reached) do we fan out
    // once to unique accounts and run exact Health in parallel.
    // V2-08: typed operational parameters (`Limits`, validated 1..=u32::MAX), no literals here.
    #[cfg(feature = "crate_health")]
    let sweep_batch_max_events = usize::try_from(
        darkbot_solana::runtime_config::active()
            .limits
            .sweep_batch_max_events,
    )?;
    #[cfg(feature = "crate_health")]
    let sweep_batch_max_age_ns = darkbot_solana::runtime_config::active()
        .limits
        .sweep_batch_max_age_ns;
    #[cfg(feature = "crate_health")]
    let sweep_dirty_banks: HashMap<Pubkey, (u64, u64)> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let sweep_batch_events: usize = 0;
    #[cfg(feature = "crate_health")]
    let sweep_batch_started_ns: Option<u64> = None;
    #[cfg(feature = "crate_health")]
    let sweep_batches: u64 = 0;
    #[cfg(feature = "crate_health")]
    let sweep_account_refs: u64 = 0;
    #[cfg(feature = "crate_health")]
    let sweep_unique_accounts: u64 = 0;
    #[cfg(feature = "crate_health")]
    let sweep_forwarded: u64 = 0;
    #[cfg(feature = "crate_health")]
    let sweep_max_unique_batch: usize = 0;
    #[cfg(feature = "crate_health")]
    let sweep_max_dirty_banks: usize = 0;

    // [R127d/P2] Candidate cache: do not re-run Projection/Sizing/Economics for the
    // same persistently-liquidatable account every 5 ms batch. Health is still recomputed
    // exactly on every affected batch. New unhealthy transitions and Bank-state invalidations
    // are always forwarded immediately. Unchanged candidates get a bounded periodic Long-Tail
    // recheck so route/fee changes that are not represented by Marginfi state are not starved.
    #[cfg(feature = "crate_health")]
    let candidate_recheck_slots = operational.candidate_recheck_slots;
    #[cfg(feature = "crate_health")]
    let candidate_error_recheck_slots = operational.candidate_error_recheck_slots;
    #[cfg(feature = "crate_health")]
    let candidate_repeat_soft_cap = usize::try_from(operational.candidate_repeat_soft_cap)?;
    #[cfg(feature = "crate_health")]
    crate::console::line(format!(
        "r127_candidate_cache_config: recheck_slots={} error_recheck_slots={} repeat_soft_cap={}",
        candidate_recheck_slots, candidate_error_recheck_slots, candidate_repeat_soft_cap
    ));
    #[cfg(feature = "crate_health")]
    let candidate_cache: HashMap<Pubkey, CandidateCacheEntry> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let candidate_force_rearm: HashSet<Pubkey> = HashSet::new();
    // [FIX-134] Global monotonic epoch plus per-Bank latest causal revision.
    // Dependency updates (oracle / integration source) advance every affected Bank.
    // Direct Bank writes advance that Bank. This survives multiple writes in one slot.
    #[cfg(feature = "crate_health")]
    let candidate_input_epoch: u64 = 0;
    #[cfg(feature = "crate_health")]
    let bank_input_revision: HashMap<Pubkey, u64> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let candidate_revision_forwards: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_pending_coalesced: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_revision_coalesced: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_physical_enqueues: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_cache_suppressed: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_cache_softcap_suppressed: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_cache_transition_forwards: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_cache_periodic_forwards: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_cache_forced_forwards: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_repeat_wait_slots_max: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_repeat_wait_slots_1: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_repeat_wait_slots_2_3: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_repeat_wait_slots_4_7: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_repeat_wait_slots_8_15: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_repeat_wait_slots_ge_16: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_fairness_rotations: u64 = 0;
    #[cfg(feature = "crate_health")]
    let candidate_periodic_due_but_deferred: u64 = 0;
    // [FIX-138] Measurement only: quantify whether remaining drain pressure comes
    // from semantically duplicate dependency writes or from downstream evaluation cost.
    #[cfg(feature = "crate_health")]
    let dependency_content_unchanged: u64 = 0;
    #[cfg(feature = "crate_health")]
    let dependency_content_changed: u64 = 0;
    #[cfg(feature = "crate_health")]
    let detect_health_calls: u64 = 0;
    #[cfg(feature = "crate_health")]
    let detect_health_ns_total: u64 = 0;
    #[cfg(feature = "crate_health")]
    let detect_health_ns_max: u64 = 0;
    #[cfg(feature = "crate_health")]
    let detect_pipeline_calls: u64 = 0;
    #[cfg(feature = "crate_health")]
    let detect_pipeline_ns_total: u64 = 0;
    #[cfg(feature = "crate_health")]
    let detect_pipeline_ns_max: u64 = 0;

    // [R126b] Preserve canonical ingress under detection bursts. When both ingress and
    // detection work are available, drain at most this many ingress events before forcing
    // one detection item. This prevents either side from starving the other.
    // V2-08: typed operational parameter (`Limits`).
    #[cfg(feature = "crate_health")]
    let ingress_burst_before_detect = usize::try_from(
        darkbot_solana::runtime_config::active()
            .limits
            .ingress_burst_before_detect,
    )?;
    #[cfg(feature = "crate_health")]
    let ingress_since_detect: usize = 0;
    // B6: at most this many candidates use the network (routes, fee quote, simulation) until
    // the ingress queue has been drained again; each stops calling the provider after the
    // deadline (default about twice the measured Mainnet pipeline maximum of 2.654 s).
    let rpc_candidates_per_drain = usize::try_from(operational.rpc_candidates_per_drain)?;
    let candidate_network_deadline_ms = operational.candidate_network_deadline_ms;
    let network = NetworkBudget::new(
        rpc_candidates_per_drain,
        candidate_network_deadline_ms.saturating_mul(1_000_000),
    );
    // [MT0] Measurement only: owner-section timing, queue-age tracking and an off-owner
    // reporter thread (the only place that writes MT0 evidence to disk).
    let mt0_path = match config::require("P0_MT0_PATH") {
        Ok(path) => crate::analytics::journal::binary_path(std::path::Path::new(&path)),
        Err(_) => mt0_default,
    };
    std::fs::create_dir_all(mt0_path.parent().unwrap_or(std::path::Path::new(".")))?;
    let mt0_journal = match segmented {
        false => crate::analytics::mt0_journal::Mt0Journal::create(&mt0_path)?,
        true => crate::analytics::mt0_journal::Mt0Journal::rolling(
            &mt0_path,
            segment_bytes,
            mt0_window.clone(),
            resumed.is_some() || econ_resume,
        )?,
    };
    let mt0_reporter = darkbot_common::mt0::spawn_reporter(
        Duration::from_secs(operational.mt0_interval_seconds),
        shutdown.clone(),
        mt0_journal,
    )?;
    let windows = match (rolling, resumed) {
        // The next window opens where the resume check closed the interrupted one (review L-5).
        (Some(r), Some((w, _))) => Some(super::windows::OwnerWindows::resumed(
            r, run_dir, mt0_window, w, resume_ns, resume_s,
        )),
        (Some(r), None) => Some(super::windows::OwnerWindows::new(r, run_dir, mt0_window)),
        (None, _) if econ_rolling => Some(super::windows::OwnerWindows::econ(
            run_dir,
            mt0_window,
            Duration::from_millis(operational.market_writer_stall_ms),
        )),
        (None, _) => None,
    };
    let mt0_owner = darkbot_common::mt0::OwnerTimer::new();
    // P8-RES-02: the reservation book of the liquidator from its durable FE-04 journal.
    let Fe04Owner {
        book: reservations,
        dir: fe04_journal,
        held: fe04_held,
        settle_due: fe04_settle_due,
        unbooked: fe04_unbooked,
    } = reservation_book(&|k| std::env::var(k).ok(), &liquidator)?
        // D07/F-06: the replay books into the current day; without a wall clock the start is
        // refused (never day 0, where today's limits would not see the booking).
        .start(
            &mut safety,
            wall_unix_s().map_err(|e| format!("CLOCK_UNAVAILABLE: {e}"))?,
        )?;
    let inventory_targets = crate::execution::inventory::parse_targets(
        &std::env::var("P0_INVENTORY_TARGETS").unwrap_or_default(),
    )?;
    let jit_funding_bank = crate::execution::inventory::parse_funding_bank(
        &std::env::var("P0_JIT_FUNDING_BANK").unwrap_or_default(),
    )?;
    #[cfg(feature = "crate_health")]
    if rebalance_mode != crate::execution::inventory::RebalanceMode::Off
        && inventory_targets.is_empty()
    {
        return Err(
            "REBALANCE_TARGETS_REQUIRED: OBSERVE plans against P0_INVENTORY_TARGETS".into(),
        );
    }
    let mut owner = ShadowOwner {
        metrics: OwnerMetrics {
            oracle_dependency_updates,
            oracle_candidate_hits,
            oracle_enqueued,
            oracle_coalesced,
            oracle_price_fallbacks,
            oracle_max_pending,
            oracle_bank_projection_submitted,
            oracle_bank_projection_ready,
            oracle_bank_projection_failed,
            oracle_bank_projection_stale,
            health_sweep_checks,
            health_sweep_healthy,
            health_sweep_liquidatable,
            health_sweep_errors,
            sweep_batches,
            sweep_account_refs,
            sweep_unique_accounts,
            sweep_forwarded,
            sweep_max_unique_batch,
            sweep_max_dirty_banks,
            candidate_revision_forwards,
            candidate_pending_coalesced,
            candidate_revision_coalesced,
            candidate_physical_enqueues,
            candidate_cache_suppressed,
            candidate_cache_softcap_suppressed,
            candidate_cache_transition_forwards,
            candidate_cache_periodic_forwards,
            candidate_cache_forced_forwards,
            candidate_repeat_wait_slots_max,
            candidate_repeat_wait_slots_1,
            candidate_repeat_wait_slots_2_3,
            candidate_repeat_wait_slots_4_7,
            candidate_repeat_wait_slots_8_15,
            candidate_repeat_wait_slots_ge_16,
            candidate_fairness_rotations,
            candidate_periodic_due_but_deferred,
            dependency_content_unchanged,
            dependency_content_changed,
            detect_health_calls,
            detect_health_ns_total,
            detect_health_ns_max,
            detect_pipeline_calls,
            detect_pipeline_ns_total,
            detect_pipeline_ns_max,
            sweep_batch_ns_total: 0,
            sweep_batch_ns_max: 0,
            sweep_batch_ns_max_accounts: 0,
            bootstrap_probes_queued: 0,
            owner_wakeup_ns: Default::default(),
            #[cfg(feature = "crate_health")]
            lane_wait_ns: Default::default(),
            jupiter_requests_per_routed_revision: Default::default(),
            readset_keys: Default::default(),
            readset_overflows: 0,
            readset_overflow_reported: false,
            result_actions: Default::default(),
        },
        eval_slippage_bps,
        zero_prefund_enabled: zero_prefund_enabled(&mode, &|k| std::env::var(k).ok())?,
        c0_census: match super::census::config(&census_journal, threshold_requested)? {
            Some(c) => Some(super::census::spawn(c, shutdown.clone(), &wake)?),
            None => None,
        },
        mode,
        run_id: super::super::startup::owner_run_id(&|k| std::env::var(k).ok()),
        status_interval: Duration::from_millis(
            darkbot_solana::runtime_config::active()
                .limits
                .owner_status_ms,
        ),
        status_published: None,
        status_seq: 0,
        ledger_view: None,
        blockhash_max_age_ns: operational.blockhash_max_age_ms.saturating_mul(1_000_000),
        fe04_settle_retry: Duration::from_millis(operational.fe04_settle_retry_ms),
        fe04_refresh: Duration::from_millis(operational.fe04_refresh_ms),
        projection_requeue_max: u8::try_from(operational.projection_requeue_max)?,
        end_result: None,
        required_dependency_keys,
        mt0_reporter,
        optional_dependency_keys,
        subscribed,
        required_set,
        optional_set,
        prereq_allowed,
        reservations,
        fe04_approval: approval_source(
            &liquidator,
            &program,
            &program_data_baseline_hash.to_string(),
            &operational,
            fe04_journal.as_deref(),
        ),
        fe04_journal,
        fe04_refreshed: std::time::Instant::now(),
        fe04_held,
        fe04_settle_due,
        fe04_unbooked,
        fe04_non_landing: Default::default(),
        hold_gate: crate::execution::hold_b::HoldGate {
            n: darkbot_solana::runtime_config::active().hold_b_min_attempts,
            ..crate::execution::hold_b::HoldGate::OBSERVE
        },
        inventory_targets,
        jit_funding_bank,
        #[cfg(feature = "crate_health")]
        rebalance: rebalance::Observer::new(rebalance_mode, &operational),
        prepared: std::cell::RefCell::new(crate::execution::prepared::PreparedCache::new(
            usize::try_from(operational.prepared_candidates)?,
            vec![program_data],
        )),
        route_cap: std::cell::RefCell::new(crate::execution::route_cap::RouteCapCache::new(
            usize::try_from(operational.prepared_candidates)?,
            crate::execution::builder::p0::RouteProvider::max_accounts(&routes),
        )),
        program_generation: 0,
        next_candidate_id: 1,
        candidate_revisions: HashMap::new(),
        detect_origin: None,
        blockhash_parking: Default::default(),
        blockhash_early: None,
        ownership: Default::default(),
        atas: LiquidatorAtas::new(liquidator),
        min_profit,
        integration_probe_enabled,
        integration_probe_limit_per_kind,
        remote_worker_capacity,
        projection_cache,
        projection_requeues: HashMap::new(),
        projection_parked: ParkedProjections::default(),
        projection_pending,
        projection_stats: ProjectionStats::default(),
        detect_queue,
        bootstrap_probe_queue: VecDeque::new(),
        oracle_lanes: CandidateLanes::with_weights([
            operational.lane_weight_l0,
            operational.lane_weight_l1,
            operational.lane_weight_l2,
        ]),
        oracle_bank_projection_pending,
        integration_probe_expected,
        integration_probe_queue,
        integration_probe_terminal,
        threshold_fallback,
        threshold_bootstrap,
        threshold_bootstrap_batch,
        threshold,
        sweep_batch_max_events,
        sweep_batch_max_age_ns,
        sweep_dirty_banks,
        time_schedule: Default::default(),
        sweep_batch_events,
        sweep_batch_started_ns,
        candidate_recheck_slots,
        candidate_error_recheck_slots,
        candidate_repeat_soft_cap,
        candidate_cache,
        candidate_force_rearm,
        candidate_input_epoch,
        bank_input_revision,
        ingress_burst_before_detect,
        ingress_since_detect,
        network,
        clock,
        econ,
        operational_start,
        operational_window,
        operational_failure,
        program,
        group,
        payer,
        liquidator,
        shutdown,
        stop_poll,
        halt,
        slot_watch,
        producer,
        consumer,
        wake,
        sources,
        program_data,
        program_data_baseline_hash,
        program_data_executable: program_data_account.executable,
        admission,
        sol_bank,
        ingress_session,
        retired_events: Default::default(),
        st,
        drv,
        dep_shared,
        opt_shared,
        blockhash,
        blockhash_thread: Some(blockhash_thread),
        safety,
        allowlist,
        #[cfg(feature = "crate_health")]
        instruction_table,
        #[cfg(feature = "crate_health")]
        dependency_gaps,
        global_allowed,
        submitter,
        bundle_config: crate::execution::bundle_path::BundleConfig::of(
            darkbot_solana::runtime_config::active(),
        ),
        failed_costs,
        routes,
        resources,
        freshness,
        remote,
        integration_probe_projection_ready,
        integration_probe_projection_failed,
        integration_probe_projection_stale,
        integration_probe_completion_reported,
        ledger,
        winner_tracker,
        evidence,
        windows,
        counts,
        last_reported_update_total,
        bootstrap_probe_done,
        program_upgrade_latched,
        program_pin,
        reverse_index,
        threshold_index,
        threshold_ready,
        mt0_owner,
    };
    // DEC-06: the threshold mode the run starts with (generation 0).
    owner.threshold_record("START");
    owner.start_sources()?;
    Ok((owner, fetch, ingress))
}

/// P8-RES-02/B3: what the durable FE-04 journal implies at startup — the reservation book, the
/// journal directory it is refreshed from and the safety replay.
pub(super) struct Fe04Start {
    pub(super) book: crate::execution::reservation::WalletReservations,
    pub(super) dir: Option<std::path::PathBuf>,
    pub(super) replay: Option<crate::execution::one_shot::SafetyReplay>,
    /// FE-04/C3: the journal of an existing attempt, held for the run (no attempt: none).
    pub(super) journal: Option<crate::execution::one_shot::Journal>,
    /// FE-04 archive: the bookings of every archived attempt (B3: their day stays booked).
    pub(super) archived: Vec<crate::execution::one_shot::SafetyReplay>,
}

/// What the owner holds of its FE-04 journal after the startup replay.
pub(super) struct Fe04Owner {
    pub(super) book: crate::execution::reservation::WalletReservations,
    pub(super) dir: Option<std::path::PathBuf>,
    pub(super) held: Option<crate::execution::one_shot::Journal>,
    pub(super) settle_due: Option<std::time::Instant>,
    pub(super) unbooked: Option<super::fe04::Fe04Unbooked>,
}

impl Fe04Start {
    /// B3/C3: a durable attempt halts the safety layer and keeps its capital reserved (released
    /// once, by the booking of its settlement); a settled attempt is booked by this replay (its
    /// day, open inventory); an UNKNOWN or unbooked attempt is settled at once.
    pub(super) fn start(self, safety: &mut Safety, now_unix_s: u64) -> Result<Fe04Owner, String> {
        if now_unix_s == 0 {
            return Err(
                "CLOCK_UNAVAILABLE: no wall clock for the FE-04 replay's booking day".into(),
            );
        }
        let unbooked = self
            .replay
            .as_ref()
            .filter(|r| r.settled.is_none())
            .map(|r| super::fe04::Fe04Unbooked {
                reserved_capital: r.held_capital_usd_micro,
            });
        // FE-04 archive: archived attempts are settled — their bookings, never a hold.
        for archived in self.archived {
            for booking in archived.settled.into_iter().chain(archived.exits) {
                safety
                    .restore_attempt(now_unix_s, None, Some(booking))
                    .map_err(|v| format!("SAFETY_REPLAY_INVALID: {}", v.code()))?;
            }
        }
        if let Some(replay) = self.replay {
            safety
                .restore_attempt(now_unix_s, replay.held_capital_usd_micro, replay.settled)
                .map_err(|v| format!("SAFETY_REPLAY_INVALID: {}", v.code()))?;
            // FE-04 exits: each settled exit in its own booking day.
            for exit in replay.exits {
                safety
                    .restore_attempt(now_unix_s, None, Some(exit))
                    .map_err(|v| format!("SAFETY_REPLAY_INVALID: {}", v.code()))?;
            }
        }
        let settle_due = self
            .journal
            .as_ref()
            .filter(|j| j.needs_settlement())
            .map(|_| std::time::Instant::now());
        Ok(Fe04Owner {
            book: self.book,
            dir: self.dir,
            held: self.journal,
            settle_due,
            unbooked,
        })
    }
}

/// P8-RES-02: the owner's reservation book at start — rebuilt from the FE-04 journal of the
/// liquidator in `P0_FE04_JOURNAL_DIR` (no directory: no FE-04 attempt, empty book); a journal
/// that cannot be read is a start error (fail closed).
pub(super) fn reservation_book(
    get: &dyn Fn(&str) -> Option<String>,
    liquidator: &Pubkey,
) -> Result<Fe04Start, String> {
    let Some(dir) = get("P0_FE04_JOURNAL_DIR").filter(|v| !v.trim().is_empty()) else {
        return Ok(Fe04Start {
            book: Default::default(),
            dir: None,
            replay: None,
            journal: None,
            archived: Vec::new(),
        });
    };
    let dir = std::path::PathBuf::from(dir);
    let wallet = liquidator.to_string();
    // FE-04 archive: every finished attempt is replayed; an unreadable archive halts the start.
    let archived = crate::execution::one_shot::archive::archived(&dir, &wallet)
        .map_err(|e| format!("RESERVATION_JOURNAL_UNREADABLE: {e}"))?
        .iter()
        .map(|a| a.safety_replay())
        .collect();
    if !dir.join(format!("{wallet}.p0j")).exists() && !dir.join(format!("{wallet}.json")).exists() {
        return Ok(Fe04Start {
            book: Default::default(),
            dir: Some(dir),
            replay: None,
            journal: None,
            archived,
        });
    }
    // One open for the run: the book, the safety replay and the settlement of its attempt.
    let journal = crate::execution::one_shot::Journal::open(&dir, &wallet)
        .map_err(|e| format!("RESERVATION_JOURNAL_UNREADABLE: {e}"))?;
    let book = crate::execution::reservation::WalletReservations::rebuild(journal.active_hold())
        .map_err(|e| format!("RESERVATION_JOURNAL_UNREADABLE: {e}"))?;
    Ok(Fe04Start {
        book,
        dir: Some(dir),
        replay: journal.safety_replay(),
        journal: Some(journal),
        archived,
    })
}

/// L4: both zero-prefund flags are parsed strictly (an invalid value is a start error, never a
/// silent `false`; no short circuit hides one); the Shadow probe applies only in Shadow mode.
/// FE-02: `P0_ZERO_PREFUND_QUALIFIED=true` is refused: a qualification is an FE-01/FE-02 record,
/// never an operator boolean, and no such record exists (FE-01 is never PASS offline).
pub(super) fn zero_prefund_enabled(
    mode: &str,
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<bool, config::ConfigError> {
    let flag = |key: &str| match get(key) {
        Some(v) if !v.trim().is_empty() => {
            v.parse::<bool>().map_err(|_| config::ConfigError::Invalid {
                key: key.into(),
                value: v,
            })
        }
        _ => Ok(false),
    };
    let qualified = flag("P0_ZERO_PREFUND_QUALIFIED")?;
    let probe = flag("P0_ZERO_PREFUND_SHADOW_PROBE")?;
    if qualified {
        return Err(config::ConfigError::Invalid {
            key: "P0_ZERO_PREFUND_QUALIFIED".into(),
            value: "true: ZERO_PREFUND_QUALIFICATION_EVIDENCE_REQUIRED".into(),
        });
    }
    Ok(mode == "shadow" && probe)
}

/// BOOT-12 (cluster identity) before any subscription or further read: the endpoint must serve
/// the pinned Mainnet genesis. Devnet, a fork or a wrong provider stop with the code only (no URL).
pub(super) fn check_cluster(observed: &solana_sdk::hash::Hash) -> Result<(), &'static str> {
    if observed.to_string() == darkbot_project0::protocol::GENESIS_HASH_MAINNET {
        Ok(())
    } else {
        Err("BOOT_CLUSTER_MISMATCH")
    }
}

/// Blockhash refresh loop of the Shadow owner (M3). Shutdown and closed admission are checked
/// before every refresh and at least every 250 ms of the wait, so no refresh starts after either;
/// a refresh already in flight is bounded by its RPC timeout (30 s). DEC-01: the owner's
/// coalesced request (`BlockhashCache::request_refresh`) starts the next refresh at once — after a
/// failed refresh (HTTP 429, timeout) never sooner than `retry_after_failure`; a successful
/// refresh satisfies every request made before it.
pub(super) fn run_blockhash_refresh(
    shutdown: &Shutdown,
    admission_closed: &dyn Fn() -> bool,
    mut refresh: impl FnMut() -> Result<(), String>,
    interval: Duration,
    requests: &BlockhashCache,
    retry_after_failure: Duration,
    stored: &dyn Fn(),
) {
    const STEP: Duration = Duration::from_millis(250);
    let stop = || shutdown.is_requested() || admission_closed();
    let mut degraded = false;
    // DEC-01: the running refresh serves an owner request; if it fails, it is retried after
    // the retry spacing instead of waiting for the interval.
    let mut requested = false;
    requests.register_worker(std::thread::current());
    while !stop() {
        let mt0_start = std::time::Instant::now(); // [MT0]
        let refreshed = refresh();
        let mt0_outcome = match &refreshed {
            Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
            Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(e),
        };
        darkbot_common::mt0::MT0.record_remote(
            darkbot_common::mt0::RemoteClass::Blockhash,
            mt0_start.elapsed(),
            mt0_outcome,
        );
        let retry = refreshed.is_err() && requested;
        match refreshed {
            Ok(_) => {
                requests.take_refresh_request();
                // DEC-01: a stored blockhash wakes the owner (parked candidates).
                stored();
                if degraded {
                    crate::console::line("blockhash refresh: recovered".into());
                    degraded = false;
                }
            }
            Err(e) => {
                if !degraded {
                    // [FIX-82] Never print provider URLs/API keys embedded in transport errors.
                    let safe = e.split(" for url ").next().unwrap_or("rpc transport error");
                    crate::console::line(format!("blockhash refresh: {}", safe));
                    degraded = true;
                }
            }
        }
        let waiting = std::time::Instant::now();
        let earliest_request = if degraded {
            retry_after_failure
        } else {
            Duration::ZERO
        };
        requested = false;
        while !stop() {
            let elapsed = waiting.elapsed();
            if elapsed >= interval {
                break;
            }
            if elapsed >= earliest_request && (retry || requests.take_refresh_request()) {
                requested = true;
                break;
            }
            let until = if elapsed < earliest_request {
                earliest_request - elapsed
            } else {
                interval - elapsed
            };
            std::thread::park_timeout(until.min(interval - elapsed).min(STEP));
        }
    }
}

#[cfg(test)]
mod cluster_tests {
    use super::*;
    #[test]
    fn fe04_reservation_book_is_optional_and_starts_empty_without_a_journal() {
        let liquidator = Pubkey::new_unique();
        let start = reservation_book(&|_| None, &liquidator).unwrap();
        assert_eq!(
            (start.book.held("SOL"), start.dir, start.replay),
            (0, None, None)
        );
        let tmp = std::env::temp_dir().join(format!("p0_fe04_book_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.to_string_lossy().to_string();
        let lookup = |k: &str| (k == "P0_FE04_JOURNAL_DIR").then(|| path.clone());
        let start = reservation_book(&lookup, &liquidator).unwrap();
        assert_eq!(
            (start.book.held("SOL"), start.dir, start.replay),
            (0, Some(tmp.clone()), None)
        );
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn zero_prefund_flags_parse_strictly_and_never_enable_implicitly() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert!(!zero_prefund_enabled("shadow", &env(&[])).unwrap());
        let q = env(&[("P0_ZERO_PREFUND_QUALIFIED", "true")]);
        for mode in ["shadow", "econ4h"] {
            assert!(zero_prefund_enabled(mode, &q)
                .unwrap_err()
                .to_string()
                .contains("ZERO_PREFUND_QUALIFICATION_EVIDENCE_REQUIRED"));
        }
        let off = env(&[("P0_ZERO_PREFUND_QUALIFIED", "false")]);
        assert!(!zero_prefund_enabled("econ4h", &off).unwrap());
        let probe = env(&[("P0_ZERO_PREFUND_SHADOW_PROBE", "true")]);
        assert!(zero_prefund_enabled("shadow", &probe).unwrap());
        assert!(!zero_prefund_enabled("econ4h", &probe).unwrap());
        for bad in [
            env(&[("P0_ZERO_PREFUND_QUALIFIED", "yes")]),
            env(&[
                ("P0_ZERO_PREFUND_QUALIFIED", "true"),
                ("P0_ZERO_PREFUND_SHADOW_PROBE", "1"),
            ]),
        ] {
            assert!(zero_prefund_enabled("shadow", &bad).is_err());
        }
    }
    use std::str::FromStr;
    #[test]
    fn cluster_check_accepts_only_the_mainnet_genesis_pin() {
        let pin =
            solana_sdk::hash::Hash::from_str(darkbot_project0::protocol::GENESIS_HASH_MAINNET)
                .unwrap();
        assert_eq!(check_cluster(&pin), Ok(()));
        for other in [
            solana_sdk::hash::Hash::new_unique(),
            solana_sdk::hash::Hash::default(),
        ] {
            assert_eq!(check_cluster(&other), Err("BOOT_CLUSTER_MISMATCH"));
        }
    }
}

#[cfg(test)]
mod blockhash_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU64, Ordering::SeqCst},
        mpsc, Arc,
    };
    use std::time::Instant;
    /// Runs the loop on its own thread; `done` fires when it returns.
    fn start(
        shutdown: &Shutdown,
        closed_from_check: Option<u64>,
        refresh_ms: u64,
    ) -> (Arc<AtomicU64>, mpsc::Receiver<()>) {
        start_with(
            shutdown,
            closed_from_check,
            refresh_ms,
            &BlockhashCache::new(),
            &[],
        )
    }
    /// `start` with the cache whose requests the loop serves and the outcomes of its refreshes
    /// (`Err` = a provider refusal such as HTTP 429; beyond the list every refresh succeeds).
    fn start_with(
        shutdown: &Shutdown,
        closed_from_check: Option<u64>,
        refresh_ms: u64,
        requests: &BlockhashCache,
        outcomes: &[Result<(), &'static str>],
    ) -> (Arc<AtomicU64>, mpsc::Receiver<()>) {
        let (calls, _, rx) =
            start_stored(shutdown, closed_from_check, refresh_ms, requests, outcomes);
        (calls, rx)
    }
    /// `start_with` that also counts the owner wake-ups after a stored blockhash.
    fn start_stored(
        shutdown: &Shutdown,
        closed_from_check: Option<u64>,
        refresh_ms: u64,
        requests: &BlockhashCache,
        outcomes: &[Result<(), &'static str>],
    ) -> (Arc<AtomicU64>, Arc<AtomicU64>, mpsc::Receiver<()>) {
        let calls = Arc::new(AtomicU64::new(0));
        let stored = Arc::new(AtomicU64::new(0));
        let (done, rx) = mpsc::channel();
        let (s, c, r) = (shutdown.clone(), calls.clone(), requests.clone());
        let outcomes = outcomes.to_vec();
        let woken = stored.clone();
        std::thread::spawn(move || {
            let checks = AtomicU64::new(0);
            let closed =
                move || closed_from_check.is_some_and(|n| checks.fetch_add(1, SeqCst) >= n);
            run_blockhash_refresh(
                &s,
                &closed,
                || {
                    let n = c.fetch_add(1, SeqCst) as usize;
                    std::thread::sleep(Duration::from_millis(refresh_ms));
                    outcomes
                        .get(n)
                        .copied()
                        .unwrap_or(Ok(()))
                        .map_err(String::from)
                },
                Duration::from_secs(10),
                &r,
                Duration::from_millis(400),
                &|| {
                    woken.fetch_add(1, SeqCst);
                },
            );
            let _ = done.send(());
        });
        (calls, stored, rx)
    }
    fn calls_reach(calls: &AtomicU64, n: u64, within: Duration) -> bool {
        let t0 = Instant::now();
        while calls.load(SeqCst) < n {
            if t0.elapsed() > within {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }
    /// DEC-01: the owner's request starts a refresh at once instead of after the 10 s interval;
    /// requests made while one is pending coalesce into one refresh.
    #[test]
    fn dec01_a_refresh_request_wakes_the_worker_at_once_and_coalesces() {
        let shutdown = Shutdown::new();
        let cache = BlockhashCache::new();
        let (calls, done) = start_with(&shutdown, None, 0, &cache, &[]);
        first_refresh(&calls);
        std::thread::sleep(Duration::from_millis(50));
        assert!(cache.request_refresh());
        assert!(!cache.request_refresh(), "coalesced");
        assert!(
            calls_reach(&calls, 2, Duration::from_secs(2)),
            "request not served at once"
        );
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(calls.load(SeqCst), 2, "one refresh per coalesced request");
        shutdown.request();
        done.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    /// DEC-01 (HTTP 429): after a failed refresh a request is served no sooner than the retry
    /// spacing (400 ms here), then the refresh succeeds.
    #[test]
    fn dec01_after_a_refusal_a_request_waits_for_the_retry_spacing() {
        let shutdown = Shutdown::new();
        let cache = BlockhashCache::new();
        let (calls, done) = start_with(
            &shutdown,
            None,
            0,
            &cache,
            &[Err("HTTP 429 Too Many Requests")],
        );
        first_refresh(&calls);
        let failed_at = Instant::now();
        cache.request_refresh();
        assert!(calls_reach(&calls, 2, Duration::from_secs(2)));
        assert!(
            failed_at.elapsed() >= Duration::from_millis(350),
            "{:?}",
            failed_at.elapsed()
        );
        shutdown.request();
        done.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    /// DEC-01 (HTTP 429 on a requested refresh): the request a failed refresh consumed is
    /// retried after the retry spacing (400 ms), not after the 10 s interval, without a new
    /// request; every stored blockhash wakes the owner, a failed refresh does not.
    #[test]
    fn dec01_a_requested_refresh_that_fails_is_retried_after_the_spacing() {
        let shutdown = Shutdown::new();
        let cache = BlockhashCache::new();
        let (calls, stored, done) = start_stored(
            &shutdown,
            None,
            0,
            &cache,
            &[Ok(()), Err("HTTP 429 Too Many Requests")],
        );
        first_refresh(&calls);
        assert!(
            calls_reach(&stored, 1, Duration::from_secs(1)),
            "owner woken"
        );
        std::thread::sleep(Duration::from_millis(50));
        assert!(cache.request_refresh());
        assert!(
            calls_reach(&calls, 2, Duration::from_secs(1)),
            "request served"
        );
        let failed_at = Instant::now();
        assert!(
            calls_reach(&calls, 3, Duration::from_secs(2)),
            "the failed requested refresh is retried before the interval"
        );
        assert!(failed_at.elapsed() >= Duration::from_millis(350));
        assert!(calls_reach(&stored, 2, Duration::from_secs(1)));
        assert_eq!(stored.load(SeqCst), 2, "a failed refresh wakes nobody");
        shutdown.request();
        done.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    fn first_refresh(calls: &AtomicU64) {
        let t0 = Instant::now();
        while calls.load(SeqCst) == 0 {
            assert!(t0.elapsed() < Duration::from_secs(5), "no first refresh");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    /// M3: without an econ handle only the general shutdown ends the loop; it is honoured
    /// during the 10 s wait, not after it.
    #[test]
    fn blockhash_refresh_stops_on_shutdown_without_econ() {
        let shutdown = Shutdown::new();
        let (calls, done) = start(&shutdown, None, 0);
        first_refresh(&calls);
        shutdown.request();
        done.recv_timeout(Duration::from_secs(1))
            .expect("refresh loop ignored shutdown");
        assert_eq!(calls.load(SeqCst), 1);
    }
    #[test]
    fn blockhash_refresh_stops_on_admission_closed() {
        let shutdown = Shutdown::new();
        // The first check (before the first refresh) is open, every later one closed.
        let (calls, done) = start(&shutdown, Some(1), 0);
        done.recv_timeout(Duration::from_secs(1))
            .expect("refresh loop ignored closed admission");
        assert_eq!(calls.load(SeqCst), 1);
        assert!(!shutdown.is_requested());
    }
    /// A refresh in flight finishes (bounded by its RPC timeout), then no further refresh.
    #[test]
    fn blockhash_refresh_in_flight_finishes_then_stops() {
        let shutdown = Shutdown::new();
        let (calls, done) = start(&shutdown, None, 300);
        first_refresh(&calls);
        shutdown.request();
        done.recv_timeout(Duration::from_secs(2))
            .expect("refresh loop ignored shutdown after the refresh in flight");
        assert_eq!(calls.load(SeqCst), 1);
    }
}

/// DEC-10: the FE-04 approval configured with `P0_FE04_APPROVAL` (native `key = value` file),
/// read once now and at every FE-04 refresh; `None` without one. It binds this wallet, program,
/// the observed ProgramData, the clean HEAD, these binaries and the operational Safety ceilings.
fn approval_source(
    liquidator: &Pubkey,
    program: &Pubkey,
    program_data_hash: &str,
    operational: &darkbot_solana::runtime_config::Limits,
    journal: Option<&std::path::Path>,
) -> Option<ApprovalSource> {
    use crate::execution::one_shot::approval_file::{ApprovalBinding, Bind, Ceilings, Usage};
    use crate::orchestration::boot_audit;
    let path = config::require("P0_FE04_APPROVAL").ok()?;
    let project = std::env::var("P0_PROJECT_DIR").ok();
    let mut source = ApprovalSource {
        path: path.into(),
        binding: ApprovalBinding {
            wallet: Bind::Known(liquidator.to_string()),
            program: Bind::Known(program.to_string()),
            program_data_hash: Bind::Known(program_data_hash.to_string()),
            code_sha: Bind::of(
                project.and_then(|p| boot_audit::clean_source_head(std::path::Path::new(&p))),
            ),
            binary_sha256: Bind::of(boot_audit::runtime_binary_identity().ok()),
            ceilings: Some(Ceilings {
                max_state_age_ms: operational.max_opportunity_age_ns / 1_000_000,
                max_slippage_bps: operational.max_slippage_bps,
                max_tip_lamports: operational.max_tip,
                max_capital_usd_micro: operational.max_total_capital,
                max_loss_usd_micro: operational.max_daily_loss,
            }),
            max_lifetime_seconds: operational.fe04_approval_max_lifetime_seconds,
        },
        read: Err("APPROVAL_NOT_READ".into()),
        usage: Usage::Unchecked("APPROVAL_NOT_READ".into()),
    };
    source.refresh(journal, &liquidator.to_string());
    Some(source)
}
