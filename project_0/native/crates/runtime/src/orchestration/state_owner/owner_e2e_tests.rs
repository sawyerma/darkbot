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
        fe04_reserved_capital: None,
        inventory_targets: Default::default(),
        prepared: std::cell::RefCell::new(crate::execution::prepared::PreparedCache::new(
            8,
            vec![],
        )),
        atas: accounts::LiquidatorAtas::new(liquidator),
        min_profit: 0,
        integration_probe_enabled: false,
        integration_probe_limit_per_kind: 8,
        remote_worker_capacity: 16,
        pending: vec![],
        projection_cache: HashMap::new(),
        projection_requeues: HashMap::new(),
        projection_parked: ParkedProjections::default(),
        projection_pending: HashSet::new(),
        projection_stats: ProjectionStats::default(),
        detect_queue: VecDeque::new(),
        bootstrap_probe_queue: VecDeque::new(),
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
        network: NetworkBudget::unbounded(),
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
    f.owner.oracle_detect_order.push_back(oracle);
    f.owner
        .oracle_detect_pending
        .insert(oracle, (oracle, 0, 10, "oracle_price"));
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
    };
    (f, Box::new(work))
}

/// A failed projection as the projection worker reports it.
fn projection_failure() -> crate::orchestration::remote_workers::ProjectionResultMessage {
    crate::orchestration::remote_workers::ProjectionResultMessage {
        account: Pubkey::new_unique(),
        base_slot: 12,
        base_account_hash: solana_sdk::hash::Hash::default(),
        t_rx_ns: 0,
        dependencies: vec![],
        dependency_fingerprint: solana_sdk::hash::Hash::default(),
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
    );
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
    f.owner.fe04_reserved_capital = start.reserved_capital;
    (f, intent, proof)
}

/// The settlement work the owner handed to the (detached) worker.
fn fe04_job(f: &Fixture) -> Option<crate::orchestration::remote_workers::Fe04SettlementWork> {
    f.results.as_ref().unwrap().fe04_jobs.try_pop()
}

/// The worker's answer through the production result queue (it rings the owner's doorbell).
fn fe04_answer(f: &Fixture, evidence: crate::execution::one_shot::settlement::SettlementEvidence) {
    let d = f.results.as_ref().unwrap();
    assert!(crate::orchestration::remote_workers::push_result(
        &d.fe04_results,
        crate::orchestration::remote_workers::Fe04SettlementResult { evidence },
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
    assert_eq!(job.intent.signatures, intent.signatures);
    assert!(job.observed.is_empty());
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
    assert_eq!((again.reserved_capital, safety.capital_in_use()), (None, 0));
    cleanup(f);
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
