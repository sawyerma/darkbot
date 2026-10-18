//! Production bootstrap and preparation of the single owner.
use super::*;

type Prepared<F> = Result<(ShadowOwner, F, IngressSessions), Box<dyn std::error::Error>>;

pub(super) fn prepare(
    clock: Clock,
    econ: Option<crate::analytics::econ::Handle>,
    admission: Option<crate::orchestration::boot_audit::NetworkAudit>,
) -> Prepared<impl FnMut() -> Result<SnapshotData, String>> {
    let operational_start = clock.now_ns();
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
        eprintln!("admission: approved network audit verified; identities bind at the first Ready");
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
    let buffer_cap: usize = config::parsed_or("P0_INGRESS_BUFFER", 65_536)?;

    let group = Pubkey::from_str(&config::require("P0_GROUP_ID")?)?;
    let payer = tx::load_keypair_json(&config::require("P0_KEYPAIR_PATH")?)?;
    let liquidator = tx::payer_of(&payer);
    let shutdown = Shutdown::new();
    let halt = IngressHalt::new();
    let slot_watch = SlotWatch::new(); // [FIX-40]
    let (producer, consumer) = bounded::<IngressEvent>("ingress", buffer_cap);

    // [FIX-78] Bootstrap only the configured group's Banks first. Full-program GPA was ~387 MB
    // in C0 and is unnecessary for dependency discovery.
    let dep_rpc = darkbot_solana::provider::rpc_client(
        config::require("P0_RPC_URL")?,
        Duration::from_secs(30),
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
    eprintln!("cluster: genesis={genesis}");
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
    eprintln!(
        "program_upgrade_watch: program={} program_data={} baseline_hash={} deployment_slot={} upgrade_authority={} pin={:?}",
        program,
        program_data,
        program_data_baseline_hash,
        deployment.deployment_slot,
        deployment.upgrade_authority.map_or_else(|| "NONE".to_string(), |a| a.to_string()),
        deployment_pin
    );

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
    eprintln!(
        "payer_preflight: pubkey={} lamports={} owner={} executable={}",
        liquidator, payer_account.lamports, payer_account.owner, payer_account.executable
    );
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
    let mut required_dependency_keys = {
        let mut v =
            darkbot_project0::health::dependency_keys(&StateView(&pre_state), &program, &bank_keys);
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
    eprintln!(
        "sol_cost_bank: {} asset_tag={} oracle_setup={:?} operational_state={:?}",
        sol_bank,
        sol_cost_bank.config.asset_tag,
        sol_cost_bank.config.oracle_setup,
        sol_cost_bank.config.operational_state
    );

    let mut subscription_keys = required_dependency_keys.clone();
    subscription_keys.extend(optional_dependency_keys.iter().copied());
    subscription_keys.sort_unstable_by_key(|k| k.to_bytes());
    subscription_keys.dedup();

    eprintln!(
        "dependencies: banks={} required={} optional_atas={}",
        bank_keys.len(),
        required_dependency_keys.len(),
        optional_dependency_keys.len()
    );
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
    let ingress_session = ingress.spawn(sub.clone(), producer.clone(), halt.clone(), 1)?;

    // State + Recovery (Snapshot via RPC, außerhalb des Decision-Loops aufgerufen).
    // Large, filtered Marginfi getProgramAccounts payloads regularly approach the
    // RpcClient::new() 30-second default. Keep a bounded, independently configurable
    // timeout for snapshot/dependency fetches; never weaken recovery/reconcile gates.
    let snapshot_rpc_timeout_secs: u64 = config::parsed_or("P0_SNAPSHOT_RPC_TIMEOUT_SECS", 90)?;
    if snapshot_rpc_timeout_secs == 0 {
        return Err("P0_SNAPSHOT_RPC_TIMEOUT_SECS must be positive".into());
    }
    eprintln!("snapshot: rpc_timeout_s={snapshot_rpc_timeout_secs}");
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
    let root_catchup_secs: u64 = config::parsed_or("P0_RECOVERY_ROOT_CATCHUP_SECS", 30)?;
    if !(30..=300).contains(&root_catchup_secs) {
        return Err("P0_RECOVERY_ROOT_CATCHUP_SECS must be 30..=300".into());
    }
    // Adaptive root-only stall extension. All limits are bounded and the hard
    // deadline is independent of progress, with no impact on trading hot paths.
    let root_stall_secs: u64 = config::parsed_or("P0_RECOVERY_ROOT_STALL_SECS", 60)?;
    let root_hard_secs: u64 = config::parsed_or("P0_RECOVERY_ROOT_HARD_MAX_SECS", 300)?;
    if !(5..=180).contains(&root_stall_secs)
        || !(30..=900).contains(&root_hard_secs)
        || root_hard_secs < root_catchup_secs
    {
        return Err("P0_RECOVERY_ROOT_STALL_SECS must be 5..=180 and P0_RECOVERY_ROOT_HARD_MAX_SECS must be 30..=900 and >= P0_RECOVERY_ROOT_CATCHUP_SECS".into());
    }
    eprintln!(
        "recovery: root_soft_s={} root_stall_s={} root_hard_s={}",
        root_catchup_secs, root_stall_secs, root_hard_secs
    );
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
        eprintln!("snapshot: start");
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
            eprintln!(
                "snapshot: failed stage=liability_gpa elapsed_ms={}",
                t0.elapsed().as_millis()
            );
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
        eprintln!(
            "snapshot: done slot={} liability_accounts={} banks={} required={} optional={} missing_required={} total={} elapsed_ms={}",
            slot,
            liability_count,
            bank_count,
            required_count,
            optional_count,
            missing_required_count,
            accounts.len(),
            t0.elapsed().as_millis()
        );
        Ok(SnapshotData {
            slot,
            accounts,
            missing_required: d.missing,
        })
    };
    // [FIX-35] Bootstrap läuft über die StreamReady-Barriere: das erste `Reconnected` (alle ACKs)
    // löst im Event-Loop die Recovery aus. Kein sleep, kein vorzeitiger Snapshot.
    eprintln!("bootstrap: waiting for StreamReady (all subscription ACKs)");
    // [FIX-1] Pipeline-Bausteine: Keypair (nur signieren, Shadow sendet nicht), Blockhash-Cache,
    // Safety aus Env (Limits sind Konfiguration), Allowlist aus Source, Submitter nur für Simulation.
    let blockhash = BlockhashCache::new();
    let bh_rpc = darkbot_solana::provider::rpc_client(
        config::require("P0_RPC_URL")?,
        Duration::from_secs(30),
        Default::default(),
        darkbot_solana::provider::Workload::LiveRead,
    );
    let bh_cache = blockhash.clone();
    let bh_clock = clock;
    let bh_econ = econ.clone();
    let bh_shutdown = shutdown.clone();
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
                    Duration::from_secs(10),
                )
            })?;
    let limits = Limits {
        max_total_capital: config::parsed_or("P0_MAX_TOTAL_CAPITAL", 0)?,
        max_outstanding_inventory: config::parsed_or("P0_MAX_INVENTORY", 0)?,
        max_concurrent_positions: config::parsed_or("P0_MAX_CONCURRENT", 1)?,
        max_exposure_per_attempt: config::parsed_or("P0_MAX_EXPOSURE", 0)?,
        max_daily_loss: config::parsed_or("P0_MAX_DAILY_LOSS", 0)?,
        max_failed_attempts_per_day: config::parsed_or("P0_MAX_FAILED", 0)?,
        max_tip: config::parsed_or("P0_MAX_TIP", 0)?,
        max_slippage_bps: config::parsed_or("P0_MAX_SLIPPAGE_BPS", 0)?,
        max_outstanding_attempts: config::parsed_or("P0_MAX_OUTSTANDING", 1)?,
    };
    // L4: one strict parse; the evaluation uses exactly the configured safety value.
    let eval_slippage_bps = limits.max_slippage_bps;
    let mut allow = Allow::default();
    allow.execution_classes.insert("longtail");
    let safety = Safety::new(limits, allow, false);
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
    allowlist.refresh_programs.insert(program); // init_liquidation_record vor Start
    let global_allowed = protocol::receivership_allowed_programs();
    // [FIX-30] Prerequisite-TXs (JIT-Buy): Swap-Programme + System/Token/Token-2022/ATA/ComputeBudget.
    let prereq_allowed: Vec<Pubkey> = [
        protocol::JUP_SWAP_PROGRAM_ID,
        protocol::TITAN_SWAP_PROGRAM_ID,
        protocol::ASSOCIATED_TOKEN_PROGRAM_ID,
        protocol::COMPUTE_BUDGET_PROGRAM_ID,
        darkbot_project0::receivership::SYSTEM_PROGRAM,
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
    ]
    .iter()
    .map(|s| Pubkey::from_str(s))
    .collect::<Result<_, _>>()?;
    let submitter = crate::execution::observe_only::ObserveOnly::new(RpcSubmitter::new(
        config::require("P0_RPC_URL")?,
        true,
    ));
    let failed_costs = FailedCostMatrix::default(); // alle None → Unknown → nicht handelbar, bis gemessen
    #[cfg(feature = "crate_health")]
    let routes = JupiterV2RouteProvider::new(
        liquidator,
        std::env::var("P0_JUPITER_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        config::parsed_or("P0_MAX_SLIPPAGE_BPS", 0)?,
        config::parsed_or("P0_JUPITER_MAX_ACCOUNTS", 64u8)?,
        true,
        false,
    )
    .map_err(|e| format!("route provider: {e}"))?;
    let resources = Resources {
        compute_unit_limit: config::parsed_or("P0_CU_LIMIT", 1_400_000)?,
        priority_micro_lamports_per_cu: config::parsed_or("P0_PRIORITY_MICRO", 0)?,
        loaded_accounts_data_size: Some(config::parsed_or("P0_LOADED_DATA", 64 * 1024 * 1024)?),
    };
    let freshness = Freshness {
        max_opportunity_age_ns: config::parsed_or("P0_MAX_AGE_NS", 2_000_000_000)?,
        execution_deadline_ns: config::parsed_or("P0_DEADLINE_NS", 2_000_000_000)?,
    };
    let min_profit: i128 = config::parsed_or::<i128>("P0_MIN_PROFIT", 0)?;

    #[cfg(feature = "crate_health")]
    let integration_probe_enabled: bool = config::parsed_or("P0_INTEGRATION_PROBE", false)?;
    #[cfg(feature = "crate_health")]
    let integration_probe_limit_per_kind: usize =
        config::parsed_or("P0_INTEGRATION_PROBE_LIMIT_PER_KIND", 8)?;
    #[cfg(feature = "crate_health")]
    if integration_probe_enabled && integration_probe_limit_per_kind == 0 {
        return Err("P0_INTEGRATION_PROBE_LIMIT_PER_KIND must be > 0 when probe is enabled".into());
    }

    // [FIX-108][R4] All remote winner/reconciliation RPC is owned by bounded workers.
    let remote_worker_capacity: usize = config::parsed_or("P0_REMOTE_WORKER_BUFFER", 256)?;
    if remote_worker_capacity == 0 {
        return Err("P0_REMOTE_WORKER_BUFFER must be > 0".into());
    }
    let remote = RemoteWorkers::start(rpc_url.clone(), program, &shutdown, remote_worker_capacity)?;
    let pending: Vec<ReconcileWork> = Vec::new();

    #[cfg(feature = "crate_health")]
    let projection_cache: HashMap<Pubkey, CachedProjection> = HashMap::new();
    #[cfg(feature = "crate_health")]
    let projection_pending: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let detect_queue: VecDeque<DetectWork> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let oracle_detect_order: VecDeque<Pubkey> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let oracle_detect_pending: HashMap<Pubkey, DetectWork> = HashMap::new();
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
        capacity: config::parsed_or(
            "P0_WINNER_WATCH_CAPACITY",
            WinnerTrackerConfig::DEFAULT.capacity,
        )?,
        ttl_s: config::parsed_or("P0_WINNER_WATCH_TTL_S", WinnerTrackerConfig::DEFAULT.ttl_s)?,
        initial_delay_s: config::parsed_or(
            "P0_WINNER_WATCH_INITIAL_DELAY_S",
            WinnerTrackerConfig::DEFAULT.initial_delay_s,
        )?,
        recheck_interval_s: config::parsed_or(
            "P0_WINNER_WATCH_RECHECK_S",
            WinnerTrackerConfig::DEFAULT.recheck_interval_s,
        )?,
        full_queue_backoff_s: config::parsed_or(
            "P0_WINNER_WATCH_FULL_BACKOFF_S",
            WinnerTrackerConfig::DEFAULT.full_queue_backoff_s,
        )?,
    }
    .validate()?;
    eprintln!(
        "winner_watch_config: capacity={} ttl_s={} initial_delay_s={} recheck_s={} full_backoff_s={} provisional=true",
        winner_watch_cfg.capacity,
        winner_watch_cfg.ttl_s,
        winner_watch_cfg.initial_delay_s,
        winner_watch_cfg.recheck_interval_s,
        winner_watch_cfg.full_queue_backoff_s
    );
    let winner_tracker = WinnerTracker::new(winner_watch_cfg);
    let record_path = config::parsed_or::<String>(
        "P0_RECORD_PATH",
        "project_0/data/shadow/opportunities.p0j".into(),
    )?;
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
    let evidence = OwnerEvidenceWriter::start(
        journal_path,
        recovery_path,
        crate::analytics::evidence::OWNER_EVIDENCE_CAPACITY,
        Duration::from_millis(
            darkbot_solana::runtime_config::active()
                .limits
                .owner_evidence_sync_ms,
        ),
    )?;

    let counts = [0u64; 6];
    let last_reported_update_total = 0u64; // [FIX-81]
    let bootstrap_probe_done = false; // [FIX-84]
                                      // [FIX-131][P1A-04]
    let program_upgrade_latched =
        deployment_pin != darkbot_project0::protocol::DeploymentPin::Reference;
    #[cfg(feature = "crate_health")]
    let reverse_index = ReverseIndex::new(); // [FIX-133a]
    #[cfg(feature = "crate_health")]
    let threshold_index = ThresholdIndex::new(); // [FIX-133d]
    #[cfg(feature = "crate_health")]
    let threshold_fallback: HashSet<Pubkey> = HashSet::new();
    #[cfg(feature = "crate_health")]
    let threshold_bootstrap: VecDeque<Pubkey> = VecDeque::new();
    #[cfg(feature = "crate_health")]
    let threshold_bootstrap_batch: usize = config::parsed_or("P0_THRESHOLD_BOOTSTRAP_BATCH", 16)?;
    #[cfg(feature = "crate_health")]
    if threshold_bootstrap_batch == 0 {
        return Err("P0_THRESHOLD_BOOTSTRAP_BATCH must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    let use_threshold_trigger: bool = config::parsed_or("P0_USE_THRESHOLD_TRIGGER", false)?;
    #[cfg(feature = "crate_health")]
    let threshold_ready = !use_threshold_trigger;
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
    #[cfg(feature = "crate_health")]
    let sweep_batch_max_events: usize = config::parsed_or("P0_SWEEP_BATCH_MAX_EVENTS", 64)?;
    #[cfg(feature = "crate_health")]
    if sweep_batch_max_events == 0 {
        return Err("P0_SWEEP_BATCH_MAX_EVENTS must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    let sweep_batch_max_age_ns: u64 = config::parsed_or("P0_SWEEP_BATCH_MAX_AGE_NS", 5_000_000)?;
    #[cfg(feature = "crate_health")]
    if sweep_batch_max_age_ns == 0 {
        return Err("P0_SWEEP_BATCH_MAX_AGE_NS must be > 0".into());
    }
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
    let candidate_recheck_slots: u64 = config::parsed_or("P0_CANDIDATE_RECHECK_SLOTS", 1)?;
    #[cfg(feature = "crate_health")]
    if candidate_recheck_slots == 0 {
        return Err("P0_CANDIDATE_RECHECK_SLOTS must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    let candidate_error_recheck_slots: u64 =
        config::parsed_or("P0_CANDIDATE_ERROR_RECHECK_SLOTS", 1)?;
    #[cfg(feature = "crate_health")]
    if candidate_error_recheck_slots == 0 {
        return Err("P0_CANDIDATE_ERROR_RECHECK_SLOTS must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    let candidate_repeat_soft_cap: usize = config::parsed_or("P0_CANDIDATE_REPEAT_SOFT_CAP", 1024)?;
    #[cfg(feature = "crate_health")]
    if candidate_repeat_soft_cap == 0 {
        return Err("P0_CANDIDATE_REPEAT_SOFT_CAP must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    eprintln!(
        "r127_candidate_cache_config: recheck_slots={} error_recheck_slots={} repeat_soft_cap={}",
        candidate_recheck_slots, candidate_error_recheck_slots, candidate_repeat_soft_cap
    );
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
    #[cfg(feature = "crate_health")]
    let ingress_burst_before_detect: usize =
        config::parsed_or("P0_INGRESS_BURST_BEFORE_DETECT", 64)?;
    #[cfg(feature = "crate_health")]
    if ingress_burst_before_detect == 0 {
        return Err("P0_INGRESS_BURST_BEFORE_DETECT must be > 0".into());
    }
    #[cfg(feature = "crate_health")]
    let ingress_since_detect: usize = 0;
    // B6: at most this many candidates use the network (routes, fee quote, simulation) until
    // the ingress queue has been drained again; each stops calling the provider after the
    // deadline (default about twice the measured Mainnet pipeline maximum of 2.654 s).
    let rpc_candidates_per_drain: usize = config::parsed_or("P0_RPC_CANDIDATES_PER_DRAIN", 4)?;
    let candidate_network_deadline_ms: u64 =
        config::parsed_or("P0_CANDIDATE_NETWORK_DEADLINE_MS", 5_000)?;
    if rpc_candidates_per_drain == 0 || candidate_network_deadline_ms == 0 {
        return Err(
            "P0_RPC_CANDIDATES_PER_DRAIN and P0_CANDIDATE_NETWORK_DEADLINE_MS must be > 0".into(),
        );
    }
    let network = NetworkBudget::new(
        rpc_candidates_per_drain,
        candidate_network_deadline_ms.saturating_mul(1_000_000),
    );
    // [MT0] Measurement only: owner-section timing, queue-age tracking and an off-owner
    // reporter thread (the only place that writes MT0 evidence to disk).
    let mt0_path = match config::parsed_or::<String>("P0_MT0_PATH", String::new())? {
        path if path.is_empty() => mt0_default,
        path => crate::analytics::journal::binary_path(std::path::Path::new(&path)),
    };
    std::fs::create_dir_all(mt0_path.parent().unwrap_or(std::path::Path::new(".")))?;
    let mt0_interval_secs: u64 = config::parsed_or("P0_MT0_INTERVAL_SECS", 10)?;
    let mt0_reporter = darkbot_common::mt0::spawn_reporter(
        Duration::from_secs(mt0_interval_secs.max(1)),
        shutdown.clone(),
        crate::analytics::mt0_journal::Mt0Journal::create(&mt0_path)?,
    )?;
    let mt0_owner = darkbot_common::mt0::OwnerTimer::new();
    // [MT0] First-trigger receive time (`t_rx` of the enqueue that created the token) of every
    // token in `oracle_detect_order`: same length, same order. Pushed in lockstep at the physical
    // enqueue; coalescing never touches it, so the first unserved trigger is kept.
    #[cfg(feature = "crate_health")]
    let mt0_order_rx: VecDeque<u64> = VecDeque::new();
    let owner = ShadowOwner {
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
        },
        eval_slippage_bps,
        zero_prefund_enabled: zero_prefund_enabled(&mode, &|k| std::env::var(k).ok())?,
        c0_census: match super::census::config(&census_journal, use_threshold_trigger)? {
            Some(c) => Some(super::census::spawn(c, shutdown.clone())?),
            None => None,
        },
        mode,
        required_dependency_keys,
        mt0_reporter,
        optional_dependency_keys,
        subscribed,
        required_set,
        optional_set,
        prereq_allowed,
        min_profit,
        integration_probe_enabled,
        integration_probe_limit_per_kind,
        remote_worker_capacity,
        pending,
        projection_cache,
        projection_pending,
        detect_queue,
        oracle_detect_order,
        oracle_detect_pending,
        oracle_bank_projection_pending,
        integration_probe_expected,
        integration_probe_queue,
        integration_probe_terminal,
        threshold_fallback,
        threshold_bootstrap,
        threshold_bootstrap_batch,
        use_threshold_trigger,
        sweep_batch_max_events,
        sweep_batch_max_age_ns,
        sweep_dirty_banks,
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
        mt0_order_rx,
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
        halt,
        slot_watch,
        producer,
        consumer,
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
        global_allowed,
        submitter,
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
        counts,
        last_reported_update_total,
        bootstrap_probe_done,
        program_upgrade_latched,
        reverse_index,
        threshold_index,
        threshold_ready,
        mt0_owner,
    };
    Ok((owner, fetch, ingress))
}

/// L4: both zero-prefund flags are parsed strictly (an invalid value is a start error, never a
/// silent `false`; no short circuit hides one); the Shadow probe applies only in Shadow mode.
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
    Ok(qualified || (mode == "shadow" && probe))
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
/// a refresh already in flight is bounded by its RPC timeout (30 s).
pub(super) fn run_blockhash_refresh(
    shutdown: &Shutdown,
    admission_closed: &dyn Fn() -> bool,
    mut refresh: impl FnMut() -> Result<(), String>,
    interval: Duration,
) {
    const STEP: Duration = Duration::from_millis(250);
    let stop = || shutdown.is_requested() || admission_closed();
    let mut degraded = false;
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
        match refreshed {
            Ok(_) => {
                if degraded {
                    eprintln!("blockhash refresh: recovered");
                    degraded = false;
                }
            }
            Err(e) => {
                if !degraded {
                    // [FIX-82] Never print provider URLs/API keys embedded in transport errors.
                    let safe = e.split(" for url ").next().unwrap_or("rpc transport error");
                    eprintln!("blockhash refresh: {}", safe);
                    degraded = true;
                }
            }
        }
        let waiting = std::time::Instant::now();
        while !stop() {
            let left = interval.saturating_sub(waiting.elapsed());
            if left.is_zero() {
                break;
            }
            std::thread::sleep(left.min(STEP));
        }
    }
}

#[cfg(test)]
mod cluster_tests {
    use super::*;
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
        assert!(zero_prefund_enabled("econ4h", &q).unwrap());
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
        let calls = Arc::new(AtomicU64::new(0));
        let (done, rx) = mpsc::channel();
        let (s, c) = (shutdown.clone(), calls.clone());
        std::thread::spawn(move || {
            let checks = AtomicU64::new(0);
            let closed =
                move || closed_from_check.is_some_and(|n| checks.fetch_add(1, SeqCst) >= n);
            run_blockhash_refresh(
                &s,
                &closed,
                || {
                    c.fetch_add(1, SeqCst);
                    std::thread::sleep(Duration::from_millis(refresh_ms));
                    Ok(())
                },
                Duration::from_secs(10),
            );
            let _ = done.send(());
        });
        (calls, rx)
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
