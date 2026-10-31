//! C1 test 7: the real Shadow owner end to end, offline. A `ShadowOwner` built from inert local
//! parts (temp journals, unused RPC endpoint, no blockhash thread, no network read) is driven
//! through its production `events()` path with a mock snapshot fetch, the real
//! `RecoveryDriver` and a fake ingress queue; `finish()` ends it in order.
use super::*;
use darkbot_solana::ingress::AccountUpdate;
use darkbot_solana::state::recovery::RecoveryOutcome;
use std::sync::{Arc, Mutex};

struct Fixture {
    owner: ShadowOwner,
    ingress: IngressSessions,
    producer: darkbot_common::channel::Producer<IngressEvent>,
    program_data_account: AccountUpdate,
    dir: std::path::PathBuf,
    /// A23-01/FE-04: detached result queues the test drives as the workers.
    results: Option<crate::orchestration::remote_workers::Detached>,
    /// The identities a fixture builder names for its tests (P8-CTRL.3: banks, mints).
    marks: std::collections::BTreeMap<&'static str, Pubkey>,
}

/// The built-in operational limits the fixture's owner runs with.
static OPERATIONAL: std::sync::LazyLock<darkbot_solana::runtime_config::Limits> =
    std::sync::LazyLock::new(Default::default);

fn e2e_limits() -> Limits {
    Limits {
        max_total_capital: 0,
        max_outstanding_inventory: 0,
        max_concurrent_positions: 1,
        max_exposure_per_attempt: 0,
        max_daily_loss: 0,
        max_failed_attempts_per_day: 0,
        max_tip: 0,
        max_slippage_bps: 0,
        max_outstanding_attempts: 1,
    }
}

fn fixture(tag: &str) -> Fixture {
    fixture_rpc(tag, "http://127.0.0.1:9")
}

/// The fixture with its remote workers against `rpc` (default: a closed port).
fn fixture_rpc(tag: &str, rpc: &str) -> Fixture {
    fixture_with(tag, Some(rpc))
}

/// `rpc = None`: the production result queues without worker threads (A23-01 test seam).
fn fixture_with(tag: &str, rpc: Option<&str>) -> Fixture {
    let detached = rpc.is_none();
    let rpc = rpc.unwrap_or("http://127.0.0.1:9");
    let dir = std::env::temp_dir().join(format!("p0_owner_e2e_{}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let clock = Clock::new();
    let shutdown = Shutdown::new();
    let program = Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).unwrap();
    let (group, program_data, liquidator) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let program_data_account = AccountUpdate {
        pubkey: program_data,
        owner: Pubkey::from_str(protocol::BPF_LOADER_UPGRADEABLE_ID).unwrap(),
        lamports: 1,
        data: vec![7; 64],
        slot: 10,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    };
    let wake = darkbot_common::channel::Wakeup::default();
    shutdown.ring_on_request(&wake);
    let (producer, consumer) = bounded::<IngressEvent>("ingress", 1024);
    let producer = producer.with_wakeup(&wake);
    let unused_rpc = rpc.to_string();
    let mut results = None;
    let limits = e2e_limits();
    let ingress = IngressSessions {
        ws_url: "ws://127.0.0.1:9".into(),
        slot_watch: SlotWatch::new(),
        clock,
        econ: None,
    };
    let owner = ShadowOwner {
        metrics: OwnerMetrics::default(),
        mode: "shadow".into(),
        run_id: format!("e2e-{tag}"),
        status_interval: Duration::from_millis(1),
        status_published: None,
        status_seq: 0,
        ledger_view: None,
        blockhash_max_age_ns: OPERATIONAL.blockhash_max_age_ms * 1_000_000,
        fe04_settle_retry: Duration::from_millis(OPERATIONAL.fe04_settle_retry_ms),
        fe04_refresh: Duration::from_millis(OPERATIONAL.fe04_refresh_ms),
        projection_requeue_max: OPERATIONAL.projection_requeue_max as u8,
        end_result: None,
        eval_slippage_bps: 0,
        zero_prefund_enabled: false,
        c0_census: None,
        required_dependency_keys: vec![],
        mt0_reporter: darkbot_common::mt0::spawn_reporter(
            Duration::from_secs(1),
            shutdown.clone(),
            crate::analytics::mt0_journal::Mt0Journal::create(&dir.join("mt0.p0j")).unwrap(),
        )
        .unwrap(),
        optional_dependency_keys: vec![],
        subscribed: HashSet::new(),
        required_set: HashSet::new(),
        optional_set: HashSet::new(),
        prereq_allowed: vec![],
        reservations: Default::default(),
        fe04_journal: None,
        fe04_refreshed: std::time::Instant::now(),
        fe04_held: None,
        fe04_approval: None,
        fe04_settle_due: None,
        fe04_unbooked: None,
        fe04_non_landing: Default::default(),
        fe04_admission: None,
        fe04_admissions: Default::default(),
        hold_gate: crate::execution::hold_b::HoldGate::OBSERVE,
        inventory_targets: Default::default(),
        jit_funding_bank: None,
        rebalance: super::rebalance::Observer::new(
            crate::execution::inventory::RebalanceMode::Off,
            &OPERATIONAL,
        ),
        prepared: std::cell::RefCell::new(crate::execution::prepared::PreparedCache::new(
            8,
            vec![],
        )),
        route_cap: std::cell::RefCell::new(crate::execution::route_cap::RouteCapCache::new(8, 64)),
        program_generation: 0,
        next_candidate_id: 1,
        candidate_revisions: HashMap::new(),
        detect_origin: None,
        blockhash_parking: Default::default(),
        blockhash_early: None,
        ownership: Default::default(),
        atas: accounts::LiquidatorAtas::new(liquidator),
        min_profit: 0,
        integration_probe_enabled: false,
        integration_probe_limit_per_kind: 8,
        remote_worker_capacity: 16,
        projection_cache: HashMap::new(),
        projection_requeues: HashMap::new(),
        projection_parked: ParkedProjections::default(),
        projection_pending: HashSet::new(),
        projection_stats: ProjectionStats::default(),
        detect_queue: VecDeque::new(),
        bootstrap_probe_queue: VecDeque::new(),
        oracle_lanes: CandidateLanes::default(),
        oracle_bank_projection_pending: HashMap::new(),
        integration_probe_expected: HashSet::new(),
        integration_probe_queue: VecDeque::new(),
        integration_probe_terminal: HashSet::new(),
        threshold_fallback: HashSet::new(),
        threshold_bootstrap: VecDeque::new(),
        threshold_bootstrap_queued: HashSet::new(),
        threshold_bootstrap_batch: 64,
        threshold: dec06_control(Default::default(), None),
        sweep_batch_max_events: 64,
        sweep_batch_max_age_ns: 5_000_000,
        sweep_dirty_banks: HashMap::new(),
        time_schedule: Default::default(),
        sweep_batch_events: 0,
        sweep_batch_started_ns: None,
        candidate_recheck_slots: 1,
        candidate_error_recheck_slots: 1,
        candidate_repeat_soft_cap: 16,
        candidate_cache: HashMap::new(),
        candidate_force_rearm: HashSet::new(),
        candidate_input_epoch: 0,
        bank_input_revision: HashMap::new(),
        ingress_burst_before_detect: 64,
        ingress_since_detect: 0,
        network: NetworkBudget::unbounded(),
        clock,
        econ: None,
        operational_start: 0,
        operational_window: None,
        operational_failure: None,
        program,
        group,
        payer: solana_sdk::signature::Keypair::new(),
        synthetic_signer: solana_sdk::signature::Keypair::new(),
        liquidator,
        shutdown: shutdown.clone(),
        stop_poll: || false,
        halt: IngressHalt::new(),
        slot_watch: SlotWatch::new(),
        producer: producer.clone(),
        consumer,
        wake: wake.clone(),
        sources: sources::OwnerSources::new(
            darkbot_solana::ingress::sources::SourceArbiter::new(&[
                crate::orchestration::source_control::control_of(
                    darkbot_solana::ingress::sources::SourceUnit::WsRpcControl,
                    darkbot_solana::ingress::sources::OperatingMode::Active,
                )
                .unwrap(),
            ])
            .unwrap(),
            None,
            Box::new(|_, _, _, _, _, _, _| Err("A23_NO_SOURCE".to_string())),
            64,
            &wake,
            None,
        ),
        program_data,
        program_data_baseline_hash: hash(&program_data_account.data),
        program_data_executable: false,
        admission: None,
        sol_bank: Pubkey::new_unique(),
        ingress_session: IngressSession::new(Shutdown::new(), std::thread::spawn(|| {}), 1),
        retired_events: Default::default(),
        st: StateCore::new(1024),
        drv: RecoveryDriver::new(3),
        dep_shared: Arc::new(Mutex::new(vec![])),
        opt_shared: Arc::new(Mutex::new(vec![])),
        blockhash: BlockhashCache::new(),
        blockhash_thread: None,
        safety: Safety::new(limits, Allow::default(), false),
        allowlist: Allowlist::default(),
        #[cfg(feature = "crate_health")]
        instruction_table: darkbot_project0::receivership::builders::instruction_table(&program)
            .unwrap(),
        #[cfg(feature = "crate_health")]
        dependency_gaps: HashMap::new(),
        global_allowed: vec![],
        submitter: crate::execution::observe_only::ObserveOnly::new(
            crate::execution::bundle_path::OwnerSubmitter::new(RpcSubmitter::new(
                unused_rpc.clone(),
                true,
            )),
        ),
        bundle_config: None,
        failed_costs: FailedCostMatrix::default(),
        routes: JupiterV2RouteProvider::new(liquidator, None, 0, 64, true).unwrap(),
        resources: Resources {
            compute_unit_limit: 1_400_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        freshness: Freshness {
            max_opportunity_age_ns: 2_000_000_000,
            execution_deadline_ns: 2_000_000_000,
        },
        remote: if detached {
            let (remote, detached) = RemoteWorkers::detached(&shutdown, 16, &wake);
            results = Some(detached);
            remote
        } else {
            RemoteWorkers::start(unused_rpc, program, &shutdown, 16, &wake).unwrap()
        },
        integration_probe_projection_ready: 0,
        integration_probe_projection_failed: 0,
        integration_probe_projection_stale: 0,
        integration_probe_completion_reported: false,
        ledger: ShadowLedger::new(),
        winner_tracker: WinnerTracker::new(WinnerTrackerConfig::DEFAULT),
        evidence: OwnerEvidenceWriter::start(
            dir.join("opportunities.p0j"),
            dir.join("recovery.p0j"),
            crate::analytics::evidence::OWNER_EVIDENCE_CAPACITY,
            Duration::from_millis(250),
        )
        .unwrap(),
        windows: None,
        counts: [0; 6],
        last_reported_update_total: 0,
        bootstrap_probe_done: true,
        program_upgrade_latched: false,
        program_pin: None,
        reverse_index: ReverseIndex::new(),
        threshold_index: ThresholdIndex::new(),
        threshold_ready: false,
        mt0_owner: darkbot_common::mt0::OwnerTimer::new(),
    };
    Fixture {
        owner,
        ingress,
        producer,
        program_data_account,
        dir,
        results,
        marks: Default::default(),
    }
}

fn reconnected() -> IngressEvent {
    IngressEvent::Reconnected {
        source: SourceKind::WsRpcControl,
        attempt: 1,
        t_rx_ns: 0,
    }
}

/// Runs `events()` until the queue is drained or the owner ends terminally.
fn drive(
    f: &mut Fixture,
    fetch: &mut dyn FnMut() -> Result<SnapshotData, String>,
) -> Result<(), &'static str> {
    let mut fetch = |/* adapter for impl FnMut */| fetch();
    for _ in 0..64 {
        match f.owner.events(&mut fetch, &f.ingress) {
            Ok(_) => {}
            Err(e) => {
                return Err(e
                    .downcast_ref::<OwnerTerminal>()
                    .map(|t| t.0)
                    .unwrap_or("OTHER_ERROR"))
            }
        }
        if f.owner.consumer.stats().sent == f.owner.consumer.stats().received {
            return Ok(());
        }
    }
    Ok(())
}

fn snapshot(f: &Fixture, missing_required: bool) -> SnapshotData {
    SnapshotData {
        slot: 10,
        accounts: vec![f.program_data_account.clone()],
        missing_required: if missing_required {
            vec![(Pubkey::new_unique(), 10)]
        } else {
            vec![]
        },
    }
}

/// P0-PROD-CTRL-04 (A01, F-05): the owner projection carries what the owner holds — the applied
/// reconnect counted (never a placeholder 0), a refused unversioned same-slot update counted,
/// Safety's configured limits, every gated capability with its reason — and without an FE-04
/// journal the finance totals stay UNKNOWN (never 0) while the reserved capital is known.
#[test]
fn owner_projection_carries_real_engine_safety_and_unknown_finance() {
    let mut f = fixture("owner_projection");
    let good = snapshot(&f, false);
    let mut fetch = || {
        Ok(SnapshotData {
            slot: good.slot,
            accounts: good.accounts.clone(),
            missing_required: vec![],
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    let versioned = |wv, byte| AccountUpdate {
        pubkey: f.program_data_account.pubkey,
        write_version: wv,
        data: vec![byte; 8],
        slot: 10,
        ..f.program_data_account.clone()
    };
    f.owner
        .st
        .on_event(IngressEvent::Account(versioned(Some(9), 1)));
    f.owner
        .st
        .on_event(IngressEvent::Account(versioned(None, 2)));
    let s = f.owner.owner_snapshot(1_760_000_000_000);
    assert_eq!((s.engine.run_state, s.engine.reconnects), ("READY", 1));
    assert_eq!(s.engine.counters.same_slot_unversioned_refused, 1);
    assert_eq!(s.run_id, "e2e-owner_projection");
    assert_eq!(
        s.safety.max_concurrent_positions,
        e2e_limits().max_concurrent_positions
    );
    let cap = |name: &str| {
        s.capabilities
            .iter()
            .find(|c| c.name == name)
            .map(|c| (c.requested, c.effective))
            .unwrap()
    };
    assert_eq!(cap("LIVE"), ("OFF", "BLOCKED"));
    assert_eq!(cap("SUBMISSION"), ("OFF", "NOT_AUTHORIZED"));
    assert_eq!(cap("JITO_BUNDLE"), ("OFF", "OFF"));
    assert_eq!(cap("JIT_EXACT_IN"), ("OFF", "OFF"));
    // A JIT plan is a bundle (buy + liquidation): without a bundle path the owner rejects every
    // one before simulation, so a configured JIT bank is BLOCKED, never a fake OBSERVE.
    f.owner.jit_funding_bank = Some(Pubkey::new_unique());
    let jit = f.owner.owner_snapshot(1_760_000_000_000);
    let jit = jit
        .capabilities
        .iter()
        .find(|c| c.name == "JIT_EXACT_IN")
        .unwrap();
    assert_eq!((jit.requested, jit.effective), ("ACTIVE", "BLOCKED"));
    assert!(jit.block_reason.contains("bundle"), "{}", jit.block_reason);
    f.owner.jit_funding_bank = None;
    assert!(s
        .capabilities
        .iter()
        .filter(|c| c.effective == "BLOCKED" || c.effective == "NOT_AUTHORIZED")
        .all(|c| !c.block_reason.is_empty()));
    assert_eq!(s.finance.status, "UNKNOWN");
    assert_eq!(
        (
            s.finance.total_capital_usd_micro,
            s.finance.realized_net_pnl_usd_micro,
            s.finance.transferred_to_treasury_usd_micro
        ),
        (None, None, None)
    );
    assert_eq!(s.finance.reserved_capital_usd_micro, Some(0));
    assert!(!s.wallets.journal_read);
    assert_eq!(s.wallets.held_attempt, "NONE");
    assert_eq!(s.end_result, None);
    // Review E4: a read journal without reconciled balances is PARTIAL (never VERIFIED while
    // a field is unknown); the run's end reaches the final snapshot.
    f.owner.ledger_view = Some(Ok(super::status_view::LedgerView::of(
        &crate::execution::accounting::Ledger::default(),
    )));
    f.owner.end_result = Some("STATE_RECOVERY_EXHAUSTED");
    let s = f.owner.owner_snapshot(1_760_000_000_000);
    assert_eq!(s.finance.status, "PARTIAL");
    assert_eq!(s.finance.realized_net_pnl_usd_micro, Some(0));
    assert_eq!(s.end_result.as_deref(), Some("STATE_RECOVERY_EXHAUSTED"));
    cleanup(f);
}

/// V2-10 (P5a-02): a delivered update that takes an indexed account out of the group's
/// MarginfiAccount layout (closed: empty data, system owner) removes it from the ReverseIndex —
/// before, only an incoming MarginfiAccount classification reached the index, so the close left
/// its banks indexed. A recovery whose snapshot lacks the account rebuilds without it too.
#[test]
fn v2_10_a_closed_indexed_account_leaves_the_reverse_index() {
    use bytemuck::Zeroable;
    use marginfi_type_crate::types::MarginfiAccount;
    let mut f = fixture("v2_10_close");
    let (account, bank) = (Pubkey::new_unique(), Pubkey::new_unique());
    let mut a = MarginfiAccount::zeroed();
    a.group = f.owner.group;
    a.lending_account.balances[0].active = 1;
    a.lending_account.balances[0].bank_pk = bank;
    let mut snap = snapshot(&f, false);
    let base = snap.accounts.clone();
    snap.accounts.push(AccountUpdate {
        pubkey: account,
        owner: f.owner.program,
        lamports: 1,
        data: [
            protocol::DISC_MARGINFI_ACCOUNT.as_slice(),
            bytemuck::bytes_of(&a),
        ]
        .concat(),
        slot: 10,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    });
    let mut fetch = || {
        Ok(SnapshotData {
            slot: snap.slot,
            accounts: snap.accounts.clone(),
            missing_required: vec![],
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    let banks = |f: &Fixture| {
        f.owner
            .reverse_index
            .banks_for_account(&account)
            .copied()
            .collect::<Vec<_>>()
    };
    assert_eq!(banks(&f), vec![bank]);
    f.producer
        .try_push(IngressEvent::Account(AccountUpdate {
            pubkey: account,
            owner: Pubkey::default(),
            lamports: 0,
            data: vec![],
            slot: 11,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }))
        .unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert_eq!(
        banks(&f),
        Vec::<Pubkey>::new(),
        "the close leaves the index"
    );
    // The recovery rebuild from a snapshot without the account keeps it out.
    let mut without = || {
        Ok(SnapshotData {
            slot: 12,
            accounts: base.clone(),
            missing_required: vec![],
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut without), Ok(()));
    assert_eq!(banks(&f), Vec::<Pubkey>::new(), "rebuilt without it");
}

/// Review E3 (V2-02): an ingress failure raised while a recovery runs (a sink overflow during
/// the snapshot fetch) is not covered by that recovery — its Ready clears only what was raised
/// before it started; the new halt stays for the lifecycle (FeedGap) and the next stream.
#[test]
fn recovery_ready_keeps_a_halt_raised_during_the_recovery() {
    let mut f = fixture("halt_during_recovery");
    f.owner.halt.raise();
    let good = snapshot(&f, false);
    let halt = f.owner.halt.clone();
    let mut fetch = || {
        halt.raise();
        Ok(SnapshotData {
            slot: good.slot,
            accounts: good.accounts.clone(),
            missing_required: vec![],
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert!(
        f.owner.halt.is_raised(),
        "a failure during the recovery stays"
    );
}

/// Review E3 (V2-02): a raised ingress halt not yet consumed by the lifecycle (same drain batch)
/// blocks the fork trigger — a root that orphans a write starts no recovery whose Ready would
/// clear that halt; the state stays halted.
#[test]
fn fork_trigger_never_runs_over_a_raised_ingress_halt() {
    use darkbot_solana::ingress::{AccountUpdate, SlotStatus, SlotUpdate};
    let mut f = fixture("fork_over_halt");
    let good = snapshot(&f, false);
    let fetches = std::cell::Cell::new(0);
    let mut fetch = || {
        fetches.set(fetches.get() + 1);
        Ok(SnapshotData {
            slot: good.slot,
            accounts: good.accounts.clone(),
            missing_required: vec![],
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert!(f.owner.st.run_state().is_tradable());
    let before = fetches.get();
    let slot = |slot, parent| {
        IngressEvent::Slot(SlotUpdate {
            slot,
            parent: Some(parent),
            status: SlotStatus::Processed,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        })
    };
    let root = |slot| IngressEvent::Root {
        slot,
        source: SourceKind::WsRpcControl,
        t_rx_ns: 0,
    };
    for ev in [
        slot(11, 10),
        root(11),
        slot(12, 11),
        IngressEvent::Account(AccountUpdate {
            pubkey: Pubkey::new_unique(),
            owner: Pubkey::new_unique(),
            lamports: 1,
            data: vec![1; 8],
            slot: 12,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }),
        slot(13, 11),
    ] {
        f.producer.try_push(ev).unwrap();
    }
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    f.owner.halt.raise();
    f.producer.try_push(root(13)).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert_eq!(fetches.get(), before, "no recovery over a raised halt");
    assert!(f.owner.halt.is_raised());
    assert!(!f.owner.st.run_state().is_tradable());
}

/// C1 test 7, Ready path: a StreamReady with a valid A/B snapshot pair (ProgramData equal to the
/// startup baseline) makes the owner Ready and clears the ingress halt; finish ends in order.
#[test]
fn owner_end_to_end_recovers_only_with_a_valid_snapshot() {
    let mut f = fixture("ready");
    f.owner.halt.raise();
    let good = snapshot(&f, false);
    let mut fetch = || {
        Ok(SnapshotData {
            slot: good.slot,
            accounts: good.accounts.clone(),
            missing_required: vec![],
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert!(
        f.owner.st.run_state().is_tradable(),
        "{:?}",
        f.owner.st.run_state()
    );
    assert!(!f.owner.halt.is_raised() && !f.owner.program_upgrade_latched);
    // A later Gap halts again; without a new StreamReady the owner stays halted.
    f.producer
        .try_push(IngressEvent::Gap {
            source: SourceKind::WsRpcControl,
            from_slot: Some(10),
            to_slot: None,
            t_rx_ns: 0,
        })
        .unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert!(!f.owner.st.run_state().is_tradable());
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

/// C1 test 7, terminal paths: snapshots that never become clean end the owner with
/// STATE_RECOVERY_EXHAUSTED (bounded: 3 attempts, no extra cycle without a fresh reconnect); a
/// changed ProgramData never becomes tradable; an ingress Fatal ends it with
/// WSS_RECOVERY_EXHAUSTED. Each terminal code reaches the start diagnostics verbatim; the econ
/// runner turns any owner error into shadow_failed + abort, which seals the market journal
/// INCOMPLETE (D1 case "abort").
#[test]
fn owner_end_to_end_ends_terminally_and_never_fakes_ready() {
    // Never-clean snapshots.
    let mut f = fixture("exhausted");
    let fetches = std::cell::Cell::new(0);
    let dirty = snapshot(&f, true);
    let mut fetch = || {
        fetches.set(fetches.get() + 1);
        Ok(SnapshotData {
            slot: dirty.slot,
            accounts: dirty.accounts.clone(),
            missing_required: dirty.missing_required.clone(),
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Err("STATE_RECOVERY_EXHAUSTED"));
    assert_eq!(fetches.get(), 3, "three bounded attempts, no extra cycle");
    assert!(!f.owner.st.run_state().is_tradable());
    assert_eq!(
        shadow_failure_code(&OwnerTerminal("STATE_RECOVERY_EXHAUSTED")),
        "STATE_RECOVERY_EXHAUSTED"
    );
    f.owner.finish().unwrap();

    // ProgramData differs from the startup baseline: recovered but latched, never tradable.
    let mut f = fixture("upgrade");
    let mut changed = snapshot(&f, false);
    changed.accounts[0].data = vec![8; 64];
    let mut fetch = || {
        Ok(SnapshotData {
            slot: changed.slot,
            accounts: changed.accounts.clone(),
            missing_required: vec![],
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert!(f.owner.program_upgrade_latched);
    assert!(!f.owner.st.run_state().is_tradable());
    f.owner.finish().unwrap();

    // Ingress Fatal: terminal without any snapshot read.
    let mut f = fixture("fatal");
    let fetches = std::cell::Cell::new(0);
    let mut fetch = || {
        fetches.set(fetches.get() + 1);
        Err("unused".to_string())
    };
    f.producer
        .try_push(IngressEvent::Fatal {
            source: SourceKind::WsRpcControl,
            detail: "WSS_RECOVERY_EXHAUSTED".into(),
            t_rx_ns: 0,
        })
        .unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Err("WSS_RECOVERY_EXHAUSTED"));
    assert_eq!(fetches.get(), 0);
    assert!(
        matches!(
            f.owner
                .drv
                .on_event(&mut f.owner.st, reconnected(), &mut fetch, &mut |_| {}),
            Some(RecoveryOutcome::Fatal { .. })
        ),
        "Fatal stays sticky: no later StreamReady recovers"
    );
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

/// C1 test 7, last link: the econ runner turns an owner error into shadow_failed + abort (the
/// market capture then seals INCOMPLETE, D1 "abort"); a clean end after admission closed does not.
#[test]
fn owner_error_aborts_the_econ_run() {
    use crate::orchestration::startup::record_shadow_end;
    use std::sync::atomic::Ordering;
    for (failed, admission_closed, aborted) in [
        (true, true, true),
        (true, false, true),
        (false, false, true),
        (false, true, false),
    ] {
        let shared = crate::analytics::econ::Shared::default();
        record_shadow_end(&shared, failed, admission_closed);
        assert_eq!(
            (
                shared.shadow_failed.load(Ordering::Acquire),
                shared.abort.load(Ordering::Acquire),
                shared.shadow_done.load(Ordering::Acquire)
            ),
            (aborted, aborted, true),
            "failed={failed} admission_closed={admission_closed}"
        );
    }
}

/// X1 at admission: a start by p0ctl binds the approved network audit to the state the owner read
/// itself at the first Ready, before the ingress halt clears. A matching record admits; a drifted
/// ProgramData or a missing required dependency ends the run with BOOT_IDENTITY_DRIFT, the halt
/// stays raised and nothing is re-pinned or retried.
#[test]
fn owner_binds_the_approved_audit_before_admission() {
    use crate::orchestration::boot_audit::{network_record_fixture, proof_of};
    let program_data = |f: &Fixture, data: &[u8]| {
        proof_of(
            &f.program_data_account.pubkey,
            &f.program_data_account.owner,
            data,
            false,
        )
    };
    for case in ["bound", "program_data", "missing"] {
        let mut f = fixture(&format!("x1_{case}"));
        let required = Pubkey::new_unique();
        let mut accounts = vec![program_data(
            &f,
            if case == "program_data" {
                &[8; 64]
            } else {
                &[7; 64]
            },
        )];
        if case == "missing" {
            f.owner.required_dependency_keys = vec![required];
            accounts.push(proof_of(&required, &f.owner.program, &[1; 32], false));
        }
        f.owner.admission = Some(Box::new(network_record_fixture(accounts)));
        f.owner.halt.raise();
        let good = snapshot(&f, false);
        let mut fetch = || {
            Ok(SnapshotData {
                slot: good.slot,
                accounts: good.accounts.clone(),
                missing_required: vec![],
            })
        };
        f.producer.try_push(reconnected()).unwrap();
        let result = drive(&mut f, &mut fetch);
        if case == "bound" {
            assert_eq!(result, Ok(()));
            assert!(f.owner.st.run_state().is_tradable() && !f.owner.halt.is_raised());
        } else {
            assert_eq!(result, Err("BOOT_IDENTITY_DRIFT"), "{case}");
            assert!(f.owner.halt.is_raised(), "{case}: no admission");
        }
        assert!(f.owner.admission.is_none(), "bound exactly once");
        let dir = f.dir.clone();
        f.owner.finish().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// N4: the projection counters reach the owner journal once at shutdown, also when the update
/// count never reached a periodic report.
#[test]
fn n4_projection_counters_are_journaled_at_shutdown() {
    use crate::analytics::payload::{decode, OwnerDiagnostic, Payload};
    let mut f = fixture("projection_stats");
    // A balancing history: 3 requested = 2 accepted + 1 failed; 2 accepted = 1 reused +
    // 1 discarded at use.
    let s = &mut f.owner.projection_stats;
    (s.requested, s.accepted, s.failed) = (3, 2, 1);
    (s.reused, s.discarded_at_consume) = (1, 1);
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut lines = vec![];
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == OwnerDiagnostic::KIND {
            let d: OwnerDiagnostic = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            lines.push(d.line);
        }
    }
    let projection: Vec<_> = lines
        .iter()
        .filter(|l| l.starts_with("projection: "))
        .collect();
    assert_eq!(
        projection,
        [
            "projection: requested=3 queue_full=0 observation_unknown=0 accepted=2 failed=1 \
             stale_result=0 discarded_result=0 in_flight=0 reused=1 invalid_at_use=0 \
             discarded_consume=1 stale_consume=0 dropped=0 cached=0 requeued=0 \
             requeue_exhausted=0 deferred=0"
        ],
        "{lines:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Z14: the bootstrap probes of the was-liquidatable accounts after a recovery never occupy the
/// bounded ingress channel the WSS session writes to (a full channel ends that session as a sink
/// overflow); they are the owner's own detection work behind live ingress.
#[test]
fn z14_bootstrap_probes_never_occupy_the_ingress_channel() {
    use crate::analytics::payload::{decode, OwnerDiagnostic, Payload};
    use bytemuck::Zeroable;
    use marginfi_type_crate::types::MarginfiAccount;
    let mut f = fixture("z14");
    f.owner.halt.raise();
    f.owner.bootstrap_probe_done = false;
    // More probe candidates than the fixture's 1024-slot ingress channel holds.
    let probes = 1_100usize;
    let mut accounts = vec![f.program_data_account.clone()];
    for _ in 0..probes {
        let mut a = MarginfiAccount::zeroed();
        a.group = f.owner.group;
        a.indexer_flags.was_liquidatable = 1;
        accounts.push(AccountUpdate {
            pubkey: Pubkey::new_unique(),
            owner: f.owner.program,
            lamports: 1,
            data: [
                protocol::DISC_MARGINFI_ACCOUNT.as_slice(),
                bytemuck::bytes_of(&a),
            ]
            .concat(),
            slot: 10,
            write_version: None,
            source: SourceKind::Snapshot,
            t_rx_ns: 0,
        });
    }
    let mut fetch = || {
        Ok(SnapshotData {
            slot: 10,
            accounts: accounts.clone(),
            missing_required: vec![],
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert!(f.owner.st.run_state().is_tradable());
    // Every probe is the owner's own detection work, in deterministic account order.
    let scheduled: Vec<Pubkey> = f.owner.bootstrap_probe_queue.iter().map(|w| w.0).collect();
    assert_eq!(scheduled.len(), probes);
    assert!(scheduled
        .windows(2)
        .all(|w| w[0].to_bytes() < w[1].to_bytes()));
    assert!(f
        .owner
        .bootstrap_probe_queue
        .iter()
        .all(|w| w.3 == "bootstrap_snapshot_probe"));
    // An oracle-triggered candidate waiting meanwhile is served before the probes.
    let oracle = Pubkey::new_unique();
    f.owner.oracle_lanes.admit(
        oracle,
        LaneAdmission {
            t_rx_ns: 0,
            slot: 10,
            trigger: "oracle_price",
            lane: Lane::L2,
            revision: 0,
            last_forwarded_slot: 0,
        },
    );
    let next = |f: &mut Fixture| match f.owner.events(&mut || unreachable!(), &f.ingress) {
        Ok(OwnerStep::Next(Some(w))) => w,
        _ => panic!("detection work expected"),
    };
    assert_eq!(next(&mut f).0, oracle, "oracle work first");
    assert_eq!(next(&mut f).0, scheduled[0], "then the probes in order");
    assert_eq!(next(&mut f).0, scheduled[1]);
    // The WSS producer still has the whole channel.
    let free = (0..1_024)
        .take_while(|_| {
            f.producer
                .try_push(IngressEvent::Gap {
                    source: SourceKind::WsRpcControl,
                    from_slot: Some(11),
                    to_slot: None,
                    t_rx_ns: 0,
                })
                .is_ok()
        })
        .count();
    assert_eq!(free, 1_024, "probes occupy no ingress slot");
    // V13: the backlog is journaled on its own line (here once at shutdown).
    let dir = f.dir.clone();
    let _ = f.owner.finish();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut lines = vec![];
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == OwnerDiagnostic::KIND {
            let d: OwnerDiagnostic = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            lines.push(d.line);
        }
    }
    let backlog: Vec<_> = lines
        .iter()
        .filter(|l| l.starts_with("bootstrap_probes: "))
        .collect();
    assert_eq!(
        backlog,
        ["bootstrap_probes: queued=1100 taken=2 pending=1098"],
        "{lines:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// E9: a projection made unusable by a change during the worker's projection (or by a lagging
/// node) is requested again through the ordinary detect queue — behind work already waiting, so
/// neither starves — at most `projection_requeue_max` times in a row; then it is counted and the
/// candidate waits for its next trigger, and a later stale result starts a fresh bounded
/// sequence. A candidate no longer in state is not requeued.
#[test]
fn e9_stale_projection_requeue_is_bounded_and_fair() {
    let mut f = fixture("e9_requeue");
    let candidate = Pubkey::new_unique();
    f.owner.st.install_snapshot(
        12,
        vec![AccountUpdate {
            pubkey: candidate,
            owner: f.owner.program,
            lamports: 1,
            data: vec![1; 8],
            slot: 12,
            write_version: None,
            source: SourceKind::Snapshot,
            t_rx_ns: 0,
        }],
    );
    let waiting = (Pubkey::new_unique(), 1, 11, "account_update");
    f.owner.detect_queue.push_back(waiting);
    let requeue = (candidate, 7, 12, "integration_projection_requeue");
    for n in 1..=f.owner.projection_requeue_max as u64 {
        f.owner.requeue_projection(candidate, 7, None);
        assert_eq!(f.owner.projection_stats.requeued, n);
    }
    let queued: Vec<_> = f.owner.detect_queue.iter().copied().collect();
    let mut expected = vec![waiting];
    expected.extend(std::iter::repeat_n(
        requeue,
        f.owner.projection_requeue_max as usize,
    ));
    assert_eq!(queued, expected, "behind the waiting work, FIFO");
    // The bound: no further request, counted, the sequence ends.
    f.owner.requeue_projection(candidate, 7, None);
    assert_eq!(f.owner.detect_queue.len(), expected.len());
    assert_eq!(f.owner.projection_stats.requeue_exhausted, 1);
    assert!(!f.owner.projection_requeues.contains_key(&candidate));
    // A later stale result (after a new trigger) gets a fresh bounded sequence.
    f.owner.requeue_projection(candidate, 9, None);
    assert_eq!(
        f.owner.detect_queue.back(),
        Some(&(candidate, 9, 12, "integration_projection_requeue"))
    );
    // Not in state: nothing queued, nothing kept.
    let gone = Pubkey::new_unique();
    let before = f.owner.detect_queue.len();
    f.owner.requeue_projection(gone, 7, None);
    assert_eq!(f.owner.detect_queue.len(), before);
    assert!(!f.owner.projection_requeues.contains_key(&gone));
    let dir = f.dir.clone();
    drop(f);
    let _ = std::fs::remove_dir_all(dir);
}

/// A loopback JSON-RPC endpoint answering each request with the next scripted response members
/// (`{"result": …}` or `{"error": …}`).
fn scripted_rpc(results: Vec<serde_json::Value>) -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let results = std::sync::Arc::new(std::sync::Mutex::new(
        results.into_iter().collect::<VecDeque<_>>(),
    ));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let results = results.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut stream = stream;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut length = 0usize;
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).unwrap_or(0) == 0 {
                            return;
                        }
                        if h == "\r\n" {
                            break;
                        }
                        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; length];
                    if reader.read_exact(&mut body).is_err() {
                        return;
                    }
                    let Some(mut answer) = results.lock().unwrap().pop_front() else {
                        return;
                    };
                    answer["jsonrpc"] = "2.0".into();
                    answer["id"] = 1.into();
                    let body = answer.to_string();
                    // One write (no Nagle/delayed-ACK stall between header and body pieces).
                    let _ = stream.set_nodelay(true);
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    );
                }
            });
        }
    });
    url
}

/// E9 through the real projection worker and the owner's result handling: a node behind the
/// required context and a stale result each requeue the candidate (typed, bounded); a ready
/// projection is cached and ends the requeue sequence.
#[test]
fn e9_projection_results_drive_the_bounded_requeue() {
    use base64::Engine;
    use solana_sdk::signature::Signer;
    let account = |data: Vec<u8>| {
        serde_json::json!({"lamports": 1, "owner": Pubkey::default().to_string(),
            "data": [base64::engine::general_purpose::STANDARD.encode(&data), "base64"],
            "executable": false, "rentEpoch": 0, "space": data.len()})
    };
    let at = |slot: u64| {
        let clock = solana_sdk::clock::Clock {
            slot,
            unix_timestamp: 1_700_000_000,
            ..Default::default()
        };
        serde_json::json!({"result": {"context": {"slot": slot}, "value": {
            "err": null, "logs": [], "unitsConsumed": 5, "returnData": null,
            "innerInstructions": null, "replacementBlockhash": null,
            "accounts": [account(vec![7; 8]), account(bincode::serialize(&clock).unwrap())]}}})
    };
    // The node has not reached slot 12 (-32016), two answers at the required slot, then a
    // failed simulation.
    let behind = serde_json::json!({"error": {"code": -32016,
        "message": "Minimum context slot has not been reached", "data": {"contextSlot": 11}}});
    let failed = serde_json::json!({"result": {"context": {"slot": 12}, "value": {
        "err": {"InstructionError": [0, "InvalidArgument"]}, "logs": [], "unitsConsumed": 5,
        "accounts": null, "returnData": null, "innerInstructions": null,
        "replacementBlockhash": null}}});
    let rpc = scripted_rpc(vec![behind, at(12), at(12), failed]);
    let mut f = fixture_rpc("e9_results", &rpc);
    let candidate = Pubkey::new_unique();
    let data = vec![1u8; 8];
    let snapshot = vec![AccountUpdate {
        pubkey: candidate,
        owner: f.owner.program,
        lamports: 1,
        data: data.clone(),
        slot: 12,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    }];
    f.owner.st.install_snapshot(12, snapshot.clone());
    f.owner.st.reconcile(12, &snapshot);
    f.owner.st.confirm_ready();
    let payer = solana_sdk::signature::Keypair::new();
    let mut built = darkbot_solana::tx::build(
        &darkbot_solana::tx::TxPlan {
            version: darkbot_solana::tx::BuildVersion::V0,
            payer: payer.pubkey(),
            instructions: vec![solana_sdk::instruction::Instruction {
                program_id: Pubkey::new_unique(),
                accounts: vec![],
                data: vec![1],
            }],
            lookup_tables: vec![].into(),
            resources: f.owner.resources,
        },
        solana_sdk::hash::Hash::new_unique(),
        &darkbot_solana::tx::TxLimits::default(),
    )
    .unwrap();
    darkbot_solana::tx::sign(&mut built, &[&payer]).unwrap();
    let empty_fingerprint = dependency_fingerprint(&f.owner.st, &[]);
    let identity = test_identity(&f.owner, candidate, &[]);
    let work = |base_account_hash: solana_sdk::hash::Hash| {
        crate::orchestration::remote_workers::ProjectionWork {
            account: candidate,
            base_slot: 12,
            base_account_hash,
            t_rx_ns: 7,
            tx: built.tx.clone(),
            targets: vec![Pubkey::new_unique()],
            dependencies: vec![],
            dependency_fingerprint: empty_fingerprint,
            min_context_slot: 12,
            identity: identity.clone(),
        }
    };
    let run = |f: &mut Fixture, w| {
        let before = f.owner.projection_stats;
        f.owner.projection_pending.insert(candidate);
        assert!(f.owner.remote.projection_jobs.try_push(w).is_ok());
        let started = std::time::Instant::now();
        while f.owner.projection_stats == before {
            assert!(started.elapsed() < std::time::Duration::from_secs(10));
            f.owner.remote_apply().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    let requeue = (candidate, 7, 12, "integration_projection_requeue");
    let real = solana_sdk::hash::hash(&data);
    run(&mut f, work(real));
    assert_eq!(f.owner.projection_stats.failed, 1, "node behind");
    assert_eq!(f.owner.projection_requeues.get(&candidate), Some(&1));
    // Backoff: parked until one more slot of account notifications is processed.
    let queued = f.owner.detect_queue.len();
    assert_eq!(f.owner.projection_parked.len(), 1);
    f.owner.release_parked_projections();
    assert_eq!(
        f.owner.detect_queue.len(),
        queued,
        "not before the next slot"
    );
    // The backoff waits for the slot after the processed level; a notification of the slot
    // after that proves it.
    let after = processed_level(&f.owner.st) + 1;
    f.owner.st.on_event(IngressEvent::Account(AccountUpdate {
        pubkey: Pubkey::new_unique(),
        owner: f.owner.program,
        lamports: 1,
        data: vec![2; 8],
        slot: after + 1,
        write_version: None,
        source: SourceKind::WsRpcControl,
        t_rx_ns: 0,
    }));
    f.owner.release_parked_projections();
    assert_eq!(f.owner.detect_queue.back(), Some(&requeue));
    assert_eq!(f.owner.projection_parked.len(), 0);
    run(&mut f, work(solana_sdk::hash::Hash::new_unique()));
    assert_eq!(f.owner.projection_stats.stale_at_result, 1);
    assert_eq!(f.owner.projection_requeues.get(&candidate), Some(&2));
    assert_eq!(f.owner.projection_stats.requeued, 2);
    run(&mut f, work(real));
    assert_eq!(f.owner.projection_stats.accepted, 1);
    assert!(f.owner.projection_cache.contains_key(&candidate));
    // A ready projection does not end the sequence: only a new external trigger or the bound
    // does (so a projection unusable again at its consumption cannot re-request without bound).
    assert_eq!(f.owner.projection_requeues.get(&candidate), Some(&2));
    // Any other failure is not requeued: it waits for the candidate's next trigger.
    let queued = f.owner.detect_queue.len();
    run(&mut f, work(real));
    assert_eq!(f.owner.projection_stats.failed, 2);
    assert_eq!(f.owner.detect_queue.len(), queued);
    assert_eq!(f.owner.projection_requeues.get(&candidate), Some(&2));
    assert_eq!(f.owner.projection_stats.requeued, 2);
    let dir = f.dir.clone();
    drop(f);
    let _ = std::fs::remove_dir_all(dir);
}

/// E9 (consume gate on the owner's state): a cached projection newer than the processed account
/// notifications is not consumed — parked, counted, its cache entry kept — until a newer
/// notification proves its slot; then it is released into the detect queue and consumable. A new
/// external trigger supersedes the wait; after a gap nothing is consumable until the state is
/// reconciled again.
#[test]
fn e9_consume_gate_waits_for_the_projection_slot() {
    let mut f = fixture("e9_gate");
    let candidate = Pubkey::new_unique();
    let snapshot = vec![AccountUpdate {
        pubkey: candidate,
        owner: f.owner.program,
        lamports: 1,
        data: vec![1; 8],
        slot: 12,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    }];
    f.owner.st.install_snapshot(12, snapshot.clone());
    f.owner.st.reconcile(12, &snapshot);
    f.owner.st.confirm_ready();
    let cache = |slot: u64| CachedProjection {
        base_account_hash: solana_sdk::hash::hash(&[1; 8]),
        projection_slot: slot,
        projection_unix_s: 1_700_000_000,
        accounts: vec![],
        dependencies: vec![],
        dependency_fingerprint: solana_sdk::hash::Hash::default(),
    };
    f.owner.projection_cache.insert(candidate, cache(14));
    let consume = (candidate, 7, 12, TRIGGER_INTEGRATION_PROJECTION);
    let gate = |f: &mut Fixture, work: DetectWork| {
        projection_consume_gate(
            &f.owner.st,
            &f.owner.projection_cache,
            &mut f.owner.projection_parked,
            &mut f.owner.projection_stats,
            work,
        )
    };
    let notify = |f: &mut Fixture, slot: u64| {
        f.owner.st.on_event(IngressEvent::Account(AccountUpdate {
            pubkey: Pubkey::new_unique(),
            owner: f.owner.program,
            lamports: 1,
            data: vec![2; 8],
            slot,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }));
    };
    // Simulated at 14, processed only to 12: parked, counted, the projection kept.
    assert!(!gate(&mut f, consume));
    assert_eq!(f.owner.projection_stats.deferred, 1);
    assert_eq!(f.owner.projection_parked.len(), 1);
    assert!(f.owner.projection_cache.contains_key(&candidate));
    notify(&mut f, 14);
    f.owner.release_parked_projections();
    assert!(
        f.owner.detect_queue.is_empty(),
        "14 itself is not proven yet"
    );
    notify(&mut f, 15);
    f.owner.release_parked_projections();
    assert_eq!(f.owner.detect_queue.pop_back(), Some(consume));
    assert!(gate(&mut f, consume), "processed through 14: consumable");
    // A new external trigger supersedes a parked wait.
    f.owner.projection_cache.insert(candidate, cache(30));
    assert!(!gate(&mut f, consume));
    assert!(gate(&mut f, (candidate, 8, 12, "account_update")));
    assert_eq!(f.owner.projection_parked.len(), 0);
    // After a gap nothing is consumable, whatever was processed before.
    f.owner.projection_cache.insert(candidate, cache(13));
    f.owner.st.on_event(IngressEvent::Gap {
        source: SourceKind::WsRpcControl,
        from_slot: Some(15),
        to_slot: None,
        t_rx_ns: 0,
    });
    assert!(!gate(&mut f, consume));
    let dir = f.dir.clone();
    drop(f);
    let _ = std::fs::remove_dir_all(dir);
}

/// A23-01: a fixture whose projection worker gets `answers` failed simulations from a loopback
/// RPC, and the projection work for one candidate.
fn a23_fixture(
    tag: &str,
    answers: usize,
) -> (
    Fixture,
    Box<dyn Fn() -> crate::orchestration::remote_workers::ProjectionWork>,
) {
    use solana_sdk::signature::Signer;
    let failed = serde_json::json!({"result": {"context": {"slot": 12}, "value": {
        "err": {"InstructionError": [0, "InvalidArgument"]}, "logs": [], "unitsConsumed": 5,
        "accounts": null, "returnData": null, "innerInstructions": null,
        "replacementBlockhash": null}}});
    let f = fixture_rpc(tag, &scripted_rpc(vec![failed; answers]));
    let payer = solana_sdk::signature::Keypair::new();
    let mut built = darkbot_solana::tx::build(
        &darkbot_solana::tx::TxPlan {
            version: darkbot_solana::tx::BuildVersion::V0,
            payer: payer.pubkey(),
            instructions: vec![solana_sdk::instruction::Instruction {
                program_id: Pubkey::new_unique(),
                accounts: vec![],
                data: vec![1],
            }],
            lookup_tables: vec![].into(),
            resources: f.owner.resources,
        },
        solana_sdk::hash::Hash::new_unique(),
        &darkbot_solana::tx::TxLimits::default(),
    )
    .unwrap();
    darkbot_solana::tx::sign(&mut built, &[&payer]).unwrap();
    let fingerprint = dependency_fingerprint(&f.owner.st, &[]);
    let candidate = Pubkey::new_unique();
    let identity = test_identity(&f.owner, candidate, &[]);
    let work = move || crate::orchestration::remote_workers::ProjectionWork {
        account: candidate,
        base_slot: 12,
        base_account_hash: solana_sdk::hash::Hash::default(),
        t_rx_ns: 0,
        tx: built.tx.clone(),
        targets: vec![Pubkey::new_unique()],
        dependencies: vec![],
        dependency_fingerprint: fingerprint,
        min_context_slot: 12,
        identity: identity.clone(),
    };
    (f, Box::new(work))
}

/// MT5a: a projection's candidate identity as `detect` would request it now — the owner's
/// current generations and the readset of `keys` (the dependency set) at this moment.
fn test_identity(
    owner: &ShadowOwner,
    account: Pubkey,
    keys: &[Pubkey],
) -> crate::execution::identity::CandidateIdentity {
    use crate::execution::identity::{CandidateIdentity, READ_SET_MAX_KEYS};
    CandidateIdentity {
        candidate_id: owner.next_candidate_id,
        account,
        trigger: darkbot_common::mt0::TriggerClass::AccountUpdate,
        lane: None,
        generations: owner.generations(),
        candidate_revision: 1,
        t_rx_first_ns: 0,
        t_rx_latest_ns: 0,
        trigger_slot: 12,
        evaluation: None,
        valid_until_ns: None,
        read_set: darkbot_project0::readset::ReadSet::observe_keys(
            keys.iter().copied().chain([account]),
            &owner.st,
            READ_SET_MAX_KEYS,
        ),
    }
}

/// MT5a: a Ready state of `accounts` on the fixture's owner (snapshot, clean reconcile, ready).
fn mt5_ready(f: &mut Fixture, accounts: &[AccountUpdate]) {
    f.owner.st.install_snapshot(12, accounts.to_vec());
    assert!(f.owner.st.reconcile(12, accounts).is_clean());
    assert!(f.owner.st.confirm_ready().is_tradable());
}

fn mt5_account(f: &Fixture, pubkey: Pubkey, data: Vec<u8>, source: SourceKind) -> AccountUpdate {
    AccountUpdate {
        pubkey,
        owner: f.owner.program,
        lamports: 1,
        data,
        slot: 12,
        write_version: None,
        source,
        t_rx_ns: 0,
    }
}

/// MT5a: a successful projection result of `candidate` (base slot 12, `base_hash`) bound to
/// `identity`, as the worker reports it.
fn mt5_result(
    candidate: Pubkey,
    base_hash: solana_sdk::hash::Hash,
    dependencies: Vec<Pubkey>,
    fingerprint: solana_sdk::hash::Hash,
    identity: crate::execution::identity::CandidateIdentity,
) -> crate::orchestration::remote_workers::ProjectionResultMessage {
    crate::orchestration::remote_workers::ProjectionResultMessage {
        account: candidate,
        base_slot: 12,
        base_account_hash: base_hash,
        t_rx_ns: 7,
        dependencies,
        dependency_fingerprint: fingerprint,
        identity,
        result: Ok((
            12,
            1_700_000_000,
            vec![(Pubkey::new_unique(), Account::default())],
            Some(5),
        )),
    }
}

/// MT5a: hands a result to the owner as the projection worker would and applies it.
fn mt5_apply(
    f: &mut Fixture,
    message: crate::orchestration::remote_workers::ProjectionResultMessage,
) {
    let account = message.account;
    {
        let d = f.results.as_ref().unwrap();
        assert!(crate::orchestration::remote_workers::push_result(
            &d.projection,
            message,
            &f.owner.shutdown,
            &d.space
        ));
    }
    f.owner.projection_pending.insert(account);
    f.owner.remote_apply().unwrap();
    assert!(!f.owner.projection_pending.contains(&account));
}

/// P6-MT.14 #6/#7 on the wired path (MT5a): a projection result bound to an older recovery
/// generation (a gap ended its Ready period; the recovered state is byte-identical) or to an
/// older program generation cannot commit — not cached, not requeued, no follow-up work,
/// counted `drop_old_generation` — while the same result at the current generation is
/// accepted and cached. The E9 identity checks it would have passed are not consulted.
#[test]
fn mt5_projection_result_of_an_older_generation_cannot_commit() {
    use crate::execution::identity::ResultAction;
    let mut f = fixture_with("mt5_generation", None);
    let candidate = Pubkey::new_unique();
    let data = vec![1u8; 8];
    let snapshot = vec![mt5_account(
        &f,
        candidate,
        data.clone(),
        SourceKind::Snapshot,
    )];
    mt5_ready(&mut f, &snapshot);
    let real = solana_sdk::hash::hash(&data);
    let fingerprint = dependency_fingerprint(&f.owner.st, &[]);
    let message = |identity| mt5_result(candidate, real, vec![], fingerprint, identity);
    let older = test_identity(&f.owner, candidate, &[]);
    assert_eq!(older.generations.recovery, 0);
    mt5_apply(&mut f, message(older.clone()));
    assert_eq!(f.owner.projection_stats.accepted, 1);
    assert!(f.owner.projection_cache.contains_key(&candidate));
    assert_eq!(f.owner.metrics.result_actions.get(ResultAction::Accept), 1);
    f.owner.projection_cache.clear();
    f.owner.detect_queue.clear();

    // The gap ends the Ready period; the recovery installs the identical state.
    f.owner.st.on_event(IngressEvent::Gap {
        source: SourceKind::WsRpcControl,
        from_slot: Some(12),
        to_slot: None,
        t_rx_ns: 0,
    });
    mt5_ready(&mut f, &snapshot);
    assert_eq!(f.owner.generations().recovery, 1);
    mt5_apply(&mut f, message(older.clone()));
    assert_eq!(
        f.owner
            .metrics
            .result_actions
            .get(ResultAction::DropOldGeneration),
        1
    );
    assert!(!f.owner.projection_cache.contains_key(&candidate));
    assert!(
        f.owner.detect_queue.is_empty(),
        "no requeue, no consumption"
    );
    assert!(!f.owner.projection_requeues.contains_key(&candidate));
    assert_eq!(f.owner.projection_stats.accepted, 1, "E9 never reached");
    assert_eq!(f.owner.projection_stats.requeued, 0);

    // The current generation commits again; a latched program upgrade drops it afterwards.
    let current = test_identity(&f.owner, candidate, &[]);
    assert_eq!(current.generations.recovery, 1);
    mt5_apply(&mut f, message(current.clone()));
    assert_eq!(f.owner.projection_stats.accepted, 2);
    f.owner.projection_cache.clear();
    f.owner.detect_queue.clear();
    f.owner.latch_program_upgrade();
    f.owner.latch_program_upgrade();
    assert_eq!(f.owner.generations().program, 1, "one latch per run");
    mt5_apply(&mut f, message(current));
    assert_eq!(
        f.owner
            .metrics
            .result_actions
            .get(ResultAction::DropOldGeneration),
        2
    );
    assert!(!f.owner.projection_cache.contains_key(&candidate));
    assert!(f.owner.detect_queue.is_empty());
    assert_eq!(f.owner.projection_stats.accepted, 2);
    let line = f.owner.identity_line();
    assert!(
        line.contains("recovery_generation=1 program_generation=1")
            && line.contains("accept=2 drop_old_generation=2"),
        "{line}"
    );
    cleanup(f);
}

/// P6-MT.14 #8 on the wired path (MT5a): a re-write of a key of the projection's readset (same
/// slot and bytes, no write_version) cannot commit — the key's epoch changed: counted
/// `recheck_current`, the result takes the E9 stale path (not cached, re-requested once from the
/// current state), and a write to a key outside the readset changes nothing. V2-06: the E9
/// fingerprint binds the same epoch now and sees the re-write as well (before: blind to it).
#[test]
fn mt5_projection_result_with_a_rewritten_readset_key_is_rechecked() {
    use crate::execution::identity::ResultAction;
    use darkbot_solana::state::ApplyOutcome;
    let mut f = fixture_with("mt5_readset", None);
    let (candidate, dep, other) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let data = vec![1u8; 8];
    let snapshot = vec![
        mt5_account(&f, candidate, data.clone(), SourceKind::Snapshot),
        mt5_account(&f, dep, vec![2; 8], SourceKind::Snapshot),
        mt5_account(&f, other, vec![3; 8], SourceKind::Snapshot),
    ];
    mt5_ready(&mut f, &snapshot);
    let real = solana_sdk::hash::hash(&data);
    let fingerprint = dependency_fingerprint(&f.owner.st, &[dep]);
    let identity = test_identity(&f.owner, candidate, &[dep]);
    let message = |identity| mt5_result(candidate, real, vec![dep], fingerprint, identity);
    // A key outside the readset: no effect.
    let mut rewrite = mt5_account(&f, other, vec![4; 8], SourceKind::WsRpcControl);
    rewrite.slot = 13;
    assert_eq!(
        f.owner.st.on_event(IngressEvent::Account(rewrite)),
        Some(ApplyOutcome::Applied)
    );
    mt5_apply(&mut f, message(identity.clone()));
    assert_eq!(f.owner.projection_stats.accepted, 1);
    assert_eq!(f.owner.metrics.result_actions.get(ResultAction::Accept), 1);
    f.owner.projection_cache.clear();
    f.owner.detect_queue.clear();
    // The identical re-write of `dep`: the readset epoch changed, and with it (V2-06) E9.
    assert_eq!(
        f.owner.st.on_event(IngressEvent::Account(mt5_account(
            &f,
            dep,
            vec![2; 8],
            SourceKind::WsRpcControl
        ))),
        Some(ApplyOutcome::Applied)
    );
    assert_ne!(
        dependency_fingerprint(&f.owner.st, &[dep]),
        fingerprint,
        "V2-06: E9 sees the same-slot re-write"
    );
    mt5_apply(&mut f, message(identity));
    assert_eq!(
        f.owner
            .metrics
            .result_actions
            .get(ResultAction::RecheckCurrent),
        1
    );
    assert_eq!(f.owner.projection_stats.accepted, 1);
    assert!(!f.owner.projection_cache.contains_key(&candidate));
    assert_eq!(f.owner.projection_stats.requeued, 1);
    assert_eq!(
        f.owner.detect_queue.back(),
        Some(&(candidate, 7, 12, "integration_projection_requeue"))
    );
    cleanup(f);
}

/// A failed projection as the projection worker reports it.
fn projection_failure() -> crate::orchestration::remote_workers::ProjectionResultMessage {
    let account = Pubkey::new_unique();
    crate::orchestration::remote_workers::ProjectionResultMessage {
        account,
        base_slot: 12,
        base_account_hash: solana_sdk::hash::Hash::default(),
        t_rx_ns: 0,
        dependencies: vec![],
        dependency_fingerprint: solana_sdk::hash::Hash::default(),
        identity: crate::execution::identity::CandidateIdentity {
            candidate_id: 0,
            account,
            trigger: darkbot_common::mt0::TriggerClass::AccountUpdate,
            lane: None,
            generations: Default::default(),
            candidate_revision: 1,
            t_rx_first_ns: 0,
            t_rx_latest_ns: 0,
            trigger_slot: 12,
            evaluation: None,
            valid_until_ns: None,
            read_set: Ok(Default::default()),
        },
        result: Err(crate::orchestration::remote_workers::ProjectionFailure {
            context_behind: false,
            error: "A23".into(),
        }),
    }
}

/// A worker thread pushing `n` results through the production `push_result` (it waits on the
/// full queue's space doorbell), pausing `gap` after each; the push instants.
fn result_worker(
    f: &Fixture,
    n: usize,
    gap: Duration,
) -> std::thread::JoinHandle<Vec<std::time::Instant>> {
    let (tx, space) = f
        .results
        .as_ref()
        .map(|d| (d.projection.clone(), d.space.clone()))
        .unwrap();
    let stop = f.owner.shutdown.clone();
    std::thread::spawn(move || {
        (0..n)
            .map(|_| {
                assert!(crate::orchestration::remote_workers::push_result(
                    &tx,
                    projection_failure(),
                    &stop,
                    &space
                ));
                let pushed = std::time::Instant::now();
                std::thread::sleep(gap);
                pushed
            })
            .collect()
    })
}

/// Production loop passes (`remote_apply`, then `events` with its doorbell wait) until `done`
/// holds after a `remote_apply` (the loop's next stage); the time it took. No snapshot fetch is
/// reachable without an event.
fn run_until(f: &mut Fixture, done: impl Fn(&ShadowOwner) -> bool) -> Duration {
    let mut fetch = || -> Result<SnapshotData, String> { Err("A23_NO_FETCH".into()) };
    run_until_with(f, &mut fetch, done)
}

/// `run_until` with a snapshot fetch for the owner's recovery.
fn run_until_with(
    f: &mut Fixture,
    fetch: &mut dyn FnMut() -> Result<SnapshotData, String>,
    done: impl Fn(&ShadowOwner) -> bool,
) -> Duration {
    let mut fetch = || fetch();
    let t = std::time::Instant::now();
    loop {
        f.owner.remote_apply().unwrap();
        if done(&f.owner) {
            return t.elapsed();
        }
        assert!(t.elapsed() < Duration::from_secs(10));
        f.owner.events(&mut fetch, &f.ingress).unwrap();
    }
}

fn cleanup(f: Fixture) {
    let dir = f.dir.clone();
    drop(f);
    let _ = std::fs::remove_dir_all(dir);
}

/// A23-01 through the real projection worker: with no ingress at all each completion arrives
/// while the idle owner sleeps and wakes it through the doorbell (signalled wakeups; the old
/// owner waited on ingress only and had none). Throughput is the provider budget's, not tested.
#[test]
fn a23_01_worker_completion_wakes_the_idle_owner_without_ingress() {
    const N: usize = 8;
    let (mut f, work) = a23_fixture("a23_done", N);
    for _ in 0..N {
        let before = f.owner.projection_stats.failed;
        assert!(f.owner.remote.projection_jobs.try_push(work()).is_ok());
        run_until(&mut f, |o| o.projection_stats.failed > before);
    }
    assert_eq!(f.owner.consumer.stats().sent, 0, "no ingress event at all");
    assert_eq!(
        f.owner.projection_stats.failed, N as u64,
        "each result once"
    );
    let woken = f.owner.metrics.owner_wakeup_ns.snapshot().count as usize;
    assert!(woken >= N / 2, "the results woke the owner: {woken}");
    cleanup(f);
}

/// A23-01 measurement: 1000 completions at 0.2–1 ms gaps, owner idle in between. Every result
/// is applied within a bounded time of its push (old owner: up to its 250-ms ingress wait);
/// prints owner_wakeup_ns (signal → owner running) and push → applied.
#[test]
fn a23_01_owner_wakeup_latency_is_measured() {
    const N: usize = 1000;
    let mut f = fixture_with("a23_measure", None);
    let mut fetch = || -> Result<SnapshotData, String> { Err("A23_NO_FETCH".into()) };
    let gaps = [200u64, 500, 1000];
    let worker = {
        let (tx, space) = f
            .results
            .as_ref()
            .map(|d| (d.projection.clone(), d.space.clone()))
            .unwrap();
        let stop = f.owner.shutdown.clone();
        std::thread::spawn(move || {
            (0..N)
                .map(|i| {
                    assert!(crate::orchestration::remote_workers::push_result(
                        &tx,
                        projection_failure(),
                        &stop,
                        &space
                    ));
                    let pushed = std::time::Instant::now();
                    std::thread::sleep(Duration::from_micros(gaps[i % gaps.len()]));
                    pushed
                })
                .collect::<Vec<_>>()
        })
    };
    let mut applied = Vec::with_capacity(N);
    let t = std::time::Instant::now();
    while applied.len() < N {
        f.owner.remote_apply().unwrap();
        let now = std::time::Instant::now();
        applied.resize(f.owner.projection_stats.failed as usize, now);
        assert!(t.elapsed() < Duration::from_secs(30));
        if applied.len() < N {
            f.owner.events(&mut fetch, &f.ingress).unwrap();
        }
    }
    let pushed = worker.join().unwrap();
    let mut lag: Vec<Duration> = pushed
        .iter()
        .zip(&applied)
        .map(|(p, a)| a.saturating_duration_since(*p))
        .collect();
    lag.sort();
    let h = f.owner.metrics.owner_wakeup_ns.snapshot();
    eprintln!(
        "A23-01 owner_wakeup_ns n={} p50={:?} p95={:?} p99={:?} max={:?}; push->applied p50={:?} p95={:?} p99={:?} max={:?}",
        h.count, h.p50, h.p95, h.p99, h.max,
        lag[N / 2], lag[N * 95 / 100], lag[N * 99 / 100], lag[N - 1]
    );
    assert!(h.count as usize >= N / 2, "the owner slept between results");
    assert!(
        lag[N * 99 / 100] < Duration::from_millis(50),
        "{:?}",
        &lag[N * 99 / 100..]
    );
    cleanup(f);
}

/// A23-01: four workers completing concurrently (queue capacity 16, so they also wait on the
/// full queue) plus an ingress event of a new session generation (the producer clone a
/// respawned session gets): every result is applied exactly once, the event is received, no
/// element is dropped and nothing waits for the idle tick.
#[test]
fn a23_01_concurrent_completions_and_a_new_generation_wake_the_owner() {
    const WORKERS: usize = 4;
    const EACH: usize = 50;
    let mut f = fixture_with("a23_many", None);
    let workers: Vec<_> = (0..WORKERS)
        .map(|_| result_worker(&f, EACH, Duration::from_micros(100)))
        .collect();
    let next_generation = f.owner.producer.clone();
    let ingress = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(2));
        assert!(next_generation
            .try_push(IngressEvent::Slot(darkbot_solana::ingress::SlotUpdate {
                slot: 13,
                parent: None,
                status: darkbot_solana::ingress::SlotStatus::Processed,
                source: SourceKind::WsRpcControl,
                t_rx_ns: 0,
            }))
            .is_ok());
    });
    let all = run_until(&mut f, |o| {
        o.projection_stats.failed == (WORKERS * EACH) as u64 && o.consumer.stats().received == 1
    });
    for w in workers {
        assert_eq!(w.join().unwrap().len(), EACH);
    }
    ingress.join().unwrap();
    let s = f.owner.remote.projection_results.stats();
    assert_eq!(f.owner.projection_stats.failed, (WORKERS * EACH) as u64);
    assert_eq!((s.sent, s.received, s.dropped), (200, 200, 0));
    assert_eq!(f.owner.consumer.stats().received, 1);
    assert!(all < Duration::from_millis(500), "{all:?}");
    cleanup(f);
}

/// A23-01/A23-08: a saturated result queue — the worker blocks on the full queue (overflow
/// counted, nothing dropped) and resumes on the owner's pops, so all results arrive in well
/// under one 1-s liveness bound of its wait.
#[test]
fn a23_08_saturated_result_queue_resumes_on_owner_pops() {
    const QUEUE: u64 = 16;
    const MORE: u64 = 4;
    let mut f = fixture_with("a23_full", None);
    let worker = result_worker(&f, (QUEUE + MORE) as usize, Duration::ZERO);
    let t = std::time::Instant::now();
    while f.owner.remote.projection_results.stats().overflow == 0 {
        assert!(t.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    assert_eq!(f.owner.remote.projection_results.stats().sent, QUEUE);
    let drain = run_until(&mut f, |o| o.projection_stats.failed == QUEUE + MORE);
    worker.join().unwrap();
    let s = f.owner.remote.projection_results.stats();
    assert_eq!((s.sent, s.received, s.dropped), (20, 20, 0));
    assert!(s.overflow >= 1);
    assert!(drain < Duration::from_millis(200), "{drain:?}");
    cleanup(f);
}

/// A23-01 control: a shutdown request and an econ abort each wake the idle owner at once; the
/// loop's lifecycle check then ends it.
#[test]
fn a23_01_shutdown_and_abort_wake_the_idle_owner() {
    let mut f = fixture("a23_stop");
    let own = f.owner.wake.clone();
    let (handle, _evidence) = crate::analytics::econ::test_handle(16);
    f.owner.wake = handle.shared.owner_wake.clone();
    f.owner.econ = Some(handle.clone());
    let shared = handle.shared.clone();
    let abort = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        shared.request_abort();
    });
    let aborted = run_until(&mut f, |o| o.stop_pending());
    abort.join().unwrap();
    // The fixture's own doorbell again (registered on its shutdown).
    f.owner.econ = None;
    f.owner.wake = own;
    let stop = f.owner.shutdown.clone();
    let request = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        stop.request();
    });
    let stopped = run_until(&mut f, |o| o.stop_pending());
    request.join().unwrap();
    assert!(!f.owner.lifecycle(), "the stop ends the loop");
    for d in [aborted, stopped] {
        assert!(d < Duration::from_millis(200), "{d:?}");
    }
    let woken = f.owner.metrics.owner_wakeup_ns.snapshot().count;
    assert!(woken >= 2);
    // The ordered end journals the wakeup quantiles once.
    use crate::analytics::payload::{decode, OwnerDiagnostic, Payload};
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut lines = vec![];
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == OwnerDiagnostic::KIND {
            let d: OwnerDiagnostic = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            lines.extend(d.line.starts_with("owner_wakeup_ns: ").then_some(d.line));
        }
    }
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with(&format!("owner_wakeup_ns: n={woken} p50=")));
    let _ = std::fs::remove_dir_all(dir);
}

/// A23-01: an open R127 sweep batch ends the idle wait at its age bound (no 1-ms poll); a batch
/// that cannot flush (state not tradable) never turns its past deadline into a busy loop — the
/// wait is the housekeeping tick.
#[test]
fn a23_01_open_sweep_batch_bounds_the_idle_wait_by_its_age() {
    let mut f = fixture("a23_sweep");
    let mut fetch = || -> Result<SnapshotData, String> { Err("A23_NO_FETCH".into()) };
    let mut wait = |f: &mut Fixture| {
        let t = std::time::Instant::now();
        f.owner.events(&mut fetch, &f.ingress).unwrap();
        t.elapsed()
    };
    f.owner
        .sweep_dirty_banks
        .insert(Pubkey::new_unique(), (0, 12));
    f.owner.sweep_batch_started_ns = Some(0);
    let idle = wait(&mut f);
    assert!(idle >= Duration::from_millis(200), "no spin: {idle:?}");
    let snapshot = vec![AccountUpdate {
        pubkey: Pubkey::new_unique(),
        owner: f.owner.program,
        lamports: 1,
        data: vec![1; 8],
        slot: 12,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    }];
    f.owner.st.install_snapshot(12, snapshot.clone());
    f.owner.st.reconcile(12, &snapshot);
    f.owner.st.confirm_ready();
    f.owner.sweep_batch_started_ns = Some(f.owner.clock.now_ns());
    let batch = wait(&mut f);
    assert!(
        batch >= Duration::from_millis(4) && batch < Duration::from_millis(100),
        "the 5-ms batch age ends the wait: {batch:?}"
    );
    cleanup(f);
}

type ControlTx =
    darkbot_common::channel::Producer<crate::orchestration::source_control::ControlMessage>;
type LoopbackSeen = Arc<Mutex<darkbot_solana::ingress::geyser_loopback::Seen>>;

fn loopback_unit() -> darkbot_solana::ingress::sources::SourceUnit {
    darkbot_solana::ingress::sources::SourceUnit::BinaryGeyser {
        provider: "loopback".into(),
    }
}

/// S1-02: the owner's sources with the Geyser unit `geyser:loopback` in `mode` against a
/// loopback gRPC server (one write of a new account at slot 50 and a processed slot, then an
/// open, pinging stream); the owner's control queue, the server log and the account.
fn with_loopback_geyser(
    f: &mut Fixture,
    mode: darkbot_solana::ingress::sources::OperatingMode,
) -> (ControlTx, LoopbackSeen, Pubkey) {
    use darkbot_solana::ingress::geyser::{frame, proto, BinaryGeyserSource, GeyserConfig};
    use darkbot_solana::ingress::geyser_grpc::GrpcTransport;
    use darkbot_solana::ingress::geyser_loopback::{serve, Reply};
    use darkbot_solana::ingress::sources::{Qualification, SourceArbiter, SourceControl};
    use proto::subscribe_update::UpdateOneof as U;
    let k = Pubkey::new_unique();
    let account = frame(&proto::SubscribeUpdate {
        filters: vec![],
        update_oneof: Some(U::Account(proto::SubscribeUpdateAccount {
            account: Some(proto::SubscribeUpdateAccountInfo {
                pubkey: k.to_bytes().to_vec(),
                lamports: 1,
                owner: f.owner.program.to_bytes().to_vec(),
                data: vec![1; 8],
                write_version: 1,
                ..Default::default()
            }),
            slot: 50,
            is_startup: false,
            bank_id: None,
        })),
    });
    let slot = frame(&proto::SubscribeUpdate {
        filters: vec![],
        update_oneof: Some(U::Slot(proto::SubscribeUpdateSlot {
            slot: 50,
            parent: Some(49),
            status: proto::SlotStatus::SlotProcessed as i32,
            dead_error: None,
            bank_id: None,
        })),
    });
    let (url, seen) = serve(Reply::Hold(vec![account, slot]));
    let clock = f.owner.clock;
    let spawn: sources::GeyserSpawner =
        Box::new(move |_control, sub, sink, stop, halt, slots, _canonical| {
            let transport =
                GrpcTransport::new(&url, None, Duration::from_secs(2), Duration::from_secs(2))
                    .map_err(|_| "LOOPBACK_TRANSPORT".to_string())?;
            let src = BinaryGeyserSource {
                config: GeyserConfig {
                    endpoint: url.clone(),
                    token_env: "",
                    commitment: proto::CommitmentLevel::Processed,
                    max_message_bytes: 1 << 20,
                    max_reconnects: 0,
                    reconnect_initial: Duration::from_millis(1),
                    reconnect_max: Duration::from_millis(1),
                    replay_from_slot: false,
                },
                subscription: sub,
                transport,
                clock,
            };
            Ok(std::thread::spawn(move || {
                ingress::run_ingress(src, sink, stop, halt, slots, None)
            }))
        });
    let arbiter = SourceArbiter::new(&[
        crate::orchestration::source_control::control_of(
            darkbot_solana::ingress::sources::SourceUnit::WsRpcControl,
            darkbot_solana::ingress::sources::OperatingMode::Active,
        )
        .unwrap(),
        SourceControl::new(loopback_unit(), mode, Qualification::NotRun).unwrap(),
    ])
    .unwrap();
    let (tx, rx) = bounded("source-control", 8);
    let tx = tx.with_wakeup(&f.owner.wake);
    f.owner.sources = sources::OwnerSources::new(
        arbiter,
        Some(loopback_unit()),
        spawn,
        64,
        &f.owner.wake,
        Some(rx),
    )
    .record_decisions_in(f.dir.join("source-control"), 0);
    f.owner.start_sources().unwrap();
    (tx, seen, k)
}

/// S1-02 OFF / OBSERVE through the real gRPC transport (loopback): OFF constructs and connects
/// nothing; OBSERVE connects once with the owner's subscription and is measured (StreamReady,
/// account, slot; healthy) — its write never reaches state, nothing enters the canonical queue,
/// the owner's halt stays down.
#[test]
fn s1_02_geyser_off_connects_nothing_and_observe_never_feeds_state() {
    use darkbot_solana::ingress::sources::OperatingMode;
    let mut f = fixture("s102_off");
    let (_control, seen, _) = with_loopback_geyser(&mut f, OperatingMode::Off);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(seen.lock().unwrap().calls, 0, "OFF never connects");
    assert!(f
        .owner
        .sources
        .line()
        .contains("canonical=wss observing=none"));
    cleanup(f);
    let mut f = fixture("s102_observe");
    let (_control, seen, k) = with_loopback_geyser(&mut f, OperatingMode::Observe);
    run_until(&mut f, |o| o.sources.stats.observed >= 3);
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.calls, 1);
        let request = seen.request.as_ref().unwrap();
        assert!(!request.accounts.is_empty(), "the owner's program filters");
    }
    assert!(
        f.owner.st.get(&k).is_none(),
        "an observed write never reaches state"
    );
    assert_eq!(
        f.owner.consumer.stats().sent,
        0,
        "nothing in the canonical queue"
    );
    assert!(f.owner.sources.health.healthy());
    assert_eq!(f.owner.sources.health.last_slot, Some(50));
    assert!(!f.owner.halt.is_raised());
    assert!(f.owner.sources.line().contains("observing=geyser:loopback"));
    // P5 S1-RACE: the canonical WSS copy of the observed write is measured: Geyser led.
    let t = f.owner.clock.now_ns();
    f.producer
        .try_push(IngressEvent::Account(AccountUpdate {
            pubkey: k,
            owner: f.owner.program,
            lamports: 1,
            data: vec![1; 8],
            slot: 50,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: t,
        }))
        .unwrap();
    run_until(&mut f, |o| o.sources.arbiter.leads.samples >= 1);
    let leads = f.owner.sources.arbiter.leads.snapshot();
    assert_eq!((leads.ahead_ns.count, leads.behind_ns.count), (1, 0));
    assert!(f.owner.sources.line().contains("lead_samples=1"));
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

/// S1-02 runtime control through the owner's control queue (as `p0ctl source set` delivers it):
/// OBSERVE→OFF ends the session; ACTIVE beside the canonical WSS unit is refused without any
/// change; WSS OFF revokes canonical authority (halted, no canonical unit — no feed alone lets
/// trading run); Geyser ACTIVE (a test-only qualification; production refuses ACTIVE) starts a
/// new canonical generation that trades only after its StreamReady drove a recovery, and its
/// write reaches state; a revoked qualification halts at once.
#[test]
fn s1_02_runtime_control_toggles_and_hands_canonical_authority_over() {
    use crate::orchestration::source_control::{control_of, ControlMessage};
    use darkbot_solana::ingress::sources::{
        OperatingMode, Qualification, SourceControl, SourceUnit,
    };
    let mut f = fixture("s102_handoff");
    let (control, seen, k) = with_loopback_geyser(&mut f, OperatingMode::Observe);
    let good = snapshot(&f, false);
    let mut fetch = || {
        Ok(SnapshotData {
            slot: good.slot,
            accounts: good.accounts.clone(),
            missing_required: vec![],
        })
    };
    f.producer.try_push(reconnected()).unwrap();
    run_until_with(&mut f, &mut fetch, |o| {
        o.st.run_state().is_tradable() && o.sources.stats.observed >= 3
    });
    assert!(f.owner.st.get(&k).is_none());
    let apply = |seq, control| ControlMessage::Apply { seq, control };
    let test_active = || {
        SourceControl::new(
            loopback_unit(),
            OperatingMode::Active,
            Qualification::Qualified {
                evidence: "TEST_ONLY_LOOPBACK".into(),
            },
        )
        .unwrap()
    };
    assert!(control
        .try_push(apply(
            1,
            control_of(loopback_unit(), OperatingMode::Off).unwrap()
        ))
        .is_ok());
    run_until_with(&mut f, &mut fetch, |o| {
        o.sources.stats.controls_applied == 1
    });
    assert!(f.owner.sources.line().contains("observing=none"));
    assert!(control.try_push(apply(2, test_active())).is_ok());
    run_until_with(&mut f, &mut fetch, |o| {
        o.sources.stats.controls_refused == 1
    });
    assert!(f.owner.sources.line().contains("canonical=wss"));
    // P0-PROD-CTRL-04/07: the projection carries every unit's applied control (mode,
    // qualification, generation, canonical) and the owner's last decision with its reason.
    let s = f.owner.owner_snapshot(1);
    let geyser = s
        .sources
        .iter()
        .find(|u| u.unit == "geyser:loopback")
        .unwrap();
    assert_eq!((geyser.mode, geyser.canonical), ("off", false));
    assert_eq!(geyser.generation, 2, "one applied change");
    assert!(s
        .sources
        .iter()
        .any(|u| u.unit == "wss" && u.canonical && u.mode == "active"));
    let decision = s.last_source_decision.clone().unwrap();
    assert_eq!((decision.seq, decision.applied), (2, false));
    assert!(!decision.detail.is_empty());
    assert_eq!((s.source_stats.applied, s.source_stats.refused), (1, 1));
    assert!(f.owner.st.run_state().is_tradable());
    assert!(control
        .try_push(apply(
            3,
            control_of(SourceUnit::WsRpcControl, OperatingMode::Off).unwrap()
        ))
        .is_ok());
    run_until_with(&mut f, &mut fetch, |o| {
        o.sources.stats.controls_applied == 2
    });
    assert!(!f.owner.st.run_state().is_tradable());
    assert!(f.owner.sources.line().contains("canonical=none"));
    let generation = f.owner.ingress_session.generation;
    assert!(control.try_push(apply(4, test_active())).is_ok());
    run_until_with(&mut f, &mut fetch, |o| {
        o.sources.stats.controls_applied == 3
    });
    assert_eq!(f.owner.ingress_session.generation, generation + 1);
    run_until_with(&mut f, &mut fetch, |o| {
        o.st.run_state().is_tradable() && o.st.get(&k).is_some()
    });
    assert_eq!(f.owner.st.get(&k).unwrap().source, SourceKind::Geyser);
    assert!(f.owner.sources.line().contains("canonical=geyser:loopback"));
    assert_eq!(
        seen.lock().unwrap().calls,
        2,
        "the observer, then the canonical session"
    );
    let revoked = SourceControl {
        unit: loopback_unit(),
        mode: OperatingMode::Active,
        qualification: Qualification::Blocked("QUALIFICATION_REVOKED"),
    };
    assert!(control.try_push(apply(5, revoked)).is_ok());
    run_until_with(&mut f, &mut fetch, |o| {
        o.sources.stats.controls_applied == 4
    });
    assert!(!f.owner.st.run_state().is_tradable());
    assert!(f.owner.halt.is_raised());
    assert!(f.owner.sources.line().contains("canonical=none"));
    // A request the control thread refused reaches the owner as such and is recorded too.
    assert!(control
        .try_push(ControlMessage::Refused {
            seq: 6,
            request: "wss on".into(),
            reason: "SOURCE_MODE_INVALID".into(),
        })
        .is_ok());
    run_until_with(&mut f, &mut fetch, |o| {
        o.sources.stats.controls_refused == 2
    });
    // P5 S1-02: every decision is recorded; the newest one is the applied state.
    let decisions = f.dir.join("source-control");
    let decided = |seq: u64| {
        crate::analytics::journal::read_typed_value::<
            crate::analytics::payload::SourceControlOutcome,
        >(&decisions.join(format!("outcome_{seq:08}.p0j")))
        .unwrap()
    };
    assert_eq!(
        (decided(1).applied, decided(1).request.as_str()),
        (true, "geyser:loopback off")
    );
    assert_eq!(
        (decided(2).applied, decided(2).detail.as_str()),
        (false, "ARBITER_SECOND_CANONICAL_SOURCE")
    );
    assert_eq!(
        (decided(5).applied, decided(5).detail.as_str()),
        (true, "canonical_revoked")
    );
    let newest = crate::orchestration::source_control::latest_outcome(&decisions)
        .unwrap()
        .unwrap();
    assert_eq!(
        (newest.seq, newest.applied, newest.request.as_str()),
        (6, false, "wss on")
    );
    assert_eq!(newest.detail, "SOURCE_MODE_INVALID");
    assert_eq!(newest.owner_pid, std::process::id());
    // CTRL-03: the projected configuration version is the newest decided request (refused ones
    // included), the CAS a GUI change states.
    assert_eq!(f.owner.sources.config_version, Some(6));
    // Journaled order: trading resumed after the grant only through a recovery.
    use crate::analytics::payload::{decode, OwnerDiagnostic, Payload};
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut lines = vec![];
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == OwnerDiagnostic::KIND {
            let d: OwnerDiagnostic = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            lines.push(d.line);
        }
    }
    let at = |p: &str| lines.iter().position(|l| l.starts_with(p)).unwrap();
    let (granted, revoked) = (
        at("source_control: applied seq=4"),
        at("source_control: applied seq=5"),
    );
    assert!(lines[granted].contains("canonical_granted_recovery_required"));
    assert!(lines[revoked].contains("canonical_revoked"));
    assert!(
        lines[granted..revoked]
            .iter()
            .any(|l| l.starts_with("recovery: Ready")),
        "{lines:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// C3: the detached fixture started from an FE-04 journal in `phase` (`"unknown"`: handed off;
/// `"finalized"`: proven not landed, not yet booked) through the production startup replay; the
/// attempt's intent and a DEC-03 non-landing read (B+C) that proves it.
fn fe04_fixture(
    tag: &str,
    phase: &str,
) -> (
    Fixture,
    crate::execution::one_shot::Intent,
    crate::execution::one_shot::non_landing::NonLandingRead,
) {
    fe04_fixture_held(tag, phase, None)
}

/// `fe04_fixture` with the attempt reserved with the holds `hold` (DEC-04).
fn fe04_fixture_held(
    tag: &str,
    phase: &str,
    hold: Option<crate::execution::hold_b::HoldEvidence>,
) -> (
    Fixture,
    crate::execution::one_shot::Intent,
    crate::execution::one_shot::non_landing::NonLandingRead,
) {
    let mut f = fixture_with(tag, None);
    let (wallet, dir) = (f.owner.liquidator.to_string(), f.dir.join("fe04"));
    let intent = crate::execution::one_shot::test_unknown_attempt_held(&dir, &wallet, hold);
    let proof = crate::execution::one_shot::non_landing::loopback::proving(
        &crate::execution::one_shot::settlement::SettlementSubject::from(&intent)
            .non_landing()
            .unwrap(),
        true,
    );
    if phase == "finalized" {
        let mut j = crate::execution::one_shot::Journal::open(&dir, &wallet).unwrap();
        j.reconcile_not_landed(&proof).unwrap();
    }
    let dir_s = dir.display().to_string();
    let start = setup::reservation_book(
        &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
        &f.owner.liquidator,
    )
    .unwrap()
    .start(&mut f.owner.safety, wall_unix_s().unwrap())
    .unwrap();
    f.owner.reservations = start.book;
    f.owner.fe04_journal = start.dir;
    f.owner.fe04_held = start.held;
    f.owner.fe04_settle_due = start.settle_due;
    f.owner.fe04_unbooked = start.unbooked;
    (f, intent, proof)
}

/// The settlement work the owner handed to the (detached) worker.
fn fe04_job(f: &Fixture) -> Option<crate::orchestration::remote_workers::Fe04SettlementWork> {
    f.results.as_ref().unwrap().fe04_jobs.try_pop()
}

/// The worker's answer through the production result queue (it rings the owner's doorbell).
fn fe04_answer(f: &Fixture, evidence: crate::execution::one_shot::settlement::SettlementEvidence) {
    fe04_answer_for(f, evidence, None)
}

/// The worker's answer for `exit` (`None`: the attempt).
fn fe04_answer_for(
    f: &Fixture,
    evidence: crate::execution::one_shot::settlement::SettlementEvidence,
    exit: Option<&str>,
) {
    let d = f.results.as_ref().unwrap();
    assert!(crate::orchestration::remote_workers::push_result(
        &d.fe04_results,
        crate::orchestration::remote_workers::Fe04SettlementResult {
            evidence,
            exit: exit.map(str::to_string),
        },
        &f.owner.shutdown,
        &d.space
    ));
}

fn fe04_settled(o: &ShadowOwner) -> bool {
    o.fe04_held
        .as_ref()
        .and_then(|j| j.safety_replay())
        .is_some_and(|r| r.settled.is_some())
}

/// C3 in the running owner: an UNKNOWN attempt from the journal holds its capital and book; the
/// owner hands the read to the settlement worker, applies the proven non-landing, books it into
/// Safety exactly once (capital released, one failed attempt, nothing realized), empties the book
/// from the journal; a repeated answer and later passes book nothing again, no new read is sent;
/// a restart replays the settled journal with nothing held and nothing to settle.
#[test]
fn c3_owner_settles_a_proven_non_landing_exactly_once() {
    use crate::execution::one_shot::settlement::SettlementEvidence;
    let (mut f, intent, proof) = fe04_fixture("fe04_once", "unknown");
    assert_eq!(f.owner.safety.capital_in_use(), intent.capital_usd_micro);
    assert_eq!(f.owner.reservations.held("SOL"), intent.holds["SOL"]);
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    let job = fe04_job(&f).expect("the read is handed to the worker");
    assert_eq!(job.subject.signatures, intent.signatures);
    assert_eq!(job.observed, Default::default());
    let evidence = SettlementEvidence {
        observations: vec![],
        non_landing: Some(Ok(proof)),
    };
    fe04_answer(&f, evidence.clone());
    run_until(&mut f, fe04_settled);
    let day = f.owner.safety.day();
    assert_eq!(f.owner.safety.capital_in_use(), 0, "released once");
    assert_eq!((day.realized_pnl, day.failed_attempts), (0, 1));
    assert!(
        f.owner.reservations.is_empty(),
        "the book follows the journal"
    );
    assert!(f.owner.safety.halted().is_some(), "the one-shot HALT stays");
    // A repeated answer and a due step change nothing and send no read.
    fe04_answer(&f, evidence);
    run_until(&mut f, |o| !o.remote.fe04_settlement_results.pending());
    f.owner.fe04_settle_due = Some(std::time::Instant::now());
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_none());
    assert_eq!(f.owner.safety.day(), day);
    assert_eq!(f.owner.safety.capital_in_use(), 0);
    // Restart: the settled journal holds nothing and needs no settlement.
    f.owner.fe04_held = None;
    let mut safety = Safety::new(e2e_limits(), Allow::default(), false);
    let dir_s = f.dir.join("fe04").display().to_string();
    let again = setup::reservation_book(
        &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
        &f.owner.liquidator,
    )
    .unwrap()
    .start(&mut safety, wall_unix_s().unwrap())
    .unwrap();
    assert!(again.book.is_empty() && again.settle_due.is_none());
    assert_eq!((again.unbooked, safety.capital_in_use()), (None, 0));
    cleanup(f);
}

/// DEC-11 in the running owner: JITO_BUNDLE starts OFF (no Jito adapter); a request without a
/// configured bundle path stays OFF; configured, OBSERVE builds the adapter (bundle plans,
/// the tip account) and ACTIVE stays OBSERVE without a qualified bundle simulator; OFF removes
/// it. The capability row shows requested, effective and why.
#[test]
fn dec11_owner_builds_the_bundle_path_through_the_capability() {
    use darkbot_solana::submit::SubmissionAdapter;
    let mut f = fixture_with("dec11_owner", None);
    let request = |requested: &str, seq| crate::analytics::payload::CapabilityControlRequest {
        seq,
        capability: crate::execution::bundle_path::JITO_BUNDLE.into(),
        requested: requested.into(),
        requested_unix_s: 1,
        request_id: format!("t-{seq}"),
        owner_epoch: 0,
    };
    let row = |f: &Fixture| {
        let r = f
            .owner
            .owner_snapshot(0)
            .capabilities
            .into_iter()
            .find(|c| c.name == crate::execution::bundle_path::JITO_BUNDLE)
            .unwrap();
        (r.requested, r.effective, r.block_reason.into_owned())
    };
    let exp = |requested, effective, reason: &str| (requested, effective, reason.to_string());
    assert_eq!(row(&f), exp("OFF", "OFF", ""));
    assert!(!f.owner.submitter.supports_bundle());
    f.owner
        .apply_capability_control(1, Ok(request("OBSERVE", 1)));
    assert_eq!(
        row(&f),
        exp(
            "OBSERVE",
            "OFF",
            crate::execution::bundle_path::JITO_UNCONFIGURED
        )
    );
    assert!(!f.owner.submitter.supports_bundle());
    f.owner.bundle_config = Some(crate::execution::bundle_path::BundleConfig {
        block_engine_url: "https://jito.invalid".into(),
        bundle_sim_rpc_url: None,
        tip: crate::execution::bundle_path::BundleTip {
            account: Pubkey::new_unique(),
            lamports: 1_000,
        },
    });
    f.owner
        .apply_capability_control(2, Ok(request("OBSERVE", 2)));
    assert_eq!(row(&f), exp("OBSERVE", "OBSERVE", ""));
    assert!(f.owner.submitter.supports_bundle());
    f.owner
        .apply_capability_control(3, Ok(request("ACTIVE", 3)));
    assert_eq!(
        row(&f),
        exp(
            "ACTIVE",
            "OBSERVE",
            crate::execution::bundle_path::BUNDLE_SIMULATOR_UNQUALIFIED
        )
    );
    f.owner.apply_capability_control(4, Ok(request("OFF", 4)));
    assert_eq!(row(&f), exp("OFF", "OFF", ""));
    assert!(!f.owner.submitter.supports_bundle());
    cleanup(f);
}

/// DEC-04 (OBSERVE) in the running owner: a settled landed attempt is one typed `HoldFlows`
/// record — A's wallet reservation and inventory hold, B and the finalized deltas, each checked
/// per asset — and the journals' B evidence; with N decided an ACTIVE request becomes effective
/// from that evidence, without N it stays OBSERVE with the B-8 reason.
#[test]
fn dec04_owner_records_the_holds_and_gates_active_on_the_evidence() {
    use crate::analytics::payload::{decode, Payload};
    use crate::execution::hold_b::{AssetFlow, BHold, HoldEvidence, HoldMode, PlanFlows};
    let hold = HoldEvidence {
        mode: HoldMode::Observe,
        a_inventory_usd_micro: 1_100,
        b: BHold::Exact(PlanFlows {
            flows: [(
                "SOL".to_string(),
                AssetFlow {
                    reserve: 20,
                    residual_max: Some(0),
                },
            )]
            .into(),
            inventory_usd_micro: 0,
            kind: "PLAIN".into(),
        }),
        reserved_inventory_usd_micro: 1_100,
    };
    let (mut f, intent, _) = fe04_fixture_held("dec04_owner", "unknown", Some(hold.clone()));
    install_sol_price(&mut f);
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some());
    fe04_answer(&f, exit_observation(&intent.signatures[0], true, -20, &[]));
    run_until(&mut f, fe04_settled);
    // The journals' evidence: one exact B without an underestimation.
    f.owner.fe04_refresh = std::time::Duration::ZERO;
    assert!(f.owner.lifecycle());
    assert_eq!(f.owner.hold_gate.evidence.exact, 1);
    let request = |requested: &str, seq| crate::analytics::payload::CapabilityControlRequest {
        seq,
        capability: crate::execution::hold_b::HOLD_B.into(),
        requested: requested.into(),
        requested_unix_s: 1,
        request_id: format!("t-{seq}"),
        owner_epoch: 0,
    };
    f.owner
        .apply_capability_control(1, Ok(request("ACTIVE", 1)));
    assert_eq!(f.owner.hold_gate.mode, HoldMode::Observe);
    assert_eq!(
        f.owner.sources.capabilities.hold_b_block,
        crate::execution::hold_b::B8_UNDECIDED
    );
    // N decided later: a refresh never promotes on its own (only a recorded request does).
    f.owner.hold_gate.n = Some(1);
    assert!(f.owner.lifecycle());
    assert_eq!(f.owner.hold_gate.mode, HoldMode::Observe);
    f.owner
        .apply_capability_control(2, Ok(request("ACTIVE", 2)));
    assert_eq!(f.owner.hold_gate.mode, HoldMode::Active);
    // The projection: the settled attempt's A and B rows, the reserved value, its inventory mark.
    let h = f.owner.owner_snapshot(0).wallets.hold_b.unwrap();
    let a = h.attempt.unwrap();
    assert!(a.settled);
    assert_eq!(
        (a.reserved_inventory_usd_micro, a.actual_inventory_usd_micro),
        (1_100, Some(0))
    );
    assert_eq!(
        (
            a.flows[0].asset.as_str(),
            a.flows[0].actual,
            a.a_flows[0].actual
        ),
        ("SOL", Some(-20), Some(-20))
    );
    // An underestimation in the evidence takes ACTIVE back at the next refresh.
    f.owner.hold_gate.evidence.underestimated = 1;
    f.owner.refresh_hold_gate(false);
    assert_eq!(
        (
            f.owner.hold_gate.mode,
            f.owner.sources.capabilities.hold_b_block
        ),
        (
            HoldMode::Observe,
            crate::execution::hold_b::B_UNDERESTIMATED
        )
    );
    f.owner.hold_gate.evidence.underestimated = 0;
    f.owner
        .apply_capability_control(3, Ok(request("ACTIVE", 3)));
    assert_eq!(f.owner.hold_gate.mode, HoldMode::Active);
    let row = f
        .owner
        .owner_snapshot(0)
        .capabilities
        .into_iter()
        .find(|c| c.name == crate::execution::hold_b::HOLD_B)
        .unwrap();
    assert_eq!(
        (row.requested, row.effective, row.block_reason.as_ref()),
        ("ACTIVE", "ACTIVE", "")
    );
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut records = vec![];
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == RecordEvent::KIND {
            if let RecordEvent::HoldFlows {
                a_wallet,
                b,
                actual,
                b_checks,
                a_checks,
                ..
            } = decode(frame.kind, frame.schema, &frame.payload).unwrap()
            {
                records.push((a_wallet, b, actual, b_checks, a_checks));
            }
        }
    }
    assert_eq!(records.len(), 1, "one record per settled attempt");
    let (a_wallet, b, actual, b_checks, a_checks) = &records[0];
    assert_eq!(a_wallet["SOL"], 20);
    assert_eq!(b, &hold.b);
    assert_eq!(actual["SOL"], -20);
    assert!(b_checks.iter().chain(a_checks).all(|c| !c.underestimated));
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-03 (N-2) in the running owner: a read with a B sighting and no landing is an open
/// contradiction — UNKNOWN, the hold kept, an alarm, and the Wallets projection shows its class,
/// sources, C result, hold duration, calls and bytes; the next read proves B+C and settles (no
/// hold, the persisted class). Every applied read is one typed `NonLanding` record.
#[test]
fn dec03_owner_records_alarms_and_projects_the_non_landing_class() {
    use crate::analytics::payload::{decode, OwnerDiagnostic, Payload};
    use crate::execution::one_shot::non_landing::{BClass, Verdict};
    use crate::execution::one_shot::settlement::SettlementEvidence;
    let (mut f, intent, proof) = fe04_fixture("dec03_owner", "unknown");
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some());
    let mut seeing = proof.clone();
    seeing.b[0].class = BClass::Sighting(intent.signatures[0].clone());
    seeing.calls = 7;
    seeing.bytes = 1_234;
    fe04_answer(
        &f,
        SettlementEvidence {
            observations: vec![],
            non_landing: Some(Ok(seeing)),
        },
    );
    run_until(&mut f, |o| !o.remote.fe04_settlement_results.pending());
    assert_eq!(f.owner.reservations.held("SOL"), intent.holds["SOL"]);
    let n = f.owner.owner_snapshot(0).wallets.non_landing.unwrap();
    assert_eq!(
        (n.subject, n.result_class, n.alarm, n.c_result),
        ("ATTEMPT", "VETO_CONTRADICTION_OPEN", true, "NEGATIVE")
    );
    assert_eq!(
        n.sources,
        [
            ("op-a".to_string(), "SIGHTING".to_string()),
            ("op-b".to_string(), "USABLE".to_string())
        ]
    );
    assert_eq!((n.calls, n.bytes, n.reads), (Some(7), Some(1_234), 1));
    assert!(n.hold_ms.is_some());
    f.owner.fe04_settle_due = Some(std::time::Instant::now());
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some(), "read again");
    fe04_answer(
        &f,
        SettlementEvidence {
            observations: vec![],
            non_landing: Some(Ok(proof)),
        },
    );
    run_until(&mut f, fe04_settled);
    let n = f.owner.owner_snapshot(0).wallets.non_landing.unwrap();
    assert_eq!(
        (n.result_class, n.alarm, n.hold_ms, n.reads),
        ("B_C_CONFIRMED", false, None, 2)
    );
    // Without a read in this run (a restart), the persisted release is shown.
    let latest = std::mem::take(&mut f.owner.fe04_non_landing);
    let n = f.owner.owner_snapshot(0).wallets.non_landing.unwrap();
    assert_eq!(
        (n.result_class, n.c_result, n.calls, n.reads),
        ("B_C_CONFIRMED", "NEGATIVE", None, 0)
    );
    assert_eq!(
        n.sources,
        [
            ("op-a".to_string(), "USABLE".to_string()),
            ("op-b".to_string(), "USABLE".to_string())
        ]
    );
    f.owner.fe04_non_landing = latest;
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let (mut records, mut alarms) = (vec![], 0);
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == RecordEvent::KIND {
            if let RecordEvent::NonLanding {
                exit, class, read, ..
            } = decode(frame.kind, frame.schema, &frame.payload).unwrap()
            {
                records.push((exit, class, read.map(|r| r.calls)));
            }
        } else if frame.kind == OwnerDiagnostic::KIND {
            let d: OwnerDiagnostic = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            alarms += usize::from(d.line.contains("ALARM VETO_CONTRADICTION_OPEN"));
        }
    }
    assert_eq!(
        records,
        [
            (None, Some(Verdict::VetoContradiction), Some(7)),
            (None, Some(Verdict::BcConfirmed), Some(0))
        ]
    );
    assert_eq!(alarms, 1);
    let _ = std::fs::remove_dir_all(dir);
}

/// C02/F-04: the FE-04 settlement is the one landing-telemetry path — a settled attempt emits
/// exactly one reconcile record (outcome, fee, the capital released), also when the answer is
/// repeated; no separate RAM reconcile polls the signature.
#[test]
fn c02_fe04_settlement_emits_the_one_landing_telemetry_record() {
    use crate::analytics::payload::{decode, Payload};
    use crate::execution::one_shot::settlement::SettlementEvidence;
    use crate::execution::pending::ReconcileRecord;
    let (mut f, intent, proof) = fe04_fixture("c02_landing", "unknown");
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some());
    let evidence = SettlementEvidence {
        observations: vec![],
        non_landing: Some(Ok(proof)),
    };
    fe04_answer(&f, evidence.clone());
    run_until(&mut f, fe04_settled);
    fe04_answer(&f, evidence);
    run_until(&mut f, |o| !o.remote.fe04_settlement_results.pending());
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut records = vec![];
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == ReconcileRecord::KIND {
            let r: ReconcileRecord = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            records.push((r.signature, r.outcome, r.exposure_released));
        }
    }
    assert_eq!(
        records,
        [(
            intent.signatures[0].clone(),
            "NOT_LANDED".to_string(),
            intent.capital_usd_micro
        )]
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// C3: evidence that proves nothing (an unexpired blockhash, a failed read) keeps the attempt
/// UNKNOWN with every hold — capital, book, HALT — and the next read waits the backoff.
#[test]
fn c3_owner_keeps_unknown_holds_and_retries_after_the_backoff() {
    use crate::execution::one_shot::settlement::SettlementEvidence;
    let (mut f, intent, _) = fe04_fixture("fe04_unknown", "unknown");
    for reason in [
        "NON_LANDING_BLOCKHASH_NOT_EXPIRED",
        "NON_LANDING_HEIGHT_UNAVAILABLE",
    ] {
        run_until(&mut f, |o| o.fe04_settle_due.is_none());
        assert!(fe04_job(&f).is_some());
        let sent = std::time::Instant::now();
        fe04_answer(
            &f,
            SettlementEvidence {
                observations: vec![],
                non_landing: Some(Err(reason.into())),
            },
        );
        run_until(&mut f, |o| o.fe04_settle_due.is_some());
        assert!(f.owner.fe04_settle_due.unwrap() >= sent + f.owner.fe04_settle_retry);
        assert!(!fe04_settled(&f.owner));
        assert_eq!(f.owner.safety.capital_in_use(), intent.capital_usd_micro);
        assert_eq!(f.owner.reservations.held("SOL"), intent.holds["SOL"]);
        assert_eq!(f.owner.safety.day().failed_attempts, 0, "nothing booked");
        // Before the backoff ends no read is sent.
        f.owner.remote_apply().unwrap();
        assert!(fe04_job(&f).is_none());
        f.owner.fe04_settle_due = Some(std::time::Instant::now());
    }
    cleanup(f);
}

/// C3: an attempt finalized before the restart (proven not landed, not booked) reserved no
/// capital; the owner settles it from the journal alone (no read is sent) and books it once.
#[test]
fn c3_owner_books_a_finalized_attempt_without_a_read() {
    let (mut f, _, _) = fe04_fixture("fe04_final", "finalized");
    assert_eq!(f.owner.safety.capital_in_use(), 0);
    assert!(f.owner.reservations.is_empty() && f.owner.fe04_settle_due.is_some());
    run_until(&mut f, fe04_settled);
    assert!(
        fe04_job(&f).is_none(),
        "no RPC read for a finalized attempt"
    );
    assert_eq!(f.owner.safety.capital_in_use(), 0);
    f.owner.fe04_settle_due = Some(std::time::Instant::now());
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_settled(&f.owner) && fe04_job(&f).is_none());
    cleanup(f);
}

/// C3 Safety booking refused: another path released the attempt's capital first, so the
/// settlement's booking is a ledger violation. Safety halts and applies nothing; the owner keeps
/// the hold, the capital to release and the booking itself; a repeated answer books nothing. The
/// settlement stays in the journal: a restart books exactly it (day, no capital, empty book).
#[test]
fn c3_owner_refused_safety_booking_retains_holds_and_the_restart_books_it() {
    use crate::execution::one_shot::settlement::SettlementEvidence;
    let (mut f, intent, proof) = fe04_fixture("fe04_refused", "unknown");
    f.owner
        .safety
        .release_without_outcome(intent.capital_usd_micro, 0)
        .unwrap();
    let day = (0, 0);
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some());
    let evidence = SettlementEvidence {
        observations: vec![],
        non_landing: Some(Ok(proof)),
    };
    fe04_answer(&f, evidence.clone());
    run_until(&mut f, fe04_settled);
    let retained = |o: &ShadowOwner| {
        (
            o.fe04_unbooked,
            o.reservations.held("SOL"),
            (o.safety.day().realized_pnl, o.safety.day().failed_attempts),
            o.safety.halted().is_some(),
        )
    };
    let expected = (
        Some(super::fe04::Fe04Unbooked {
            reserved_capital: Some(intent.capital_usd_micro),
            reserved_inventory: 0,
        }),
        intent.holds["SOL"],
        day,
        true,
    );
    assert_eq!(retained(&f.owner), expected);
    fe04_answer(&f, evidence);
    run_until(&mut f, |o| !o.remote.fe04_settlement_results.pending());
    assert_eq!(
        retained(&f.owner),
        expected,
        "a repeated answer books nothing"
    );
    // Restart: the journal's settlement is booked by the replay.
    f.owner.fe04_held = None;
    let mut safety = Safety::new(e2e_limits(), Allow::default(), false);
    let dir_s = f.dir.join("fe04").display().to_string();
    let now = wall_unix_s().unwrap();
    let again = setup::reservation_book(
        &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
        &f.owner.liquidator,
    )
    .unwrap()
    .start(&mut safety, now)
    .unwrap();
    assert!(again.book.is_empty() && again.unbooked.is_none());
    assert_eq!(safety.capital_in_use(), 0);
    assert_eq!(
        (safety.day().realized_pnl, safety.day().failed_attempts),
        (0, 1)
    );
    cleanup(f);
}

/// C3 UTC day: an attempt evaluated long before (1970) lands with a token inflow and is settled
/// near midnight. The running owner books it at the persisted booking second; a restart replays
/// the same ledger — day PnL/outcome, open inventory, no capital — before and after midnight.
#[test]
fn c3_settlement_day_is_the_same_in_the_run_and_after_a_restart_across_midnight() {
    use crate::execution::one_shot::receipts::FinalizedObservation;
    use crate::execution::one_shot::settlement::{SettleStep, SettlementEvidence};
    let midnight = 20_750 * 86_400;
    let value = |asset: &str, d: i128| match asset {
        "SOL" => Some(d / 10),
        "MINT" => Some(d),
        _ => None,
    };
    for (n, (booked, restart)) in [
        (midnight - 1, midnight - 1),
        (midnight - 1, midnight + 30),
        (midnight + 1, midnight + 30),
    ]
    .into_iter()
    .enumerate()
    {
        let dir = std::env::temp_dir().join(format!("p0_c3_day_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let wallet = Pubkey::new_unique();
        let intent = crate::execution::one_shot::test_unknown_attempt(&dir, &wallet.to_string());
        let start = |safety: &mut Safety, now| {
            let dir_s = dir.display().to_string();
            setup::reservation_book(
                &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
                &wallet,
            )
            .unwrap()
            .start(safety, now)
            .unwrap()
        };
        // The run: startup replay, then the settling step at `booked`.
        let mut live = Safety::new(e2e_limits(), Allow::default(), false);
        let mut run = start(&mut live, booked - 600);
        let SettleStep::Settled(s) = run
            .held
            .as_mut()
            .unwrap()
            .apply_settlement_evidence(
                SettlementEvidence {
                    observations: vec![(
                        intent.signatures[0].clone(),
                        Ok(FinalizedObservation {
                            signature: intent.signatures[0].clone(),
                            slot: 70,
                            success: true,
                            fee_lamports: 5_000,
                            wallet_sol_delta_lamports: -5_000,
                            wallet_token_deltas: Some([("MINT".to_string(), 1_000)].into()),
                            unknown_reasons: vec![],
                        }),
                    )],
                    non_landing: None,
                },
                &value,
                booked,
            )
            .unwrap()
        else {
            panic!("settled");
        };
        let settled = run
            .held
            .as_ref()
            .unwrap()
            .safety_replay()
            .unwrap()
            .settled
            .unwrap();
        super::fe04::book_settlement(&mut live, run.unbooked.unwrap(), &s, settled).unwrap();
        // A later operation of the run at `restart` (rolls its day).
        let _ = live.admit_loss(restart, 0);
        drop(run);
        // The restart at `restart`.
        let mut restarted = Safety::new(e2e_limits(), Allow::default(), false);
        let again = start(&mut restarted, restart);
        // P8-CTRL.6: booked once; its open position stays held across the restart (no exit yet).
        assert!(again.unbooked.is_none());
        assert_eq!(
            (again.book.held("MINT"), again.book.held("SOL")),
            (1_000, 0)
        );
        let ledger = |s: &Safety| (s.capital_in_use(), s.inventory_outstanding(), s.day());
        assert_eq!(
            ledger(&restarted),
            ledger(&live),
            "booked {booked}, restart {restart}"
        );
        assert_eq!(
            live.inventory_outstanding(),
            1_000,
            "open inventory, unrealized"
        );
        assert_eq!(
            live.day().realized_pnl,
            if booked / 86_400 == restart / 86_400 {
                -500
            } else {
                0
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// FE-04 exit restart: a settled exit replays into Safety with its attempt — the day's realized
/// net of both (fee −500, proceeds +500), the released mark leaving the inventory — and the
/// journal holds nothing and needs no settlement.
#[test]
fn fe04_exit_restart_replays_the_attempt_and_its_exit() {
    let dir = std::env::temp_dir().join(format!("p0_exit_restart_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let wallet = Pubkey::new_unique();
    let now = wall_unix_s().unwrap();
    crate::execution::one_shot::exit::tests::exited(&dir, &wallet.to_string(), now);
    let mut safety = Safety::new(e2e_limits(), Allow::default(), false);
    let dir_s = dir.display().to_string();
    let start = setup::reservation_book(
        &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
        &wallet,
    )
    .unwrap()
    .start(&mut safety, now)
    .unwrap();
    assert!(start.book.is_empty() && start.unbooked.is_none() && start.settle_due.is_none());
    assert_eq!(
        (safety.day().realized_pnl, safety.inventory_outstanding()),
        (0, 0)
    );
    drop(start);
    let _ = std::fs::remove_dir_all(&dir);
}

/// FE-04 archive (B3): an archived attempt keeps its day booked after a restart — the owner start
/// replays the archive also without an active journal (before, a finished attempt moved out of
/// the active position would have been forgotten by the day's loss limit); a foreign archive
/// file halts the start.
#[test]
fn fe04_archived_attempt_stays_booked_after_restart() {
    let dir = std::env::temp_dir().join(format!("p0_archive_restart_{}", Pubkey::new_unique()));
    let wallet = Pubkey::new_unique();
    let now = wall_unix_s().unwrap();
    crate::execution::one_shot::exit::tests::archived_fee_only_attempt(
        &dir,
        &wallet.to_string(),
        now,
    );
    assert!(!dir.join(format!("{wallet}.p0j")).exists());
    let dir_s = dir.display().to_string();
    let get = |k: &str| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone());
    let mut safety = Safety::new(e2e_limits(), Allow::default(), false);
    let start = setup::reservation_book(&get, &wallet)
        .unwrap()
        .start(&mut safety, now)
        .unwrap();
    assert!(start.book.is_empty() && start.held.is_none());
    assert_eq!(
        safety.day().realized_pnl,
        -500,
        "the archived attempt is booked"
    );
    drop(start);
    std::fs::write(
        dir.join(format!("attempts/{wallet}/forged.p0j")),
        b"not a journal",
    )
    .unwrap();
    assert!(setup::reservation_book(&get, &wallet)
        .err()
        .is_some_and(|e| e.starts_with("RESERVATION_JOURNAL_UNREADABLE")));
    let _ = std::fs::remove_dir_all(&dir);
}

/// FE-04 exit: the canonical state that prices SOL — the chain clock and the SOL bank with a
/// fixed oracle price of 150 USD (9 decimals).
fn install_sol_price(f: &mut Fixture) {
    use marginfi_type_crate::types::{Bank, BankOperationalState, OracleSetup, WrappedI80F48};
    let fixed = |v: i128| WrappedI80F48 {
        value: (v << 48).to_le_bytes(),
    };
    let mut b: Bank = bytemuck::Zeroable::zeroed();
    b.group = f.owner.group;
    b.mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    b.mint_decimals = 9;
    b.asset_share_value = fixed(1);
    b.liability_share_value = fixed(1);
    b.config.asset_weight_init = fixed(1);
    b.config.asset_weight_maint = fixed(1);
    b.config.liability_weight_init = fixed(1);
    b.config.liability_weight_maint = fixed(1);
    b.config.operational_state = BankOperationalState::Operational;
    b.config.oracle_setup = OracleSetup::Fixed;
    b.config.fixed_price = fixed(150);
    let mut clock = 100u64.to_le_bytes().to_vec();
    clock.extend_from_slice(&[0u8; 24]);
    clock.extend_from_slice(&(wall_unix_s().unwrap() as i64).to_le_bytes());
    let account = |pubkey, owner, data| AccountUpdate {
        pubkey,
        owner,
        lamports: 1,
        data,
        slot: 100,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    };
    f.owner.st.install_snapshot(
        100,
        vec![
            account(
                solana_sdk::sysvar::clock::id(),
                solana_sdk::sysvar::id(),
                clock,
            ),
            account(
                f.owner.sol_bank,
                f.owner.program,
                [
                    darkbot_project0::protocol::DISC_BANK.as_slice(),
                    bytemuck::bytes_of(&b),
                ]
                .concat(),
            ),
        ],
    );
}

/// FE-04 exit: the detached fixture started from a journal with a settled attempt (1000 units
/// open, mark 1000) and its exit `exit-1` handed off, through the production startup replay.
fn fe04_exit_fixture(tag: &str) -> (Fixture, crate::execution::one_shot::exit::ExitIntent) {
    let mut f = fixture_with(tag, None);
    let dir = f.dir.join("fe04");
    let exit = crate::execution::one_shot::exit::tests::handed_off_exit_journal(
        &dir,
        &f.owner.liquidator.to_string(),
        wall_unix_s().unwrap(),
    );
    let dir_s = dir.display().to_string();
    let start = setup::reservation_book(
        &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
        &f.owner.liquidator,
    )
    .unwrap()
    .start(&mut f.owner.safety, wall_unix_s().unwrap())
    .unwrap();
    f.owner.reservations = start.book;
    f.owner.fe04_journal = start.dir;
    f.owner.fe04_held = start.held;
    f.owner.fe04_settle_due = start.settle_due;
    f.owner.fe04_unbooked = start.unbooked;
    (f, exit)
}

fn exit_observation(
    sig: &str,
    success: bool,
    sol: i128,
    tokens: &[(&str, i128)],
) -> crate::execution::one_shot::settlement::SettlementEvidence {
    crate::execution::one_shot::settlement::SettlementEvidence {
        observations: vec![(
            sig.to_string(),
            Ok(crate::execution::one_shot::receipts::FinalizedObservation {
                signature: sig.into(),
                slot: 85,
                success,
                fee_lamports: 5_000,
                wallet_sol_delta_lamports: sol,
                wallet_token_deltas: Some(
                    tokens.iter().map(|(m, d)| (m.to_string(), *d)).collect(),
                ),
                unknown_reasons: vec![],
            }),
        )],
        non_landing: None,
    }
}

fn exit_open(o: &ShadowOwner) -> bool {
    o.fe04_held
        .as_ref()
        .is_some_and(|j| j.open_exit_id().is_some())
}

/// FE-04 exit in the running owner: the open exit (its hold in the book, the attempt's open
/// inventory in Safety) is read by the settlement worker for exactly this exit; the sold position
/// settles and is booked once as the restart replays it — realized the SOL proceeds at the fixed
/// SOL price, the mark released, the book empty. A repeated or a stale read books nothing.
#[test]
fn fe04_exit_owner_settles_the_sold_position_and_books_it_once() {
    let (mut f, exit) = fe04_exit_fixture("fe04_exit_once");
    install_sol_price(&mut f);
    assert_eq!(f.owner.safety.inventory_outstanding(), 1_000);
    assert_eq!(f.owner.reservations.held(&exit.mint), exit.amount_in);
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    let job = fe04_job(&f).expect("the exit read is handed to the worker");
    assert_eq!(job.exit.as_deref(), Some("exit-1"));
    assert_eq!(job.subject.signatures, exit.signatures);
    assert_eq!(
        job.subject.expected_wallet_assets,
        [
            "SOL".to_string(),
            exit.mint.clone(),
            crate::execution::one_shot::exit::WSOL_MINT.to_string()
        ]
        .into()
    );
    let day_before = f.owner.safety.day().realized_pnl;
    let sig = &exit.signatures[0];
    let sold = exit_observation(sig, true, 5_000, &[(&exit.mint, -1_000)]);
    fe04_answer_for(&f, sold.clone(), Some("exit-1"));
    run_until(&mut f, |o| !exit_open(o));
    // +5000 lamports at 150 USD/SOL = 750 USD micro, a gain rounded down (749), realized; the
    // mark 1000 released.
    let booked = (
        f.owner.safety.day().realized_pnl - day_before,
        f.owner.safety.inventory_outstanding(),
        f.owner.reservations.is_empty(),
    );
    assert_eq!(booked, (749, 0, true));
    for exit_tag in [Some("exit-1"), Some("exit-0"), None] {
        fe04_answer_for(&f, sold.clone(), exit_tag);
        run_until(&mut f, |o| !o.remote.fe04_settlement_results.pending());
        assert_eq!(
            (
                f.owner.safety.day().realized_pnl - day_before,
                f.owner.safety.inventory_outstanding(),
                f.owner.reservations.is_empty(),
            ),
            booked,
            "{exit_tag:?} books nothing again"
        );
    }
    cleanup(f);
}

/// FE-04 exit ("unvollständiger Exit") in the running owner: a read made for another exit never
/// applies; a partial sale never finalizes — the exit stays UNKNOWN, its hold stays in the book,
/// Safety books nothing, the next read waits the backoff.
#[test]
fn fe04_exit_owner_keeps_a_partial_sale_unknown_with_its_holds() {
    let (mut f, exit) = fe04_exit_fixture("fe04_exit_partial");
    install_sol_price(&mut f);
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some());
    let ledger = |o: &ShadowOwner| (o.safety.day(), o.safety.inventory_outstanding());
    let before = ledger(&f.owner);
    let sig = &exit.signatures[0];
    // A read made for another exit never applies to this one, even a complete sale.
    fe04_answer_for(
        &f,
        exit_observation(sig, true, 5_000, &[(&exit.mint, -1_000)]),
        Some("exit-0"),
    );
    run_until(&mut f, |o| !o.remote.fe04_settlement_results.pending());
    assert!(exit_open(&f.owner) && ledger(&f.owner) == before);
    f.owner.fe04_settle_due = Some(std::time::Instant::now());
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some());
    let sent = std::time::Instant::now();
    fe04_answer_for(
        &f,
        exit_observation(sig, true, 5_000, &[(&exit.mint, -500)]),
        Some("exit-1"),
    );
    run_until(&mut f, |o| o.fe04_settle_due.is_some());
    assert!(exit_open(&f.owner));
    assert_eq!(f.owner.reservations.held(&exit.mint), exit.amount_in);
    assert_eq!(ledger(&f.owner), before);
    assert!(f.owner.fe04_settle_due.unwrap() >= sent + f.owner.fe04_settle_retry);
    f.owner.remote_apply().unwrap();
    assert!(fe04_job(&f).is_none(), "no read before the backoff ends");
    cleanup(f);
}

/// FE-04 exit: a consistent receipt without a SOL price finalizes the exit but books nothing and
/// keeps the book; once canonical state prices SOL the exit is settled from the journal alone (no
/// read) and booked.
#[test]
fn fe04_exit_owner_books_a_finalized_exit_once_sol_is_priced() {
    let (mut f, exit) = fe04_exit_fixture("fe04_exit_unpriced");
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some());
    let before = (f.owner.safety.day(), f.owner.safety.inventory_outstanding());
    let sig = &exit.signatures[0];
    fe04_answer_for(
        &f,
        exit_observation(sig, true, 5_000, &[(&exit.mint, -1_000)]),
        Some("exit-1"),
    );
    run_until(&mut f, |o| {
        o.fe04_held
            .as_ref()
            .is_some_and(|j| j.exit_awaits_booking())
    });
    assert_eq!(
        (f.owner.safety.day(), f.owner.safety.inventory_outstanding()),
        before
    );
    assert_eq!(f.owner.reservations.held(&exit.mint), exit.amount_in);
    install_sol_price(&mut f);
    f.owner.fe04_settle_due = Some(std::time::Instant::now());
    run_until(&mut f, |o| !exit_open(o));
    assert!(fe04_job(&f).is_none(), "settled from the journal alone");
    assert_eq!(f.owner.safety.inventory_outstanding(), 0);
    assert!(f.owner.reservations.is_empty());
    cleanup(f);
}

/// P2-04/P2-05 fixture: the chain clock at `unix` as of `slot`.
fn p2_clock(slot: u64, unix: u64) -> AccountUpdate {
    let mut c = slot.to_le_bytes().to_vec();
    c.extend_from_slice(&[0u8; 24]);
    c.extend_from_slice(&(unix as i64).to_le_bytes());
    AccountUpdate {
        pubkey: solana_sdk::sysvar::clock::id(),
        owner: solana_sdk::sysvar::id(),
        lamports: 1,
        data: c,
        slot,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    }
}

/// P2-04/P2-05 fixture: a ready owner state at slot 100 and chain second `t0` with one account
/// holding 100 USD collateral (maintenance weight 0.9) and `liab` USD debt on a premium-active
/// bank (≈ 10 %/year since `t0`); fixed 1-USD oracles. Returns the account.
fn p2_state(f: &mut Fixture, liab: f64, t0: u64) -> Pubkey {
    let (state, account, _, _) = p2_accounts(f, liab, t0, true);
    f.owner.st.install_snapshot(100, state.clone());
    f.owner.st.reconcile(100, &state);
    f.owner.st.confirm_ready();
    assert!(f.owner.st.run_state().is_tradable());
    account
}

/// The accounts of `p2_state` (DEC-06: `premium` off gives exact thresholds): the state, the
/// account, its collateral bank and its liability bank.
fn p2_accounts(
    f: &Fixture,
    liab: f64,
    t0: u64,
    premium: bool,
) -> (Vec<AccountUpdate>, Pubkey, Pubkey, Pubkey) {
    use bytemuck::Zeroable;
    use marginfi_type_crate::constants::PREMIUM_ACTIVE;
    use marginfi_type_crate::types::{
        Bank, BankOperationalState, MarginfiAccount, MarginfiGroup, OracleSetup, WrappedI80F48,
    };
    let num = |v: f64| WrappedI80F48 {
        value: ((v * (1u64 << 48) as f64) as i128).to_le_bytes(),
    };
    let update = |pubkey, data| AccountUpdate {
        pubkey,
        owner: f.owner.program,
        lamports: 1,
        data,
        slot: 100,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    };
    let group = f.owner.group;
    let bank = |asset_weight: f64, flags: u64| {
        let mut b = Bank::zeroed();
        b.group = group;
        b.mint = Pubkey::new_unique();
        b.mint_decimals = 6;
        b.asset_share_value = num(1.0);
        b.liability_share_value = num(1.0);
        b.config.asset_weight_init = num(asset_weight);
        b.config.asset_weight_maint = num(asset_weight);
        b.config.liability_weight_init = num(1.0);
        b.config.liability_weight_maint = num(1.0);
        b.config.operational_state = BankOperationalState::Operational;
        b.config.oracle_setup = OracleSetup::Fixed;
        b.config.fixed_price = num(1.0);
        b.flags = flags;
        [protocol::DISC_BANK.as_slice(), bytemuck::bytes_of(&b)].concat()
    };
    let (asset_bank, liab_bank, account) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let mut a = MarginfiAccount::zeroed();
    a.group = group;
    // Pinned Marginfi keeps the balances sorted by bank key, descending.
    let (i, j) = if asset_bank > liab_bank {
        (0, 1)
    } else {
        (1, 0)
    };
    let bal = &mut a.lending_account.balances;
    (bal[i].active, bal[i].bank_pk, bal[i].asset_shares) = (1, asset_bank, num(100e6));
    (bal[j].active, bal[j].bank_pk, bal[j].liability_shares) = (1, liab_bank, num(liab * 1e6));
    (bal[j].premium_rate_snapshot, bal[j].last_update) = (u32::MAX / 100, t0);
    let state = vec![
        p2_clock(100, t0),
        update(
            group,
            [
                protocol::DISC_MARGINFI_GROUP.as_slice(),
                bytemuck::bytes_of(&MarginfiGroup::zeroed()),
            ]
            .concat(),
        ),
        update(asset_bank, bank(0.9, 0)),
        update(
            liab_bank,
            bank(1.0, if premium { PREMIUM_ACTIVE } else { 0 }),
        ),
        update(
            account,
            [
                protocol::DISC_MARGINFI_ACCOUNT.as_slice(),
                bytemuck::bytes_of(&a),
            ]
            .concat(),
        ),
    ];
    (state, account, asset_bank, liab_bank)
}

/// V2-13 (P4-03/P5b-02): a bank-wide re-check without a new cause is not admitted again — a
/// second failed price projection of the same bank input revision, after the detection consumed
/// the first admission, admits nothing (counted suppressed); a newer bank revision admits the
/// account again.
#[test]
fn v2_13_a_repeated_bank_recheck_without_a_new_cause_is_suppressed() {
    use crate::orchestration::remote_workers::OraclePriceProjectionResultMessage;
    let mut f = fixture_with("v2_13_repeat", None);
    f.owner.threshold_test_active();
    let account = p2_state(&mut f, 85.0, 1_700_000_000);
    let bank = Pubkey::new_unique();
    f.owner.reverse_index.set_account_banks(account, &[bank]);
    f.owner.bank_input_revision.insert(bank, 7);
    let failed = || OraclePriceProjectionResultMessage {
        bank,
        base_slot: 100,
        base_bank_hash: solana_sdk::hash::Hash::default(),
        result: Err("RPC_TRANSPORT".into()),
    };
    let admitted = |f: &mut Fixture| {
        assert!(f
            .results
            .as_ref()
            .unwrap()
            .oracle
            .try_push(failed())
            .is_ok());
        f.owner.remote_apply().unwrap();
        let mut accounts = vec![];
        while let Some(p) = f.owner.oracle_lanes.next() {
            accounts.push(p.work.0);
        }
        accounts
    };
    assert_eq!(admitted(&mut f), vec![account]);
    let suppressed = f.owner.metrics.candidate_cache_suppressed;
    assert_eq!(admitted(&mut f), Vec::<Pubkey>::new(), "no new cause");
    assert_eq!(f.owner.metrics.candidate_cache_suppressed, suppressed + 1);
    f.owner.bank_input_revision.insert(bank, 8);
    assert_eq!(admitted(&mut f), vec![account], "a newer bank revision");
    cleanup(f);
}

/// P2-04 (0.1.12): a healthy account with premium-active debt is re-evaluated when its premium
/// boundary passes on the chain clock, with no bank or account event: the due wake-up alone opens
/// and flushes the sweep batch (one second earlier nothing is due), a healthy evaluation renews
/// the wake-up from the account's own health and premium, and once pending premium made it
/// liquidatable it is counted liquidatable and has no wake-up left.
#[test]
fn p2_04_premium_boundary_wakes_the_sweep_without_an_event() {
    p2_04_case(false);
}

/// V2-03 (P5c-03): the opt-in threshold mode consumes the same premium wake-ups (exact
/// re-evaluation on the chain clock alone, renewed wake-up, none once liquidatable).
#[test]
fn v2_03_threshold_mode_consumes_premium_wakeups() {
    p2_04_case(true);
}

/// V2-03 (P5c-03): in the threshold mode the exact evaluation of a detection plans the
/// account's premium wake-up (P2-04: every evaluation renews or removes it), so a healthy
/// account with premium-active debt is woken without any sweep of the sweep mode.
#[test]
fn v2_03_threshold_detection_plans_the_premium_wakeup() {
    let mut f = fixture("v2_03_detect");
    f.owner.threshold_test_active();
    let t0 = 1_700_000_000u64;
    let account = p2_state(&mut f, 85.0, t0);
    assert!(f.owner.time_schedule.is_empty());
    f.owner
        .detect(Some((account, 0, 100, "oracle_threshold")))
        .unwrap();
    let at = f
        .owner
        .time_schedule
        .next_due()
        .expect("a healthy premium account is woken");
    assert!(at > t0 + 30 * 86_400, "months away: {at}");
    cleanup(f);
}

fn p2_04_case(threshold: bool) {
    use darkbot_project0::schedule::Trigger;
    let mut f = fixture(if threshold { "v2_03_p2_04" } else { "p2_04" });
    if threshold {
        f.owner.threshold_test_active();
    }
    f.owner.threshold_ready = true;
    let t0 = 1_700_000_000u64;
    // ≈ 10 %/year on 85 USD against a 5 USD maintenance margin: liquidatable in about 7 months.
    let account = p2_state(&mut f, 85.0, t0);
    assert!(!f.owner.sweep_batch_open(), "nothing dirty, nothing due");

    f.owner
        .time_schedule
        .upsert(account, Trigger::PremiumBoundary, Some(t0));
    assert!(
        f.owner.sweep_batch_open(),
        "a due wake-up alone opens the batch"
    );
    f.owner.sweep();
    assert_eq!(f.owner.metrics.health_sweep_healthy, 1);
    let mut slot = 100;
    let mut set_clock = |f: &mut Fixture, unix: u64| {
        slot += 1;
        f.owner
            .st
            .on_event(IngressEvent::Account(p2_clock(slot, unix)));
    };
    let mut wakes = 0;
    while f.owner.metrics.health_sweep_liquidatable == 0 {
        let at = f.owner.time_schedule.next_due().expect("healthy ⇒ woken");
        assert!(at > t0 + 30 * 86_400, "months away: {at}");
        set_clock(&mut f, at - 1);
        assert!(!f.owner.sweep_batch_open(), "not due a second earlier");
        set_clock(&mut f, at);
        assert!(f.owner.sweep_batch_open(), "due on the chain clock alone");
        f.owner.sweep();
        wakes += 1;
        assert!(wakes <= 3, "converges");
    }
    assert_eq!(f.owner.metrics.health_sweep_checks, 1 + wakes);
    assert!(f.owner.time_schedule.is_empty(), "liquidatable: no wake-up");
    assert!(!f.owner.sweep_batch_open());
    cleanup(f);
}

/// P2-05 (0.1.12): a tagged candidate that was just forwarded (its repeat interval not yet
/// passed) is forwarded again when its tag wake-up is due on the chain clock — a premium
/// wake-up of the same account is not a forced forward.
#[test]
fn p2_05_tag_wakeup_forwards_a_held_candidate_again() {
    p2_05_case(false);
}

/// V2-03 (P5c-03): the opt-in threshold mode forwards a held candidate again when its tag
/// wake-up is due (forced re-arm, lane L0), exactly as the sweep mode.
#[test]
fn v2_03_threshold_mode_consumes_tag_wakeups() {
    p2_05_case(true);
}

fn p2_05_case(threshold: bool) {
    use darkbot_project0::schedule::Trigger;
    let mut f = fixture(if threshold { "v2_03_p2_05" } else { "p2_05" });
    if threshold {
        f.owner.threshold_test_active();
    }
    f.owner.threshold_ready = true;
    let t0 = 1_700_000_000u64;
    // 90 USD maintenance collateral against 95 USD debt: liquidatable now.
    let account = p2_state(&mut f, 95.0, t0);
    f.owner.candidate_recheck_slots = 1_000;
    let held = CandidateCacheEntry {
        state: CandidateHealthState::Liquidatable,
        last_forwarded_slot: 100,
        last_forwarded_revision: 0,
    };
    for (trigger, forwarded) in [(Trigger::PremiumBoundary, 0), (Trigger::TagProfitable, 1)] {
        f.owner.candidate_cache.insert(account, held);
        f.owner.time_schedule.upsert(account, trigger, Some(t0));
        assert!(f.owner.sweep_batch_open(), "{trigger:?} due");
        f.owner.sweep();
        assert_eq!(
            (
                f.owner.metrics.sweep_forwarded,
                f.owner.metrics.candidate_cache_forced_forwards
            ),
            (forwarded, forwarded),
            "{trigger:?}"
        );
    }
    // MT3 (P6-MT.5): a due t_profitable wake-up is a forced re-arm and waits in lane L0.
    assert_eq!(
        f.owner.oracle_lanes.get(&account).map(|p| p.lane),
        Some(super::candidates::Lane::L0)
    );
    assert!(!f.owner.candidate_force_rearm.contains(&account));
    assert!(f.owner.time_schedule.is_empty());
    // The detection's record decides the tag wake-up: this fixture has no plan (no FeeState),
    // so a former wake-up is dropped.
    f.owner
        .time_schedule
        .upsert(account, Trigger::TagProfitable, Some(t0 + 3_600));
    f.owner
        .detect(Some((account, 0, 100, "oracle_health_ready")))
        .unwrap();
    assert!(f.owner.time_schedule.is_empty(), "no t_profitable: dropped");
    cleanup(f);
}

/// P6-MT.1 (MT5b) on the wired path — restart and #34: an unresolved FE-04 attempt reopened
/// from the journal (UNKNOWN) makes its target SUBMITTED(signature, last_valid_block_height)
/// through the reservation book at the owner's first pass; a detection of that target is not
/// evaluated (no Health call, the identity is allocated and the trigger deferred as the one
/// newest pending revision, later triggers coalesced); the FE-04 settlement path — and only
/// it — releases the target (book empty ⇒ RELEASED) and re-arms the newest pending revision
/// through lane L0.
#[test]
fn mt5b_submitted_follows_the_fe04_journal_and_defers_the_targets_detection() {
    use crate::execution::one_shot::settlement::SettlementEvidence;
    use crate::execution::ownership::{State, Submission};
    let (mut f, intent, proof) = fe04_fixture("mt5b_submitted", "unknown");
    let target: Pubkey = intent.target.parse().unwrap();
    assert_eq!(f.owner.ownership.state(&target), None, "nothing synced yet");
    assert!(f.owner.lifecycle(), "the owner pass synchronizes the table");
    let attempt = f
        .owner
        .reservations
        .targets()
        .map(|(_, h)| h.attempt.clone())
        .next()
        .unwrap();
    let submitted = State::Submitted(Submission {
        attempt,
        signature: intent.signatures.first().cloned(),
        last_valid_block_height: intent.last_valid_block_height,
    });
    assert_eq!(f.owner.ownership.state(&target), Some(submitted.clone()));
    assert_eq!(f.owner.ownership.counts().submitted, 1);
    // The target's detection is refused and deferred, never evaluated; a later trigger
    // coalesces into the same pending revision (the newest work wins).
    assert!(!f
        .owner
        .detect(Some((target, 5, 12, "oracle_health_ready")))
        .unwrap());
    assert!(!f
        .owner
        .detect(Some((target, 9, 13, "account_update")))
        .unwrap());
    assert_eq!(f.owner.metrics.detect_health_calls, 0);
    assert_eq!(f.owner.candidate_revisions.get(&target), Some(&2));
    let s = f.owner.ownership.stats;
    assert_eq!(
        (s.refused_acquire, s.deferred, s.coalesced, s.acquired),
        (2, 1, 1, 0)
    );
    assert_eq!(
        f.owner
            .ownership
            .pending(&target)
            .map(|p| (p.t_rx_ns, p.slot, p.trigger)),
        Some((9, 13, "account_update"))
    );
    assert!(f.owner.oracle_lanes.is_empty(), "not in a lane while owned");
    assert_eq!(f.owner.ownership.state(&target), Some(submitted));
    // The settlement path (C3): the proven non-landing settles the attempt, the book follows
    // the journal and the ownership follows the book.
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some());
    fe04_answer(
        &f,
        SettlementEvidence {
            observations: vec![],
            non_landing: Some(Ok(proof)),
        },
    );
    f.owner.remote_apply().unwrap();
    assert!(fe04_settled(&f.owner));
    assert!(f.owner.reservations.is_empty());
    assert_eq!(
        f.owner.ownership.state(&target),
        None,
        "RELEASED with the book"
    );
    let s = f.owner.ownership.stats;
    assert_eq!(
        (s.submitted_synced, s.submitted_released, s.rearmed),
        (1, 1, 1)
    );
    let entry = f
        .owner
        .oracle_lanes
        .get(&target)
        .copied()
        .expect("re-armed");
    assert_eq!(
        (entry.lane, entry.work),
        (
            super::candidates::Lane::L0,
            (target, 9, 13, "account_update")
        )
    );
    // Idle passes change nothing.
    assert!(f.owner.lifecycle());
    assert_eq!(f.owner.ownership.stats, s);
    let line = f.owner.identity_line();
    assert!(
        line.contains("ownership: executing=0 signed=0 submitted=0 pending=0 acquired=0"),
        "{line}"
    );
    cleanup(f);
}

/// P6-MT.1 (MT5b) on the wired path — #2/#3/#5: while a candidate owns an account
/// (EXECUTING), the account's triggers are deferred and coalesced (never evaluated twice, never
/// lost); a release by a foreign candidate is refused and the ownership stays; the owner's
/// release re-arms the newest pending revision at L0, and its detection acquires and releases
/// the account again.
#[test]
fn mt5b_an_executing_account_defers_its_triggers_until_the_owner_releases() {
    use crate::execution::ownership::State;
    let mut f = fixture_with("mt5b_executing", None);
    let a = Pubkey::new_unique();
    f.owner.ownership.acquire(a, 99).unwrap();
    assert!(!f
        .owner
        .detect(Some((a, 5, 12, "oracle_health_ready")))
        .unwrap());
    assert!(!f.owner.detect(Some((a, 9, 13, "account_update"))).unwrap());
    assert_eq!(f.owner.metrics.detect_health_calls, 0);
    assert_eq!(
        f.owner.ownership.state(&a),
        Some(State::Executing { candidate_id: 99 })
    );
    // #5: a foreign release never deletes the ownership (counted, logged, not applied).
    f.owner.release_candidate(a, 98);
    assert_eq!(
        f.owner.ownership.state(&a),
        Some(State::Executing { candidate_id: 99 })
    );
    assert_eq!(f.owner.ownership.stats.refused_release, 1);
    assert!(f.owner.oracle_lanes.is_empty());
    // The owner's release re-arms the newest pending revision.
    f.owner.release_candidate(a, 99);
    assert_eq!(f.owner.ownership.state(&a), None);
    let entry = f.owner.oracle_lanes.get(&a).copied().expect("re-armed");
    assert_eq!(entry.work, (a, 9, 13, "account_update"));
    assert_eq!(
        entry.t_rx_first, 5,
        "the first deferred trigger's receive time is kept"
    );
    assert_eq!(entry.lane, super::candidates::Lane::L0);
    // Its detection owns the account for the evaluation and releases it afterwards (the
    // fixture has no state: the evaluation stops at the integration plan, after the acquire).
    let work = f.owner.oracle_lanes.next().unwrap().work;
    f.owner.detect(Some(work)).unwrap();
    assert_eq!(f.owner.ownership.state(&a), None);
    let s = f.owner.ownership.stats;
    assert_eq!(
        (s.acquired, s.released, s.deferred, s.coalesced, s.rearmed),
        (2, 2, 1, 1, 1)
    );
    cleanup(f);
}

/// P6-MT.1/MT.2 (MT5b) on the wired path: the projection result's judge now sees the
/// account's in-flight ownership and newest revision — a result for a target SUBMITTED by a
/// durable attempt is dropped foreign (not cached, no requeue, the ownership untouched); a
/// result of an older revision than the account's newest yields to it (not cached, re-requested
/// once from the current state through the bounded E9 requeue); the current revision without
/// an owner is accepted as before.
#[test]
fn mt5b_projection_results_are_judged_against_ownership_and_newest_revision() {
    use crate::execution::identity::ResultAction;
    use crate::execution::one_shot::ActiveHold;
    use crate::execution::reservation::WalletReservations;
    let mut f = fixture_with("mt5b_judge", None);
    let candidate = Pubkey::new_unique();
    let data = vec![1u8; 8];
    let snapshot = vec![mt5_account(
        &f,
        candidate,
        data.clone(),
        SourceKind::Snapshot,
    )];
    mt5_ready(&mut f, &snapshot);
    let real = solana_sdk::hash::hash(&data);
    let fingerprint = dependency_fingerprint(&f.owner.st, &[]);
    let message = |identity| mt5_result(candidate, real, vec![], fingerprint, identity);
    let identity = test_identity(&f.owner, candidate, &[]);
    // SUBMITTED by a durable attempt (the book's target hold): dropped foreign.
    f.owner.reservations = WalletReservations::rebuild([ActiveHold {
        attempt: "A1".into(),
        target: candidate.to_string(),
        unknown_outcome: true,
        holds: Default::default(),
        signature: Some("sig".into()),
        last_valid_block_height: Some(900),
    }])
    .unwrap();
    f.owner.sync_ownership();
    assert_eq!(f.owner.ownership.counts().submitted, 1);
    mt5_apply(&mut f, message(identity.clone()));
    assert_eq!(
        f.owner
            .metrics
            .result_actions
            .get(ResultAction::DropForeignInflight),
        1
    );
    assert!(!f.owner.projection_cache.contains_key(&candidate));
    assert!(f.owner.detect_queue.is_empty());
    assert_eq!(f.owner.projection_stats.requeued, 0);
    assert_eq!(f.owner.ownership.counts().submitted, 1, "untouched");
    // Released by the book: an older revision than the account's newest yields to it.
    f.owner.reservations = WalletReservations::default();
    f.owner.sync_ownership();
    assert!(f.owner.ownership.is_empty());
    f.owner.candidate_revisions.insert(candidate, 2);
    mt5_apply(&mut f, message(identity.clone()));
    assert_eq!(
        f.owner
            .metrics
            .result_actions
            .get(ResultAction::RetryNewestPending),
        1
    );
    assert!(!f.owner.projection_cache.contains_key(&candidate));
    assert_eq!(f.owner.projection_stats.requeued, 1);
    assert_eq!(
        f.owner.detect_queue.back().map(|w| (w.0, w.3)),
        Some((candidate, TRIGGER_PROJECTION_REQUEUE))
    );
    // The newest revision, nobody owning the account: accepted and cached.
    f.owner.detect_queue.clear();
    f.owner.candidate_revisions.insert(candidate, 1);
    mt5_apply(&mut f, message(identity));
    assert_eq!(f.owner.metrics.result_actions.get(ResultAction::Accept), 1);
    assert!(f.owner.projection_cache.contains_key(&candidate));
    cleanup(f);
}

/// F-01: the operator stop (`p0ctl run stop`, SIGTERM/SIGINT) ends a standalone Shadow owner's
/// loop at the next pass, so `finish` seals its journals instead of the process being killed.
#[test]
fn f01_operator_stop_ends_a_standalone_shadow_owner() {
    let mut f = fixture("f01-stop");
    assert!(f.owner.lifecycle(), "running without a stop");
    f.owner.stop_poll = || true;
    assert!(!f.owner.lifecycle(), "the operator stop ends the loop");
}

/// D07/F-06: a start without a wall clock refuses the FE-04 replay instead of booking a settled
/// attempt into day 0, where today's loss and failure limits would never see it.
#[test]
fn d07_fe04_replay_without_a_wall_clock_is_refused() {
    let (mut f, _, _) = fe04_fixture("fe04_d07", "finalized");
    run_until(&mut f, fe04_settled);
    f.owner.fe04_held = None;
    let dir_s = f.dir.join("fe04").display().to_string();
    let liquidator = f.owner.liquidator;
    let book = || {
        setup::reservation_book(
            &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
            &liquidator,
        )
        .unwrap()
    };
    let mut safety = Safety::new(e2e_limits(), Allow::default(), false);
    let refused = book().start(&mut safety, 0).err();
    assert!(
        refused
            .as_deref()
            .is_some_and(|e| e.starts_with("CLOCK_UNAVAILABLE")),
        "{refused:?}"
    );
    assert_eq!(safety.day().failed_attempts, 0, "nothing booked");
    let mut safety = Safety::new(e2e_limits(), Allow::default(), false);
    book().start(&mut safety, wall_unix_s().unwrap()).unwrap();
    assert_eq!(safety.day().failed_attempts, 1, "booked into today");
    cleanup(f);
}

/// B01 (P8-INV-01): rebalancing in OBSERVE from the wallet's real balances in canonical state
/// (the liquidator's lamports, a validated ATA). The owner asks the full ExactIn quote off its
/// thread; the answer leads to the quote of exactly the shortfall's amount; without a route that
/// asset stays blocked with an unknown cost (never traded, never guessed). The plan and the
/// OBSERVE capability reach the projection; the next observation waits for its interval.
#[test]
fn b01_rebalancing_observe_plans_from_real_balances_and_quotes_without_trading() {
    use crate::execution::inventory::{RebalanceMode, Target};
    use crate::orchestration::remote_workers::RebalanceQuoteResult;
    let mut f = fixture("b01_rebalance");
    let good = snapshot(&f, false);
    let mut fetch = || {
        Ok(SnapshotData {
            slot: good.slot,
            accounts: good.accounts.clone(),
            missing_required: vec![],
        })
    };
    // Not before canonical state is Ready: a bootstrapping owner plans nothing.
    f.owner.rebalance = super::rebalance::Observer::new(
        crate::execution::inventory::RebalanceMode::Observe,
        &OPERATIONAL,
    );
    f.owner.inventory_targets = [(
        "SOL".to_string(),
        Target {
            reserve: 0,
            min: 1,
            target: 2,
            max: 3,
            dust: 0,
        },
    )]
    .into();
    f.owner.rebalance_observe();
    assert_eq!(
        f.owner.owner_snapshot(1).rebalance.unwrap().at_unix_ms,
        None
    );
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert_eq!(
        f.owner.st.run_state(),
        darkbot_common::lifecycle::RunState::Ready
    );
    let mint = Pubkey::new_unique();
    let band = |min, target, max| Target {
        reserve: 0,
        min,
        target,
        max,
        dust: 5,
    };
    f.owner.inventory_targets = [
        (
            "SOL".to_string(),
            band(1_000_000_000, 2_000_000_000, 3_000_000_000),
        ),
        (mint.to_string(), band(100, 200, 300)),
    ]
    .into();
    let slot = f.owner.st.highest_slot() + 1;
    let account = |pubkey, owner, lamports, data| {
        IngressEvent::Account(AccountUpdate {
            pubkey,
            owner,
            lamports,
            data,
            slot,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 1,
        })
    };
    f.owner.st.on_event(account(
        f.owner.liquidator,
        Pubkey::default(),
        10_000_000_000,
        vec![],
    ));
    let (token_program, ata) = f.owner.atas.of(&mint)[0];
    let mut data = vec![0u8; 165];
    data[..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(f.owner.liquidator.as_ref());
    data[64..72].copy_from_slice(&50u64.to_le_bytes());
    data[108] = 1;
    f.owner
        .st
        .on_event(account(ata, token_program, 2_039_280, data));
    f.owner.rebalance = super::rebalance::Observer::new(RebalanceMode::Observe, &OPERATIONAL);
    let (jobs, results) = f.owner.remote.detached_rebalance();
    f.owner.rebalance_observe();
    let job = jobs.try_pop().expect("the full surplus quote is asked");
    assert_eq!(
        job.key,
        ("SOL".to_string(), mint.to_string(), 8_000_000_000)
    );
    assert_eq!(
        job.sell_mint.to_string(),
        darkbot_project0::protocol::NATIVE_SOL_MINT
    );
    assert_eq!(job.buy_mint, mint);
    let plan = |f: &Fixture| f.owner.owner_snapshot(1).rebalance.unwrap();
    assert_eq!(plan(&f).quotes_pending, 1);
    assert!(plan(&f).trades.is_empty());
    // A pass while the quote is pending changes nothing.
    f.owner.rebalance_observe();
    assert!(jobs.try_pop().is_none());
    results
        .try_push(RebalanceQuoteResult {
            key: job.key,
            minimum_out: Ok(Some(400)),
        })
        .ok()
        .unwrap();
    f.owner.rebalance_observe();
    // 8e9 lamports quote 400: the shortfall of 150 needs ceil(8e9 * 150 / 400).
    let scaled = jobs.try_pop().expect("the shortfall's quote is asked");
    assert_eq!(scaled.key.2, 3_000_000_000);
    results
        .try_push(RebalanceQuoteResult {
            key: scaled.key,
            minimum_out: Ok(None),
        })
        .ok()
        .unwrap();
    f.owner.rebalance_observe();
    let s = f.owner.owner_snapshot(1);
    let r = s.rebalance.unwrap();
    assert_eq!((r.mode, r.quotes_pending), ("OBSERVE", 0));
    assert!(
        r.trades.is_empty(),
        "nothing is traded without a known cost"
    );
    assert_eq!(
        r.blocked,
        vec![(mint.to_string(), "REBALANCE_COST_UNKNOWN")]
    );
    assert_eq!(r.budget_usd_micro, OPERATIONAL.rebalance_budget_usd_micro);
    let cap = s
        .capabilities
        .iter()
        .find(|c| c.name == "INVENTORY_REBALANCING")
        .unwrap();
    assert_eq!((cap.requested, cap.effective), ("OBSERVE", "OBSERVE"));
    // The next observation waits for its interval.
    f.owner.rebalance_observe();
    assert!(jobs.try_pop().is_none());
    cleanup(f);
}

/// DEC-10 (N-5): the Liquidator projection carries the status of the configured FE-04 approval
/// (native text, bound to this owner's wallet, program, ProgramData and ceilings): none, refused
/// without an FE-04 journal, valid, spent once an attempt used it, JSON refused — never a LIVE
/// authorization.
#[test]
fn dec10_projection_shows_the_configured_approval_status() {
    use crate::execution::one_shot::approval_file::{
        ApprovalBinding, Bind, Ceilings, Usage, ATTEMPT_SCHEMA,
    };
    let mut f = fixture("dec10");
    assert_eq!(f.owner.owner_snapshot(1).approval.state, "NONE");
    let wallet = f.owner.liquidator.to_string();
    let file = f.dir.join("approval.conf");
    let hash = solana_sdk::hash::Hash::new_unique().to_string();
    let expires = darkbot_common::timing::wall_now().unwrap().as_secs() + 3_600;
    std::fs::write(
        &file,
        format!(
            "schema = {ATTEMPT_SCHEMA}\nauthorized = true\napproval_id = pilot-1\ncode_sha = {}\nbinary_sha256 = {}\nwallet = {wallet}\nprogram = {}\nprogram_data_hash = {hash}\nfunding_class = NO_SWAP_INVENTORY\nallowed_targets = {}\nexpires_unix_s = {expires}\nmax_state_age_ms = 500\nmax_capital_usd_micro = 10000000\nmax_total_sol_debit_lamports = 50000\nmax_tip_lamports = 0\nmax_rent_lamports = 3000000\nmax_loss_usd_micro = 1000000\nmin_net_usd_micro = 100\nmax_slippage_bps = 50\n",
            "a".repeat(40),
            "b".repeat(64),
            f.owner.program,
            solana_sdk::pubkey::Pubkey::new_unique()
        ),
    )
    .unwrap();
    let journal = f.dir.join("fe04");
    std::fs::create_dir_all(&journal).unwrap();
    let mut source = ApprovalSource {
        path: file.clone(),
        binding: ApprovalBinding {
            wallet: Bind::Known(wallet.clone()),
            program: Bind::Known(f.owner.program.to_string()),
            program_data_hash: Bind::Known(hash),
            code_sha: Bind::Known("a".repeat(40)),
            binary_sha256: Bind::Known("b".repeat(64)),
            ceilings: Some(Ceilings {
                max_state_age_ms: 2_000,
                max_slippage_bps: 50,
                max_tip_lamports: 0,
                max_capital_usd_micro: 10_000_000,
                max_loss_usd_micro: 1_000_000,
            }),
            max_lifetime_seconds: 86_400,
        },
        read: Err("APPROVAL_NOT_READ".into()),
        usage: Usage::Unchecked("APPROVAL_NOT_READ".into()),
    };
    // Without an FE-04 journal the single use cannot be checked: never VALID.
    source.refresh(None, &wallet);
    f.owner.fe04_approval = Some(source);
    let s = f.owner.owner_snapshot(1).approval;
    assert_eq!(
        (s.state, s.reason.as_str()),
        ("INVALID", "FE04_JOURNAL_NOT_CONFIGURED")
    );
    let refresh = |f: &mut Fixture| {
        f.owner
            .fe04_approval
            .as_mut()
            .unwrap()
            .refresh(Some(&journal), &wallet)
    };
    refresh(&mut f);
    let s = f.owner.owner_snapshot(1).approval;
    assert_eq!(
        (s.state, s.kind, s.approval_id.as_str()),
        ("VALID", "attempt", "pilot-1"),
        "{s:?}"
    );
    // An unreadable archive never proves the approval unspent.
    let spent = journal.join("attempts").join(&wallet);
    std::fs::create_dir_all(&spent).unwrap();
    std::fs::write(spent.join("pilot-1.p0j"), b"not a journal").unwrap();
    refresh(&mut f);
    let s = f.owner.owner_snapshot(1).approval;
    assert_eq!(
        (s.state, s.reason.as_str()),
        ("INVALID", "FE04_JOURNAL_UNREADABLE")
    );
    std::fs::remove_dir_all(journal.join("attempts")).unwrap();
    // A JSON approval file is refused, never adopted.
    std::fs::write(&file, b"{\"schema\":1}").unwrap();
    refresh(&mut f);
    let s = f.owner.owner_snapshot(1).approval;
    assert!(
        s.state == "INVALID" && s.reason.starts_with("APPROVAL_JSON_REFUSED"),
        "{s:?}"
    );
}

/// DEC-01: a blockhash the refresh worker would store now (`fresh`) or one already older than
/// the owner's maximum age (the tests set it to 1 ms after the clock passed 2 ms).
fn dec01_blockhash(f: &Fixture, fresh: bool) -> darkbot_solana::state::blockhash::CachedBlockhash {
    darkbot_solana::state::blockhash::CachedBlockhash {
        hash: solana_sdk::hash::Hash::new_unique(),
        last_valid_block_height: 1,
        refreshed_ns: if fresh {
            f.owner.clock.now_ns() + 60_000_000_000
        } else {
            0
        },
        context_slot: None,
    }
}

fn dec01_fixture(tag: &str) -> Fixture {
    let mut f = fixture_with(tag, None);
    f.owner.blockhash_max_age_ns = 1_000_000;
    std::thread::sleep(Duration::from_millis(2));
    f
}

fn dec01_origin(
    lane: super::candidates::Lane,
    t_rx_first_ns: u64,
    revision: u64,
) -> super::generations::DetectOrigin {
    super::generations::DetectOrigin {
        lane,
        t_rx_first_ns,
        revision,
        last_forwarded_slot: 4,
    }
}

fn dec01_refused(generation: u64) -> Option<crate::execution::pipeline::BlockhashPark> {
    Some(crate::execution::pipeline::BlockhashPark::Refused { generation })
}

/// DEC-01 (V2-09) on the owner: a lane candidate refused before planning wakes the refresh
/// worker once (later refusals coalesce) and is parked — one entry per account, a second
/// refusal coalesced (never demoted: L1 stays L1); a detection without a lane origin is never
/// parked. Nothing returns before a newer blockhash however often the owner passes (no busy
/// loop); after one, each returns once to its original lane with its original work, first
/// receive time, revision and L2 key, never as a second entry. Every step is a typed `.p0j`
/// record (the re-arm one record), the Liquidator projection counts it and the shutdown journal
/// carries the counters.
#[test]
fn dec01_a_refused_lane_candidate_is_parked_and_returns_to_its_lane_after_a_newer_blockhash() {
    use super::candidates::Lane;
    use crate::analytics::evidence::{BlockhashParkEvent, RecordEvent};
    use crate::analytics::payload::{decode, OwnerDiagnostic, Payload};
    let mut f = dec01_fixture("dec01_park");
    let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
    let g = f.owner.blockhash.generation();
    for (work, origin) in [
        (
            (a, 5, 12, "oracle_health_ready"),
            Some(dec01_origin(Lane::L1, 3, 7)),
        ),
        (
            (a, 9, 13, "account_update"),
            Some(dec01_origin(Lane::L2, 9, 8)),
        ),
        (
            (b, 6, 12, "oracle_health_ready"),
            Some(dec01_origin(Lane::L0, 6, 2)),
        ),
        (
            (Pubkey::new_unique(), 7, 12, "integration_projection"),
            None,
        ),
    ] {
        f.owner
            .blockhash_after_detect(work, origin, dec01_refused(g))
            .unwrap();
    }
    let s = f.owner.blockhash_parking.stats;
    assert_eq!(
        (
            f.owner.blockhash_parking.len(),
            s.refused,
            s.merged,
            s.refresh_requests,
            s.refresh_coalesced
        ),
        (2, 3, 1, 1, 2)
    );
    assert!(
        f.owner.blockhash.take_refresh_request(),
        "the worker was woken"
    );
    let trigger = f.owner.owner_snapshot(0).trigger;
    assert_eq!(
        (
            trigger.blockhash_refused,
            trigger.blockhash_parked,
            trigger.blockhash_refresh_requests
        ),
        (3, 2, 1)
    );
    for _ in 0..3 {
        f.owner.rearm_blockhash_parked().unwrap();
    }
    assert!(
        f.owner.oracle_lanes.is_empty(),
        "nothing before a newer blockhash"
    );
    let fresh = dec01_blockhash(&f, true);
    f.owner.blockhash.set(fresh);
    f.owner.rearm_blockhash_parked().unwrap();
    let entry = f.owner.oracle_lanes.get(&a).copied().expect("re-armed");
    assert_eq!(
        (
            entry.lane,
            entry.work,
            entry.t_rx_first,
            entry.revision,
            entry.last_forwarded_slot
        ),
        (Lane::L1, (a, 9, 13, "account_update"), 3, 8, 4)
    );
    assert_eq!(f.owner.oracle_lanes.get(&b).map(|e| e.lane), Some(Lane::L0));
    f.owner.rearm_blockhash_parked().unwrap();
    assert_eq!(f.owner.oracle_lanes.len(), 2, "never a second entry");
    assert_eq!(
        (
            f.owner.blockhash_parking.len(),
            f.owner.blockhash_parking.stats.rearmed
        ),
        (0, 2)
    );
    assert_eq!(f.owner.owner_snapshot(0).trigger.blockhash_rearmed, 2);
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let (mut events, mut rearms, mut lines) = (vec![], vec![], vec![]);
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == RecordEvent::KIND {
            match decode(frame.kind, frame.schema, &frame.payload).unwrap() {
                RecordEvent::BlockhashPark { event, entry, .. } => events.push((
                    entry.account,
                    event,
                    entry.lane.into_owned(),
                    entry.t_rx_ns,
                    entry.generation,
                )),
                RecordEvent::BlockhashRearm {
                    cache_generation,
                    entries,
                } => rearms.push((
                    cache_generation,
                    entries
                        .into_iter()
                        .map(|e| (e.account, e.lane.into_owned(), e.t_rx_first_ns))
                        .collect::<Vec<_>>(),
                )),
                _ => {}
            }
        } else if frame.kind == OwnerDiagnostic::KIND {
            let d: OwnerDiagnostic = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            lines.push(d.line);
        }
    }
    use BlockhashParkEvent::{Merged, Parked};
    assert_eq!(
        events,
        [
            (a, Parked, "L1".to_string(), 5, g),
            (a, Merged, "L1".to_string(), 9, g),
            (b, Parked, "L0".to_string(), 6, g),
        ]
    );
    assert_eq!(
        rearms,
        [(
            g + 1,
            vec![(a, "L1".to_string(), 3), (b, "L0".to_string(), 6)]
        )]
    );
    assert!(
        lines.iter().any(|l| l
            == "blockhash_park: refused=3 merged=1 rearmed=2 superseded=0 parked_at_end=0 \
                refresh_requests=1 refresh_coalesced=2"),
        "{lines:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-01: a refresh that lands between the refusal's freshness check and the parking still
/// returns the candidate (the refusal carries the generation read before its check); a
/// candidate refused on the newest blockhash waits for the next one; only an evaluation past a
/// fresh blockhash with work at least as new supersedes a parked entry — one that never reached
/// the check, saw a stale blockhash without parking or carried older work leaves it parked
/// (never evaluated twice, never lost).
#[test]
fn dec01_a_racing_refresh_is_never_missed_and_only_a_fresh_evaluation_supersedes() {
    use super::candidates::Lane;
    use crate::execution::pipeline::BlockhashPark;
    let mut f = dec01_fixture("dec01_race");
    let (a, b, c, d) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let g = f.owner.blockhash.generation();
    let fresh = dec01_blockhash(&f, true);
    f.owner.blockhash.set(fresh);
    f.owner
        .blockhash_after_detect(
            (a, 5, 12, "oracle_health_ready"),
            Some(dec01_origin(Lane::L1, 5, 1)),
            dec01_refused(g),
        )
        .unwrap();
    // Refused on the newest blockhash: it waits for the next one.
    let newest = f.owner.blockhash.generation();
    f.owner
        .blockhash_after_detect(
            (c, 6, 12, "oracle_health_ready"),
            Some(dec01_origin(Lane::L2, 6, 1)),
            dec01_refused(newest),
        )
        .unwrap();
    f.owner.rearm_blockhash_parked().unwrap();
    assert_eq!(f.owner.oracle_lanes.get(&a).map(|e| e.lane), Some(Lane::L1));
    assert!(
        f.owner.oracle_lanes.get(&c).is_none(),
        "waits for a newer blockhash"
    );
    let fresh = dec01_blockhash(&f, true);
    f.owner.blockhash.set(fresh);
    f.owner.rearm_blockhash_parked().unwrap();
    assert_eq!(f.owner.oracle_lanes.get(&c).map(|e| e.lane), Some(Lane::L2));
    let g = f.owner.blockhash.generation();
    for (account, slot) in [(b, 12), (d, 14)] {
        f.owner
            .blockhash_after_detect(
                (account, 5, slot, "oracle_health_ready"),
                Some(dec01_origin(Lane::L2, 5, 1)),
                dec01_refused(g),
            )
            .unwrap();
    }
    for (early, work) in [
        (None, (d, 9, 15, "account_update")),
        (Some(BlockhashPark::Off), (d, 9, 15, "account_update")),
        (Some(BlockhashPark::Passed), (d, 9, 13, "account_update")),
    ] {
        f.owner.blockhash_after_detect(work, None, early).unwrap();
        assert!(
            f.owner.blockhash_parking.get(&d).is_some(),
            "{early:?} {work:?}"
        );
    }
    f.owner
        .blockhash_after_detect(
            (b, 8, 14, "account_update"),
            None,
            Some(BlockhashPark::Passed),
        )
        .unwrap();
    let s = f.owner.blockhash_parking.stats;
    assert_eq!((f.owner.blockhash_parking.len(), s.superseded), (1, 1));
    let fresh = dec01_blockhash(&f, true);
    f.owner.blockhash.set(fresh);
    f.owner.rearm_blockhash_parked().unwrap();
    assert!(
        f.owner.oracle_lanes.get(&b).is_none(),
        "covered, not evaluated twice"
    );
    assert!(f.owner.oracle_lanes.get(&d).is_some(), "never lost");
    cleanup(f);
}

/// DEC-01: a newer blockhash ends the owner's idle wait while candidates are parked; one that
/// is already stale on arrival re-arms nothing, asks the worker again and is not looked at
/// again (no spin); the next fresh one re-arms. A lane detection that ends before the
/// pipeline (here: no state, the integration plan) reaches no blockhash check and leaves the
/// account's parked entry in place.
#[test]
fn dec01_a_stale_newer_blockhash_rearms_nothing_and_an_unevaluated_detection_keeps_the_entry() {
    use super::candidates::Lane;
    let mut f = dec01_fixture("dec01_stale_newer");
    let a = Pubkey::new_unique();
    let g = f.owner.blockhash.generation();
    f.owner
        .blockhash_after_detect(
            (a, 5, 12, "oracle_health_ready"),
            Some(dec01_origin(Lane::L0, 5, 1)),
            dec01_refused(g),
        )
        .unwrap();
    assert!(f.owner.blockhash.take_refresh_request());
    assert!(!f
        .owner
        .blockhash_parking
        .pending(f.owner.blockhash.generation()));
    let stale = dec01_blockhash(&f, false);
    f.owner.blockhash.set(stale);
    assert!(f
        .owner
        .blockhash_parking
        .pending(f.owner.blockhash.generation()));
    f.owner.rearm_blockhash_parked().unwrap();
    assert!(f.owner.oracle_lanes.is_empty());
    assert!(f.owner.blockhash.take_refresh_request(), "asked again");
    assert!(!f
        .owner
        .blockhash_parking
        .pending(f.owner.blockhash.generation()));
    f.owner.detect_origin = Some(dec01_origin(Lane::L0, 9, 2));
    f.owner.detect(Some((a, 9, 13, "account_update"))).unwrap();
    assert_eq!(f.owner.blockhash_early, None);
    assert!(f.owner.blockhash_parking.get(&a).is_some(), "kept");
    let fresh = dec01_blockhash(&f, true);
    f.owner.blockhash.set(fresh);
    f.owner.rearm_blockhash_parked().unwrap();
    assert_eq!(f.owner.oracle_lanes.get(&a).map(|e| e.lane), Some(Lane::L0));
    cleanup(f);
}

/// DEC-13: the owner decides capability requests: a valid LIVE request becomes the requested
/// value while LIVE stays effective BLOCKED (FE-06) in the projection, the outcome record and the
/// detection's pipeline mode (Shadow); an unknown or unreadable request is refused and changes
/// nothing; OFF withdraws the request.
#[test]
fn dec13_a_live_request_is_recorded_and_never_effective() {
    use crate::analytics::payload::CapabilityControlRequest;
    use crate::orchestration::capability_control;
    let mut f = fixture_with("dec13_live", None);
    let dir = f.dir.join("capability-control");
    f.owner.sources.capabilities.dir = Some(dir.clone());
    f.owner.sources.capabilities.config_version = Some(0);
    let request = |seq, capability: &str, requested: &str| {
        Ok(CapabilityControlRequest {
            seq,
            capability: capability.into(),
            requested: requested.into(),
            requested_unix_s: 1,
            request_id: format!("t-{seq}"),
            owner_epoch: 0,
        })
    };
    f.owner
        .apply_capability_control(1, request(1, "LIVE", "LIVE"));
    let snapshot = f.owner.owner_snapshot(0);
    let live = snapshot
        .capabilities
        .iter()
        .find(|c| c.name == "LIVE")
        .unwrap();
    assert_eq!((live.requested, live.effective), ("LIVE", "BLOCKED"));
    // P8-CTRL.2: every missing gate, the first one first — FE-06 among them.
    assert!(live
        .block_reason
        .starts_with("ECON_LIVE DISABLED (missing too:"));
    assert!(live
        .block_reason
        .ends_with(&format!("{})", capability_control::LIVE_BLOCK_REASON)));
    assert_eq!(snapshot.capability_config_version, Some(1));
    assert!(snapshot.last_capability_decision.unwrap().applied);
    let outcome = capability_control::latest_outcome(&dir).unwrap().unwrap();
    assert_eq!(
        (
            outcome.seq,
            outcome.requested.as_str(),
            outcome.effective.as_str(),
            outcome.applied
        ),
        (1, "LIVE", "BLOCKED", true)
    );
    assert_eq!(
        capability_control::pipeline_mode(&f.owner.sources.live_gates()),
        crate::execution::pipeline::Mode::Shadow
    );
    f.owner
        .apply_capability_control(2, request(2, "JITO", "LIVE"));
    f.owner
        .apply_capability_control(3, Err("CAPABILITY_CONTROL_REQUEST_INVALID: x".into()));
    let c = &f.owner.sources.capabilities;
    assert_eq!(
        (c.live_requested, c.applied, c.refused, c.config_version),
        ("LIVE", 1, 2, Some(3))
    );
    assert!(
        !capability_control::latest_outcome(&dir)
            .unwrap()
            .unwrap()
            .applied
    );
    f.owner
        .apply_capability_control(4, request(4, "LIVE", "OFF"));
    assert_eq!(f.owner.sources.capabilities.live_requested, "OFF");
    cleanup(f);
}

/// P8-CTRL-01: the owner decides the economic and wallet switches of its epoch — each a partial
/// permission, effective as requested and recorded with the request's id and the epoch; the same
/// id again has no second effect, with other content it is a conflict; another epoch, an
/// unconfigured signer or a missing id is refused. LIVE stays BLOCKED with every switch and the
/// signer ENABLED until FE-06 (its only missing gate then), and any one switch taken back blocks
/// it again; an ENABLED without its receipt does not stand; a new owner starts all DISABLED.
#[test]
fn p8ctrl01_the_owner_decides_switches_per_epoch_and_live_needs_every_gate() {
    use crate::analytics::payload::CapabilityControlRequest;
    use crate::orchestration::capability_control as cap;
    let mut f = fixture_with("p8ctrl01", None);
    let dir = f.dir.join("capability-control");
    let signer = f.owner.liquidator;
    {
        let c = &mut f.owner.sources.capabilities;
        (c.dir, c.config_version, c.epoch) = (Some(dir.clone()), Some(0), Some(3));
        c.wallets = vec![cap::WalletSwitch::configured(
            signer,
            &[cap::ROLE_LIQUIDATOR, cap::ROLE_FEE_PAYER],
        )];
    }
    let request = |seq, capability: &str, requested: &str, id: &str, epoch| {
        Ok(CapabilityControlRequest {
            seq,
            capability: capability.into(),
            requested: requested.into(),
            requested_unix_s: 1,
            request_id: id.into(),
            owner_epoch: epoch,
        })
    };
    let shown = |f: &Fixture, name: &str| {
        let c = f
            .owner
            .owner_snapshot(0)
            .capabilities
            .into_iter()
            .find(|c| c.name == name)
            .unwrap();
        (
            c.requested.to_string(),
            c.effective.to_string(),
            c.block_reason.to_string(),
        )
    };
    let detail = |seq| cap::outcome_at(&dir, seq).unwrap().unwrap();
    f.owner
        .apply_capability_control(1, request(1, cap::ECON_SIGN, cap::ENABLED, "a", 3));
    assert_eq!(
        shown(&f, cap::ECON_SIGN),
        ("ENABLED".into(), "ENABLED".into(), String::new())
    );
    let r = detail(1);
    assert_eq!(
        (
            r.applied,
            r.capability.as_str(),
            r.request_id.as_str(),
            r.owner_epoch,
            r.effective.as_str()
        ),
        (true, cap::ECON_SIGN, "a", 3, "ENABLED")
    );
    let unknown = format!("WALLET:{}", Pubkey::new_unique());
    for (seq, capability, value, id, epoch, refusal) in [
        (
            2,
            cap::ECON_SIGN,
            cap::ENABLED,
            "a",
            3,
            "CAPABILITY_REQUEST_REPLAYED",
        ),
        (
            3,
            cap::ECON_SIGN,
            cap::DISABLED,
            "a",
            3,
            "CAPABILITY_REQUEST_ID_CONFLICT",
        ),
        (
            4,
            cap::ECON_ARM,
            cap::ENABLED,
            "b",
            2,
            "CAPABILITY_EPOCH_STALE",
        ),
        (
            5,
            unknown.as_str(),
            cap::ENABLED,
            "c",
            3,
            "WALLET_NOT_CONFIGURED",
        ),
        (
            6,
            cap::ECON_ARM,
            cap::ENABLED,
            "",
            3,
            "CAPABILITY_REQUEST_ID_INVALID",
        ),
    ] {
        f.owner
            .apply_capability_control(seq, request(seq, capability, value, id, epoch));
        let o = detail(seq);
        assert!(
            !o.applied && o.detail.starts_with(refusal),
            "{seq}: {}",
            o.detail
        );
    }
    let c = &f.owner.sources.capabilities;
    assert_eq!(
        (
            c.econ.get(cap::ECON_SIGN),
            c.econ.get(cap::ECON_ARM),
            c.applied,
            c.refused
        ),
        (Some(true), Some(false), 1, 5)
    );
    // Every switch and the signer: LIVE needs FE-06 alone.
    let wallet = format!("WALLET:{signer}");
    let mut seq = 7;
    for capability in [
        cap::ECON_LIVE,
        cap::ECON_ARM,
        cap::ECON_SEND,
        cap::ECON_ATTEMPT,
        wallet.as_str(),
    ] {
        f.owner.apply_capability_control(
            seq,
            request(seq, capability, cap::ENABLED, &format!("on-{seq}"), 3),
        );
        assert!(detail(seq).applied, "{capability}");
        seq += 1;
    }
    f.owner
        .apply_capability_control(seq, request(seq, cap::LIVE, cap::LIVE, "live", 3));
    assert_eq!(
        shown(&f, cap::LIVE),
        (
            "LIVE".into(),
            "BLOCKED".into(),
            cap::LIVE_BLOCK_REASON.into()
        )
    );
    assert_eq!(
        shown(&f, &wallet),
        ("ENABLED".into(), "ENABLED".into(), String::new())
    );
    assert_eq!(
        (
            detail(seq).effective.as_str(),
            detail(seq).block_reason.as_str()
        ),
        ("BLOCKED", cap::LIVE_BLOCK_REASON)
    );
    let admission = f.owner.owner_snapshot(0).admission;
    assert_eq!(
        (admission.state, admission.block_reason.as_str()),
        ("OFF", cap::LIVE_BLOCK_REASON)
    );
    assert!(!f.owner.live_effective());
    // With the FE-06 stand-in every gate holds; one switch taken back blocks LIVE again.
    f.owner.sources.capabilities.fe06 = Some(cap::Fe06Authorization::offline_test());
    assert!(f.owner.live_effective() && f.owner.exit_live_effective());
    assert_eq!(shown(&f, cap::LIVE).1, "LIVE");
    // Review M1: ECON_ATTEMPT stops new liquidations only — an open position still exits.
    seq += 1;
    f.owner.apply_capability_control(
        seq,
        request(seq, cap::ECON_ATTEMPT, cap::DISABLED, "no-new", 3),
    );
    assert!(!f.owner.live_effective() && f.owner.exit_live_effective());
    seq += 1;
    f.owner.apply_capability_control(
        seq,
        request(seq, cap::ECON_ATTEMPT, cap::ENABLED, "new-again", 3),
    );
    assert!(f.owner.live_effective());
    // Review M3: a request DEC-13 wrote (schema 2: no id, epoch 0) is refused by this owner.
    seq += 1;
    let mut old = request(seq, cap::LIVE, cap::LIVE_OFF, "", 0).unwrap();
    old.request_id.clear();
    f.owner.apply_capability_control(seq, Ok(old));
    assert!(detail(seq).detail.starts_with("CAPABILITY_EPOCH_STALE"));
    assert_eq!(f.owner.sources.capabilities.live_requested, cap::LIVE);
    // Review L1: a refused id stays decided — the same id with other content is a conflict.
    seq += 1;
    f.owner
        .apply_capability_control(seq, request(seq, cap::ECON_LIVE, "ON", "refused-1", 3));
    seq += 1;
    f.owner.apply_capability_control(
        seq,
        request(seq, cap::ECON_LIVE, cap::DISABLED, "refused-1", 3),
    );
    assert!(detail(seq)
        .detail
        .starts_with("CAPABILITY_REQUEST_ID_CONFLICT"));
    assert!(f.owner.live_effective());
    seq += 1;
    f.owner
        .apply_capability_control(seq, request(seq, cap::ECON_SEND, cap::DISABLED, "off", 3));
    assert!(!f.owner.live_effective());
    assert_eq!(
        shown(&f, cap::LIVE),
        ("LIVE".into(), "BLOCKED".into(), "ECON_SEND DISABLED".into())
    );
    // The receipts are the epoch's decisions.
    let decided: Vec<(String, String)> = cap::epoch_decisions(&dir, 3)
        .unwrap()
        .into_iter()
        .map(|o| (o.capability, o.requested))
        .collect();
    assert!(decided.contains(&(cap::ECON_SEND.into(), cap::DISABLED.into())));
    assert!(decided.contains(&(wallet.clone(), cap::ENABLED.into())));
    // A permission stands only recorded: no receipt, no ENABLED.
    let blocked = f.dir.join("capability-blocked");
    std::fs::write(&blocked, b"not a directory").unwrap();
    f.owner.sources.capabilities.dir = Some(blocked);
    seq += 1;
    f.owner.apply_capability_control(
        seq,
        request(seq, cap::ECON_SEND, cap::ENABLED, "unrecorded", 3),
    );
    assert_eq!(
        f.owner.sources.capabilities.econ.get(cap::ECON_SEND),
        Some(false)
    );
    assert!(!f.owner.live_effective());
    // Review L2: the projected decision says it was not recorded.
    let last = f.owner.sources.capabilities.last_decision.clone().unwrap();
    assert!(
        !last.applied && last.detail.contains("RECEIPT_NOT_RECORDED"),
        "{}",
        last.detail
    );
    cleanup(f);
    // A new owner: a fresh start, every switch DISABLED and LIVE requested OFF.
    let fresh = super::sources::OwnerCapabilities::default();
    assert!(cap::ECON.iter().all(|e| fresh.econ.get(e) == Some(false)));
    assert_eq!(
        (fresh.live_requested, fresh.epoch, fresh.fe06.is_none()),
        (cap::LIVE_OFF, None, true)
    );
}

/// P8-CTRL-02: no grant is taken while the operator authentication is undecided; the grant in
/// force is revoked by its digest only — LIVE is BLOCKED at once and the submitter refuses again —
/// and another digest revokes nothing.
#[test]
fn p8ctrl02_no_grant_without_the_operator_and_a_revocation_ends_it_at_once() {
    use crate::analytics::payload::CapabilityControlRequest;
    use crate::orchestration::{capability_control as cap, fe06};
    use darkbot_solana::submit::SubmissionAdapter;
    let mut f = fixture_with("p8ctrl02", None);
    let dir = f.dir.join("capability-control");
    let scope = fe06::parse(
        fe06::tests::text(&f.owner.liquidator, &[]).as_bytes(),
        &fe06::tests::ceilings(),
    )
    .unwrap();
    let digest = fe06::digest(&scope).unwrap();
    {
        let c = &mut f.owner.sources.capabilities;
        (c.dir, c.config_version, c.epoch) = (Some(dir.clone()), Some(0), Some(3));
    }
    let request = |seq, capability: &str, requested: &str| {
        Ok(CapabilityControlRequest {
            seq,
            capability: capability.into(),
            requested: requested.into(),
            requested_unix_s: 1,
            request_id: format!("r-{seq}"),
            owner_epoch: 3,
        })
    };
    let detail = |seq| cap::outcome_at(&dir, seq).unwrap().unwrap();
    f.owner
        .apply_capability_control(1, request(1, cap::FE06_GRANT, &digest));
    let o = detail(1);
    assert!(
        !o.applied && o.detail == cap::FE06_AUTH_UNDECIDED,
        "{}",
        o.detail
    );
    assert!(f.owner.sources.capabilities.fe06.is_none());
    // A grant in force (the offline tests' stand-in of an authenticated one), the submitter open.
    let grant = cap::Fe06Authorization::offline_test_of(scope);
    f.owner.submitter.open(&grant);
    f.owner.sources.capabilities.fe06 = Some(grant);
    f.owner
        .apply_capability_control(2, request(2, cap::FE06_REVOKE, &"0".repeat(64)));
    assert!(detail(2).detail.starts_with("FE06_NOT_IN_FORCE"));
    assert!(f.owner.sources.capabilities.fe06.is_some());
    f.owner
        .apply_capability_control(3, request(3, cap::FE06_REVOKE, &digest));
    let o = detail(3);
    assert!(o.applied, "{}", o.detail);
    assert_eq!(
        (
            o.capability.as_str(),
            o.requested.as_str(),
            o.effective.as_str()
        ),
        (cap::FE06_REVOKE, digest.as_str(), "NONE")
    );
    assert!(f.owner.sources.capabilities.fe06.is_none() && !f.owner.live_effective());
    // The submitter refuses again (counted, never forwarded).
    let refused = f
        .owner
        .submitter
        .attempts
        .load(std::sync::atomic::Ordering::Relaxed);
    let tx = solana_sdk::transaction::VersionedTransaction::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    assert!(f.owner.submitter.submit(&tx, 0, deadline).is_err());
    assert_eq!(
        f.owner
            .submitter
            .attempts
            .load(std::sync::atomic::Ordering::Relaxed),
        refused + 1
    );
    cleanup(f);
}

/// DEC-16 (D-4, D-7): the real owner pass of an unbounded Shadow run opens its first window at
/// the first tradable READY; the operator's stop ends the owner loop with that reason, the
/// winner watches drain, and `finish` ends the run in order — the open window's record carries
/// the stop and the windows journal is sealed complete.
#[test]
fn dec16_the_operator_stop_ends_an_unbounded_shadow_run_in_order() {
    use crate::orchestration::until_stopped as us;
    let mut f = fixture("dec16_stop");
    let mut b = us::tests::binding(60, 120);
    b.mode = "shadow".into();
    let rolling = us::Rolling {
        project: f.dir.clone(),
        binding: b,
        qualification_id: "0123456789abcdef".into(),
        config: darkbot_solana::runtime_config::RuntimeConfig::default(),
        max_runtime_seconds: None,
        resume: None,
        autostart: None,
    };
    let mt0 = Arc::new(std::sync::atomic::AtomicU64::new(0));
    f.owner.windows = Some(super::windows::OwnerWindows::new(
        rolling,
        f.dir.clone(),
        mt0.clone(),
    ));
    assert!(f.owner.lifecycle(), "not ready: no window yet");
    assert!(!f.dir.join(us::WINDOWS_JOURNAL).exists());
    mt5_ready(&mut f, &[]);
    f.owner.threshold_ready = true;
    assert!(f.owner.lifecycle());
    assert!(
        f.dir.join(us::WINDOWS_JOURNAL).exists(),
        "window 1 opens at the first ready"
    );
    assert_eq!(mt0.load(std::sync::atomic::Ordering::Acquire), 1);
    f.owner.stop_poll = || true;
    assert!(!f.owner.lifecycle(), "the stop ends the loop");
    assert_eq!(
        f.owner.windows.as_ref().and_then(|w| w.stop_reason()),
        Some(us::OPERATOR_STOP)
    );
    f.owner.drain_watches().unwrap();
    f.owner.end_result = Some(us::OPERATOR_STOP);
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut r =
        crate::storage::segments::ChainReader::open(&dir.join(us::WINDOWS_JOURNAL)).unwrap();
    while r.next_frame().unwrap().is_some() {}
    assert!(
        r.sealed && r.complete,
        "an ordered end completes the windows journal"
    );
    let mut e =
        crate::storage::segments::ChainReader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut ends = Vec::new();
    while let Some(frame) = e.next_frame().unwrap() {
        if frame.kind == crate::analytics::journal::kind::OWNER_WINDOW {
            ends.push(
                frame
                    .typed::<crate::analytics::evidence::OwnerWindow>()
                    .unwrap()
                    .end,
            );
        }
    }
    assert_eq!(ends, [Some(us::OPERATOR_STOP.to_string())]);
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-16 (review M-2): a window that ends in the very pass the operator stops — its check still
/// in progress when the owner loop ends — is part of the run's result: its HALT is the run's
/// terminal result, never an ordered OPERATOR_STOP.
#[test]
fn dec16_a_window_ending_with_the_stop_is_part_of_the_run_result() {
    use crate::orchestration::until_stopped as us;
    let mut f = fixture("dec16_last_window");
    let mut b = us::tests::binding(1, 2);
    b.mode = "shadow".into();
    let rolling = us::Rolling {
        project: f.dir.clone(),
        binding: b,
        qualification_id: "0123456789abcdef".into(),
        config: darkbot_solana::runtime_config::RuntimeConfig::default(),
        max_runtime_seconds: None,
        resume: None,
        autostart: None,
    };
    let full = |_: &std::path::Path| crate::storage::preflight::Environment {
        free_disk_bytes: Some(0),
        ..Default::default()
    };
    f.owner.windows = Some(
        super::windows::OwnerWindows::new(
            rolling,
            f.dir.clone(),
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
        )
        .with_environment(full),
    );
    mt5_ready(&mut f, &[]);
    f.owner.threshold_ready = true;
    assert!(f.owner.lifecycle(), "window 1 opens");
    std::thread::sleep(Duration::from_millis(1_100));
    // The same pass ends window 1 (its check pending) and takes the operator's stop.
    f.owner.stop_poll = || true;
    assert!(!f.owner.lifecycle());
    let err = f.owner.settle_windows().unwrap_err();
    assert_eq!(
        err.downcast_ref::<OwnerTerminal>().map(|t| t.0),
        Some(us::HALT_STOP)
    );
    assert_eq!(
        f.owner.windows.as_ref().and_then(|w| w.stop_reason()),
        Some(us::HALT_STOP),
        "the HALT wins over the stop"
    );
    let dir = f.dir.clone();
    let _ = f.owner.finish();
    let mut r =
        crate::storage::segments::ChainReader::open(&dir.join(us::WINDOWS_JOURNAL)).unwrap();
    let mut verdicts = Vec::new();
    while let Some(frame) = r.next_frame().unwrap() {
        if frame.kind == crate::analytics::journal::kind::UNTIL_STOPPED_WINDOW {
            verdicts.push(frame.typed::<us::WindowCheck>().unwrap().verdict);
        }
    }
    assert_eq!(verdicts, ["UNTIL_STOPPED_RUNWAY_EXHAUSTED"]);
    assert!(r.sealed && !r.complete);
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-06 tests: the OBSERVE budget of the whole owner time, one second banked — the tests are
/// about the comparison, and a throttled CI runner can measure one unit above the default's
/// 20 ms bank (1 543 s instead of ~350 s for the suite at `162f5953`); the tests about the budget
/// exhaust it explicitly (`charge`).
const DEC06_TEST_BUDGET_PPM: u64 = 1_000_000;

/// DEC-06: a threshold mode as the setup builds it (with the tests' OBSERVE budget).
fn dec06_control(
    requested: crate::orchestration::threshold_mode::Requested,
    min_ticks: Option<u64>,
) -> super::threshold::ThresholdControl {
    super::threshold::ThresholdControl::new(
        requested,
        min_ticks,
        DEC06_TEST_BUDGET_PPM,
        usize::try_from(OPERATIONAL.threshold_crosscheck_accounts).unwrap(),
        0,
    )
}

/// DEC-06: `p2_state` as the Ready leaves it — the reverse index rebuilt from the state.
fn dec06_ready(f: &mut Fixture, liab: f64) -> Pubkey {
    let account = p2_state(f, liab, 1_700_000_000);
    f.owner.reverse_index =
        rebuild_reverse_index(&f.owner.st, &f.owner.program, &f.owner.group, None).0;
    account
}

/// DEC-06: the threshold-mode records of a finished owner's journal (generation, effective,
/// block reason, cause).
fn dec06_transitions(dir: &std::path::Path) -> Vec<(u64, String, String, String)> {
    use crate::analytics::evidence::ThresholdTransition;
    use crate::analytics::payload::{decode, Payload};
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut out = vec![];
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == ThresholdTransition::KIND {
            let t: ThresholdTransition = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            out.push((t.generation, t.effective, t.block_reason, t.cause));
        }
    }
    out
}

/// DEC-06: a requested ACTIVE (the configuration's `threshold_trigger = on`) follows the index —
/// SWEEP with INDEX_BOOTSTRAP until every account's thresholds are known — and without the user's
/// N (B-8) stays OBSERVE: the sweep stays authoritative, the requested mode is kept apart, and
/// every change is a new generation with its record.
#[test]
fn dec06_a_requested_active_follows_the_index_and_stays_observe_without_n() {
    use crate::orchestration::threshold_mode::{self as tm, Effective, Requested};
    let mut f = fixture("dec06_follow");
    f.owner.threshold = dec06_control(Requested::Active, None);
    let account = dec06_ready(&mut f, 95.0);
    f.owner.threshold_index_reset();
    f.owner.threshold_refresh("INDEX");
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Sweep, tm::INDEX_BOOTSTRAP)
    );
    assert_eq!(
        Vec::from(f.owner.threshold_bootstrap.clone()),
        vec![account]
    );
    assert!(f.owner.lifecycle());
    assert!(f.owner.threshold_bootstrap.is_empty() && f.owner.threshold.bootstrapped);
    assert_eq!(
        f.owner.threshold_index.tracked_accounts() + f.owner.threshold_fallback.len(),
        1
    );
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Observe, tm::B8_UNDECIDED)
    );
    assert!(!f.owner.threshold_authority(), "the sweep admits");
    assert_eq!(f.owner.threshold.requested, Requested::Active);
    // The sweep evaluates and admits the liquidatable account; nothing else does.
    assert_eq!(f.owner.mark_all_banks_dirty(), 2);
    f.owner.sweep();
    assert_eq!(f.owner.oracle_lanes.len(), 1);
    assert_eq!(
        f.owner.threshold.effective,
        Effective::Observe,
        "N stays undecided"
    );
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    assert_eq!(
        dec06_transitions(&dir),
        [
            (
                1,
                "SWEEP".into(),
                tm::INDEX_BOOTSTRAP.into(),
                "INDEX".into()
            ),
            (2, "OBSERVE".into(), tm::B8_UNDECIDED.into(), "STATE".into()),
        ]
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-06: ACTIVE needs N comparable ticks without a miss and waits for the open sweep batch (the
/// barrier); the batch's flush lets it in. Leaving ACTIVE re-sweeps every bank: an account the
/// threshold trigger did not wake is admitted exactly once, a pending one is coalesced, none is
/// lost or doubled.
#[test]
fn dec06_active_waits_for_the_batch_and_its_fallback_resweeps_every_bank() {
    use crate::orchestration::threshold_mode::{self as tm, Effective, Requested};
    let mut f = fixture("dec06_barrier");
    f.owner.threshold = dec06_control(Requested::Active, Some(3));
    let account = dec06_ready(&mut f, 95.0);
    f.owner.threshold_index_reset();
    assert!(f.owner.lifecycle());
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Observe, tm::EVIDENCE_SHORT)
    );
    f.owner.threshold.stats.streak = 3;
    assert_eq!(f.owner.mark_all_banks_dirty(), 2);
    f.owner.threshold_refresh("STATE");
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Observe, tm::BARRIER)
    );
    f.owner.sweep();
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Active, ""),
        "the flushed batch opens the barrier"
    );
    assert!(!f.owner.sweep_batch_pending());
    assert!(f.owner.threshold_authority());
    assert_eq!(
        f.owner.oracle_lanes.len(),
        1,
        "the batch admitted the account once"
    );
    // The threshold trigger "missed" the account: no pending entry, no forward on record.
    while f.owner.oracle_lanes.next().is_some() {}
    f.owner.candidate_cache.clear();
    f.owner.threshold.latched = true;
    f.owner.threshold_refresh("MISS");
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Observe, tm::MISS_FALLBACK)
    );
    assert_eq!(f.owner.threshold.stats.fallbacks, 1);
    assert_eq!(f.owner.sweep_dirty_banks.len(), 2, "every bank is dirty");
    f.owner.sweep();
    let admitted: Vec<Pubkey> = std::iter::from_fn(|| f.owner.oracle_lanes.next())
        .map(|e| e.work.0)
        .collect();
    assert_eq!(admitted, vec![account], "the full sweep admits it once");
    // A candidate the threshold trigger admitted and that still waits is coalesced by the full
    // sweep of the next fallback, never doubled.
    f.owner.threshold.latched = false;
    f.owner.threshold_refresh("REQUEST");
    assert_eq!(f.owner.threshold.effective, Effective::Active);
    f.owner.candidate_cache.clear();
    f.owner
        .admit_oracle_accounts(HashSet::from([account]), HashSet::new(), (0, 100), 1);
    assert_eq!(f.owner.oracle_lanes.len(), 1);
    f.owner.candidate_cache.clear();
    let coalesced = f.owner.metrics.candidate_pending_coalesced;
    f.owner.threshold.min_ticks = None;
    f.owner.threshold_refresh("STATE");
    assert_eq!(f.owner.threshold.stats.fallbacks, 2);
    f.owner.sweep();
    assert_eq!(f.owner.oracle_lanes.len(), 1, "one entry per account");
    assert_eq!(f.owner.metrics.candidate_pending_coalesced, coalesced + 1);
    cleanup(f);
}

/// DEC-06: a gap (state not tradable) takes ACTIVE back to the sweep; the next Ready rebuilds the
/// index — SWEEP with INDEX_BOOTSTRAP, the requested ACTIVE kept — and the effective mode follows
/// the index back to ACTIVE.
#[test]
fn dec06_a_gap_falls_back_to_the_sweep_and_the_ready_follows_the_index() {
    use crate::orchestration::threshold_mode::{self as tm, Effective, Requested};
    let mut f = fixture("dec06_gap");
    dec06_ready(&mut f, 95.0);
    f.owner.threshold_test_active();
    f.owner.threshold_refresh("STATE");
    assert!(f.owner.threshold_authority());
    f.owner.st.halt(darkbot_common::error::HaltReason::FeedGap);
    f.owner.threshold_refresh("STATE");
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Sweep, tm::STATE_NOT_TRADABLE)
    );
    assert_eq!(f.owner.threshold.stats.fallbacks, 1);
    // The Ready after the recovery (events.rs) clears the batch and rebuilds the index.
    dec06_ready(&mut f, 95.0);
    f.owner.sweep_dirty_banks.clear();
    f.owner.sweep_batch_started_ns = None;
    f.owner.threshold_index_reset();
    f.owner.threshold_refresh("INDEX");
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Sweep, tm::INDEX_BOOTSTRAP)
    );
    assert_eq!(f.owner.threshold.requested, Requested::Active);
    assert!(f.owner.lifecycle());
    assert_eq!(
        f.owner.threshold.effective,
        Effective::Active,
        "follows the index"
    );
    cleanup(f);
}

/// DEC-06 OBSERVE fixture: a tradable state with exact thresholds (no premium), the reverse index
/// rebuilt, OBSERVE requested and its index bootstrapped. Returns the account and its banks.
fn dec06_observe(f: &mut Fixture, liab: f64) -> (Pubkey, Pubkey, Pubkey) {
    use crate::orchestration::threshold_mode::{Effective, Requested};
    let (state, account, asset_bank, liab_bank) = p2_accounts(f, liab, 1_700_000_000, false);
    f.owner.st.install_snapshot(100, state.clone());
    f.owner.st.reconcile(100, &state);
    assert!(f.owner.st.confirm_ready().is_tradable());
    f.owner.reverse_index =
        rebuild_reverse_index(&f.owner.st, &f.owner.program, &f.owner.group, None).0;
    f.owner.threshold = dec06_control(Requested::Observe, None);
    f.owner.threshold_index_reset();
    assert!(f.owner.lifecycle());
    assert_eq!(f.owner.threshold.effective, Effective::Observe);
    (account, asset_bank, liab_bank)
}

/// DEC-06: one oracle tick of `banks` as `events.rs` takes it in OBSERVE — the threshold
/// trigger's lookup beside the sweep, and the banks dirty for the sweep — then the batch's flush.
fn dec06_tick_and_flush(f: &mut Fixture, banks: &[Pubkey]) {
    let clock = chain_clock(&f.owner.st);
    let t_rx = f.owner.clock.now_ns();
    f.owner.threshold_observe_tick(banks, t_rx, clock);
    f.owner.mark_all_banks_dirty();
    f.owner.sweep();
}

/// DEC-06 OBSERVE: the sweep stays authoritative and admits once; the threshold trigger's lookup
/// of the same tick admits nothing and is compared on the same state — a woken liquidatable
/// account is a hit with both detection times, the tick counts, the streak grows, and the owner
/// time of the threshold work is measured.
#[test]
fn dec06_observe_compares_the_tick_with_the_sweep_and_admits_nothing() {
    let mut f = fixture("dec06_observe_hit");
    let (account, asset_bank, liab_bank) = dec06_observe(&mut f, 95.0);
    assert_eq!(
        f.owner.threshold_index.tracked_accounts(),
        1,
        "exact thresholds"
    );
    let clock = chain_clock(&f.owner.st);
    let t_rx = f.owner.clock.now_ns();
    f.owner
        .threshold_observe_tick(&[asset_bank, liab_bank], t_rx, clock);
    assert!(f.owner.threshold.batch.woken.contains_key(&account));
    assert!(f.owner.oracle_lanes.is_empty(), "OBSERVE admits nothing");
    f.owner.mark_all_banks_dirty();
    f.owner.sweep();
    let s = &f.owner.threshold.stats;
    assert_eq!(
        (s.hits, s.misses, s.extra, s.ticks, s.streak),
        (1, 0, 0, 1, 1)
    );
    assert_eq!(
        (s.sweep_detect_ns.count(), s.threshold_detect_ns.count()),
        (1, 1)
    );
    assert!(s.observe_owner_ns.count() >= 2 && s.observe_owner_ns_total > 0);
    assert_eq!(f.owner.oracle_lanes.len(), 1, "the sweep admitted once");
    assert_eq!(f.owner.threshold.batch.ticks, 0, "the batch was consumed");
    cleanup(f);
}

/// DEC-06 OBSERVE: a liquidatable account the threshold trigger does not wake (a stale index) is
/// a miss — journaled, the streak back to 0 — while the sweep admits it as before.
#[test]
fn dec06_observe_records_a_miss_and_resets_the_streak() {
    use crate::analytics::evidence::ThresholdMiss;
    use crate::analytics::payload::{decode, Payload};
    use darkbot_project0::index::Side;
    let mut f = fixture("dec06_observe_miss");
    let (account, asset_bank, liab_bank) = dec06_observe(&mut f, 95.0);
    f.owner.threshold.stats.streak = 7;
    f.owner
        .threshold_index
        .set_account(account, &[(asset_bank, Side::Asset, 0)]);
    dec06_tick_and_flush(&mut f, &[asset_bank, liab_bank]);
    let s = &f.owner.threshold.stats;
    assert_eq!((s.hits, s.misses, s.ticks, s.streak), (0, 1, 1, 0));
    assert_eq!(f.owner.oracle_lanes.len(), 1, "the sweep admits it");
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut misses = vec![];
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == ThresholdMiss::KIND {
            let m: ThresholdMiss = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            misses.push((m.account, m.phase, m.alarm));
        }
    }
    assert_eq!(misses, [(account, "OBSERVE".to_string(), false)]);
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-06 OBSERVE: a woken healthy account is an extra wake-up; without budget (B-9) a tick is
/// sampled — not looked up, its batch not comparable — and the sweep evaluates exactly as before;
/// a generation change inside a batch makes it not comparable too.
#[test]
fn dec06_observe_counts_extra_wakeups_and_samples_without_budget() {
    use darkbot_project0::index::Side;
    let mut f = fixture("dec06_observe_budget");
    let (account, asset_bank, liab_bank) = dec06_observe(&mut f, 85.0);
    f.owner
        .threshold_index
        .set_account(account, &[(asset_bank, Side::Asset, u128::MAX)]);
    dec06_tick_and_flush(&mut f, &[asset_bank, liab_bank]);
    let s = &f.owner.threshold.stats;
    assert_eq!((s.hits, s.misses, s.extra, s.ticks), (0, 0, 1, 1));
    f.owner.threshold.budget.charge(u64::MAX / 2);
    let checks = f.owner.metrics.health_sweep_checks;
    let clock = chain_clock(&f.owner.st);
    f.owner
        .threshold_observe_tick(&[asset_bank, liab_bank], f.owner.clock.now_ns(), clock);
    assert!(
        f.owner.threshold.batch.woken.is_empty(),
        "a sampled tick looks nothing up"
    );
    assert!(f.owner.threshold.batch.unlooked.contains(&asset_bank));
    f.owner.mark_all_banks_dirty();
    f.owner.sweep();
    let s = &f.owner.threshold.stats;
    assert_eq!(
        (s.ticks, s.uncomparable_ticks, s.extra, s.sampled_out),
        (1, 1, 1, 1)
    );
    assert_eq!(
        f.owner.metrics.health_sweep_checks,
        checks + 1,
        "the sweep is not slowed"
    );
    f.owner.threshold.budget = crate::orchestration::threshold_mode::ObserveBudget::new(
        DEC06_TEST_BUDGET_PPM,
        f.owner.clock.now_ns(),
    );
    let clock = chain_clock(&f.owner.st);
    f.owner
        .threshold_observe_tick(&[asset_bank], f.owner.clock.now_ns(), clock);
    f.owner.threshold.generation += 1;
    f.owner.mark_all_banks_dirty();
    f.owner.sweep();
    assert_eq!(
        (
            f.owner.threshold.stats.ticks,
            f.owner.threshold.stats.uncomparable_ticks
        ),
        (1, 2),
        "a generation change"
    );
    cleanup(f);
}

/// DEC-06 OBSERVE: an account update without budget (B-9) leaves the index conservative — the
/// account is re-checked bank-wide (fallback) and queued for its thresholds — and the bootstrap
/// evaluates it once the budget allows (the real ingress path, `events()`).
#[test]
fn dec06_observe_defers_the_index_upkeep_without_budget() {
    let mut f = fixture("dec06_upkeep");
    let (account, _, _) = dec06_observe(&mut f, 95.0);
    let data = f.owner.st.get(&account).unwrap().data.clone();
    f.owner.threshold.budget.charge(u64::MAX / 2);
    f.producer
        .try_push(IngressEvent::Account(AccountUpdate {
            pubkey: account,
            owner: f.owner.program,
            lamports: 1,
            data,
            slot: 101,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }))
        .unwrap();
    let mut fetch = || Err("no recovery".to_string());
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert_eq!(f.owner.threshold_index.tracked_accounts(), 0);
    assert!(f.owner.threshold_fallback.contains(&account));
    assert_eq!(
        Vec::from(f.owner.threshold_bootstrap.clone()),
        vec![account]
    );
    assert!(!f.owner.threshold_ready);
    assert!(f.owner.lifecycle());
    assert_eq!(f.owner.threshold_bootstrap.len(), 1, "still no budget");
    f.owner.threshold.budget = crate::orchestration::threshold_mode::ObserveBudget::new(
        DEC06_TEST_BUDGET_PPM,
        f.owner.clock.now_ns(),
    );
    assert!(f.owner.lifecycle());
    assert_eq!(
        f.owner.threshold_index.tracked_accounts(),
        1,
        "re-evaluated"
    );
    assert!(f.owner.threshold_ready && !f.owner.threshold_fallback.contains(&account));
    cleanup(f);
}

/// DEC-06 ACTIVE fixture: `dec06_observe`'s state with an oracle key of both banks (an account
/// update of it is an oracle tick of `events()`), the threshold trigger effective ACTIVE and the
/// cross-check queues answered by the test. Returns the account, its banks and the oracle.
#[allow(clippy::type_complexity)]
fn dec06_active(
    f: &mut Fixture,
    liab: f64,
) -> (
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    darkbot_common::channel::Consumer<crate::orchestration::remote_workers::CrossCheckWork>,
    darkbot_common::channel::Producer<crate::orchestration::remote_workers::CrossCheckResult>,
) {
    let (account, asset_bank, liab_bank) = dec06_observe(f, liab);
    let oracle = Pubkey::new_unique();
    f.owner
        .reverse_index
        .set_bank_oracles(asset_bank, &[oracle]);
    f.owner.reverse_index.set_bank_oracles(liab_bank, &[oracle]);
    f.owner.threshold_test_active();
    f.owner.threshold_refresh("STATE");
    assert!(f.owner.threshold_authority());
    let (jobs, results) = f.owner.remote.detached_crosscheck();
    (account, asset_bank, liab_bank, oracle, jobs, results)
}

/// DEC-06: an oracle tick of `oracle` through `events()`.
fn dec06_oracle_tick(f: &mut Fixture, oracle: Pubkey, slot: u64) {
    f.producer
        .try_push(IngressEvent::Account(AccountUpdate {
            pubkey: oracle,
            owner: Pubkey::new_unique(),
            lamports: 1,
            data: slot.to_le_bytes().to_vec(),
            slot,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: f.owner.clock.now_ns(),
        }))
        .unwrap();
    let mut fetch = || Err("no recovery".to_string());
    assert_eq!(drive(f, &mut fetch), Ok(()));
}

/// DEC-06 ACTIVE: on an oracle tick the threshold trigger misses a liquidatable account (a stale
/// index); the sweep cross-check samples it off the owner and finds it liquidatable on the
/// unchanged state — a miss: the alarm record, the latch, ACTIVE back to the sweep with a full
/// sweep that admits the account exactly once. In OBSERVE the same tick is looked up beside the
/// sweep and marks its banks dirty.
#[test]
fn dec06_active_crosscheck_miss_alarms_latches_and_resweeps() {
    use crate::analytics::evidence::ThresholdMiss;
    use crate::analytics::payload::{decode, Payload};
    use crate::orchestration::threshold_mode::{self as tm, Effective};
    use darkbot_project0::index::Side;
    let mut f = fixture("dec06_crosscheck_miss");
    let (account, asset_bank, _, oracle, jobs, results) = dec06_active(&mut f, 95.0);
    f.owner
        .threshold_index
        .set_account(account, &[(asset_bank, Side::Asset, 0)]);
    dec06_oracle_tick(&mut f, oracle, 101);
    assert!(
        f.owner.oracle_lanes.is_empty(),
        "the stale index woke nothing"
    );
    let work = jobs.try_pop().expect("the not-woken account is sampled");
    assert_eq!(work.account, account);
    assert_eq!(f.owner.threshold.stats.crosscheck_samples, 1);
    let result = crate::orchestration::remote_workers::crosscheck_evaluate(work);
    assert_eq!(result.liquidatable, Ok(true));
    assert!(results.try_push(result).is_ok());
    f.owner.remote_apply().unwrap();
    let s = &f.owner.threshold.stats;
    assert_eq!(
        (s.crosscheck_misses, s.misses, s.streak, s.fallbacks),
        (1, 1, 0, 1)
    );
    assert!(f.owner.threshold.latched);
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Observe, tm::MISS_FALLBACK)
    );
    assert_eq!(f.owner.sweep_dirty_banks.len(), 2);
    f.owner.sweep();
    let admitted: Vec<Pubkey> = std::iter::from_fn(|| f.owner.oracle_lanes.next())
        .map(|e| e.work.0)
        .collect();
    assert_eq!(admitted, vec![account], "the full sweep admits it once");
    // OBSERVE: the tick is looked up beside the sweep and marks its banks dirty.
    dec06_oracle_tick(&mut f, oracle, 102);
    assert_eq!(f.owner.threshold.batch.ticks, 1);
    assert_eq!(f.owner.sweep_dirty_banks.len(), 2);
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let mut reader =
        crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
    let mut alarms = vec![];
    while let Some(frame) = reader.next_frame().unwrap() {
        if frame.kind == ThresholdMiss::KIND {
            let m: ThresholdMiss = decode(frame.kind, frame.schema, &frame.payload).unwrap();
            alarms.push((m.account, m.phase, m.alarm));
        }
    }
    assert_eq!(alarms, [(account, "ACTIVE".to_string(), true)]);
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-06 ACTIVE: no alarm without a miss — a healthy sample; a liquidatable one whose state
/// changed since the tick, or of an earlier generation (stale); one the trigger wakes on another
/// of its banks (covered). A full cross-check queue skips the sample, counted.
#[test]
fn dec06_active_crosscheck_ignores_healthy_stale_and_covered_samples() {
    use crate::orchestration::remote_workers::crosscheck_evaluate;
    use darkbot_project0::index::Side;
    let mut f = fixture("dec06_crosscheck_quiet");
    let (account, asset_bank, liab_bank, _, jobs, results) = dec06_active(&mut f, 95.0);
    let none = HashSet::new();
    let answer = |f: &mut Fixture, change: &dyn Fn(&mut Fixture)| {
        f.owner.threshold_crosscheck_sample(&[asset_bank], &none);
        let result = crosscheck_evaluate(jobs.try_pop().unwrap());
        change(f);
        assert!(results.try_push(result).is_ok());
        f.owner.remote_apply().unwrap();
    };
    // Covered: the trigger wakes the account on its liability bank.
    f.owner.threshold_index.set_account(
        account,
        &[
            (asset_bank, Side::Asset, 0),
            (liab_bank, Side::Liability, 0),
        ],
    );
    answer(&mut f, &|_| {});
    // Stale: the account changed since the tick.
    f.owner
        .threshold_index
        .set_account(account, &[(asset_bank, Side::Asset, 0)]);
    answer(&mut f, &|f| {
        let mut r = f.owner.st.get(&account).unwrap().clone();
        r.slot += 1;
        f.owner.st.on_event(IngressEvent::Account(AccountUpdate {
            pubkey: account,
            owner: r.owner,
            lamports: r.lamports,
            data: r.data,
            slot: 102,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }));
    });
    // Stale: an earlier generation.
    answer(&mut f, &|f| f.owner.threshold.generation += 1);
    let s = &f.owner.threshold.stats;
    assert_eq!(
        (
            s.crosscheck_samples,
            s.crosscheck_misses,
            s.crosscheck_stale
        ),
        (3, 0, 2)
    );
    assert!(f.owner.threshold_authority() && !f.owner.threshold.latched);
    // Healthy: no miss.
    let mut h = fixture("dec06_crosscheck_healthy");
    let (_, asset_bank, _, _, jobs, results) = dec06_active(&mut h, 85.0);
    h.owner
        .threshold_crosscheck_sample(&[asset_bank], &HashSet::new());
    let result = crosscheck_evaluate(jobs.try_pop().unwrap());
    assert_eq!(result.liquidatable, Ok(false));
    assert!(results.try_push(result).is_ok());
    h.owner.remote_apply().unwrap();
    assert_eq!(h.owner.threshold.stats.crosscheck_misses, 0);
    assert!(h.owner.threshold_authority());
    // A full queue skips (capacity 16, one sample per call).
    h.owner.threshold.crosscheck_accounts = 1;
    for _ in 0..17 {
        h.owner
            .threshold_crosscheck_sample(&[asset_bank], &HashSet::new());
    }
    assert_eq!(
        (
            h.owner.threshold.stats.crosscheck_samples,
            h.owner.threshold.stats.crosscheck_skipped
        ),
        (17, 1)
    );
    cleanup(f);
    cleanup(h);
}

/// DEC-06 (N-5): the threshold mode through the one controller, hot at the owner — OFF → ACTIVE
/// refused (recorded, nothing changes); OBSERVE turns the index on (SWEEP, INDEX_BOOTSTRAP, then
/// OBSERVE); ACTIVE needs N; a new ACTIVE request lifts a latched miss fallback; OFF takes ACTIVE
/// back with a full sweep and clears the index. Every decision is a durable receipt.
#[test]
fn dec06_the_controller_switches_the_threshold_mode_hot() {
    use crate::analytics::payload::CapabilityControlRequest;
    use crate::orchestration::capability_control;
    use crate::orchestration::threshold_mode::{self as tm, Effective, Requested};
    let mut f = fixture_with("dec06_controller", None);
    let dir = f.dir.join("capability-control");
    f.owner.sources.capabilities.dir = Some(dir.clone());
    f.owner.sources.capabilities.config_version = Some(0);
    let account = dec06_ready(&mut f, 95.0);
    let request = |seq, requested: &str| {
        Ok(CapabilityControlRequest {
            seq,
            capability: tm::THRESHOLD_TRIGGER.into(),
            requested: requested.into(),
            requested_unix_s: 1,
            request_id: format!("t-{seq}"),
            owner_epoch: 0,
        })
    };
    let decided = |f: &Fixture| {
        let o = capability_control::latest_outcome(&dir).unwrap().unwrap();
        let t = &f.owner.threshold;
        (
            o.seq,
            o.applied,
            o.requested,
            o.effective,
            t.effective,
            t.block,
        )
    };
    f.owner.apply_capability_control(1, request(1, "ACTIVE"));
    let (seq, applied, ..) = decided(&f);
    assert_eq!((seq, applied), (1, false), "OFF -> ACTIVE refused");
    assert!(capability_control::latest_outcome(&dir)
        .unwrap()
        .unwrap()
        .detail
        .contains(tm::OFF_TO_ACTIVE));
    assert_eq!(f.owner.threshold.requested, Requested::Off);
    f.owner.apply_capability_control(2, request(2, "OBSERVE"));
    assert_eq!(
        decided(&f),
        (
            2,
            true,
            "OBSERVE".into(),
            "SWEEP".into(),
            Effective::Sweep,
            tm::INDEX_BOOTSTRAP
        )
    );
    assert_eq!(f.owner.threshold.requested_source, "controller seq 2");
    assert_eq!(
        Vec::from(f.owner.threshold_bootstrap.clone()),
        vec![account]
    );
    assert!(f.owner.lifecycle());
    assert_eq!(f.owner.threshold.effective, Effective::Observe);
    f.owner.apply_capability_control(3, request(3, "ACTIVE"));
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Observe, tm::B8_UNDECIDED)
    );
    f.owner.threshold.min_ticks = Some(1);
    f.owner.threshold.stats.streak = 1;
    f.owner.threshold_refresh("STATE");
    assert!(f.owner.threshold_authority());
    f.owner.threshold.latched = true;
    f.owner.threshold_refresh("MISS");
    assert_eq!(f.owner.threshold.block, tm::MISS_FALLBACK);
    f.owner.apply_capability_control(4, request(4, "ACTIVE"));
    assert!(
        !f.owner.threshold.latched,
        "a new ACTIVE request lifts the latch"
    );
    assert_eq!(
        f.owner.threshold.block,
        tm::BARRIER,
        "the fallback's sweep first"
    );
    f.owner.sweep();
    assert!(f.owner.threshold_authority());
    let fallbacks = f.owner.threshold.stats.fallbacks;
    f.owner.apply_capability_control(5, request(5, "OFF"));
    assert_eq!(
        decided(&f),
        (5, true, "OFF".into(), "OFF".into(), Effective::Off, "")
    );
    assert_eq!(f.owner.threshold.stats.fallbacks, fallbacks + 1);
    assert_eq!(
        f.owner.sweep_dirty_banks.len(),
        2,
        "ACTIVE left: full sweep"
    );
    assert_eq!(
        f.owner.threshold_index.tracked_accounts() + f.owner.threshold_bootstrap.len(),
        0,
        "no index without a mode"
    );
    assert_eq!(f.owner.sources.capabilities.config_version, Some(5));
    cleanup(f);
}

/// DEC-06 OBSERVE: a liquidatable account whose time wake-up is due is no miss — both paths
/// consume the same wake-ups (V2-03), so the comparison leaves it out; the tick still counts.
#[test]
fn dec06_observe_leaves_due_time_wakeups_out_of_the_comparison() {
    use darkbot_project0::index::Side;
    use darkbot_project0::schedule::Trigger;
    let mut f = fixture("dec06_observe_due");
    let (account, asset_bank, liab_bank) = dec06_observe(&mut f, 95.0);
    f.owner
        .threshold_index
        .set_account(account, &[(asset_bank, Side::Asset, 0)]);
    f.owner
        .time_schedule
        .upsert(account, Trigger::PremiumBoundary, Some(1_700_000_000));
    dec06_tick_and_flush(&mut f, &[asset_bank, liab_bank]);
    let s = &f.owner.threshold.stats;
    assert_eq!((s.hits, s.misses, s.ticks, s.streak), (0, 0, 1, 1));
    cleanup(f);
}

/// DEC-06 ACTIVE: an account the tick woke is never sampled — the cross-check spends its
/// samples on what the threshold trigger did not wake.
#[test]
fn dec06_active_crosscheck_samples_only_what_the_tick_did_not_wake() {
    let mut f = fixture("dec06_crosscheck_woken");
    let (account, asset_bank, _, _, jobs, _) = dec06_active(&mut f, 95.0);
    f.owner
        .threshold_crosscheck_sample(&[asset_bank], &HashSet::from([account]));
    assert!(jobs.try_pop().is_none());
    assert_eq!(f.owner.threshold.stats.crosscheck_samples, 0);
    cleanup(f);
}

/// DEC-06: a price projection the threshold trigger asked for in ACTIVE that returns after ACTIVE
/// was left admits nothing (counted stale): the full sweep at that change covers its bank.
#[test]
fn dec06_a_projection_after_active_was_left_admits_nothing() {
    use crate::orchestration::remote_workers::OraclePriceProjectionResultMessage;
    let mut f = fixture_with("dec06_projection_left", None);
    let (_, asset_bank, _) = dec06_observe(&mut f, 95.0);
    let stale = f.owner.metrics.oracle_bank_projection_stale;
    assert!(f
        .results
        .as_ref()
        .unwrap()
        .oracle
        .try_push(OraclePriceProjectionResultMessage {
            bank: asset_bank,
            base_slot: 100,
            base_bank_hash: solana_sdk::hash::Hash::default(),
            result: Err("RPC_TRANSPORT".into()),
        })
        .is_ok());
    f.owner.remote_apply().unwrap();
    assert!(f.owner.oracle_lanes.is_empty(), "OBSERVE: nothing admitted");
    assert_eq!(f.owner.metrics.oracle_bank_projection_stale, stale + 1);
    cleanup(f);
}

/// DEC-06: an ACTIVE request whose decision cannot be recorded does not stand (durable receipts):
/// the requested mode falls back to OBSERVE.
#[test]
fn dec06_an_unrecorded_active_request_does_not_stand() {
    use crate::analytics::payload::CapabilityControlRequest;
    use crate::orchestration::threshold_mode::{self as tm, Requested};
    let mut f = fixture_with("dec06_unrecorded", None);
    dec06_ready(&mut f, 95.0);
    // The decisions' directory is a file: no receipt can be written.
    let blocked = f.dir.join("capability-control");
    std::fs::write(&blocked, b"not a directory").unwrap();
    f.owner.sources.capabilities.dir = Some(blocked);
    f.owner.sources.capabilities.config_version = Some(0);
    let request = |seq, requested: &str| {
        Ok(CapabilityControlRequest {
            seq,
            capability: tm::THRESHOLD_TRIGGER.into(),
            requested: requested.into(),
            requested_unix_s: 1,
            request_id: format!("t-{seq}"),
            owner_epoch: 0,
        })
    };
    f.owner.apply_capability_control(1, request(1, "OBSERVE"));
    assert_eq!(f.owner.threshold.requested, Requested::Observe);
    f.owner.apply_capability_control(2, request(2, "ACTIVE"));
    assert_eq!(
        f.owner.threshold.requested,
        Requested::Observe,
        "unrecorded ACTIVE"
    );
    cleanup(f);
}

/// DEC-06: a reconnect through the real recovery path (`events()`: Reconnected, snapshot, Ready)
/// rebuilds the threshold index — the requested ACTIVE kept apart, SWEEP with INDEX_BOOTSTRAP —
/// and the effective mode follows the index back to ACTIVE behind the startup sweep's batch.
#[test]
fn dec06_a_reconnect_rebuilds_the_index_and_active_follows_it() {
    use crate::orchestration::threshold_mode::{self as tm, Effective, Requested};
    let mut f = fixture("dec06_reconnect");
    let (state, account, _, _) = p2_accounts(&f, 95.0, 1_700_000_000, false);
    let mut accounts = vec![f.program_data_account.clone()];
    accounts.extend(state);
    let mut fetch = || {
        Ok(SnapshotData {
            slot: 100,
            accounts: accounts.clone(),
            missing_required: vec![],
        })
    };
    f.owner.threshold_test_active();
    f.producer.try_push(reconnected()).unwrap();
    assert_eq!(drive(&mut f, &mut fetch), Ok(()));
    assert!(f.owner.st.run_state().is_tradable());
    assert_eq!(f.owner.threshold.requested, Requested::Active);
    assert_eq!(
        (f.owner.threshold.effective, f.owner.threshold.block),
        (Effective::Sweep, tm::INDEX_BOOTSTRAP)
    );
    assert_eq!(
        Vec::from(f.owner.threshold_bootstrap.clone()),
        vec![account]
    );
    assert!(
        f.owner.sweep_batch_pending(),
        "the startup sweep of the Ready"
    );
    assert!(f.owner.lifecycle());
    assert_eq!(f.owner.threshold.block, tm::BARRIER);
    f.owner.sweep();
    assert_eq!(
        f.owner.threshold.effective,
        Effective::Active,
        "follows the index"
    );
    cleanup(f);
}

/// DEC-06 (review H1): a threshold mode beside the authoritative sweep never delays readiness —
/// an unbounded run's first window opens at the sweep's READY while the OBSERVE index still waits
/// for budget to bootstrap.
#[test]
fn dec06_a_threshold_mode_beside_the_sweep_never_delays_readiness() {
    use crate::orchestration::threshold_mode::{Effective, Requested};
    use crate::orchestration::until_stopped as us;
    let mut f = fixture("dec06_ready");
    let mut b = us::tests::binding(60, 120);
    b.mode = "shadow".into();
    let rolling = us::Rolling {
        project: f.dir.clone(),
        binding: b,
        qualification_id: "0123456789abcdef".into(),
        config: darkbot_solana::runtime_config::RuntimeConfig::default(),
        max_runtime_seconds: None,
        resume: None,
        autostart: None,
    };
    f.owner.windows = Some(super::windows::OwnerWindows::new(
        rolling,
        f.dir.clone(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    ));
    dec06_ready(&mut f, 95.0);
    f.owner.threshold = dec06_control(Requested::Observe, None);
    f.owner.threshold_index_reset();
    f.owner.threshold.budget.charge(u64::MAX / 2);
    assert!(f.owner.lifecycle());
    assert!(!f.owner.threshold_ready, "the bootstrap waits for budget");
    assert_eq!(f.owner.threshold.effective, Effective::Sweep);
    assert!(
        f.dir.join(us::WINDOWS_JOURNAL).exists(),
        "window 1 opens at the sweep's READY"
    );
    assert_eq!(
        f.owner.threshold.stats.sampled_out, 0,
        "a waiting bootstrap is no sampled unit (L4)"
    );
    cleanup(f);
}

/// DEC-06 (review H2/M1): an invalidated account (a bank write, a deferred upkeep) is queued once
/// however often it is invalidated, and leaves the index incomplete; ticks then test nothing —
/// not comparable, never toward N — until the bootstrap evaluated it again.
#[test]
fn dec06_ticks_on_an_incomplete_index_never_count_toward_n() {
    let mut f = fixture("dec06_incomplete");
    let (account, asset_bank, liab_bank) = dec06_observe(&mut f, 95.0);
    for _ in 0..3 {
        f.owner.threshold_defer(account);
    }
    assert_eq!(
        Vec::from(f.owner.threshold_bootstrap.clone()),
        vec![account],
        "queued once"
    );
    assert!(!f.owner.threshold_ready);
    dec06_tick_and_flush(&mut f, &[asset_bank, liab_bank]);
    let s = &f.owner.threshold.stats;
    assert_eq!(
        (s.ticks, s.uncomparable_ticks, s.hits, s.streak),
        (0, 1, 0, 0)
    );
    // The bootstrap evaluates it again (once), the next tick is comparable.
    assert!(f.owner.lifecycle());
    assert!(f.owner.threshold_ready && f.owner.threshold_bootstrap_queued.is_empty());
    dec06_tick_and_flush(&mut f, &[asset_bank, liab_bank]);
    assert_eq!(
        (f.owner.threshold.stats.ticks, f.owner.threshold.stats.hits),
        (1, 1)
    );
    cleanup(f);
}

/// DEC-06 (review M3): a miss on banks that were looked up counts — the streak back to 0 — also
/// in a batch another tick of which could not be looked up (its ticks not comparable).
#[test]
fn dec06_a_miss_counts_in_a_batch_that_is_not_comparable() {
    use darkbot_project0::index::Side;
    let mut f = fixture("dec06_miss_uncomparable");
    let (account, asset_bank, _) = dec06_observe(&mut f, 95.0);
    f.owner.threshold.stats.streak = 9;
    f.owner
        .threshold_index
        .set_account(account, &[(asset_bank, Side::Asset, 0)]);
    let clock = chain_clock(&f.owner.st);
    let now = f.owner.clock.now_ns();
    f.owner.threshold_observe_tick(&[asset_bank], now, clock);
    // A bank without state: its price cannot be looked up (ACTIVE would project it).
    f.owner
        .threshold_observe_tick(&[Pubkey::new_unique()], now, clock);
    f.owner.mark_all_banks_dirty();
    f.owner.sweep();
    let s = &f.owner.threshold.stats;
    assert_eq!(
        (s.misses, s.streak, s.ticks, s.uncomparable_ticks),
        (1, 0, 0, 2)
    );
    cleanup(f);
}

/// DEC-06 (review M2): an unrecorded ACTIVE demotion is journaled as a change of its own, so a
/// resume restores OBSERVE, never the unrecorded ACTIVE.
#[test]
fn dec06_an_unrecorded_active_is_never_restored() {
    use crate::analytics::payload::CapabilityControlRequest;
    use crate::orchestration::threshold_mode::{self as tm, Requested};
    let mut f = fixture_with("dec06_unrecorded_restore", None);
    // Not bootstrapped yet: the effective mode stays SWEEP through both requests.
    p2_state(&mut f, 95.0, 1_700_000_000);
    let blocked = f.dir.join("capability-control");
    std::fs::write(&blocked, b"not a directory").unwrap();
    f.owner.sources.capabilities.dir = Some(blocked);
    f.owner.sources.capabilities.config_version = Some(0);
    let request = |seq, requested: &str| {
        Ok(CapabilityControlRequest {
            seq,
            capability: tm::THRESHOLD_TRIGGER.into(),
            requested: requested.into(),
            requested_unix_s: 1,
            request_id: format!("t-{seq}"),
            owner_epoch: 0,
        })
    };
    f.owner.apply_capability_control(1, request(1, "OBSERVE"));
    f.owner.apply_capability_control(2, request(2, "ACTIVE"));
    assert_eq!(f.owner.threshold.requested, Requested::Observe);
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    assert_eq!(
        super::threshold::restore(&dir.join("opportunities.p0j")).map(|r| r.requested),
        Some(Requested::Observe)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// DEC-06 (review L5): a liquidatable sample the owner wakes without the sweep — it waits in a
/// lane, or its time wake-up is due — is covered, no alarm.
#[test]
fn dec06_active_crosscheck_counts_pending_and_due_accounts_as_covered() {
    use crate::orchestration::remote_workers::crosscheck_evaluate;
    use darkbot_project0::index::Side;
    use darkbot_project0::schedule::Trigger;
    let mut f = fixture("dec06_crosscheck_covered");
    let (account, asset_bank, _, _, jobs, results) = dec06_active(&mut f, 95.0);
    f.owner
        .threshold_index
        .set_account(account, &[(asset_bank, Side::Asset, 0)]);
    let answer = |f: &mut Fixture| {
        f.owner
            .threshold_crosscheck_sample(&[asset_bank], &HashSet::new());
        let result = crosscheck_evaluate(jobs.try_pop().unwrap());
        assert_eq!(result.liquidatable, Ok(true));
        assert!(results.try_push(result).is_ok());
        f.owner.remote_apply().unwrap();
    };
    f.owner
        .time_schedule
        .upsert(account, Trigger::PremiumBoundary, Some(1_700_000_000));
    answer(&mut f);
    f.owner.time_schedule.remove_account(&account);
    f.owner
        .admit_oracle_accounts(HashSet::from([account]), HashSet::new(), (0, 100), 1);
    answer(&mut f);
    assert_eq!(f.owner.threshold.stats.crosscheck_misses, 0);
    assert!(f.owner.threshold_authority() && !f.owner.threshold.latched);
    cleanup(f);
}

/// DEC-06 (review M4): the cross-check's sampling is work beside ACTIVE's authoritative path —
/// within its budget and measured; without budget the sample is skipped (counted), nothing queued.
#[test]
fn dec06_active_crosscheck_without_budget_is_skipped() {
    let mut f = fixture("dec06_crosscheck_budget");
    let (_, asset_bank, _, _, jobs, _) = dec06_active(&mut f, 95.0);
    f.owner
        .threshold_crosscheck_sample(&[asset_bank], &HashSet::new());
    assert!(jobs.try_pop().is_some());
    assert!(
        f.owner.threshold.stats.observe_owner_ns.count() >= 1,
        "measured"
    );
    f.owner.threshold.budget.charge(u64::MAX / 2);
    f.owner
        .threshold_crosscheck_sample(&[asset_bank], &HashSet::new());
    assert!(jobs.try_pop().is_none());
    assert_eq!(
        (
            f.owner.threshold.stats.crosscheck_samples,
            f.owner.threshold.stats.crosscheck_skipped
        ),
        (1, 1)
    );
    cleanup(f);
}

/// DEC-13b: the owner's production RPC submitter against a loopback node that quotes, simulates
/// and records every send (`sendTransaction` answers the sent signature; with `send_ok` false a
/// server error, the send's fate unknown; a simulation answered after `sim_delay_ms`) under a
/// budget that admits the test's calls at once; the node's methods in call order.
fn dec13b_live_node(f: &mut Fixture, send_ok: bool, sim_delay_ms: u64) -> Arc<Mutex<Vec<String>>> {
    use crate::execution::one_shot::non_landing::loopback::{endpoint, Reply};
    use base64::Engine;
    let (url, calls) =
        endpoint(Arc::new(
            move |method: &str, params: &serde_json::Value| match method {
                "getFeeForMessage" => {
                    Reply::Result(serde_json::json!({"context": {"slot": 100}, "value": 5000}))
                }
                "simulateTransaction" => {
                    std::thread::sleep(Duration::from_millis(sim_delay_ms));
                    Reply::Result(serde_json::json!({"context": {"slot": 100}, "value": {
                "err": null, "logs": [], "accounts": null, "unitsConsumed": 100,
                "returnData": null, "innerInstructions": null, "replacementBlockhash": null}}))
                }
                "sendTransaction" if !send_ok => Reply::Status(500),
                "sendTransaction" => {
                    let wire = base64::engine::general_purpose::STANDARD
                        .decode(params[0].as_str().unwrap())
                        .unwrap();
                    let tx: solana_sdk::transaction::VersionedTransaction =
                        bincode::deserialize(&wire).unwrap();
                    Reply::Result(serde_json::json!(tx.signatures[0].to_string()))
                }
                _ => Reply::Status(500),
            },
        ));
    // The owner's candidate transport (fee quote, simulation, send) under a budget that admits
    // the test's calls at once.
    let mut c = darkbot_solana::runtime_config::RuntimeConfig::default();
    (
        c.limits.rpc_rps,
        c.limits.rpc_burst,
        c.limits.rpc_concurrency,
    ) = (1_000, 100, 4);
    (c.limits.simulation_rps, c.limits.fee_rps, c.limits.send_rps) = (1_000, 1_000, 1_000);
    (c.limits.hotpath_rps, c.limits.rpc_backoff_ms) = (1_000, 10);
    let budget = Arc::new(darkbot_solana::provider::Budget::configured(&c));
    f.owner.submitter = crate::execution::observe_only::ObserveOnly::new(
        crate::execution::bundle_path::OwnerSubmitter::new(
            RpcSubmitter::with_budget(url, true, budget).unwrap(),
        ),
    );
    calls
}

/// DEC-13b: LIVE requested — P8-CTRL.1: every economic switch and the owner's signer ENABLED —
/// and, with `fe06`, the FE-06 stand-in opening it and the submitter (without it FE-06 alone is
/// missing).
fn dec13b_live(f: &mut Fixture, fe06: bool) {
    use crate::orchestration::capability_control as cap;
    let c = &mut f.owner.sources.capabilities;
    c.live_requested = cap::LIVE;
    for e in cap::ECON {
        c.econ.set(e, true);
    }
    let mut signer = cap::WalletSwitch::configured(
        f.owner.liquidator,
        &[cap::ROLE_LIQUIDATOR, cap::ROLE_FEE_PAYER],
    );
    signer.enabled = true;
    c.wallets = vec![signer];
    if fe06 {
        let fe06 = crate::orchestration::capability_control::Fe06Authorization::offline_test();
        f.owner.submitter.open(&fe06);
        f.owner.sources.capabilities.fe06 = Some(fe06);
    }
}

/// DEC-13b: the LIVE owner of one liquidatable account — the real state (group, a SOL bank and
/// two 1-USD banks on fixed oracles, FeeState, clock, rent, the liquidator's lamports and ATAs)
/// through the real detect path, the production RPC submitter against a loopback node that
/// quotes, simulates and records every send (`sendTransaction` answers the sent signature), the
/// attempt approval of this owner's wallet and target and an FE-04 journal. `fe06`: the FE-06
/// authorization (the offline tests' stand-in) opens LIVE and the submitter; without it nothing
/// opens. Returns the fixture, the target and the node's methods in call order.
fn dec13b_fixture(tag: &str, fe06: bool) -> (Fixture, Pubkey, Arc<Mutex<Vec<String>>>) {
    dec13b_fixture_with(tag, fe06, true, 0)
}

/// `dec13b_fixture` whose node answers every send with a server error unless `send_ok` and every
/// simulation after `sim_delay_ms`.
fn dec13b_fixture_with(
    tag: &str,
    fe06: bool,
    send_ok: bool,
    sim_delay_ms: u64,
) -> (Fixture, Pubkey, Arc<Mutex<Vec<String>>>) {
    use crate::execution::one_shot::approval_file::{ApprovalBinding, Bind, Usage, ATTEMPT_SCHEMA};
    use bytemuck::Zeroable;
    use marginfi_type_crate::types::{
        Bank, BankOperationalState, FeeState, MarginfiAccount, MarginfiGroup, OracleSetup,
        WrappedI80F48,
    };
    use solana_sdk::signer::Signer;
    let mut f = fixture_with(tag, None);
    let payer = solana_sdk::signature::Keypair::new();
    let liquidator = payer.pubkey();
    (f.owner.liquidator, f.owner.payer) = (liquidator, payer);
    f.owner.atas = accounts::LiquidatorAtas::new(liquidator);
    f.owner.zero_prefund_enabled = true;
    let calls = dec13b_live_node(&mut f, send_ok, sim_delay_ms);
    let num = |v: f64| WrappedI80F48 {
        value: ((v * (1u64 << 48) as f64) as i128).to_le_bytes(),
    };
    let token_program: Pubkey = protocol::TOKEN_PROGRAM_ID.parse().unwrap();
    let program = f.owner.program;
    let update = |pubkey, owner, lamports, data| AccountUpdate {
        pubkey,
        owner,
        lamports,
        data,
        slot: 100,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    };
    let group = f.owner.group;
    let bank = |mint: Pubkey, decimals: u8, price: f64, asset_weight: f64| {
        let mut b = Bank::zeroed();
        b.group = group;
        b.mint = mint;
        b.mint_decimals = decimals;
        b.asset_share_value = num(1.0);
        b.liability_share_value = num(1.0);
        b.config.asset_weight_init = num(asset_weight);
        b.config.asset_weight_maint = num(asset_weight);
        b.config.liability_weight_init = num(1.0);
        b.config.liability_weight_maint = num(1.0);
        b.config.operational_state = BankOperationalState::Operational;
        b.config.oracle_setup = OracleSetup::Fixed;
        b.config.fixed_price = num(price);
        [protocol::DISC_BANK.as_slice(), bytemuck::bytes_of(&b)].concat()
    };
    let sol_mint: Pubkey = protocol::NATIVE_SOL_MINT.parse().unwrap();
    let (asset_mint, liab_mint) = (Pubkey::new_unique(), Pubkey::new_unique());
    // A loopback Jupiter `/build`: the collateral sold 1:1 for the liability at 50 bps.
    f.owner.routes = JupiterV2RouteProvider::new(liquidator, None, 50, 64, false)
        .unwrap()
        .with_base_url(dec13b_jupiter(
            liquidator,
            token_program,
            asset_mint,
            liab_mint,
        ));
    let mut keys = [Pubkey::new_unique(), Pubkey::new_unique()];
    keys.sort_unstable_by(|a, b| b.cmp(a));
    let (asset_bank, liab_bank, target) = (keys[0], keys[1], Pubkey::new_unique());
    f.owner.sol_bank = Pubkey::new_unique();
    f.marks.extend([
        ("asset_bank", asset_bank),
        ("liab_bank", liab_bank),
        ("asset_mint", asset_mint),
        ("liab_mint", liab_mint),
        ("sol_mint", sol_mint),
    ]);
    let mut a = MarginfiAccount::zeroed();
    a.group = group;
    // A linked liquidation record: the plan pays no record rent.
    a.liquidation_record = protocol::liquidation_record_pda(&program, &target);
    let bal = &mut a.lending_account.balances;
    (bal[0].active, bal[0].bank_pk, bal[0].asset_shares) = (1, asset_bank, num(1_000e6));
    (bal[1].active, bal[1].bank_pk, bal[1].liability_shares) = (1, liab_bank, num(900e6));
    let mut fee = FeeState::zeroed();
    fee.global_fee_wallet = Pubkey::new_unique();
    fee.liquidation_max_fee = num(0.1);
    let ata = |mint: &Pubkey, amount: u64| {
        let mut d = vec![0u8; 165];
        d[0..32].copy_from_slice(mint.as_ref());
        d[32..64].copy_from_slice(liquidator.as_ref());
        d[64..72].copy_from_slice(&amount.to_le_bytes());
        d[108] = 1;
        update(
            accounts::associated_token_address(&liquidator, mint, &token_program),
            token_program,
            2_039_280,
            d,
        )
    };
    let now_s = wall_unix_s().unwrap();
    let state = vec![
        p2_clock(100, now_s),
        update(
            solana_sdk::sysvar::rent::id(),
            solana_sdk::sysvar::id(),
            1,
            bincode::serialize(&solana_sdk::rent::Rent::default()).unwrap(),
        ),
        update(
            group,
            program,
            1,
            [
                protocol::DISC_MARGINFI_GROUP.as_slice(),
                bytemuck::bytes_of(&MarginfiGroup::zeroed()),
            ]
            .concat(),
        ),
        update(
            protocol::fee_state_pda(&program),
            program,
            1,
            [
                protocol::DISC_FEE_STATE.as_slice(),
                bytemuck::bytes_of(&fee),
            ]
            .concat(),
        ),
        update(f.owner.sol_bank, program, 1, bank(sol_mint, 9, 100.0, 1.0)),
        update(asset_bank, program, 1, bank(asset_mint, 6, 1.0, 0.8)),
        update(liab_bank, program, 1, bank(liab_mint, 6, 1.0, 1.0)),
        update(asset_mint, token_program, 1, vec![0; 82]),
        update(liab_mint, token_program, 1, vec![0; 82]),
        update(
            target,
            program,
            1,
            [
                protocol::DISC_MARGINFI_ACCOUNT.as_slice(),
                bytemuck::bytes_of(&a),
            ]
            .concat(),
        ),
        update(liquidator, Pubkey::default(), 10_000_000_000, vec![]),
        ata(&liab_mint, 2_000_000_000),
        ata(&asset_mint, 0),
    ];
    f.owner.st.install_snapshot(100, state.clone());
    f.owner.st.reconcile(100, &state);
    f.owner.st.confirm_ready();
    f.owner.slot_watch.observe(100);
    f.owner
        .blockhash
        .set(darkbot_solana::state::blockhash::CachedBlockhash {
            hash: solana_sdk::hash::Hash::new_unique(),
            last_valid_block_height: 1_000,
            refreshed_ns: f.owner.clock.now_ns(),
            context_slot: Some(100),
        });
    let open = darkbot_project0::safety::Limits {
        max_total_capital: u64::MAX / 4,
        max_outstanding_inventory: u64::MAX / 4,
        max_concurrent_positions: 1,
        max_exposure_per_attempt: u64::MAX / 4,
        max_daily_loss: u64::MAX / 4,
        max_failed_attempts_per_day: 10,
        max_tip: 0,
        max_slippage_bps: 50,
        max_outstanding_attempts: 1,
    };
    let mut allow = Allow::default();
    allow.execution_classes.insert("longtail");
    allow.assets.extend([asset_mint, liab_mint]);
    f.owner.safety = Safety::new(open, allow, fe06);
    // As in the owner's setup: the protocol and the pinned swap program.
    let jup: Pubkey = protocol::JUP_SWAP_PROGRAM_ID.parse().unwrap();
    f.owner.allowlist.swap_programs.insert(jup);
    f.owner.global_allowed = vec![program, jup];
    dec13b_live(&mut f, fe06);
    // The attempt approval of this wallet, program, deployment and target.
    let journal = f.dir.join("fe04");
    std::fs::create_dir_all(&journal).unwrap();
    let file = f.dir.join("approval.conf");
    std::fs::write(
        &file,
        format!(
            "schema = {ATTEMPT_SCHEMA}\nauthorized = true\napproval_id = dec13b-1\ncode_sha = {}\nbinary_sha256 = {}\nwallet = {liquidator}\nprogram = {program}\nprogram_data_hash = {}\nfunding_class = ZERO_PREFUND_SAME_TX\nallowed_targets = {target}\nexpires_unix_s = {}\nmax_state_age_ms = 60000\nmax_capital_usd_micro = 100000000000\nmax_total_sol_debit_lamports = 50000000\nmax_tip_lamports = 0\nmax_rent_lamports = 50000000\nmax_loss_usd_micro = 100000000000\nmin_net_usd_micro = 1\nmax_slippage_bps = 50\n",
            "a".repeat(40),
            "b".repeat(64),
            f.owner.program_data_baseline_hash,
            now_s + 3_600
        ),
    )
    .unwrap();
    let wallet = liquidator.to_string();
    let mut source = ApprovalSource {
        path: file,
        binding: ApprovalBinding {
            wallet: Bind::Known(wallet.clone()),
            program: Bind::Known(program.to_string()),
            program_data_hash: Bind::Known(f.owner.program_data_baseline_hash.to_string()),
            code_sha: Bind::Known("a".repeat(40)),
            binary_sha256: Bind::Known("b".repeat(64)),
            ceilings: None,
            max_lifetime_seconds: 86_400,
        },
        read: Err("APPROVAL_NOT_READ".into()),
        usage: Usage::Unchecked("APPROVAL_NOT_READ".into()),
    };
    source.refresh(Some(&journal), &wallet);
    f.owner.fe04_approval = Some(source);
    f.owner.fe04_journal = Some(journal);
    (f, target, calls)
}

/// DEC-13b: a loopback Jupiter answering every `/build` with the ExactIn route of the requested
/// amount of `input` into `output` 1:1 (the taker's ATAs, 50 bps); its URL.
fn dec13b_jupiter(taker: Pubkey, token_program: Pubkey, input: Pubkey, output: Pubkey) -> String {
    use base64::Engine;
    use std::io::{BufRead, BufReader, Write};
    let jup: Pubkey = protocol::JUP_SWAP_PROGRAM_ID.parse().unwrap();
    let ata = |m: &Pubkey| accounts::associated_token_address(&taker, m, &token_program);
    let (source, destination, authority) = (ata(&input), ata(&output), Pubkey::new_unique());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                    break;
                }
            }
            let amount: u64 = line
                .split(['?', '&', ' '])
                .find_map(|kv| kv.strip_prefix("amount="))
                .and_then(|a| a.parse().ok())
                .unwrap_or(0);
            let (out, threshold) = (amount, amount * 995 / 1_000);
            let mut data = vec![229, 23, 203, 151, 122, 227, 173, 42];
            data.extend(1u32.to_le_bytes());
            data.extend([7, 100, 0, 1]);
            data.extend(amount.to_le_bytes());
            data.extend(out.to_le_bytes());
            data.extend(50u16.to_le_bytes());
            data.push(0);
            let acct = |k: &Pubkey, signer: bool, writable: bool| serde_json::json!({"pubkey": k.to_string(), "isSigner": signer, "isWritable": writable});
            let body = serde_json::json!({
                "inAmount": amount.to_string(), "outAmount": out.to_string(),
                "otherAmountThreshold": threshold.to_string(), "swapMode": "ExactIn",
                "setupInstructions": [], "cleanupInstruction": null, "otherInstructions": [],
                "addressesByLookupTableAddress": null,
                "swapInstruction": {
                    "programId": jup.to_string(),
                    "data": base64::engine::general_purpose::STANDARD.encode(&data),
                    "accounts": [
                        acct(&token_program, false, false), acct(&taker, true, false),
                        acct(&source, false, true), acct(&destination, false, true),
                        acct(&jup, false, false), acct(&output, false, false),
                        acct(&jup, false, false), acct(&authority, false, false),
                        acct(&jup, false, false)]}})
            .to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    url
}

/// DEC-13b (FS-F-09, C3-Rest): with LIVE effective (FE-06) the real detect path hands its
/// actionable candidate to the owner's FE-04 admission: the payload the pipeline simulated is
/// reused at the handoff (DEC-02), reserved durably and sent exactly once; the attempt becomes
/// the held one — SUBMITTED ownership, the approval spent, no second admission — and the capital
/// and inventory `check()` reserved are released only by the journal settlement's booking, once
/// (before: the booking released no inventory, the reservation stayed for ever).
#[test]
fn dec13b_a_live_candidate_is_admitted_sent_once_and_released_by_the_settlement() {
    use crate::execution::one_shot::settlement::{SettlementEvidence, SettlementSubject};
    let (mut f, target, calls) = dec13b_fixture("dec13b_live", true);
    assert_eq!(f.owner.approval_status().state, "VALID");
    // The admission checks its holds against the lamports and every target bank's ATA amount.
    let balances = f.owner.fe04_wallet_balances(&target);
    assert_eq!(balances["SOL"], 10_000_000_000);
    assert_eq!(
        balances
            .values()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        [0, 2_000_000_000, 10_000_000_000].into(),
        "{balances:?}"
    );
    assert_eq!(f.owner.owner_snapshot(1).admission.state, "WAITING");
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    let methods = || calls.lock().unwrap().clone();
    assert_eq!(
        methods(),
        [
            "getFeeForMessage",
            "getFeeForMessage",
            "simulateTransaction",
            "sendTransaction"
        ],
        "one simulation, reused at the handoff, and one send"
    );
    let a = &f.owner.fe04_admissions;
    assert_eq!((a.submitted, a.refused, a.unknown), (1, 0, 0));
    assert_eq!((a.simulation.reused, a.simulation.resimulated), (1, 0));
    let unbooked = f
        .owner
        .fe04_unbooked
        .expect("the check() reservation waits for the booking");
    assert_eq!(
        unbooked.reserved_capital,
        Some(f.owner.safety.capital_in_use())
    );
    assert!(unbooked.reserved_inventory > 0);
    assert_eq!(
        f.owner.safety.inventory_reserved(),
        unbooked.reserved_inventory
    );
    let (intent, _) = f
        .owner
        .fe04_held
        .as_ref()
        .unwrap()
        .settlement_job()
        .unwrap();
    assert_eq!(intent.target, target.to_string());
    let s = f.owner.owner_snapshot(2).admission;
    assert_eq!((s.state, s.submitted), ("BLOCKED", 1));
    assert!(
        s.block_reason.starts_with("FE04_ATTEMPT_HELD"),
        "{}",
        s.block_reason
    );
    // Single flight: the target is SUBMITTED, a new detection is deferred and sends nothing.
    assert!(!f
        .owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            101,
            "account_update"
        )))
        .unwrap());
    assert_eq!(methods().len(), 4);
    let (dir, wallet) = (f.dir.join("fe04"), f.owner.liquidator.to_string());
    f.owner
        .fe04_approval
        .as_mut()
        .unwrap()
        .refresh(Some(&dir), &wallet);
    assert_eq!(f.owner.approval_status().state, "CONSUMED");
    // The settlement reads the sent signature; its proven non-landing books the attempt once.
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    let job = fe04_job(&f).expect("the first read is due at once");
    assert_eq!(job.subject.signatures, intent.signatures);
    let proof = crate::execution::one_shot::non_landing::loopback::proving(
        &SettlementSubject::from(&intent).non_landing().unwrap(),
        true,
    );
    fe04_answer(
        &f,
        SettlementEvidence {
            observations: vec![],
            non_landing: Some(Ok(proof)),
        },
    );
    run_until(&mut f, fe04_settled);
    assert_eq!(f.owner.fe04_unbooked, None);
    assert_eq!(
        (
            f.owner.safety.capital_in_use(),
            f.owner.safety.inventory_reserved()
        ),
        (0, 0),
        "released once by the booking"
    );
    assert_eq!(f.owner.safety.day().failed_attempts, 1);
    assert_eq!(
        f.owner.safety.halted(),
        Some(darkbot_common::error::HaltReason::LimitBreached),
        "a consumed one-shot halts as its restart replay does"
    );
    assert!(f.owner.reservations.is_empty());
    assert_eq!(methods().len(), 4, "nothing sent again");
    cleanup(f);
}

/// P8-CTRL.7 (L03): with LIVE effective the payload still needs its grant's scope — a scope
/// without the RPC adapter permits no real signature of it (review H1: nothing signed, simulated
/// or sent) and nothing durable.
#[test]
fn p8ctrl03_a_grant_whose_scope_lacks_the_adapter_permits_no_send() {
    let (mut f, target, calls) = dec13b_fixture("p8ctrl03_adapter", true);
    // A scope that binds what runs, without the RPC adapter.
    p8ctrl03_scoped(&mut f, &[("submission_adapters", "jito_single")], |_| {});
    assert!(f.owner.live_effective());
    // Review H1: refused by its sign permit — no real signature, no simulation, no send.
    p8ctrl03_refused(
        f,
        target,
        &calls,
        "adapter",
        "SIGN_ADAPTER_NOT_IN_SCOPE: rpc ",
    );
}

/// DEC-13 (negative): a LIVE request without FE-06 stays Shadow on the same candidate — built,
/// signed and simulated, no FE-04 admission (the wallet journal is not even opened) and not a
/// single submit call; the admission is OFF with LIVE's block reason.
#[test]
fn dec13b_without_fe06_a_live_request_admits_and_sends_nothing() {
    let (mut f, target, calls) = dec13b_fixture("dec13b_shadow", false);
    assert_eq!(f.owner.approval_status().state, "VALID");
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    let methods = calls.lock().unwrap().clone();
    assert!(
        methods.contains(&"simulateTransaction".to_string()),
        "{methods:?}"
    );
    assert!(
        !methods.contains(&"sendTransaction".to_string()),
        "{methods:?}"
    );
    assert_eq!(
        f.owner
            .submitter
            .attempts
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert!(f.owner.fe04_held.is_none() && f.owner.fe04_admission.is_none());
    assert_eq!(f.owner.fe04_admissions, Default::default());
    let s = f.owner.owner_snapshot(1).admission;
    assert_eq!(
        (s.state, s.block_reason.as_str()),
        (
            "OFF",
            crate::orchestration::capability_control::LIVE_BLOCK_REASON
        )
    );
    // The wallet lock is free: no admission holds the journal.
    crate::execution::one_shot::Journal::open(&f.dir.join("fe04"), &f.owner.liquidator.to_string())
        .unwrap();
    cleanup(f);
}

/// DEC-13b: no admission without its every condition — a held attempt, an FE-04 journal, a VALID
/// attempt approval naming the target, no ProgramData upgrade and a blockhash of known validity;
/// each refusal is the projected reason and the candidate stops at the send boundary unsent.
#[test]
fn dec13b_an_admission_needs_every_condition_and_names_the_missing_one() {
    let (mut f, target, calls) = dec13b_fixture("dec13b_refused", true);
    let reason = |f: &ShadowOwner| f.fe04_admission_facts(&target).err().map(|(r, _)| r);
    assert_eq!(reason(&f.owner), None);
    let other = Pubkey::new_unique();
    assert_eq!(
        f.owner.fe04_admission_facts(&other).err(),
        Some(("FE04_TARGET_NOT_APPROVED".into(), false))
    );
    f.owner.program_upgrade_latched = true;
    assert_eq!(reason(&f.owner).as_deref(), Some("FE04_PROGRAM_UPGRADED"));
    f.owner.program_upgrade_latched = false;
    let bh = f.owner.blockhash.get().unwrap();
    f.owner
        .blockhash
        .set(darkbot_solana::state::blockhash::CachedBlockhash {
            context_slot: None,
            ..bh
        });
    assert_eq!(
        reason(&f.owner).as_deref(),
        Some("FE04_BLOCKHASH_VALIDITY_UNKNOWN")
    );
    f.owner.blockhash.set(bh);
    let journal = f.owner.fe04_journal.take();
    assert_eq!(
        f.owner.fe04_admission_facts(&target).err(),
        Some(("FE04_JOURNAL_NOT_CONFIGURED".into(), true))
    );
    f.owner.fe04_journal = journal;
    let approval = f.owner.fe04_approval.take();
    assert_eq!(reason(&f.owner).as_deref(), Some("FE04_APPROVAL_NONE"));
    // The detection stops at the send boundary with that reason projected; nothing is sent.
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    assert!(!calls
        .lock()
        .unwrap()
        .contains(&"sendTransaction".to_string()));
    let s = f.owner.owner_snapshot(1).admission;
    assert_eq!(
        (s.state, s.block_reason.as_str()),
        ("BLOCKED", "FE04_APPROVAL_NONE")
    );
    f.owner.fe04_approval = approval;
    let dir = f.dir.join("fe04");
    crate::execution::one_shot::test_unknown_attempt(&dir, &f.owner.liquidator.to_string());
    f.owner.fe04_held = Some(
        crate::execution::one_shot::Journal::open(&dir, &f.owner.liquidator.to_string()).unwrap(),
    );
    assert!(reason(&f.owner).unwrap().starts_with("FE04_ATTEMPT_HELD"));
    // A halted Safety is shown as the block of every admission.
    f.owner.fe04_held = None;
    f.owner
        .safety
        .halt(darkbot_common::error::HaltReason::LimitBreached);
    let s = f.owner.owner_snapshot(2).admission;
    assert_eq!(
        (s.state, s.block_reason),
        (
            "BLOCKED",
            format!(
                "SAFETY_HALTED:{}",
                darkbot_common::error::HaltReason::LimitBreached.as_str()
            )
        )
    );
    cleanup(f);
}

/// DEC-13b: a loopback Jupiter answering every `/build` with the native-SOL sale of 1 000 units of
/// `mint` by `taker` (create its WSOL ATA, the route at 50 bps into it, close it to the taker).
fn dec13b_exit_jupiter(taker: Pubkey, mint: Pubkey) -> String {
    use base64::Engine;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use std::io::{BufRead, BufReader, Write};
    let jup: Pubkey = protocol::JUP_SWAP_PROGRAM_ID.parse().unwrap();
    let token: Pubkey = protocol::TOKEN_PROGRAM_ID.parse().unwrap();
    let wsol: Pubkey = crate::execution::route::WSOL_MINT.parse().unwrap();
    let ata = |m: &Pubkey| accounts::associated_token_address(&taker, m, &token);
    let json_ix = |ix: &Instruction| {
        serde_json::json!({
            "programId": ix.program_id.to_string(),
            "data": base64::engine::general_purpose::STANDARD.encode(&ix.data),
            "accounts": ix.accounts.iter().map(|a| serde_json::json!({
                "pubkey": a.pubkey.to_string(), "isSigner": a.is_signer,
                "isWritable": a.is_writable})).collect::<Vec<_>>()})
    };
    let mut data = vec![229, 23, 203, 151, 122, 227, 173, 42];
    data.extend(1u32.to_le_bytes());
    data.extend([7, 100, 0, 1]);
    data.extend(1_000u64.to_le_bytes());
    data.extend(20_000_000u64.to_le_bytes());
    data.extend(50u16.to_le_bytes());
    data.push(0);
    let swap = Instruction {
        program_id: jup,
        accounts: vec![
            AccountMeta::new_readonly(token, false),
            AccountMeta::new_readonly(taker, true),
            AccountMeta::new(ata(&mint), false),
            AccountMeta::new(ata(&wsol), false),
            AccountMeta::new_readonly(jup, false),
            AccountMeta::new_readonly(wsol, false),
            AccountMeta::new_readonly(jup, false),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
            AccountMeta::new_readonly(jup, false),
        ],
        data,
    };
    let close = Instruction {
        program_id: token,
        accounts: vec![
            AccountMeta::new(ata(&wsol), false),
            AccountMeta::new(taker, false),
            AccountMeta::new_readonly(taker, true),
        ],
        data: vec![9],
    };
    let create =
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &taker, &taker, &wsol, &token,
        );
    let body = serde_json::json!({
        "inAmount": "1000", "outAmount": "20000000", "otherAmountThreshold": "19900000",
        "swapMode": "ExactIn", "setupInstructions": [json_ix(&create)],
        "swapInstruction": json_ix(&swap), "cleanupInstruction": json_ix(&close),
        "otherInstructions": [], "addressesByLookupTableAddress": null})
    .to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    url
}

/// DEC-13b: with LIVE effective the held attempt's open position exits with its exit approval —
/// the route quoted now, one durable exit admission and exactly one send; the settlement then
/// reads that exit. An open exit admits no second one; without FE-06 nothing exits.
/// DEC-13b: a held, settled attempt with 1 000 units of a mint open, the exit approval of that
/// position (written, not read yet), its ATA, an empty WSOL ATA, lamports, a fresh blockhash, a
/// loopback Jupiter selling the position and the loopback node (`send_ok`); LIVE not requested.
fn dec13b_exit_fixture(
    tag: &str,
    send_ok: bool,
) -> (
    Fixture,
    Arc<Mutex<Vec<String>>>,
    crate::execution::one_shot::exit::ExitApproval,
) {
    use crate::execution::one_shot::approval_file::{ApprovalBinding, Bind, Usage, EXIT_SCHEMA};
    use solana_sdk::signer::Signer;
    let mut f = fixture_with(tag, None);
    let payer = solana_sdk::signature::Keypair::new();
    let liquidator = payer.pubkey();
    (f.owner.liquidator, f.owner.payer) = (liquidator, payer);
    f.owner.atas = accounts::LiquidatorAtas::new(liquidator);
    let (dir, wallet) = (f.dir.join("fe04"), liquidator.to_string());
    let now_s = wall_unix_s().unwrap();
    let mint_s =
        crate::execution::one_shot::exit::tests::settled_with_position(&dir, &wallet, now_s);
    let mint: Pubkey = mint_s.parse().unwrap();
    let jup = protocol::JUP_SWAP_PROGRAM_ID;
    let e = crate::execution::one_shot::exit::tests::exit_approval(&wallet, &mint_s, jup);
    let file = f.dir.join("exit.conf");
    std::fs::write(
        &file,
        format!(
            "schema = {EXIT_SCHEMA}\nauthorized = true\napproval_id = {}\nattempt_approval_id = {}\ncode_sha = {}\nbinary_sha256 = {}\nwallet = {wallet}\nmint = {mint_s}\nroute_program = {jup}\nexpires_unix_s = {}\nmax_state_age_ms = {}\nmin_out_lamports = {}\nmax_slippage_bps = {}\nmax_fee_lamports = {}\n",
            e.approval_id,
            e.attempt_approval_id,
            e.code_sha,
            e.binary_sha256,
            now_s + 3_600,
            e.max_state_age_ms.max(60_000),
            e.min_out_lamports,
            e.max_slippage_bps,
            e.max_fee_lamports
        ),
    )
    .unwrap();
    let dir_s = dir.display().to_string();
    let start = setup::reservation_book(
        &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
        &liquidator,
    )
    .unwrap()
    .start(&mut f.owner.safety, now_s)
    .unwrap();
    f.owner.reservations = start.book;
    f.owner.fe04_journal = start.dir;
    f.owner.fe04_held = start.held;
    f.owner.fe04_unbooked = start.unbooked;
    let source = ApprovalSource {
        path: file,
        binding: ApprovalBinding {
            wallet: Bind::Known(wallet.clone()),
            code_sha: Bind::Known(e.code_sha.clone()),
            binary_sha256: Bind::Known(e.binary_sha256.clone()),
            max_lifetime_seconds: 86_400,
            ..Default::default()
        },
        read: Err("APPROVAL_NOT_READ".into()),
        usage: Usage::Unchecked("APPROVAL_NOT_READ".into()),
    };
    // Not read yet: while LIVE is effective the owner's FE-04 refresh reads it (no status tap).
    f.owner.fe04_approval = Some(source);
    // The state the exit is checked against: the position's ATA, an empty WSOL ATA, lamports.
    let token: Pubkey = protocol::TOKEN_PROGRAM_ID.parse().unwrap();
    let wsol: Pubkey = crate::execution::route::WSOL_MINT.parse().unwrap();
    let ata = |m: &Pubkey, amount: u64| {
        let mut d = vec![0u8; 165];
        d[0..32].copy_from_slice(m.as_ref());
        d[32..64].copy_from_slice(liquidator.as_ref());
        d[64..72].copy_from_slice(&amount.to_le_bytes());
        d[108] = 1;
        AccountUpdate {
            pubkey: accounts::associated_token_address(&liquidator, m, &token),
            owner: token,
            lamports: 2_039_280,
            data: d,
            slot: 100,
            write_version: None,
            source: SourceKind::Snapshot,
            t_rx_ns: 0,
        }
    };
    let state = vec![
        p2_clock(100, now_s),
        AccountUpdate {
            pubkey: liquidator,
            owner: Pubkey::default(),
            lamports: 10_000_000_000,
            data: vec![],
            slot: 100,
            write_version: None,
            source: SourceKind::Snapshot,
            t_rx_ns: 0,
        },
        ata(&mint, 1_000),
        ata(&wsol, 0),
    ];
    f.owner.st.install_snapshot(100, state.clone());
    f.owner.st.reconcile(100, &state);
    f.owner.st.confirm_ready();
    f.owner.slot_watch.observe(100);
    f.owner
        .blockhash
        .set(darkbot_solana::state::blockhash::CachedBlockhash {
            hash: solana_sdk::hash::Hash::new_unique(),
            last_valid_block_height: 1_000,
            refreshed_ns: f.owner.clock.now_ns(),
            context_slot: Some(100),
        });
    f.owner.network = NetworkBudget::new(4, 5_000_000_000);
    f.owner.routes = JupiterV2RouteProvider::new(liquidator, None, 50, 64, false)
        .unwrap()
        .with_base_url(dec13b_exit_jupiter(liquidator, mint));
    let calls = dec13b_live_node(&mut f, send_ok, 0);
    (f, calls, e)
}

#[test]
fn dec13b_an_open_position_exits_once_with_its_approval_while_live() {
    let (mut f, calls, e) = dec13b_exit_fixture("dec13b_exit", true);
    // Without FE-06 nothing exits (the approval read and VALID).
    let (dir, wallet) = (f.dir.join("fe04"), f.owner.liquidator.to_string());
    f.owner
        .fe04_approval
        .as_mut()
        .unwrap()
        .refresh(Some(&dir), &wallet);
    assert_eq!(f.owner.approval_status().state, "VALID");
    dec13b_live(&mut f, false);
    f.owner.fe04_exit_admission();
    assert!(calls.lock().unwrap().is_empty());
    // Not read again yet: while LIVE is effective the FE-04 refresh reads it (no status tap).
    f.owner.fe04_approval.as_mut().unwrap().read = Err("APPROVAL_NOT_READ".into());
    dec13b_live(&mut f, true);
    // P8-CTRL.5: ECON_ATTEMPT DISABLED stops new liquidations, never this exit.
    f.owner.sources.capabilities.econ.set(
        crate::orchestration::capability_control::ECON_ATTEMPT,
        false,
    );
    f.owner.fe04_refreshed = std::time::Instant::now()
        .checked_sub(f.owner.fe04_refresh + Duration::from_millis(1))
        .unwrap();
    f.owner.lifecycle();
    assert_eq!(f.owner.approval_status().state, "VALID");
    assert_eq!(
        calls.lock().unwrap().clone(),
        ["getFeeForMessage", "simulateTransaction", "sendTransaction"],
        "{:?}",
        f.owner.fe04_admissions
    );
    assert_eq!(f.owner.fe04_admissions.exits_submitted, 1);
    let held = f.owner.fe04_held.as_ref().unwrap();
    assert_eq!(held.open_exit_id(), Some(e.approval_id.clone()));
    // An open exit admits no second one.
    f.owner.fe04_exit_admission();
    assert_eq!(calls.lock().unwrap().len(), 3);
    // The settlement reads exactly this exit.
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    let job = fe04_job(&f).expect("the exit read is due at once");
    assert_eq!(job.exit, Some(e.approval_id.clone()));
    cleanup(f);
}

/// DEC-13b (review M5, H1, M1, M2): a candidate refused after `check()` (here the approval's SOL
/// debit bound) releases its reservation at once — nothing durable, nothing sent, nothing to
/// book — and its admission stays open; the FE-04 refresh keeps the book readable while the
/// admission holds the journal. After a restart the unused admission's journal blocks nothing:
/// the next candidate is admitted on it. LIVE ending closes the admission and frees the wallet.
#[test]
fn dec13b_a_refused_candidate_releases_at_once_and_an_unused_journal_blocks_nothing() {
    let (mut f, target, calls) = dec13b_fixture("dec13b_refused_check", true);
    let (dir, wallet) = (f.dir.join("fe04"), f.owner.liquidator.to_string());
    let file = f.dir.join("approval.conf");
    let approval = std::fs::read_to_string(&file).unwrap();
    let set_approval = |f: &mut Fixture, text: &str| {
        std::fs::write(&file, text).unwrap();
        f.owner
            .fe04_approval
            .as_mut()
            .unwrap()
            .refresh(Some(&dir), &wallet);
    };
    set_approval(
        &mut f,
        &approval.replace(
            "max_total_sol_debit_lamports = 50000000",
            "max_total_sol_debit_lamports = 1",
        ),
    );
    let detect = |f: &mut Fixture, slot| {
        f.owner
            .detect(Some((
                target,
                f.owner.clock.now_ns(),
                slot,
                "account_update",
            )))
            .unwrap()
    };
    detect(&mut f, 100);
    let a = &f.owner.fe04_admissions;
    assert_eq!((a.submitted, a.refused, a.unknown), (0, 1, 0));
    assert_eq!(a.last_refusal, "APPROVAL_BUDGET_EXCEEDED:SOL_DEBIT");
    assert_eq!(
        (
            f.owner.safety.capital_in_use(),
            f.owner.safety.inventory_reserved()
        ),
        (0, 0),
        "released at once, before anything durable"
    );
    assert!(f.owner.fe04_unbooked.is_none() && f.owner.fe04_held.is_none());
    assert!(f.owner.fe04_admission.is_some(), "the admission stays open");
    assert!(!calls
        .lock()
        .unwrap()
        .contains(&"sendTransaction".to_string()));
    // The FE-04 refresh while the admission holds the journal (and its lock).
    f.owner.fe04_refreshed = std::time::Instant::now()
        .checked_sub(f.owner.fe04_refresh + Duration::from_millis(1))
        .unwrap();
    f.owner.lifecycle();
    assert!(
        !f.owner.reservations.is_unreadable(),
        "the book follows the open journal"
    );
    // Restart: the unused journal is the held one and blocks nothing.
    f.owner.fe04_admission = None;
    let dir_s = dir.display().to_string();
    let start = setup::reservation_book(
        &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
        &f.owner.liquidator,
    )
    .unwrap()
    .start(&mut f.owner.safety, wall_unix_s().unwrap())
    .unwrap();
    assert!(start
        .held
        .as_ref()
        .is_some_and(|j| j.step_state().is_none()));
    f.owner.fe04_held = start.held;
    set_approval(&mut f, &approval);
    assert_eq!(f.owner.fe04_admission_facts(&target).err(), None);
    detect(&mut f, 101);
    assert_eq!(
        f.owner.fe04_admissions.submitted, 1,
        "{:?}",
        f.owner.fe04_admissions
    );
    assert!(f.owner.fe04_held.as_ref().unwrap().step_state().is_some());
    cleanup(f);
}

/// DEC-13b (review M2, M3): an admission is closed once LIVE is no longer effective (its wallet
/// lock released), and a journal holding an attempt another owner left is never admitted over:
/// it is closed again with the reason, nothing is reserved or sent.
#[test]
fn dec13b_live_ending_closes_the_admission_and_a_foreign_attempt_is_never_admitted_over() {
    let (mut f, target, calls) = dec13b_fixture("dec13b_foreign", true);
    let (dir, wallet) = (f.dir.join("fe04"), f.owner.liquidator.to_string());
    let file = f.dir.join("approval.conf");
    let approval = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        approval.replace(
            "max_total_sol_debit_lamports = 50000000",
            "max_total_sol_debit_lamports = 1",
        ),
    )
    .unwrap();
    f.owner
        .fe04_approval
        .as_mut()
        .unwrap()
        .refresh(Some(&dir), &wallet);
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    assert!(f.owner.fe04_admission.is_some());
    f.owner.sources.capabilities.live_requested =
        crate::orchestration::capability_control::LIVE_OFF;
    f.owner.lifecycle();
    assert!(f.owner.fe04_admission.is_none(), "closed with LIVE");
    // An attempt another owner left (UNKNOWN) in the wallet journal.
    crate::execution::one_shot::test_unknown_attempt(&dir, &wallet);
    f.owner.sources.capabilities.live_requested = crate::orchestration::capability_control::LIVE;
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            101,
            "account_update",
        )))
        .unwrap();
    assert_eq!(
        f.owner.fe04_admissions.block_reason,
        "FE04_JOURNAL_HAS_ATTEMPT:UNKNOWN"
    );
    assert!(f.owner.fe04_admission.is_none() && f.owner.fe04_held.is_none());
    assert_eq!(f.owner.safety.capital_in_use(), 0);
    assert!(!calls
        .lock()
        .unwrap()
        .contains(&"sendTransaction".to_string()));
    // The journal is free again for its owner.
    crate::execution::one_shot::Journal::open(&dir, &wallet).unwrap();
    cleanup(f);
}

/// DEC-13b (review M4, M5): a send whose fate is unknown (the node answers an error after the
/// durable handoff) halts Safety and keeps every hold — capital and inventory reserved, the
/// attempt UNKNOWN in the journal read back from the disk — until the settlement's proven
/// non-landing books it, releasing everything exactly once; a repeated answer releases nothing.
#[test]
fn dec13b_an_unknown_send_keeps_every_hold_until_the_settlement_books_it_once() {
    use crate::execution::one_shot::settlement::{SettlementEvidence, SettlementSubject};
    let (mut f, target, calls) = dec13b_fixture_with("dec13b_unknown", true, false, 0);
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    assert!(calls
        .lock()
        .unwrap()
        .contains(&"sendTransaction".to_string()));
    let a = &f.owner.fe04_admissions;
    assert_eq!((a.submitted, a.unknown), (0, 1));
    assert_eq!(
        f.owner.safety.halted(),
        Some(darkbot_common::error::HaltReason::UnknownEconomics)
    );
    let held = f.owner.fe04_held.as_ref().unwrap();
    assert_eq!(
        held.step_state(),
        Some(crate::execution::accounting::StepState::Unknown)
    );
    let unbooked = f.owner.fe04_unbooked.unwrap();
    assert!(unbooked.reserved_inventory > 0);
    assert_eq!(
        f.owner.safety.inventory_reserved(),
        unbooked.reserved_inventory
    );
    let (intent, _) = held.settlement_job().unwrap();
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert_eq!(fe04_job(&f).unwrap().subject.signatures, intent.signatures);
    let evidence = SettlementEvidence {
        observations: vec![],
        non_landing: Some(Ok(
            crate::execution::one_shot::non_landing::loopback::proving(
                &SettlementSubject::from(&intent).non_landing().unwrap(),
                true,
            ),
        )),
    };
    fe04_answer(&f, evidence.clone());
    run_until(&mut f, fe04_settled);
    assert_eq!(
        (
            f.owner.safety.capital_in_use(),
            f.owner.safety.inventory_reserved()
        ),
        (0, 0)
    );
    let day = f.owner.safety.day();
    fe04_answer(&f, evidence);
    run_until(&mut f, |o| !o.remote.fe04_settlement_results.pending());
    assert_eq!(f.owner.safety.day(), day, "booked once");
    assert_eq!(
        f.owner.safety.halted(),
        Some(darkbot_common::error::HaltReason::UnknownEconomics),
        "no ledger violation"
    );
    cleanup(f);
}

/// DEC-13b (review M5, L4): an exit whose send fate is unknown halts Safety, stays open UNKNOWN
/// in the journal for the settlement and ends the exits of the run (no second route or send).
#[test]
fn dec13b_an_unknown_exit_halts_and_ends_the_exits_of_the_run() {
    let (mut f, calls, e) = dec13b_exit_fixture("dec13b_exit_unknown", false);
    dec13b_live(&mut f, true);
    f.owner.fe04_refreshed = std::time::Instant::now()
        .checked_sub(f.owner.fe04_refresh + Duration::from_millis(1))
        .unwrap();
    f.owner.lifecycle();
    assert_eq!(
        f.owner.fe04_admissions.exits_unknown, 1,
        "{:?}",
        f.owner.fe04_admissions
    );
    // Halted (the replayed one-shot already was; the first reason stays).
    assert!(f.owner.safety.halted().is_some());
    assert_eq!(
        f.owner.fe04_held.as_ref().unwrap().open_exit_id(),
        Some(e.approval_id.clone())
    );
    let sends = calls.lock().unwrap().len();
    f.owner.fe04_exit_admission();
    assert_eq!(
        calls.lock().unwrap().len(),
        sends,
        "no second exit in this run"
    );
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert_eq!(fe04_job(&f).unwrap().exit, Some(e.approval_id.clone()));
    cleanup(f);
}

/// DEC-13b (review M4): a reservation the handoff refused after it was made durable (the state
/// aged past the approval's `max_state_age_ms` during a slow simulation; nothing sent) is read
/// back from the disk the way a restart reads it — UNKNOWN, every hold kept — so the running
/// owner's settlement proves its non-landing instead of the attempt staying RESERVED all run.
#[test]
fn dec13b_a_reservation_the_handoff_refused_is_read_back_unknown_for_the_settlement() {
    let (mut f, target, calls) = dec13b_fixture_with("dec13b_aged", true, true, 1_200);
    let (dir, wallet) = (f.dir.join("fe04"), f.owner.liquidator.to_string());
    let file = f.dir.join("approval.conf");
    let approval = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        approval.replace("max_state_age_ms = 60000", "max_state_age_ms = 1000"),
    )
    .unwrap();
    f.owner
        .fe04_approval
        .as_mut()
        .unwrap()
        .refresh(Some(&dir), &wallet);
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    assert!(!calls
        .lock()
        .unwrap()
        .contains(&"sendTransaction".to_string()));
    assert_eq!(
        f.owner.fe04_admissions.unknown, 1,
        "{:?}",
        f.owner.fe04_admissions
    );
    let held = f.owner.fe04_held.as_ref().unwrap();
    assert_eq!(
        held.step_state(),
        Some(crate::execution::accounting::StepState::Unknown),
        "never RESERVED for the rest of the run"
    );
    assert!(f.owner.fe04_unbooked.is_some());
    run_until(&mut f, |o| o.fe04_settle_due.is_none());
    assert!(fe04_job(&f).is_some(), "the settlement reads it");
    cleanup(f);
}

/// P8-CTRL.3/.5 (review M3): the status names the grant in force — its scope's mode, validity,
/// campaign and limits and the attempts booked against them (today, the campaign, the grant; one
/// of an earlier day or another campaign or grant counts only where it belongs; an unreadable
/// journal leaves them unknown, never 0) — and only this epoch's previews, read at the refresh
/// cadence (a new store entry appears at the next refresh, not before); a grant outside its
/// validity is not in force, with LIVE's reason.
#[test]
fn p8ctrl04_the_status_names_the_grant_its_usage_and_this_epochs_previews() {
    use crate::execution::one_shot::Fe06Attempt;
    use crate::orchestration::{capability_control as cap, fe06};
    let mut f = fixture_with("p8ctrl04_status", None);
    let signer = f.owner.liquidator;
    let now = super::accounts::wall_unix_s().unwrap();
    let scope = |epoch: u64, campaign: &str, from: u64, until: u64| {
        fe06::parse(
            fe06::tests::text(
                &signer,
                &[
                    ("owner_epoch", &epoch.to_string()),
                    ("campaign_id", campaign),
                    ("not_before_unix_s", &from.to_string()),
                    ("expires_unix_s", &until.to_string()),
                    ("max_attempts_per_utc_day", "2"),
                    ("max_attempts_campaign", "3"),
                    ("max_attempts_grant", "4"),
                ],
            )
            .as_bytes(),
            &fe06::tests::ceilings(),
        )
        .unwrap()
    };
    let granted = scope(3, "first-profit", now - 60, now + 3_000);
    let digest = fe06::digest(&granted).unwrap();
    fe06::put_preview(&f.dir, &granted).unwrap();
    fe06::put_preview(&f.dir, &scope(2, "first-profit", now - 60, now + 3_000)).unwrap();
    {
        let c = &mut f.owner.sources.capabilities;
        (c.dir, c.config_version, c.epoch) = (Some(cap::request_dir(&f.dir)), Some(0), Some(3));
        c.fe06 = Some(cap::Fe06Authorization::offline_test_of(granted.clone()));
    }
    let today = now / 86_400;
    let booking = |grant: &str, campaign: &str, utc_day| Fe06Attempt {
        grant: grant.into(),
        campaign: campaign.into(),
        utc_day,
        owner_epoch: Some(3),
    };
    f.owner.ledger_view = Some(Ok(super::status_view::LedgerView {
        totals: Default::default(),
        verdict: crate::execution::accounting::Verdict::NoTrade,
        bookings: vec![
            booking(&digest, "first-profit", today),
            booking(&digest, "first-profit", today - 1),
            booking("another-grant", "first-profit", today),
            booking("another-grant", "another-campaign", today),
        ],
        // P8-CTRL.5: one exit under this grant, one under another.
        exit_bookings: vec![
            booking(&digest, "first-profit", today),
            booking("another-grant", "first-profit", today),
        ],
    }));
    f.owner
        .sources
        .capabilities
        .refresh_previews(Duration::ZERO);
    let s = f.owner.owner_snapshot(1).fe06;
    assert_eq!(
        (
            s.grant_digest.as_deref(),
            s.scope_mode,
            s.valid_from_unix_s,
            s.valid_until_unix_s,
            s.in_force,
            s.campaign_id.as_str(),
            s.block_reason.as_str()
        ),
        (
            Some(digest.as_str()),
            "ALL",
            Some(now - 60),
            Some(now + 3_000),
            true,
            "first-profit",
            ""
        )
    );
    assert_eq!(
        (
            s.max_attempts_per_utc_day,
            s.max_attempts_campaign,
            s.max_attempts_grant
        ),
        (2, 3, 4)
    );
    assert_eq!(
        (s.attempts_today, s.attempts_campaign, s.attempts_grant),
        (Some(3), Some(3), Some(2))
    );
    assert_eq!((s.max_exits_grant, s.exits_grant), (1, Some(1)));
    // Only this epoch's preview (the granted scope), with the owner's terminal text.
    assert_eq!(
        s.previews
            .iter()
            .map(|p| (p.digest.as_str(), p.owner_epoch))
            .collect::<Vec<_>>(),
        [(digest.as_str(), 3)]
    );
    assert!(s.previews[0]
        .terminal
        .starts_with(&format!("PROJECT 0 FE-06 SCOPE digest={digest}")));
    // A scope previewed after the refresh appears only at the next one.
    let later = scope(3, "second", now - 60, now + 3_000);
    let later_digest = fe06::put_preview(&f.dir, &later).unwrap();
    f.owner
        .sources
        .capabilities
        .refresh_previews(Duration::from_secs(3_600));
    assert_eq!(f.owner.owner_snapshot(2).fe06.previews.len(), 1);
    f.owner.sources.capabilities.previews = Default::default();
    f.owner
        .sources
        .capabilities
        .refresh_previews(Duration::from_secs(3_600));
    let shown: Vec<_> = f
        .owner
        .owner_snapshot(3)
        .fe06
        .previews
        .into_iter()
        .map(|p| p.digest)
        .collect();
    assert!(
        shown.contains(&later_digest) && shown.len() == 2,
        "{shown:?}"
    );
    // A preview no longer in the store is no preview any more (review NEW-L1).
    std::fs::remove_file(
        f.dir
            .join(fe06::STORE)
            .join(format!("scope_{later_digest}.p0j")),
    )
    .unwrap();
    f.owner
        .sources
        .capabilities
        .refresh_previews(Duration::ZERO);
    assert!(!f
        .owner
        .owner_snapshot(3)
        .fe06
        .previews
        .iter()
        .any(|p| p.digest == later_digest));
    // An unreadable journal: the usage is unknown, never 0.
    f.owner.ledger_view = Some(Err("JOURNAL_CORRUPT".into()));
    let s = f.owner.owner_snapshot(4).fe06;
    assert_eq!(
        (
            s.attempts_today,
            s.attempts_campaign,
            s.attempts_grant,
            s.exits_grant
        ),
        (None, None, None, None)
    );
    // A grant outside its validity is not in force, with LIVE's reason.
    let expired = scope(3, "first-profit", now - 3_000, now - 60);
    let expired_digest = fe06::digest(&expired).unwrap();
    f.owner.sources.capabilities.fe06 = Some(cap::Fe06Authorization::offline_test_of(expired));
    let s = f.owner.owner_snapshot(5).fe06;
    assert!(!s.in_force);
    assert_eq!(s.block_reason, cap::grant_not_in_force(&expired_digest));
    cleanup(f);
}

/// P8-CTRL.3: the running identity of a DEC-13b fixture owner and a scope that binds exactly it —
/// the fixture's program, deployment, signer, banks and mints; generous bounds within the test
/// ceilings — of owner epoch 3; `edit` changes the scope's text, the identity after it.
fn p8ctrl03_scoped(
    f: &mut Fixture,
    overrides: &[(&str, &str)],
    edit: impl FnOnce(&mut crate::orchestration::fe06::Running),
) {
    use crate::orchestration::{capability_control as cap, fe06};
    let now = super::accounts::wall_unix_s().unwrap();
    let mark = |k: &str| f.marks.get(k).map(ToString::to_string);
    let (from, until) = ((now - 60).to_string(), (now + 3_000).to_string());
    // The fixture's banks and mints where its builder names them (else the scope's own).
    let banks = mark("asset_bank")
        .zip(mark("liab_bank"))
        .map(|(a, l)| format!("{a},{l}"));
    let mints = mark("asset_mint")
        .zip(mark("liab_mint"))
        .zip(mark("sol_mint"))
        .map(|((a, l), s)| format!("{a},{l},{s}"));
    let program = f.owner.program.to_string();
    let deployment = f.owner.program_data_baseline_hash.to_string();
    let mut fields: Vec<(&str, &str)> = vec![
        ("programs", &program),
        ("program_data_hash", &deployment),
        ("max_input_per_attempt_usd_micro", "100000000"),
        ("max_exposure_usd_micro", "100000000"),
        ("max_loss_usd_micro", "50000000"),
        ("max_rent_lamports", "50000000"),
        ("max_spend_lamports", "50000000"),
        ("max_protocol_fee_lamports", "50000000"),
        ("not_before_unix_s", &from),
        ("expires_unix_s", &until),
    ];
    if let Some(banks) = &banks {
        fields.push(("banks", banks));
    }
    if let Some(mints) = &mints {
        fields.push(("mints", mints));
    }
    for (k, v) in overrides {
        fields.retain(|(f, _)| f != k);
        fields.push((k, v));
    }
    let scope = fe06::parse(
        fe06::tests::text(&f.owner.liquidator, &fields).as_bytes(),
        &fe06::tests::ceilings(),
    )
    .unwrap();
    let mut running = fe06::Running {
        genesis_hash: darkbot_project0::protocol::GENESIS_HASH_MAINNET.into(),
        program: program.clone(),
        program_data_hash: deployment.clone(),
        code_sha: Some(scope.code_sha.clone()),
        binary_sha256: Some(scope.binary_sha256.clone()),
        runtime_config_sha256: scope.runtime_config_sha256.clone(),
        signers: [format!("{}:LIQUIDATOR+FEE_PAYER", f.owner.liquidator)].into(),
        owner_epoch: scope.owner_epoch,
    };
    edit(&mut running);
    let c = &mut f.owner.sources.capabilities;
    c.fe06 = Some(cap::Fe06Authorization::offline_test_of(scope));
    c.running = Some(running);
}

/// P8-CTRL.3/.5 (L03/L04/L14): a grant whose scope binds exactly what runs sends the candidate
/// once; one differing identity — the signers, the binary, an unknown identity — keeps LIVE
/// BLOCKED (a Shadow candidate, never a real signature), and one fact of the attempt outside the
/// scope — its fee, data source, the grant's spend, the reserve, the priority fee, no canonical
/// source — refuses its real signature (review H1: before any byte is signed, simulated or
/// sent), nothing durable.
#[test]
fn p8ctrl03_only_a_scope_that_binds_what_runs_and_the_attempt_sends() {
    let detect = |f: &mut Fixture, target: Pubkey| {
        f.owner
            .detect(Some((
                target,
                f.owner.clock.now_ns(),
                100,
                "account_update",
            )))
            .unwrap();
    };
    let methods = |calls: &Arc<Mutex<Vec<String>>>| calls.lock().unwrap().clone();
    let (mut f, target, calls) = dec13b_fixture("p8ctrl03_bound", true);
    p8ctrl03_scoped(&mut f, &[("max_exit_cost_lamports", "123456")], |_| {});
    assert!(f.owner.live_effective());
    detect(&mut f, target);
    assert_eq!(
        methods(&calls)
            .iter()
            .filter(|m| *m == "sendTransaction")
            .count(),
        1,
        "{:?} {:?}",
        methods(&calls),
        f.owner.fe04_admissions
    );
    // P8-CTRL.5 (WP1): the attempt holds its SOL debit and, beside it, the grant's exit reserve.
    let held = f.owner.fe04_held.as_ref().unwrap();
    let debit = held.attempt_records().unwrap()[0]
        .sol_debit_lamports
        .unwrap();
    assert_eq!(held.holds()["SOL"], debit + 123_456);
    cleanup(f);
    // What runs differs: no real signature at all.
    type Edit = Box<dyn FnOnce(&mut crate::orchestration::fe06::Running)>;
    let identities: [(&str, Option<Edit>, &str); 3] = [
        (
            "signers",
            Some(Box::new(|r| {
                r.signers = [format!("{}:LIQUIDATOR", Pubkey::new_unique())].into()
            })),
            "FE06_SCOPE_BINDING: signers differs",
        ),
        (
            "binary",
            Some(Box::new(|r| r.binary_sha256 = Some("a".repeat(64)))),
            "FE06_SCOPE_BINDING: binary_sha256 differs",
        ),
        ("unknown", None, "what the owner runs is unknown"),
    ];
    for (tag, edit, reason) in identities {
        let (mut f, target, calls) = dec13b_fixture(&format!("p8ctrl03_runs_{tag}"), true);
        p8ctrl03_scoped(&mut f, &[], |r| {
            if let Some(edit) = edit {
                edit(r)
            }
        });
        if tag == "unknown" {
            f.owner.sources.capabilities.running = None;
        }
        // Review M3: LIVE says so — BLOCKED with the binding's reason; the candidate is a
        // Shadow one, signed synthetically (P8-CTRL.4), never sent.
        assert!(!f.owner.live_effective(), "{tag}");
        let (_, blocked) =
            crate::orchestration::capability_control::live_decision(&f.owner.sources.live_gates());
        assert!(blocked.contains(reason), "{tag}: {blocked}");
        detect(&mut f, target);
        let m = methods(&calls);
        assert!(!m.iter().any(|m| m == "sendTransaction"), "{tag}: {m:?}");
        assert!(f.owner.fe04_held.is_none(), "{tag}");
        cleanup(f);
    }
    // One fact of the attempt outside the scope: refused by its sign permit, nothing durable.
    let facts: [(&str, (&str, &str), &str); 3] = [
        (
            "fee",
            ("max_fee_lamports", "4999"),
            "FE06_SCOPE_USE: fee_lamports 5000 ",
        ),
        (
            "source",
            ("data_sources", "geyser:other"),
            "FE06_SCOPE_USE: data_source ",
        ),
        (
            "spend",
            ("max_spend_lamports", "1"),
            "FE06_SCOPE_USE: grant_spend_lamports ",
        ),
    ];
    for (tag, field, reason) in facts {
        let (mut f, target, calls) = dec13b_fixture(&format!("p8ctrl03_use_{tag}"), true);
        p8ctrl03_scoped(&mut f, &[field], |_| {});
        p8ctrl03_refused(f, target, &calls, tag, reason);
    }
    // The wallet's SOL after the whole spend below the scope's reserve (10 SOL held).
    let (mut f, target, calls) = dec13b_fixture("p8ctrl03_use_reserve", true);
    p8ctrl03_scoped(&mut f, &[("reserve_lamports", "10000000000")], |_| {});
    let reason = "FE06_SCOPE_USE: sol_after_spend_lamports ";
    p8ctrl03_refused(f, target, &calls, "reserve", reason);
    // The priority fee the signed payload requests (1 400 000 CU × 1 000 µL ⇒ 1 400 lamports).
    let (mut f, target, calls) = dec13b_fixture("p8ctrl03_use_priority", true);
    f.owner.resources.priority_micro_lamports_per_cu = 1_000;
    p8ctrl03_scoped(&mut f, &[("max_priority_fee_lamports", "1399")], |_| {});
    let reason = "FE06_SCOPE_USE: priority_fee_lamports 1400 ";
    p8ctrl03_refused(f, target, &calls, "priority", reason);
    // No canonical source (WS OFF): the state's source is unknown.
    let (mut f, target, calls) = dec13b_fixture("p8ctrl03_use_no_source", true);
    p8ctrl03_scoped(&mut f, &[], |_| {});
    p8ctrl06_sources(
        &mut f,
        darkbot_solana::ingress::sources::OperatingMode::Off,
        None,
    );
    let reason = "FE06_SCOPE_USE_UNKNOWN: data_source ";
    p8ctrl03_refused(f, target, &calls, "no_source", reason);
}

/// The candidate of `target` refused by its sign permit with `reason` (review H1: before any byte
/// is signed): nothing signed with the wallet, simulated or sent, nothing durable; the candidate's
/// record names the refusal.
fn p8ctrl03_refused(
    mut f: Fixture,
    target: Pubkey,
    calls: &Arc<Mutex<Vec<String>>>,
    tag: &str,
    reason: &str,
) {
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    let m = calls.lock().unwrap().clone();
    assert!(
        !m.iter()
            .any(|m| m == "sendTransaction" || m == "simulateTransaction"),
        "{tag}: {m:?}"
    );
    assert!(f.owner.fe04_held.is_none(), "{tag}: nothing durable");
    let dir = f.dir.clone();
    f.owner.finish().unwrap();
    let journal = std::fs::read(dir.join("opportunities.p0j")).unwrap();
    assert!(
        journal
            .windows(reason.len())
            .any(|w| w == reason.as_bytes()),
        "{tag}: {reason}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// P8-CTRL.6/.7: the fixture owner's sources rebuilt with the WS unit in `mode` and the control
/// queue `controls`, its capability state kept.
fn p8ctrl06_sources(
    f: &mut Fixture,
    mode: darkbot_solana::ingress::sources::OperatingMode,
    controls: Option<
        darkbot_common::channel::Consumer<crate::orchestration::source_control::ControlMessage>,
    >,
) {
    let capabilities = std::mem::take(&mut f.owner.sources.capabilities);
    f.owner.sources = sources::OwnerSources::new(
        darkbot_solana::ingress::sources::SourceArbiter::new(&[
            crate::orchestration::source_control::control_of(
                darkbot_solana::ingress::sources::SourceUnit::WsRpcControl,
                mode,
            )
            .unwrap(),
        ])
        .unwrap(),
        None,
        Box::new(|_, _, _, _, _, _, _| Err("P8CTRL06_NO_SOURCE".to_string())),
        64,
        &f.owner.wake.clone(),
        controls,
    );
    f.owner.sources.capabilities = capabilities;
}

/// P8-CTRL.7/.10 (L07): a DISABLE is linearized against the egress — before the candidate it
/// leaves a Shadow one (never sent); durably requested while the candidate is simulated it waits
/// as a counted stop and prevents the egress before anything durable (review M5: no network wait
/// delays it); once the payload is handed off it cannot recall it: the attempt stays held
/// (SUBMITTED, UNKNOWN until its settlement) and LIVE is BLOCKED from then on.
#[test]
fn p8ctrl06_l07_a_disable_is_linearized_before_or_after_the_egress_never_inside() {
    use crate::orchestration::capability_control as cap;
    use crate::orchestration::source_control::ControlMessage;
    // Before: ECON_SIGN DISABLED — LIVE is BLOCKED by it; the candidate is a Shadow one,
    // signed synthetically (P8-CTRL.4), never sent.
    let (mut f, target, calls) = dec13b_fixture("p8ctrl06_l07_before", true);
    p8ctrl03_scoped(&mut f, &[], |_| {});
    f.owner.sources.capabilities.econ.set(cap::ECON_SIGN, false);
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    assert!(!f.owner.live_effective());
    assert!(!calls.lock().unwrap().iter().any(|m| m == "sendTransaction"));
    assert!(f.owner.fe04_held.is_none());
    cleanup(f);
    // During (review M5): a DISABLE durably requested while the candidate is being simulated —
    // p0ctl's request log, the real control thread — waits as a counted stop before the owner's
    // next step: the egress permit is refused, nothing durable, nothing sent.
    let (mut f, target, calls) = dec13b_fixture_with("p8ctrl06_l07_during", true, true, 800);
    p8ctrl03_scoped(&mut f, &[], |_| {});
    let control = cap::request_dir(&f.dir);
    {
        let c = &mut f.owner.sources.capabilities;
        (c.dir, c.config_version, c.epoch) = (Some(control.clone()), Some(0), Some(3));
    }
    let (tx, rx) = darkbot_common::channel::bounded::<ControlMessage>("p8ctrl06-l07", 8);
    p8ctrl06_sources(
        &mut f,
        darkbot_solana::ingress::sources::OperatingMode::Active,
        Some(rx),
    );
    let shutdown = darkbot_common::lifecycle::Shutdown::new();
    let sources_dir = crate::orchestration::source_control::request_dir(&f.dir);
    let thread = crate::orchestration::source_control::spawn(
        sources_dir.clone(),
        tx,
        shutdown.clone(),
        crate::orchestration::source_control::head(&sources_dir).unwrap(),
        Some((
            control.clone(),
            cap::head(&control).unwrap(),
            f.owner.sources.capabilities.stop.clone(),
        )),
    )
    .unwrap();
    // The operator's DISABLE while the candidate's simulation runs: p0ctl's request, its
    // doorbell, the control thread counting it — before the simulation answers.
    let writer = {
        let (control, calls, stop) = (
            control.clone(),
            calls.clone(),
            f.owner.sources.capabilities.stop.clone(),
        );
        std::thread::spawn(move || {
            let t = std::time::Instant::now();
            while !calls
                .lock()
                .unwrap()
                .iter()
                .any(|m| m == "simulateTransaction")
            {
                assert!(
                    t.elapsed() < Duration::from_secs(10),
                    "the simulation starts"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            cap::write_request(
                &control,
                &cap::NewRequest {
                    capability: cap::ECON_SEND,
                    requested: cap::DISABLED,
                    request_id: "l07-disable",
                    owner_epoch: 3,
                    expected: Some(0),
                    now_unix_s: 1,
                },
            )
            .unwrap();
            // SAFETY: SIGUSR1 to this process: the control thread's doorbell (as p0ctl rings it).
            unsafe {
                libc::kill(std::process::id() as i32, libc::SIGUSR1);
            }
            while !stop.pending() {
                assert!(t.elapsed() < Duration::from_secs(10), "the stop is counted");
                std::thread::sleep(Duration::from_millis(1));
            }
        })
    };
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    writer.join().unwrap();
    let sent = |calls: &Arc<Mutex<Vec<String>>>| {
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|m| *m == "sendTransaction")
            .count()
    };
    assert_eq!(sent(&calls), 0, "the waiting stop prevented the egress");
    assert!(f.owner.fe04_held.is_none(), "nothing durable");
    assert_eq!(f.owner.fe04_admissions.last_refusal, cap::STOP_PENDING);
    // The owner's next step decides it: from then on LIVE is BLOCKED by it.
    let t = std::time::Instant::now();
    while f.owner.sources.capabilities.stop.pending() {
        f.owner.sources_step(&f.ingress).unwrap();
        assert!(t.elapsed() < Duration::from_secs(5), "the stop is decided");
    }
    let (effective, reason) = cap::live_decision(&f.owner.sources.live_gates());
    assert_eq!(effective, "BLOCKED");
    assert!(reason.starts_with("ECON_SEND DISABLED"), "{reason}");
    shutdown.request();
    thread.join().unwrap();
    cleanup(f);
    // After the start: a DISABLE that comes once the payload is handed off cannot recall it —
    // the attempt stays held (SUBMITTED, UNKNOWN until its settlement), LIVE is BLOCKED.
    let (mut f, target, calls) = dec13b_fixture("p8ctrl06_l07_after", true);
    p8ctrl03_scoped(&mut f, &[], |_| {});
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    assert_eq!(sent(&calls), 1);
    f.owner.apply_capability_control(
        1,
        Ok(crate::analytics::payload::CapabilityControlRequest {
            seq: 1,
            capability: cap::ECON_SEND.into(),
            requested: cap::DISABLED.into(),
            requested_unix_s: 1,
            request_id: "l07-after".into(),
            owner_epoch: 3,
        }),
    );
    assert!(!f.owner.live_effective());
    assert!(
        f.owner.fe04_held.is_some() && f.owner.fe04_unbooked.is_some(),
        "the handed-off attempt keeps every hold"
    );
    cleanup(f);
}

/// P8-CTRL.5/.10 (L11): EXIT_ONLY exits exactly its named open positions — the named one exits
/// once, booked against the grant; another position is refused by its sign permit (nothing
/// signed, simulated or sent); ALL takes no position neither named nor of its own attempts; a
/// stop — ECON_LIVE or ECON_SEND DISABLED, the grant revoked — blocks every new exit (no stop
/// bypass).
#[test]
fn p8ctrl06_l11_exit_only_exits_exactly_its_positions_and_never_past_a_stop() {
    use crate::orchestration::capability_control as cap;
    // ALL (review M18): only the positions it names or of attempts booked against it — the
    // fixture's attempt was made under no grant.
    let (mut f, calls, e) = dec13b_exit_fixture("p8ctrl06_l11_all", true);
    let (dir, wallet) = (f.dir.join("fe04"), f.owner.liquidator.to_string());
    f.owner
        .fe04_approval
        .as_mut()
        .unwrap()
        .refresh(Some(&dir), &wallet);
    dec13b_live(&mut f, true);
    p8ctrl03_scoped(&mut f, &[("mints", e.mint.as_str())], |_| {});
    f.owner.fe04_refreshed = std::time::Instant::now()
        .checked_sub(f.owner.fe04_refresh + Duration::from_millis(1))
        .unwrap();
    f.owner.lifecycle();
    assert!(!calls
        .lock()
        .unwrap()
        .iter()
        .any(|m| m == "sendTransaction" || m == "simulateTransaction"));
    assert!(
        f.owner
            .fe04_admissions
            .last_exit_refusal
            .starts_with("FE06_SCOPE_USE: exit_position "),
        "{:?}",
        f.owner.fe04_admissions
    );
    cleanup(f);
    for (tag, named, stop) in [
        ("named", true, None),
        ("other", false, None),
        ("stopped", true, Some(cap::ECON_LIVE)),
        ("send_off", true, Some(cap::ECON_SEND)),
        ("revoked", true, Some(cap::FE06_REVOKE)),
    ] {
        let (mut f, calls, e) = dec13b_exit_fixture(&format!("p8ctrl06_l11_{tag}"), true);
        let (dir, wallet) = (f.dir.join("fe04"), f.owner.liquidator.to_string());
        f.owner
            .fe04_approval
            .as_mut()
            .unwrap()
            .refresh(Some(&dir), &wallet);
        dec13b_live(&mut f, true);
        let position = if named {
            format!("{}:{}", e.attempt_approval_id, e.mint)
        } else {
            format!("another-attempt:{}", e.mint)
        };
        p8ctrl03_scoped(
            &mut f,
            &[
                ("mode", "EXIT_ONLY"),
                ("exit_positions", position.as_str()),
                ("mints", e.mint.as_str()),
                ("max_attempts_per_utc_day", "0"),
                ("max_attempts_campaign", "0"),
                ("max_attempts_grant", "0"),
            ],
            |_| {},
        );
        let digest = f
            .owner
            .sources
            .capabilities
            .fe06
            .as_ref()
            .unwrap()
            .digest()
            .to_string();
        // No stop bypass (P8-CTRL.5): ECON_LIVE or ECON_SEND DISABLED, or the grant revoked.
        match stop {
            Some(cap::FE06_REVOKE) => f.owner.sources.capabilities.fe06 = None,
            Some(switch) => {
                f.owner.sources.capabilities.econ.set(switch, false);
            }
            None => {}
        }
        f.owner.fe04_refreshed = std::time::Instant::now()
            .checked_sub(f.owner.fe04_refresh + Duration::from_millis(1))
            .unwrap();
        f.owner.lifecycle();
        let sends = calls
            .lock()
            .unwrap()
            .iter()
            .filter(|m| *m == "sendTransaction")
            .count();
        match tag {
            "named" => {
                assert_eq!(sends, 1, "{:?}", f.owner.fe04_admissions);
                let records = f
                    .owner
                    .fe04_held
                    .as_ref()
                    .unwrap()
                    .attempt_records()
                    .unwrap();
                let booked: Vec<_> = records
                    .iter()
                    .flat_map(|r| r.exits.iter())
                    .filter_map(|x| x.fe06.as_ref())
                    .map(|b| b.grant.clone())
                    .collect();
                assert_eq!(booked, [digest], "the exit is booked against its grant");
            }
            "other" => {
                // Review H1: refused by its sign permit — nothing signed, simulated or sent.
                assert!(!calls
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|m| m == "sendTransaction" || m == "simulateTransaction"));
                assert!(
                    f.owner
                        .fe04_admissions
                        .last_exit_refusal
                        .starts_with("FE06_SCOPE_USE: exit_position "),
                    "{:?}",
                    f.owner.fe04_admissions
                );
            }
            _ => {
                assert_eq!(sends, 0, "a stop blocks every new exit");
                assert!(!f.owner.exit_live_effective());
            }
        }
        cleanup(f);
    }
}

/// P8-CTRL.6/.10 (L16): a restart with an UNKNOWN attempt resets every switch and the grant,
/// while the attempt, its holds and its FE-06 booking survive in the FE-04 journal and are
/// replayed by the owner's start (`reservation_book`); a request of the earlier epoch reaches the
/// new owner refused (nothing re-armed).
#[test]
fn p8ctrl06_l16_a_restart_resets_the_switches_and_keeps_every_hold_and_booking() {
    use crate::orchestration::capability_control as cap;
    let (mut f, target, calls) = dec13b_fixture("p8ctrl06_l16", true);
    p8ctrl03_scoped(&mut f, &[], |_| {});
    let digest = f
        .owner
        .sources
        .capabilities
        .fe06
        .as_ref()
        .unwrap()
        .digest()
        .to_string();
    f.owner
        .detect(Some((
            target,
            f.owner.clock.now_ns(),
            100,
            "account_update",
        )))
        .unwrap();
    assert!(calls.lock().unwrap().iter().any(|m| m == "sendTransaction"));
    // The restart: the owner's state is gone; the journal is replayed as at every start.
    f.owner.fe04_held = None;
    let mut safety = Safety::new(e2e_limits(), Allow::default(), false);
    let dir_s = f.dir.join("fe04").display().to_string();
    let start = setup::reservation_book(
        &|k| (k == "P0_FE04_JOURNAL_DIR").then(|| dir_s.clone()),
        &f.owner.liquidator,
    )
    .unwrap()
    .start(&mut safety, wall_unix_s().unwrap())
    .unwrap();
    assert!(
        !start.book.is_empty(),
        "the UNKNOWN attempt keeps its holds"
    );
    // Held for the run, settled at once, its capital reserved until that booking.
    let unbooked = start
        .unbooked
        .expect("the UNKNOWN attempt waits for its booking");
    assert!(start.held.is_some() && start.settle_due.is_some());
    assert_eq!(
        Some(safety.capital_in_use()),
        unbooked.reserved_capital,
        "the replay reserves the attempt's capital"
    );
    let records = crate::execution::one_shot::archive::records(
        &f.dir.join("fe04"),
        &f.owner.liquidator.to_string(),
    )
    .unwrap();
    assert_eq!(
        records
            .iter()
            .filter_map(|r| r.fe06.as_ref())
            .map(|b| b.grant.as_str())
            .collect::<Vec<_>>(),
        [digest.as_str()],
        "the attempt stays booked against its grant"
    );
    // Every switch DISABLED, LIVE OFF, no grant: what a new owner starts with.
    let fresh = sources::OwnerCapabilities::default();
    assert_eq!(fresh.live_requested, cap::LIVE_OFF);
    assert!(cap::ECON.iter().all(|e| fresh.econ.get(e) != Some(true)));
    assert!(fresh.fe06.is_none());
    let (effective, _) = cap::live_decision(&fresh.live_gates());
    assert_ne!(effective, cap::LIVE);
    // A fresh epoch at every start: the old one's requests are stale.
    let control = cap::request_dir(&f.dir);
    let first = cap::open_epoch(&control, std::process::id(), None).unwrap();
    let second = cap::open_epoch(&control, std::process::id(), None).unwrap();
    assert!(second > first);
    // Review LOW: a request of the earlier epoch reaches the new owner refused — nothing re-armed.
    {
        let c = &mut f.owner.sources.capabilities;
        *c = sources::OwnerCapabilities::default();
        (c.dir, c.config_version, c.epoch) = (Some(control.clone()), Some(0), Some(second));
    }
    f.owner.apply_capability_control(
        1,
        Ok(crate::analytics::payload::CapabilityControlRequest {
            seq: 1,
            capability: cap::ECON_SIGN.into(),
            requested: cap::ENABLED.into(),
            requested_unix_s: 1,
            request_id: "l16-earlier-epoch".into(),
            owner_epoch: first,
        }),
    );
    assert_ne!(
        f.owner.sources.capabilities.econ.get(cap::ECON_SIGN),
        Some(true)
    );
    let d = f.owner.sources.capabilities.last_decision.clone().unwrap();
    assert!(
        !d.applied && d.detail.starts_with("CAPABILITY_EPOCH_STALE"),
        "{d:?}"
    );
    cleanup(f);
}

/// P8-CTRL.4/.10 (L15, review N1 and its follow-up): a SIM_ONLY grant in force — LIVE BLOCKED for
/// it, so the candidate is a Shadow one — with ECON_SIGN and the signer ENABLED binds a real
/// simulation signature only where the simulation can never send it; the owner's RPC adapter
/// can always broadcast what it simulates (`simulates_without_sending` is false), so the wallet's
/// key never reaches it: the simulation is signed synthetically and never sent, with the grant
/// as without it.
#[test]
fn p8ctrl06_l15_a_sim_only_grant_never_hands_the_wallets_signature_to_a_sending_simulator() {
    use crate::analytics::payload::{decode, Payload};
    use crate::analytics::record::{OpportunityRecord, Signing};
    for (tag, sim_only) in [("sim_only", true), ("no_grant", false)] {
        let (mut f, target, calls) = dec13b_fixture(&format!("p8ctrl06_l15_{tag}"), true);
        if sim_only {
            p8ctrl03_scoped(
                &mut f,
                &[
                    ("mode", "SIM_ONLY"),
                    ("max_attempts_per_utc_day", "0"),
                    ("max_attempts_campaign", "0"),
                    ("max_attempts_grant", "0"),
                    ("max_exits_grant", "0"),
                ],
                |_| {},
            );
            // The binding facts exist; only the sending simulator rules the real signature out.
            assert!(f.owner.sim_only_use_facts(&target).is_some(), "{tag}");
        } else {
            f.owner.sources.capabilities.fe06 = None;
        }
        assert!(!f.owner.live_effective(), "{tag}: a Shadow candidate");
        f.owner
            .detect(Some((
                target,
                f.owner.clock.now_ns(),
                100,
                "account_update",
            )))
            .unwrap();
        let m = calls.lock().unwrap().clone();
        assert!(m.iter().any(|m| m == "simulateTransaction"), "{tag}: {m:?}");
        assert!(!m.iter().any(|m| m == "sendTransaction"), "{tag}: {m:?}");
        let dir = f.dir.clone();
        f.owner.finish().unwrap();
        let mut reader =
            crate::analytics::journal::Reader::open(&dir.join("opportunities.p0j")).unwrap();
        let mut signings = vec![];
        while let Some(frame) = reader.next_frame().unwrap() {
            if frame.kind == OpportunityRecord::KIND {
                let r: OpportunityRecord =
                    decode(frame.kind, frame.schema, &frame.payload).unwrap();
                signings.extend(r.signing);
            }
        }
        assert_eq!(signings, [Signing::Synthetic], "{tag}");
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// S4-04 (PHASENPLAN S4 Performance-Baseline): the core stages of the real owner on the DEC-13b
/// offline fixture (loopback node and Jupiter, synthetic keys, nothing reaches a network) — the
/// liquidatable account's update through the ingress queue, its state, detection, plan,
/// signature and handoff, `P0_S4_BASELINE_RUNS` times, a fresh owner each — per stage as count,
/// p50/p95/p99/max, allocations (the counting build, `--features alloc-count`) and queue wait.
/// Steady state: each owner's FE-04 admission is open before `T_rx`, as after its first LIVE
/// evaluation; that one-time open (its journal) is measured on its own line. A local loopback
/// baseline, no Mainnet tail. Asserts only that every run passes every stage.
/// Run explicitly: `cargo test --release -p darkbot-runner --features crate_health,alloc-count
/// --lib s4_04 -- --ignored --nocapture --test-threads=1`.
#[test]
#[ignore]
fn s4_04_core_stage_baseline() {
    use darkbot_common::mt0::{HistSnapshot, MT0};
    use darkbot_common::timing::{CoreStage, CORE_STAGES};
    let runs: usize = std::env::var("P0_S4_BASELINE_RUNS")
        .ok()
        .map(|v| v.parse().expect("P0_S4_BASELINE_RUNS"))
        .unwrap_or(200);
    let before: Vec<u64> = CORE_STAGES
        .iter()
        .map(|s| MT0.core_stage(*s).0.count)
        .collect();
    let mut admission_open = Vec::with_capacity(runs);
    for i in 0..runs {
        let (mut f, target, calls) = dec13b_fixture(&format!("s4_04_{i}"), true);
        p8ctrl03_scoped(&mut f, &[], |_| {});
        let opened = std::time::Instant::now();
        let facts = f.owner.fe04_admission_facts(&target);
        let admission = fe04::admission_for(
            &mut f.owner.fe04_admission,
            &mut f.owner.fe04_held,
            &mut f.owner.fe04_admissions,
            facts,
        );
        admission_open.push(opened.elapsed().as_nanos() as u64);
        assert!(
            admission.is_some(),
            "run {i}: {:?}",
            f.owner.fe04_admissions
        );
        f.owner.fe04_admission = admission;
        let record = f
            .owner
            .st
            .get(&target)
            .expect("the fixture's liquidatable account");
        let (owner, lamports, data) = (record.owner, record.lamports, record.data.to_vec());
        let update = AccountUpdate {
            pubkey: target,
            owner,
            lamports,
            data,
            slot: 101,
            write_version: None,
            source: SourceKind::WsRpcControl,
            t_rx_ns: f.owner.clock.now_ns(),
        };
        f.producer.try_push(IngressEvent::Account(update)).unwrap();
        let mut fetch = || Err::<SnapshotData, String>("S4-04: no recovery in a run".into());
        let sent = |calls: &Arc<Mutex<Vec<String>>>| {
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|m| *m == "sendTransaction")
                .count()
        };
        // The owner loop's steps (`run_shadow`), until the update is handed off.
        for _ in 0..64 {
            f.owner.remote_apply().unwrap();
            f.owner.sweep();
            if let OwnerStep::Next(detect) = f.owner.events(&mut fetch, &f.ingress).unwrap() {
                f.owner.detect(detect).unwrap();
            }
            if sent(&calls) > 0 {
                break;
            }
        }
        assert_eq!(
            sent(&calls),
            1,
            "run {i}: the update is detected and handed off once"
        );
        cleanup(f);
    }
    let row = |h: &HistSnapshot| {
        let v = |x: Option<u64>| x.map_or("-".to_string(), |x| x.to_string());
        format!(
            "{} {} {} {} {}",
            h.count,
            v(h.p50),
            v(h.p95),
            v(h.p99),
            v(h.max)
        )
    };
    let counted = darkbot_common::alloc::thread_allocations().is_some();
    let snapshot = MT0.snapshot(0, darkbot_common::mt0::mono_ns(), &HistSnapshot::default());
    println!(
        "S4_04_BASELINE runs={runs} allocations={}",
        if counted {
            "counted"
        } else {
            "NOT_MEASURED (a build without --features alloc-count)"
        }
    );
    println!("stage | count p50 p95 p99 max (ns) | allocations count p50 p95 p99 max | queue");
    for (stage, before) in CORE_STAGES.iter().zip(before) {
        let (ns, allocations) = MT0.core_stage(*stage);
        assert_eq!(
            ns.count - before,
            runs as u64,
            "{}: every run",
            stage.name()
        );
        let queue = match stage {
            CoreStage::IngressToState => format!(
                "ingress_queue_wait {}",
                row(&snapshot.ingress_queue_wait_ns)
            ),
            _ => "owner thread; its RPC calls' admission waits below".to_string(),
        };
        println!(
            "{} | {} | {} | {queue}",
            stage.name(),
            row(&ns),
            row(&allocations)
        );
    }
    admission_open.sort_unstable();
    let rank = |q: usize| admission_open[(admission_open.len() * q).div_ceil(100).max(1) - 1];
    println!(
        "fe04_admission_open (once per owner, before T_rx) | {} {} {} {} {}",
        admission_open.len(),
        rank(50),
        rank(95),
        rank(99),
        admission_open[admission_open.len() - 1]
    );
    for (class, h) in &snapshot.queue_wait_latest_ns {
        if h.count > 0 {
            println!("trigger_queue_wait {class} {}", row(h));
        }
    }
    // The queue in front of every RPC call inside the stages (the RPC budget's admission).
    for class in darkbot_common::mt0::ADMISSION_CLASSES {
        let h = MT0.admission_wait(class);
        if h.count > 0 {
            println!("rpc_admission_wait {} {}", class.name(), row(&h));
        }
    }
    // The loopback's own answers inside the stages (remote boundaries, not core work).
    for r in snapshot.remote.iter().filter(|r| r.requests > 0) {
        println!(
            "remote {} requests={} {}",
            r.class,
            r.requests,
            row(&r.latency_ns)
        );
    }
}
