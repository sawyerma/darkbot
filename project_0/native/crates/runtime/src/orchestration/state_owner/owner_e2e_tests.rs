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
    fixture_rpc(tag, "http://127.0.0.1:9")
}

/// The fixture with its remote workers against `rpc` (default: a closed port).
fn fixture_rpc(tag: &str, rpc: &str) -> Fixture {
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
    let unused_rpc = rpc.to_string();
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
        reservations: Default::default(),
        fe04_journal: None,
        fe04_refreshed: std::time::Instant::now(),
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
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                        body.len(),
                        body
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
