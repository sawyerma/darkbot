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
