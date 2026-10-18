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
}

fn fixture(tag: &str) -> Fixture {
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
    let (producer, consumer) = bounded::<IngressEvent>("ingress", 1024);
    let unused_rpc = "http://127.0.0.1:9".to_string();
    let limits = Limits {
        max_total_capital: 0,
        max_outstanding_inventory: 0,
        max_concurrent_positions: 1,
        max_exposure_per_attempt: 0,
        max_daily_loss: 0,
        max_failed_attempts_per_day: 0,
        max_tip: 0,
        max_slippage_bps: 0,
        max_outstanding_attempts: 1,
    };
    let ingress = IngressSessions {
        ws_url: "ws://127.0.0.1:9".into(),
        slot_watch: SlotWatch::new(),
        clock,
        econ: None,
    };
    let owner = ShadowOwner {
        metrics: OwnerMetrics::default(),
        mode: "shadow".into(),
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
        min_profit: 0,
        integration_probe_enabled: false,
        integration_probe_limit_per_kind: 8,
        remote_worker_capacity: 16,
        pending: vec![],
        projection_cache: HashMap::new(),
        projection_pending: HashSet::new(),
        detect_queue: VecDeque::new(),
        oracle_detect_order: VecDeque::new(),
        oracle_detect_pending: HashMap::new(),
        oracle_bank_projection_pending: HashMap::new(),
        integration_probe_expected: HashSet::new(),
        integration_probe_queue: VecDeque::new(),
        integration_probe_terminal: HashSet::new(),
        threshold_fallback: HashSet::new(),
        threshold_bootstrap: VecDeque::new(),
        threshold_bootstrap_batch: 64,
        use_threshold_trigger: false,
        sweep_batch_max_events: 64,
        sweep_batch_max_age_ns: 5_000_000,
        sweep_dirty_banks: HashMap::new(),
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
        mt0_order_rx: VecDeque::new(),
        clock,
        econ: None,
        operational_start: 0,
        operational_window: None,
        operational_failure: None,
        program,
        group,
        payer: solana_sdk::signature::Keypair::new(),
        liquidator,
        shutdown: shutdown.clone(),
        halt: IngressHalt::new(),
        slot_watch: SlotWatch::new(),
        producer: producer.clone(),
        consumer,
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
        global_allowed: vec![],
        submitter: crate::execution::observe_only::ObserveOnly::new(RpcSubmitter::new(
            unused_rpc.clone(),
            true,
        )),
        failed_costs: FailedCostMatrix::default(),
        routes: JupiterV2RouteProvider::new(liquidator, None, 0, 64, true, false).unwrap(),
        resources: Resources {
            compute_unit_limit: 1_400_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        freshness: Freshness {
            max_opportunity_age_ns: 2_000_000_000,
            execution_deadline_ns: 2_000_000_000,
        },
        remote: RemoteWorkers::start(unused_rpc, program, &shutdown, 16).unwrap(),
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
        counts: [0; 6],
        last_reported_update_total: 0,
        bootstrap_probe_done: true,
        program_upgrade_latched: false,
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
