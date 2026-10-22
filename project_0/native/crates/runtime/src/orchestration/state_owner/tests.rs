//! Integration-probe and Shadow helper tests of the p0-longtail binary (split from main.rs, structure iteration 29).
use super::*;

/// C1/C7: the owner's terminal codes reach the start diagnostics verbatim (fixed codes only).
#[test]
fn owner_terminal_codes_are_reported_verbatim() {
    for code in [
        "STATE_RECOVERY_EXHAUSTED",
        "WSS_RECOVERY_EXHAUSTED",
        "INGRESS_FATAL",
        "INGRESS_SESSION_NOT_QUIESCENT",
        "INGRESS_NOT_QUIESCENT",
        "BOOT_IDENTITY_DRIFT",
        "BOOT_NETWORK_AUDIT_MISSING",
        "BOOT_NETWORK_EVIDENCE_INVALID",
        "BOOT_NETWORK_EXPIRED",
        "BOOT_NETWORK_TOKEN",
        "BOOT_NETWORK_NOT_READY",
        "BOOT_NETWORK_CHECK_MISSING",
        "BOOT_ADMISSION_REFUSED",
    ] {
        assert_eq!(shadow_failure_code(&super::OwnerTerminal(code)), code);
    }
}

/// N5: classified errors carry a secret-free detail; everything else stays a category.
#[test]
fn failure_report_shows_classified_details_only() {
    use super::super::startup::{failure_report, OperationalConfigError};
    use darkbot_common::config::ConfigError;
    let secret = "https://x.invalid/?api-key=SECRET";
    let client = solana_client::client_error::ClientError::from(
        solana_client::client_error::ClientErrorKind::Custom(secret.into()),
    );
    assert_eq!(failure_report(&client), "RPC_OTHER");
    assert_eq!(
        failure_report(&ConfigError::Missing("P0_WS_URL".into())),
        "CONFIG_MISSING P0_WS_URL"
    );
    let invalid = ConfigError::Invalid {
        key: "P0_RPC_URL".into(),
        value: secret.into(),
    };
    assert_eq!(failure_report(&invalid), "CONFIG_INVALID P0_RPC_URL");
    let unclassified: Box<dyn std::error::Error> = secret.into();
    assert_eq!(
        failure_report(unclassified.as_ref()),
        "SHADOW_RUNTIME_ERROR"
    );
    // Operational-config rejections are local texts without endpoint values.
    let rejected = darkbot_solana::runtime_config::Resolved::parse(
        &["--rpc-endpoints".into(), "file:///tmp/SECRET".into()],
        &|_| None,
    )
    .err()
    .unwrap();
    let report = failure_report(&OperationalConfigError(rejected));
    assert!(
        report.starts_with("OPERATIONAL_CONFIG: ") && !report.contains("SECRET"),
        "{report}"
    );
    assert_eq!(
        failure_report(&super::OwnerTerminal("STATE_RECOVERY_EXHAUSTED")),
        "STATE_RECOVERY_EXHAUSTED"
    );
}

#[test]
fn shadow_error_category_never_formats_secret_payload() {
    let e = solana_client::client_error::ClientError::from(
        solana_client::client_error::ClientErrorKind::Custom("https://host/?api-key=SECRET".into()),
    );
    assert_eq!(shadow_failure_code(&e), "RPC_OTHER");
    assert_eq!(
        shadow_failure_code(&std::io::Error::other("SECRET")),
        "LOCAL_IO"
    );
}

#[test]
fn target_waits_unsigned_until_worker_is_idle() {
    let account = Pubkey::new_unique();
    let mut queue = VecDeque::from([(account, 42)]);
    assert!(next_integration_probe(&mut queue, true, 100).is_none());
    assert_eq!(queue.len(), 1);
    // A long qualification backlog does not backdate a newly started probe.
    assert_eq!(
        next_integration_probe(&mut queue, false, 9_000),
        Some((account, 9_000, 42, "integration_target_probe"))
    );
    assert!(queue.is_empty());
}

#[test]
fn candidate_singleflight_coalesces_latest_work_without_duplicate_queue_token() {
    let account = Pubkey::new_unique();
    let mut order = VecDeque::new();
    let mut pending = HashMap::new();
    let mut enqueued = 0u64;
    let mut coalesced = 0u64;
    let mut max_pending = 0usize;

    assert_eq!(
        enqueue_or_coalesce_candidate(
            account,
            (100, 7, "oracle_health_ready"),
            &mut order,
            &mut pending,
            &mut enqueued,
            &mut coalesced,
            &mut max_pending,
        ),
        CandidateQueueAdmission::Enqueued
    );
    assert_eq!(
        enqueue_or_coalesce_candidate(
            account,
            (200, 7, "oracle_health_ready"),
            &mut order,
            &mut pending,
            &mut enqueued,
            &mut coalesced,
            &mut max_pending,
        ),
        CandidateQueueAdmission::Coalesced
    );
    assert_eq!(
        enqueue_or_coalesce_candidate(
            account,
            (300, 8, "oracle_health_error"),
            &mut order,
            &mut pending,
            &mut enqueued,
            &mut coalesced,
            &mut max_pending,
        ),
        CandidateQueueAdmission::Coalesced
    );

    assert_eq!(order.len(), 1);
    assert_eq!(pending.len(), 1);
    assert_eq!(enqueued, 1);
    assert_eq!(coalesced, 2);
    assert_eq!(max_pending, 1);
    assert_eq!(
        next_oracle_detect(&mut order, &mut pending),
        Some((account, 300, 8, "oracle_health_error"))
    );
    assert!(pending.is_empty());
    assert!(order.is_empty());
}

#[test]
fn oracle_detect_returns_latest_coalesced_work_once() {
    let a = Pubkey::new_unique();
    let mut order = VecDeque::from([a]);
    let mut pending = HashMap::from([(a, (a, 200, 9, "oracle_threshold"))]);
    assert_eq!(
        next_oracle_detect(&mut order, &mut pending),
        Some((a, 200, 9, "oracle_threshold"))
    );
    assert!(pending.is_empty());
    assert!(next_oracle_detect(&mut order, &mut pending).is_none());
}

#[test]
fn oracle_detect_skips_stale_order_entries() {
    let stale = Pubkey::new_unique();
    let live = Pubkey::new_unique();
    let mut order = VecDeque::from([stale, live]);
    let mut pending = HashMap::from([(live, (live, 300, 10, "oracle_threshold"))]);
    assert_eq!(
        next_oracle_detect(&mut order, &mut pending).unwrap().0,
        live
    );
    assert!(order.is_empty());
}

#[test]
fn candidate_cache_forwards_transition_force_revision_and_bounded_periodic_recheck() {
    let liquid = CandidateHealthState::Liquidatable;
    let healthy = CandidateHealthState::Healthy;
    let prev = CandidateCacheEntry {
        state: liquid,
        last_forwarded_slot: 100,
        last_forwarded_revision: 7,
    };

    assert!(candidate_should_forward(
        None, liquid, 100, 7, 64, false, true
    ));
    assert!(!candidate_should_forward(
        Some(prev),
        liquid,
        120,
        7,
        64,
        false,
        true
    ));
    assert!(candidate_should_forward(
        Some(prev),
        liquid,
        164,
        7,
        64,
        false,
        true
    ));
    assert!(!candidate_should_forward(
        Some(prev),
        liquid,
        164,
        7,
        64,
        false,
        false
    ));

    // [FIX-134] Same slot, same class, changed canonical dependency revision:
    // must forward even when periodic repeat capacity is exhausted.
    assert!(candidate_should_forward(
        Some(prev),
        liquid,
        100,
        8,
        64,
        false,
        false
    ));
    // Identical revision remains suppressible inside the same slot.
    assert!(!candidate_should_forward(
        Some(prev),
        liquid,
        100,
        7,
        1,
        false,
        true
    ));

    // Production default is one slot: unchanged candidates may be reconsidered on the
    // next slot, but never every 5 ms batch within the same slot.
    assert!(candidate_should_forward(
        Some(prev),
        liquid,
        101,
        7,
        1,
        false,
        true
    ));
    assert!(candidate_should_forward(
        Some(prev),
        liquid,
        101,
        7,
        64,
        true,
        false
    ));
    assert!(!candidate_should_forward(
        Some(prev),
        healthy,
        1000,
        99,
        64,
        true,
        true
    ));

    let was_healthy = CandidateCacheEntry {
        state: healthy,
        last_forwarded_slot: 100,
        last_forwarded_revision: 7,
    };
    assert!(candidate_should_forward(
        Some(was_healthy),
        liquid,
        101,
        7,
        64,
        false,
        false
    ));

    let was_error = CandidateCacheEntry {
        state: CandidateHealthState::Error,
        last_forwarded_slot: 100,
        last_forwarded_revision: 12,
    };
    // Changed integration/dependency input in the same slot must retry an Error.
    assert!(candidate_should_forward(
        Some(was_error),
        CandidateHealthState::Error,
        100,
        13,
        64,
        false,
        false,
    ));
    // Exact duplicate input is still suppressed.
    assert!(!candidate_should_forward(
        Some(was_error),
        CandidateHealthState::Error,
        100,
        12,
        64,
        false,
        true,
    ));
}

#[test]
fn candidate_fairness_services_all_above_soft_cap_without_starvation() {
    const N: usize = 1025;
    const CAP: usize = 1024;

    let accounts: Vec<Pubkey> = (0..N)
        .map(|i| {
            let mut bytes = [0u8; 32];
            bytes[..8].copy_from_slice(&(i as u64).to_le_bytes());
            Pubkey::new_from_array(bytes)
        })
        .collect();
    let mut cache: HashMap<Pubkey, CandidateCacheEntry> = accounts
        .iter()
        .copied()
        .map(|account| {
            (
                account,
                CandidateCacheEntry {
                    state: CandidateHealthState::Liquidatable,
                    last_forwarded_slot: 0,
                    last_forwarded_revision: 1,
                },
            )
        })
        .collect();
    let mut serviced = HashSet::new();

    for slot in 1..=2u64 {
        let mut ordered = accounts.clone();
        ordered.sort_unstable_by_key(|account| {
            candidate_fairness_key(
                *account,
                cache.get(account).copied(),
                CandidateHealthState::Liquidatable,
                slot,
                1,
                false,
                1,
            )
        });

        let mut admitted = 0usize;
        for account in ordered {
            let previous = cache.get(&account).copied();
            let repeat_capacity = admitted < CAP;
            if candidate_should_forward(
                previous,
                CandidateHealthState::Liquidatable,
                slot,
                1,
                1,
                false,
                repeat_capacity,
            ) {
                admitted += 1;
                serviced.insert(account);
                cache.insert(
                    account,
                    CandidateCacheEntry {
                        state: CandidateHealthState::Liquidatable,
                        last_forwarded_slot: slot,
                        last_forwarded_revision: 1,
                    },
                );
            }
        }
        assert!(admitted <= CAP);
    }

    assert_eq!(serviced.len(), N);
}

#[test]
fn candidate_fairness_repeat_admission_remains_bounded() {
    const N: usize = 4097;
    const CAP: usize = 1024;
    let mut cache = HashMap::new();
    let mut accounts = Vec::with_capacity(N);
    for i in 0..N {
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&(i as u64).to_le_bytes());
        let account = Pubkey::new_from_array(bytes);
        accounts.push(account);
        cache.insert(
            account,
            CandidateCacheEntry {
                state: CandidateHealthState::Liquidatable,
                last_forwarded_slot: 0,
                last_forwarded_revision: 1,
            },
        );
    }

    accounts.sort_unstable_by_key(|account| {
        candidate_fairness_key(
            *account,
            cache.get(account).copied(),
            CandidateHealthState::Liquidatable,
            1,
            1,
            false,
            1,
        )
    });

    let mut admitted = 0usize;
    let mut deferred = 0usize;
    for account in accounts {
        let previous = cache.get(&account).copied();
        let repeat_capacity = admitted < CAP;
        if candidate_should_forward(
            previous,
            CandidateHealthState::Liquidatable,
            1,
            1,
            1,
            false,
            repeat_capacity,
        ) {
            admitted += 1;
        } else {
            deferred += 1;
        }
    }

    assert_eq!(admitted, CAP);
    assert_eq!(deferred, N - CAP);
    assert!(admitted <= CAP);
}

#[test]
fn only_one_target_is_dispatched_per_idle_turn() {
    let a = Pubkey::new_unique();
    let b = Pubkey::new_unique();
    let mut queue = VecDeque::from([(a, 1), (b, 2)]);
    assert_eq!(next_integration_probe(&mut queue, false, 100).unwrap().0, a);
    assert!(next_integration_probe(&mut queue, true, 101).is_none());
    assert_eq!(queue.front(), Some(&(b, 2)));
    assert_eq!(next_integration_probe(&mut queue, false, 200).unwrap().0, b);
    assert!(next_integration_probe(&mut queue, false, 201).is_none());
}

#[test]
fn evidence_failures_remain_explicit_static_categories() {
    use crate::analytics::evidence::EvidenceFailure;
    for code in [
        "OWNER_EVIDENCE_OVERFLOW",
        "OWNER_EVIDENCE_WRITE_FAILED_RECOVERY_SAVED",
        "OWNER_EVIDENCE_RECOVERY_FAILED",
        "OWNER_EVIDENCE_SYNC_FAILED",
    ] {
        assert_eq!(shadow_failure_code(&EvidenceFailure(code)), code);
    }
}

/// C6/D10: a session is stopped and joined within its budget; one that does not end in time is
/// reported as still running instead of being detached.
#[test]
fn ingress_session_stop_and_join_is_bounded() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let stop = Shutdown::new();
    let sd = stop.clone();
    let thread = std::thread::spawn(move || {
        while !sd.is_requested() {
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let mut session = IngressSession::new(stop, thread, 1);
    assert!(session.stop_and_join(Duration::from_secs(5)));
    assert!(session.stop_and_join(Duration::ZERO), "already joined");

    let release = Arc::new(AtomicBool::new(false));
    let r = release.clone();
    let stuck = std::thread::spawn(move || {
        while !r.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let mut session = IngressSession::new(Shutdown::new(), stuck, 2);
    let started = std::time::Instant::now();
    assert!(!session.stop_and_join(Duration::from_millis(50)));
    assert!(started.elapsed() < Duration::from_secs(5));
    release.store(true, Ordering::Release);
    assert!(session.stop_and_join(Duration::from_secs(5)));
}

/// C1 test 6 / C6 / D10: events a replaced session queued before it was joined can never lift
/// the state to Ready; only the StreamReady of the new generation starts the recovery.
#[test]
fn no_duplicate_generation_or_old_session_readiness() {
    use darkbot_solana::ingress::AccountUpdate;
    use std::cell::{Cell, RefCell};
    let pk = Pubkey::new_unique();
    let upd = |slot: u64| AccountUpdate {
        pubkey: pk,
        owner: Pubkey::new_from_array([0xA5; 32]),
        lamports: 1,
        data: vec![1],
        slot,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    };
    let reconnected = || IngressEvent::Reconnected {
        source: SourceKind::WsRpcControl,
        attempt: 1,
        t_rx_ns: 0,
    };
    let fetches = Cell::new(0u32);
    let mut fetch = || {
        fetches.set(fetches.get() + 1);
        Ok(SnapshotData {
            slot: 10,
            accounts: vec![upd(10)],
            missing_required: vec![],
        })
    };
    // Generation 1 is ready; then the dependency set changes and the owner retires it (as in
    // `events`): HALT, join, retire the queued events, open the gap.
    let ready_then_retired = |fetch: &mut dyn FnMut() -> Result<SnapshotData, String>| {
        let mut st = StateCore::new(10);
        let mut drv = RecoveryDriver::new(3);
        let out = drv.on_event(&mut st, reconnected(), fetch, &mut |_| {});
        assert_eq!(
            out,
            Some(darkbot_solana::state::recovery::RecoveryOutcome::Ready)
        );
        st.halt(darkbot_common::error::HaltReason::StateGap);
        (st, drv)
    };
    // Still queued from generation 1 after its join: a late StreamReady and an account write.
    let old_generation = || vec![IngressEvent::Account(upd(11)), reconnected()];

    // Before C6 the old StreamReady reached the recovery driver and made the state Ready although
    // the new generation had not confirmed a single subscription.
    let (mut st, mut drv) = ready_then_retired(&mut fetch);
    let before = fetches.get();
    for ev in old_generation() {
        drv.on_event(&mut st, ev, &mut fetch, &mut |_| {});
    }
    assert!(
        st.run_state().is_tradable(),
        "control: the hazard the fix removes"
    );
    assert!(fetches.get() > before);

    // With C6: the retired StreamReady is dropped, the data is applied, no recovery runs.
    let (mut st, mut drv) = ready_then_retired(&mut fetch);
    let queue = RefCell::new(std::collections::VecDeque::from(old_generation()));
    let mut retired = RetiredEvents::default();
    retired.retire(queue.borrow().len() as u64);
    st.on_event(IngressEvent::Gap {
        source: SourceKind::WsRpcControl,
        from_slot: None,
        to_slot: None,
        t_rx_ns: 0,
    });
    let before = fetches.get();
    let mut owner_turn =
        |st: &mut StateCore, drv: &mut RecoveryDriver, retired: &mut RetiredEvents| {
            let ev = queue.borrow_mut().pop_front()?;
            let ev = retired.admit(ev)?;
            drv.on_event(st, ev, &mut fetch, &mut |s: &mut StateCore| {
                while let Some(ev) = queue.borrow_mut().pop_front() {
                    s.on_event(ev);
                }
            })
        };
    while !queue.borrow().is_empty() {
        assert_eq!(owner_turn(&mut st, &mut drv, &mut retired), None);
    }
    assert_eq!(retired.stream_ready_dropped, 1);
    assert_eq!(
        fetches.get(),
        before,
        "no recovery read for a retired generation"
    );
    assert!(!st.run_state().is_tradable());
    assert_eq!(
        st.get(&pk).map(|r| r.slot),
        Some(11),
        "retired data is still applied"
    );
    assert!(st.gap_open(), "no StreamReady of the new generation yet");

    // The new generation's StreamReady (after all its ACKs) starts the recovery and recovers.
    queue.borrow_mut().push_back(reconnected());
    assert_eq!(
        owner_turn(&mut st, &mut drv, &mut retired),
        Some(darkbot_solana::state::recovery::RecoveryOutcome::Ready)
    );
    assert!(st.run_state().is_tradable());
    assert_eq!(retired.stream_ready_dropped, 1);
}

/// D3: owner block time of one sweep batch, synthetic and deterministic: one dirty bank with N
/// dependent accounts (each with a deposit on a second bank), fixed-price banks, the same
/// dedupe → `par_iter` exact health → fairness sort as `sweep`, on 1 and 2 rayon workers. Real
/// oracles (Pyth/Switchboard decode) cost more; the Mainnet detect path measured ~3.4 µs/call.
/// Run: `cargo test --release -p darkbot-runner --features crate_health --lib -- --ignored
/// sweep_fanout_owner_block_is_measured --nocapture`.
#[cfg(feature = "crate_health")]
#[test]
#[ignore]
fn sweep_fanout_owner_block_is_measured() {
    use bytemuck::Zeroable;
    use darkbot_project0::health::{AccountSource, HealthEngine};
    use marginfi_type_crate::types::{
        Bank, BankOperationalState, MarginfiAccount, MarginfiGroup, OracleSetup, WrappedI80F48,
    };
    struct Source(HashMap<Pubkey, Account>);
    impl AccountSource for Source {
        fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            self.0
                .get(key)
                .map(|a| (a.owner, a.lamports, a.data.as_slice()))
        }
    }
    let one = |x: i128| WrappedI80F48 {
        value: (x << 48).to_le_bytes(),
    };
    let wrapped = |data: &[u8], disc: [u8; 8], owner: Pubkey| Account {
        owner,
        lamports: 1,
        data: [disc.as_slice(), data].concat(),
        ..Account::default()
    };
    let program = Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).unwrap();
    let group = Pubkey::new_unique();
    let (asset_bank, liability_bank) = (Pubkey::new_unique(), Pubkey::new_unique());
    let mut state = HashMap::new();
    state.insert(
        group,
        wrapped(
            bytemuck::bytes_of(&MarginfiGroup::zeroed()),
            protocol::DISC_MARGINFI_GROUP,
            program,
        ),
    );
    for bank_key in [asset_bank, liability_bank] {
        let mut bank = Bank::zeroed();
        bank.group = group;
        bank.mint_decimals = 6;
        bank.asset_share_value = one(1);
        bank.liability_share_value = one(1);
        bank.config.asset_weight_init = one(1);
        bank.config.asset_weight_maint = one(1);
        bank.config.liability_weight_init = one(1);
        bank.config.liability_weight_maint = one(1);
        bank.config.operational_state = BankOperationalState::Operational;
        bank.config.oracle_setup = OracleSetup::Fixed;
        bank.config.fixed_price = one(1);
        state.insert(
            bank_key,
            wrapped(bytemuck::bytes_of(&bank), protocol::DISC_BANK, program),
        );
    }
    let mut accounts = Vec::new();
    for i in 0..100_000usize {
        let key = Pubkey::new_unique();
        let mut a = MarginfiAccount::zeroed();
        a.group = group;
        // Active balances sorted by bank key, descending (the on-chain order).
        let (asset, liability) = if asset_bank > liability_bank {
            (0, 1)
        } else {
            (1, 0)
        };
        let b = &mut a.lending_account.balances;
        b[asset].active = 1;
        b[asset].bank_pk = asset_bank;
        b[asset].asset_shares = one(1_000);
        b[liability].active = 1;
        b[liability].bank_pk = liability_bank;
        // Every second account is under water.
        b[liability].liability_shares = one(if i % 2 == 0 { 500 } else { 1_500 });
        state.insert(
            key,
            wrapped(
                bytemuck::bytes_of(&a),
                protocol::DISC_MARGINFI_ACCOUNT,
                program,
            ),
        );
        accounts.push(key);
    }
    let source = Source(state);
    let engine = darkbot_project0::health::crate_impl::CrateHealth {
        state: &source,
        group,
    };
    let probe = engine.health(&accounts[1], 1_000, 100).unwrap();
    assert!(probe.liquidatable, "the fixture evaluates the real path");
    for n in [100usize, 10_000, 100_000] {
        // The bank's fan-out is exactly n accounts (index construction is not owner time here).
        let mut index = ReverseIndex::new();
        for key in &accounts[..n] {
            index.set_account_banks(*key, &[asset_bank, liability_bank]);
        }
        for threads in [1usize, 2] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            let started = std::time::Instant::now();
            let mut work: HashMap<Pubkey, (u64, u64, u64)> = HashMap::new();
            for account in index.accounts_for_bank(&liability_bank) {
                work.entry(*account).or_insert((1, 100, 0));
            }
            let mut evaluated: Vec<(Pubkey, bool)> = pool.install(|| {
                work.into_par_iter()
                    .map(|(account, _)| {
                        let h = engine.health(&account, 1_000, 100).unwrap();
                        (account, h.liquidatable)
                    })
                    .collect()
            });
            evaluated.sort_unstable_by_key(|(account, liquidatable)| (!*liquidatable, *account));
            let ms = started.elapsed().as_secs_f64() * 1e3;
            assert_eq!(evaluated.len(), n);
            assert_eq!(evaluated.iter().filter(|e| e.1).count(), n / 2);
            println!(
                "D3_SWEEP_FANOUT accounts={n} workers={threads} owner_block_ms={ms:.1} us_per_account={:.2}",
                ms * 1e3 / n as f64
            );
        }
    }
}

/// C9: one blockhash freshness bound at every owner gate; inclusive at the cutoff, stale 1 ns
/// after it, and no gate keeps its own literal.
#[test]
fn blockhash_freshness_is_one_policy() {
    let cache = BlockhashCache::new();
    cache.set(darkbot_solana::state::blockhash::CachedBlockhash {
        hash: solana_sdk::hash::Hash::default(),
        last_valid_block_height: 1,
        refreshed_ns: 1_000,
    });
    assert!(cache
        .fresh(1_000 + BLOCKHASH_MAX_AGE_NS, BLOCKHASH_MAX_AGE_NS)
        .is_some());
    assert!(cache
        .fresh(1_000 + BLOCKHASH_MAX_AGE_NS + 1, BLOCKHASH_MAX_AGE_NS)
        .is_none());
    for (name, source) in [
        ("detect.rs", include_str!("detect.rs")),
        ("events.rs", include_str!("events.rs")),
    ] {
        assert!(
            !source.contains("30_000_000_000"),
            "{name} keeps its own bound"
        );
        assert!(source.contains("BLOCKHASH_MAX_AGE_NS"), "{name}");
    }
}

/// R10: a requested stop of the real WSS source never marks the measurement failed or aborted;
/// a real recovery exhaustion (no stop) sets both. Runs the production thread body
/// `run_ingress` with the econ status flags; loopback only, never Mainnet.
#[test]
fn requested_ingress_stop_never_aborts_the_measurement() {
    use darkbot_solana::ingress::ws_rpc::{WsRpcConfig, WsRpcControlSource};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    let closed_port = || {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", l.local_addr().unwrap());
        drop(l);
        url
    };
    let source = |backoff: Duration, attempts: u32, budget: Duration| {
        let mut cfg = WsRpcConfig::new(
            closed_port(),
            Subscription {
                slots: true,
                ..Default::default()
            },
        );
        cfg.reconnect_initial = backoff;
        cfg.reconnect_max = backoff;
        cfg.recovery_budget = Some(budget);
        cfg.recovery_attempts = attempts;
        WsRpcControlSource::new(cfg, Clock::new())
    };
    let run = |src: WsRpcControlSource, stop_after: Option<Duration>| {
        let shared = Arc::new(crate::analytics::econ::Shared::default());
        let (tx, rx) = bounded::<IngressEvent>("r10", 1024);
        let stop = Shutdown::new();
        let (sd, flags) = (stop.clone(), Some(shared.clone()));
        let t = std::thread::spawn(move || {
            ingress::run_ingress(src, tx, sd, IngressHalt::new(), SlotWatch::new(), flags)
        });
        if let Some(d) = stop_after {
            std::thread::sleep(d);
            stop.request();
        }
        let started = std::time::Instant::now();
        while !t.is_finished() {
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "ingress thread not ending"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        t.join().unwrap();
        drop(rx);
        (
            shared.shadow_failed.load(Ordering::Acquire),
            shared.abort.load(Ordering::Acquire),
        )
    };
    // Requested stop during the reconnect backoff: no failure, no abort.
    let stopped = run(
        source(Duration::from_secs(20), 8, Duration::from_secs(120)),
        Some(Duration::from_millis(300)),
    );
    assert_eq!(stopped, (false, false));
    // Real recovery exhaustion without a stop: failed and aborted.
    let exhausted = run(
        source(Duration::from_millis(1), 2, Duration::from_secs(5)),
        None,
    );
    assert_eq!(exhausted, (true, true));
}

/// R6: the ingress thread and the evidence writer's Drop report through the bounded console,
/// never a blocking `eprintln!` (console non-blocking: `blocked_console_never_blocks_the_caller`).
#[test]
fn ingress_end_and_evidence_drop_never_write_stderr_directly() {
    for (name, source) in [
        ("state_owner/ingress.rs", include_str!("ingress.rs")),
        (
            "analytics/evidence.rs Drop",
            include_str!("../../analytics/evidence.rs")
                .split("impl Drop for OwnerEvidenceWriter")
                .nth(1)
                .unwrap()
                .split("\nstruct Sink")
                .next()
                .unwrap(),
        ),
    ] {
        assert!(!source.contains("eprintln!"), "{name}");
    }
}

/// R11: events of a retired generation at the resubscribe boundary. Its StreamReady is a
/// dropped control signal (counted); its Gap keeps the state gap open; its Fatal is a real
/// terminal cause and stays terminal — a requested stop never emits Fatal
/// (`requested_stop_is_ok_without_fatal_in_every_session_phase`), so a retired Fatal is always a
/// recovery exhaustion that happened before the stop, and its thread has aborted the run
/// (`requested_ingress_stop_never_aborts_the_measurement`). No path promotes to Ready early.
#[test]
fn retired_generation_controls_are_classified_by_provenance() {
    use darkbot_solana::ingress::AccountUpdate;
    use darkbot_solana::state::recovery::RecoveryOutcome;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    let pk = Pubkey::new_unique();
    let account = |slot: u64| {
        IngressEvent::Account(AccountUpdate {
            pubkey: pk,
            owner: Pubkey::new_from_array([0xA5; 32]),
            lamports: 1,
            data: vec![1],
            slot,
            write_version: None,
            source: SourceKind::Snapshot,
            t_rx_ns: 0,
        })
    };
    let reconnected = || IngressEvent::Reconnected {
        source: SourceKind::WsRpcControl,
        attempt: 1,
        t_rx_ns: 0,
    };
    let gap = || IngressEvent::Gap {
        source: SourceKind::WsRpcControl,
        from_slot: Some(11),
        to_slot: None,
        t_rx_ns: 0,
    };
    let fatal = || IngressEvent::Fatal {
        source: SourceKind::WsRpcControl,
        detail: "WSS_RECOVERY_EXHAUSTED".into(),
        t_rx_ns: 0,
    };
    let mut fetch = || {
        Ok(SnapshotData {
            slot: 10,
            accounts: vec![match account(10) {
                IngressEvent::Account(a) => a,
                _ => unreachable!(),
            }],
            missing_required: vec![],
        })
    };
    // Ready generation 1, then the owner's resubscribe retires it (HALT, join, retire, gap).
    let scenario = |retired: Vec<IngressEvent>,
                    fresh: Vec<IngressEvent>,
                    fetch: &mut dyn FnMut() -> Result<SnapshotData, String>| {
        let mut st = StateCore::new(10);
        let mut drv = RecoveryDriver::new(3);
        assert_eq!(
            drv.on_event(&mut st, reconnected(), fetch, &mut |_| {}),
            Some(RecoveryOutcome::Ready)
        );
        st.halt(darkbot_common::error::HaltReason::StateGap);
        let queue = RefCell::new(VecDeque::from(retired));
        let mut retire = RetiredEvents::default();
        retire.retire(queue.borrow().len() as u64);
        st.on_event(gap());
        queue.borrow_mut().extend(fresh);
        let mut outcomes = Vec::new();
        loop {
            let Some(ev) = queue.borrow_mut().pop_front() else {
                break;
            };
            let Some(ev) = retire.admit(ev) else {
                continue;
            };
            if let Some(o) = drv.on_event(&mut st, ev, fetch, &mut |s: &mut StateCore| {
                while let Some(ev) = queue.borrow_mut().pop_front() {
                    s.on_event(ev);
                }
            }) {
                outcomes.push(o);
            }
            if st.fatal_detail().is_some() {
                break; // the owner ends the run here (OwnerTerminal)
            }
        }
        (
            outcomes,
            retire.stream_ready_dropped,
            st.run_state().is_tradable(),
        )
    };

    // Ordered resubscribe: retired Account, StreamReady and Gap; only the new StreamReady
    // recovers, after a full A/B recovery.
    let (outcomes, dropped, ready) = scenario(
        vec![account(11), reconnected(), gap()],
        vec![reconnected()],
        &mut fetch,
    );
    assert_eq!(outcomes, vec![RecoveryOutcome::Ready]);
    assert_eq!((dropped, ready), (1, true));

    // Before the new StreamReady: a retired Gap alone never yields Ready.
    let (outcomes, dropped, ready) =
        scenario(vec![account(11), reconnected(), gap()], vec![], &mut fetch);
    assert!(outcomes.is_empty());
    assert_eq!((dropped, ready), (1, false));

    // A retired Fatal (old session exhausted before the stop) is terminal: no recovery, no
    // Ready, even though the new generation's StreamReady follows.
    let (outcomes, dropped, ready) = scenario(
        vec![account(11), reconnected(), fatal()],
        vec![reconnected()],
        &mut fetch,
    );
    assert_eq!(
        outcomes,
        vec![RecoveryOutcome::Fatal {
            code: "WSS_RECOVERY_EXHAUSTED"
        }]
    );
    assert_eq!((dropped, ready), (1, false));

    // A Fatal of the new generation before its StreamReady is terminal as well (C7).
    let (outcomes, _, ready) =
        scenario(vec![account(11)], vec![fatal(), reconnected()], &mut fetch);
    assert_eq!(
        outcomes,
        vec![RecoveryOutcome::Fatal {
            code: "WSS_RECOVERY_EXHAUSTED"
        }]
    );
    assert!(!ready);
}

/// R1 (Shadow side, C6): `IngressSession::stop_and_join` ends a real WSS session far below the
/// shutdown budget (default 180 s) in the reachable blocking phases — reconnect backoff, a
/// hanging WebSocket handshake and the wait for WSS admission in the shared provider budget —
/// twice in a row like two resubscribes, without failure flags.
/// A session that does not end is reported at its budget (`ingress_session_stop_and_join_is_bounded`).
#[test]
fn shadow_session_stop_and_join_is_far_below_the_shutdown_budget() {
    use darkbot_solana::ingress::ws_rpc::{WsRpcConfig, WsRpcControlSource};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    let budget = Duration::from_secs(
        darkbot_solana::runtime_config::RuntimeConfig::default()
            .limits
            .shutdown_seconds,
    );
    assert_eq!(budget, Duration::from_secs(180));
    let hang = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let hang_url = format!("ws://{}", hang.local_addr().unwrap());
    // Holds every accepted connection without answering the handshake; bounded lifetime.
    hang.set_nonblocking(true).unwrap();
    let holder = std::thread::spawn(move || {
        let mut kept = Vec::new();
        let until = std::time::Instant::now() + Duration::from_secs(6);
        while std::time::Instant::now() < until {
            match hang.accept() {
                Ok((s, _)) => kept.push(s),
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        kept.len()
    });
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", l.local_addr().unwrap());
        drop(l);
        url
    };
    let mut generation = 0;
    for round in 0..2 {
        for (phase, url, admission) in [
            ("reconnect_backoff", &closed, false),
            ("ws_handshake", &hang_url, false),
            ("wss_admission_or_later", &hang_url, true),
        ] {
            let mut cfg = WsRpcConfig::new(
                url.clone(),
                Subscription {
                    slots: true,
                    ..Default::default()
                },
            );
            cfg.idle_timeout = Duration::from_secs(30);
            cfg.reconnect_initial = Duration::from_secs(20);
            cfg.reconnect_max = Duration::from_secs(20);
            // Without a recovery budget the session skips the provider admission.
            cfg.recovery_budget = admission.then(|| Duration::from_secs(120));
            let src = WsRpcControlSource::new(cfg, Clock::new());
            let shared = Arc::new(crate::analytics::econ::Shared::default());
            let (tx, rx) = bounded::<IngressEvent>("r1", 1024);
            let stop = Shutdown::new();
            let (sd, flags) = (stop.clone(), Some(shared.clone()));
            let thread = std::thread::spawn(move || {
                ingress::run_ingress(src, tx, sd, IngressHalt::new(), SlotWatch::new(), flags)
            });
            generation += 1;
            let mut session = IngressSession::new(stop, thread, generation);
            std::thread::sleep(Duration::from_millis(200));
            let started = std::time::Instant::now();
            assert!(session.stop_and_join(budget), "{phase} round {round}");
            let joined = started.elapsed();
            println!(
                "R1_SHADOW_STOP_JOIN round={round} phase={phase} ms={}",
                joined.as_millis()
            );
            assert!(joined < Duration::from_secs(2), "{phase}: {joined:?}");
            assert!(
                !shared.shadow_failed.load(Ordering::Acquire)
                    && !shared.abort.load(Ordering::Acquire),
                "{phase}: a requested stop is no failure"
            );
            drop(rx);
        }
    }
    assert!(
        holder.join().unwrap() >= 2,
        "both handshake rounds reached the listener"
    );
}

/// R3: the composite owner block of one recovery Ready in sweep mode — reverse-index rebuild
/// over the whole state, every group bank dirty, then the full debtor sweep (dedupe → exact
/// health → sort) — repeated like back-to-back reconnects, on 1 and 2 rayon workers, while a
/// producer fills the production-size ingress queue (65,536) at a fixed rate. Synthetic,
/// fixed-price banks; release build: `cargo test --release -p darkbot-runner --features
/// crate_health --lib -- --ignored recovery_ready_rebuild_and_full_sweep_is_measured --nocapture`.
#[cfg(feature = "crate_health")]
#[test]
#[ignore]
fn recovery_ready_rebuild_and_full_sweep_is_measured() {
    use bytemuck::Zeroable;
    use darkbot_project0::health::HealthEngine;
    use darkbot_solana::ingress::AccountUpdate;
    use marginfi_type_crate::types::{
        Bank, BankOperationalState, MarginfiAccount, MarginfiGroup, OracleSetup, WrappedI80F48,
    };
    const DEBTORS: usize = 100_000;
    const CYCLES: usize = 6;
    const QUEUE: usize = 65_536;
    const RATE_PER_S: u64 = 50_000;
    let one = |x: i128| WrappedI80F48 {
        value: (x << 48).to_le_bytes(),
    };
    let program = Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).unwrap();
    let group = Pubkey::new_unique();
    let (asset_bank, liability_bank) = (Pubkey::new_unique(), Pubkey::new_unique());
    let update = |pubkey: Pubkey, disc: [u8; 8], payload: &[u8]| AccountUpdate {
        pubkey,
        owner: program,
        lamports: 1,
        data: [disc.as_slice(), payload].concat(),
        slot: 10,
        write_version: None,
        source: SourceKind::Snapshot,
        t_rx_ns: 0,
    };
    let mut accounts = vec![update(
        group,
        protocol::DISC_MARGINFI_GROUP,
        bytemuck::bytes_of(&MarginfiGroup::zeroed()),
    )];
    for bank_key in [asset_bank, liability_bank] {
        let mut bank = Bank::zeroed();
        bank.group = group;
        bank.mint_decimals = 6;
        bank.asset_share_value = one(1);
        bank.liability_share_value = one(1);
        bank.config.asset_weight_init = one(1);
        bank.config.asset_weight_maint = one(1);
        bank.config.liability_weight_init = one(1);
        bank.config.liability_weight_maint = one(1);
        bank.config.operational_state = BankOperationalState::Operational;
        bank.config.oracle_setup = OracleSetup::Fixed;
        bank.config.fixed_price = one(1);
        accounts.push(update(
            bank_key,
            protocol::DISC_BANK,
            bytemuck::bytes_of(&bank),
        ));
    }
    let (asset, liability) = if asset_bank > liability_bank {
        (0, 1)
    } else {
        (1, 0)
    };
    for i in 0..DEBTORS {
        let mut a = MarginfiAccount::zeroed();
        a.group = group;
        let b = &mut a.lending_account.balances;
        b[asset].active = 1;
        b[asset].bank_pk = asset_bank;
        b[asset].asset_shares = one(1_000);
        b[liability].active = 1;
        b[liability].bank_pk = liability_bank;
        b[liability].liability_shares = one(if i % 2 == 0 { 500 } else { 1_500 });
        accounts.push(update(
            Pubkey::new_unique(),
            protocol::DISC_MARGINFI_ACCOUNT,
            bytemuck::bytes_of(&a),
        ));
    }
    let mut st = StateCore::new(16);
    st.install_snapshot(10, accounts);
    for workers in [1usize, 2] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut blocks = Vec::new();
        for _ in 0..CYCLES {
            // Ingress keeps arriving while the owner is blocked.
            let (tx, rx) = bounded::<u64>("r3-ingress", QUEUE);
            let filling = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let fill = filling.clone();
            let producer = std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let mut sent = 0u64;
                while fill.load(std::sync::atomic::Ordering::Acquire) {
                    let due = (started.elapsed().as_micros() as u64) * RATE_PER_S / 1_000_000;
                    while sent < due {
                        let _ = tx.try_push(sent);
                        sent += 1;
                    }
                    std::thread::sleep(Duration::from_micros(200));
                }
                tx.stats()
            });
            let started = std::time::Instant::now();
            let (index, banks, indexed, _) = rebuild_reverse_index(&st, &program, &group, None);
            assert_eq!((banks, indexed), (2, DEBTORS));
            let view = StateView(&st);
            let engine = darkbot_project0::health::crate_impl::CrateHealth {
                state: &view,
                group,
            };
            let mut work: HashMap<Pubkey, (u64, u64, u64)> = HashMap::new();
            for bank in [asset_bank, liability_bank] {
                for account in index.accounts_for_bank(&bank) {
                    work.entry(*account).or_insert((1, 10, 0));
                }
            }
            let mut evaluated: Vec<(Pubkey, bool)> = pool.install(|| {
                work.into_par_iter()
                    .map(|(account, _)| {
                        (
                            account,
                            engine.health(&account, 1_000, 10).unwrap().liquidatable,
                        )
                    })
                    .collect()
            });
            evaluated.sort_unstable_by_key(|(account, liquidatable)| (!*liquidatable, *account));
            let block = started.elapsed();
            filling.store(false, std::sync::atomic::Ordering::Release);
            let stats = producer.join().unwrap();
            drop(rx);
            assert_eq!(evaluated.len(), DEBTORS);
            println!(
                "R3_CYCLE workers={workers} owner_block_ms={:.1} queued={} overflow={}",
                block.as_secs_f64() * 1e3,
                stats.sent,
                stats.overflow
            );
            assert_eq!(stats.overflow, 0, "queue overflow during the owner block");
            blocks.push((block, stats.sent));
        }
        let mut ms: Vec<f64> = blocks.iter().map(|(b, _)| b.as_secs_f64() * 1e3).collect();
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let max = ms[ms.len() - 1];
        println!(
            "R3_RECOVERY_SWEEP workers={workers} debtors={DEBTORS} cycles={CYCLES} owner_block_ms_min={:.1} max={max:.1} queued_max={} overflow=0 break_even_events_per_s={:.0}",
            ms[0],
            blocks.iter().map(|(_, s)| *s).max().unwrap(),
            QUEUE as f64 / (max / 1e3)
        );
    }
}

/// C11 latency (run with `--release -- --ignored --nocapture`): a validated ATA lookup with a fresh
/// derivation against the owner's derived-once addresses. Prints, asserts no speed. (Here, not in
/// `accounts.rs`: owner-thread sources never print — PERSIST-02 source scan.)
#[test]
#[ignore]
fn c11_ata_lookup_latency_benchmark() {
    struct One(Pubkey, Pubkey, Vec<u8>);
    impl AccountSource for One {
        fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            (*k == self.0).then_some((self.1, 0, self.2.as_slice()))
        }
    }
    const N: usize = 20_000;
    let (liquidator, mint) = (Pubkey::new_unique(), Pubkey::new_unique());
    let token = Pubkey::from_str(protocol::TOKEN_PROGRAM_ID).unwrap();
    let mut data = vec![0u8; 165];
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(liquidator.as_ref());
    data[108] = 1;
    let state = One(
        associated_token_address(&liquidator, &mint, &token),
        token,
        data,
    );
    let warm = LiquidatorAtas::new(liquidator);
    let measure = |label: &str, run: &dyn Fn()| {
        let mut ns: Vec<u128> = (0..N)
            .map(|_| {
                let t = std::time::Instant::now();
                run();
                t.elapsed().as_nanos()
            })
            .collect();
        ns.sort_unstable();
        println!(
            "C11_BENCH {label} n={N} p50_ns={} p99_ns={}",
            ns[N / 2],
            ns[N * 99 / 100]
        );
    };
    measure("validated_ata_derived", &|| {
        let fresh = LiquidatorAtas::new(liquidator);
        std::hint::black_box(validated_liquidator_ata(&state, &mint, &fresh));
    });
    measure("validated_ata_cached", &|| {
        std::hint::black_box(validated_liquidator_ata(&state, &mint, &warm));
    });
}
