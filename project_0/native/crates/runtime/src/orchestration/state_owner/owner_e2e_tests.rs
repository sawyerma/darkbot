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
}

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
        fe04_settle_due: None,
        fe04_unbooked: None,
        inventory_targets: Default::default(),
        jit_funding_bank: None,
        prepared: std::cell::RefCell::new(crate::execution::prepared::PreparedCache::new(
            8,
            vec![],
        )),
        route_cap: std::cell::RefCell::new(crate::execution::route_cap::RouteCapCache::new(8, 64)),
        program_generation: 0,
        next_candidate_id: 1,
        candidate_revisions: HashMap::new(),
        detect_origin: None,
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
        threshold_bootstrap_batch: 64,
        use_threshold_trigger: false,
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
        results,
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
/// neither starves — at most `PROJECTION_REQUEUE_MAX` times in a row; then it is counted and the
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
    for n in 1..=PROJECTION_REQUEUE_MAX as u64 {
        f.owner.requeue_projection(candidate, 7, None);
        assert_eq!(f.owner.projection_stats.requeued, n);
    }
    let queued: Vec<_> = f.owner.detect_queue.iter().copied().collect();
    let mut expected = vec![waiting];
    expected.extend(std::iter::repeat_n(
        requeue,
        PROJECTION_REQUEUE_MAX as usize,
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
    .record_decisions_in(f.dir.join("source-control"));
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
/// attempt's intent and the non-landing proof that fits it.
fn fe04_fixture(
    tag: &str,
    phase: &str,
) -> (
    Fixture,
    crate::execution::one_shot::Intent,
    crate::execution::one_shot::settlement::NonLandingProof,
) {
    let mut f = fixture_with(tag, None);
    let (wallet, dir) = (f.owner.liquidator.to_string(), f.dir.join("fe04"));
    let intent = crate::execution::one_shot::test_unknown_attempt(&dir, &wallet);
    let proof = crate::execution::one_shot::settlement::NonLandingProof {
        finalized_block_height: intent.last_valid_block_height.unwrap() + 1,
        history_from_slot: intent.state_slot,
        absent_signatures: intent.signatures.iter().cloned().collect(),
    };
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
        proof: Some(Ok(proof)),
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
        proof: Some(Ok(proof)),
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
                proof: Some(Err(reason.into())),
            },
        );
        run_until(&mut f, |o| o.fe04_settle_due.is_some());
        assert!(f.owner.fe04_settle_due.unwrap() >= sent + super::fe04::FE04_SETTLE_RETRY);
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
        proof: Some(Ok(proof)),
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
                    proof: None,
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
        assert!(again.unbooked.is_none() && again.book.is_empty());
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
        proof: None,
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
    assert!(f.owner.fe04_settle_due.unwrap() >= sent + super::fe04::FE04_SETTLE_RETRY);
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
        update(liab_bank, bank(1.0, PREMIUM_ACTIVE)),
        update(
            account,
            [
                protocol::DISC_MARGINFI_ACCOUNT.as_slice(),
                bytemuck::bytes_of(&a),
            ]
            .concat(),
        ),
    ];
    f.owner.st.install_snapshot(100, state.clone());
    f.owner.st.reconcile(100, &state);
    f.owner.st.confirm_ready();
    assert!(f.owner.st.run_state().is_tradable());
    account
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
    f.owner.use_threshold_trigger = true;
    f.owner.threshold_ready = true;
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
    f.owner.use_threshold_trigger = threshold;
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
    f.owner.use_threshold_trigger = threshold;
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
            proof: Some(Ok(proof)),
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
