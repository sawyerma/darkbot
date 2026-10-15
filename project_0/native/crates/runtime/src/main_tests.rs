//! Integration-probe and Shadow helper tests of the p0-longtail binary (split from main.rs, structure iteration 29).
use super::*;

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
            100,
            7,
            "oracle_health_ready",
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
            200,
            7,
            "oracle_health_ready",
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
            300,
            8,
            "oracle_health_error",
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
