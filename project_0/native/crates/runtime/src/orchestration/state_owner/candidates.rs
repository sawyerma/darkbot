//! Candidate health cache, admission and fairness queue of the Shadow runtime (split from main.rs,
//! structure iteration 29). Private to the p0-longtail binary.
use super::*;

/// S2-State als AccountSource für die Health-Engine (keine Kopie, keine RPC).
pub(crate) struct StateView<'a>(pub(crate) &'a StateCore);
impl<'a> AccountSource for StateView<'a> {
    fn account(
        &self,
        key: &solana_sdk::pubkey::Pubkey,
    ) -> Option<(solana_sdk::pubkey::Pubkey, u64, &[u8])> {
        self.0
            .get(key)
            .map(|r| (r.owner, r.lamports, r.data.as_slice()))
    }
}

#[cfg(feature = "crate_health")]
pub(crate) struct CachedProjection {
    pub(crate) base_account_hash: Hash,
    pub(crate) projection_slot: u64,
    pub(crate) projection_unix_s: i64,
    pub(crate) accounts: Vec<(Pubkey, Account)>,
    /// E9: every account the projection and its later evaluation depend on besides the
    /// candidate (refresh instruction accounts, health observation accounts) …
    pub(crate) dependencies: Vec<Pubkey>,
    /// … and their canonical revisions when the projection was requested.
    pub(crate) dependency_fingerprint: Hash,
}

/// E9: one fingerprint over the canonical revision (presence, slot, data hash) of every
/// dependency key; any change of any of them changes it, unrelated accounts never do.
pub(crate) fn dependency_fingerprint(st: &StateCore, keys: &[Pubkey]) -> Hash {
    let mut parts: Vec<Vec<u8>> = Vec::with_capacity(keys.len());
    for k in keys {
        let mut part = k.to_bytes().to_vec();
        match st.get(k) {
            Some(r) => {
                part.push(1);
                part.extend_from_slice(&r.slot.to_le_bytes());
                part.extend_from_slice(&hash(&r.data).to_bytes());
            }
            None => part.push(0),
        }
        parts.push(part);
    }
    let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
    solana_sdk::hash::hashv(&refs)
}

/// E9: the sorted, deduplicated dependency set of a candidate's projection.
pub(crate) fn projection_dependencies(
    candidate: &Pubkey,
    refreshes: &[solana_sdk::instruction::Instruction],
    observation: &[Pubkey],
) -> Vec<Pubkey> {
    let mut keys: Vec<Pubkey> = refreshes
        .iter()
        .flat_map(|ix| ix.accounts.iter().map(|m| m.pubkey))
        .chain(observation.iter().copied())
        .filter(|k| k != candidate)
        .collect();
    keys.sort_unstable_by_key(|k| k.to_bytes());
    keys.dedup();
    keys
}

#[cfg(feature = "crate_health")]
pub(crate) type DetectWork = (Pubkey, u64, u64, &'static str);

#[cfg(feature = "crate_health")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateHealthState {
    Healthy,
    Liquidatable,
    Error,
}

#[cfg(feature = "crate_health")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct CandidateCacheEntry {
    pub(crate) state: CandidateHealthState,
    pub(crate) last_forwarded_slot: u64,
    /// [FIX-134] Monotonic causal revision of the newest canonical Bank/dependency
    /// input that was admitted downstream for this candidate. Slot alone is insufficient:
    /// WSS can deliver multiple distinct writes inside one Solana slot.
    pub(crate) last_forwarded_revision: u64,
}

#[cfg(feature = "crate_health")]
pub(crate) fn candidate_should_forward(
    previous: Option<CandidateCacheEntry>,
    current: CandidateHealthState,
    evaluation_slot: u64,
    input_revision: u64,
    recheck_slots: u64,
    forced: bool,
    repeat_capacity: bool,
) -> bool {
    if current == CandidateHealthState::Healthy {
        return false;
    }
    let transition = previous.map(|e| e.state) != Some(current);
    let last_forwarded = previous.map(|e| e.last_forwarded_slot).unwrap_or(0);
    let last_revision = previous.map(|e| e.last_forwarded_revision).unwrap_or(0);
    let revision_changed = input_revision > last_revision;
    let periodic_due = evaluation_slot.saturating_sub(last_forwarded) >= recheck_slots;
    // [FIX-134] A changed canonical input is correctness work, not a periodic repeat:
    // it bypasses the repeat soft-cap exactly like a class transition / forced re-arm.
    forced || transition || revision_changed || (periodic_due && repeat_capacity)
}

/// Fairness/scheduling key of one sweep candidate (`candidate_fairness_key`).
#[cfg(feature = "crate_health")]
pub(crate) type FairnessKey = (u8, std::cmp::Reverse<u64>, [u8; 32]);

/// D12: orders sweep work by its fairness key, computing each key exactly once; returns whether
/// any candidate is an ordinary due repeat (class 1, a fairness rotation). The keys are unique
/// (pubkey tie-breaker), so the order equals sorting with the key recomputed per comparison.
#[cfg(feature = "crate_health")]
pub(crate) fn order_by_fairness<T>(items: &mut Vec<T>, key: impl Fn(&T) -> FairnessKey) -> bool {
    let mut keyed: Vec<(FairnessKey, T)> = items.drain(..).map(|t| (key(&t), t)).collect();
    let rotation = keyed.iter().any(|(k, _)| k.0 == 1);
    keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    items.extend(keyed.into_iter().map(|(_, t)| t));
    rotation
}

#[cfg(feature = "crate_health")]
pub(crate) fn candidate_fairness_key(
    account: Pubkey,
    previous: Option<CandidateCacheEntry>,
    current: CandidateHealthState,
    evaluation_slot: u64,
    input_revision: u64,
    forced: bool,
    recheck_slots: u64,
) -> (u8, std::cmp::Reverse<u64>, [u8; 32]) {
    // [FIX-135] Correctness work always sorts before ordinary repeats and is never
    // throttled by repeat capacity. Ordinary repeats are ordered oldest-first so
    // a persistent candidate that was deferred becomes strictly more urgent.
    if current == CandidateHealthState::Healthy {
        return (2, std::cmp::Reverse(0), account.to_bytes());
    }

    let transition = previous.map(|e| e.state) != Some(current);
    let last_revision = previous.map(|e| e.last_forwarded_revision).unwrap_or(0);
    let revision_changed = input_revision > last_revision;
    if forced || transition || revision_changed {
        return (0, std::cmp::Reverse(u64::MAX), account.to_bytes());
    }

    let last_forwarded = previous.map(|e| e.last_forwarded_slot).unwrap_or(0);
    let wait_slots = evaluation_slot.saturating_sub(last_forwarded);
    if wait_slots >= recheck_slots {
        (1, std::cmp::Reverse(wait_slots), account.to_bytes())
    } else {
        (2, std::cmp::Reverse(wait_slots), account.to_bytes())
    }
}

// [FIX-121][R6] Qualification targets wait as unsigned identities, not signed RPC jobs.
// This timestamp is the start of an explicit synthetic probe; real ingress timestamps
// and projection latency remain untouched and are still subject to freshness guards.
#[cfg(feature = "crate_health")]
pub(crate) fn next_integration_probe(
    queue: &mut VecDeque<(Pubkey, u64)>,
    projection_busy: bool,
    now_ns: u64,
) -> Option<DetectWork> {
    if projection_busy {
        return None;
    }
    queue
        .pop_front()
        .map(|(account, slot)| (account, now_ns, slot, "integration_target_probe"))
}

#[cfg(feature = "crate_health")]
pub(crate) fn next_oracle_detect(
    order: &mut VecDeque<Pubkey>,
    pending: &mut HashMap<Pubkey, DetectWork>,
) -> Option<DetectWork> {
    while let Some(account) = order.pop_front() {
        if let Some(work) = pending.remove(&account) {
            return Some(work);
        }
    }
    None
}

#[cfg(feature = "crate_health")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateQueueAdmission {
    Enqueued,
    Coalesced,
}

#[cfg(feature = "crate_health")]
pub(crate) fn enqueue_or_coalesce_candidate(
    account: Pubkey,
    (t_rx_ns, event_slot, trigger): (u64, u64, &'static str),
    order: &mut VecDeque<Pubkey>,
    pending: &mut HashMap<Pubkey, DetectWork>,
    enqueued: &mut u64,
    coalesced: &mut u64,
    max_pending: &mut usize,
) -> CandidateQueueAdmission {
    let work = (account, t_rx_ns, event_slot, trigger);
    if let Some(existing) = pending.get_mut(&account) {
        // [FIX-137] Single-flight latest-state semantics: the queue token already
        // exists, so carry the newest causal work into that token instead of
        // allocating/deduping another one. Downstream reads canonical state again.
        *existing = work;
        *coalesced = coalesced.saturating_add(1);
        CandidateQueueAdmission::Coalesced
    } else {
        order.push_back(account);
        pending.insert(account, work);
        *enqueued = enqueued.saturating_add(1);
        *max_pending = (*max_pending).max(pending.len());
        CandidateQueueAdmission::Enqueued
    }
}

#[cfg(feature = "crate_health")]
pub(crate) fn enqueue_oracle_accounts(
    accounts: impl IntoIterator<Item = Pubkey>,
    (t_rx_ns, event_slot, trigger): (u64, u64, &'static str),
    order: &mut VecDeque<Pubkey>,
    pending: &mut HashMap<Pubkey, DetectWork>,
    enqueued: &mut u64,
    coalesced: &mut u64,
    max_pending: &mut usize,
) -> usize {
    let mut unique = HashSet::new();
    unique.extend(accounts);
    let count = unique.len();
    let mut accounts: Vec<Pubkey> = unique.into_iter().collect();
    accounts.sort_unstable_by_key(|pk| pk.to_bytes());
    for account in accounts {
        let work = (account, t_rx_ns, event_slot, trigger);
        if let Some(existing) = pending.get_mut(&account) {
            *existing = work;
            *coalesced = coalesced.saturating_add(1);
        } else {
            order.push_back(account);
            pending.insert(account, work);
            *enqueued = enqueued.saturating_add(1);
        }
    }
    *max_pending = (*max_pending).max(pending.len());
    count
}

#[cfg(all(test, feature = "crate_health"))]
mod d12_tests {
    use super::*;

    /// D12: each key is computed once, the order is exactly the per-comparison sort's, and the
    /// rotation flag matches a class-1 key; empty work stays empty.
    #[test]
    fn d12_fairness_keys_are_computed_once_with_identical_order() {
        let keys: Vec<FairnessKey> = (0..500u64)
            .map(|i| {
                let class = (i % 3) as u8;
                let mut account = [0u8; 32];
                account[..8].copy_from_slice(&i.wrapping_mul(0x9E37_79B9_7F4A_7C15).to_le_bytes());
                (class, std::cmp::Reverse(i % 17), account)
            })
            .collect();
        let mut expected = keys.clone();
        expected.sort_unstable();
        let calls = std::cell::Cell::new(0usize);
        let mut items = keys.clone();
        let rotation = order_by_fairness(&mut items, |k| {
            calls.set(calls.get() + 1);
            *k
        });
        assert_eq!(calls.get(), keys.len());
        assert_eq!(items, expected);
        assert!(rotation);
        let mut no_repeat: Vec<FairnessKey> = keys.into_iter().filter(|k| k.0 != 1).collect();
        assert!(!order_by_fairness(&mut no_repeat, |k| *k));
        let mut empty: Vec<FairnessKey> = vec![];
        assert!(!order_by_fairness(&mut empty, |k| *k) && empty.is_empty());
    }
}

#[cfg(all(test, feature = "crate_health"))]
mod e9_tests {
    use super::*;

    /// E9: the projection fingerprint follows every dependency revision (slot, data, presence)
    /// and ignores the candidate itself and unrelated accounts.
    #[test]
    fn e9_dependency_fingerprint_tracks_every_dependency_revision() {
        let (candidate, bank, oracle, unrelated) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let account = |slot: u64, byte: u8| (slot, vec![byte; 8]);
        let refresh = solana_sdk::instruction::Instruction {
            program_id: Pubkey::new_unique(),
            accounts: vec![
                solana_sdk::instruction::AccountMeta::new(oracle, false),
                solana_sdk::instruction::AccountMeta::new_readonly(candidate, false),
            ],
            data: vec![],
        };
        let deps = projection_dependencies(&candidate, &[refresh], &[bank, oracle]);
        assert_eq!(deps.len(), 2);
        assert!(!deps.contains(&candidate));
        let install = |entries: &[(Pubkey, (u64, Vec<u8>))]| {
            let mut st = StateCore::new(1);
            st.install_snapshot(
                10,
                entries
                    .iter()
                    .map(|(k, (slot, data))| darkbot_solana::ingress::AccountUpdate {
                        pubkey: *k,
                        owner: Pubkey::new_from_array([3; 32]),
                        lamports: 1,
                        data: data.clone(),
                        slot: *slot,
                        write_version: None,
                        source: darkbot_solana::ingress::SourceKind::Snapshot,
                        t_rx_ns: 0,
                    })
                    .collect(),
            );
            st
        };
        let base = [
            (candidate, account(10, 1)),
            (bank, account(10, 2)),
            (oracle, account(10, 3)),
            (unrelated, account(10, 4)),
        ];
        let fp = dependency_fingerprint(&install(&base), &deps);
        let mut changed_candidate = base.clone();
        changed_candidate[0].1 = account(11, 9);
        changed_candidate[3].1 = account(12, 9);
        assert_eq!(
            dependency_fingerprint(&install(&changed_candidate), &deps),
            fp
        );
        for (i, replacement) in [(1, account(10, 7)), (2, account(11, 3))] {
            let mut changed = base.clone();
            changed[i].1 = replacement;
            assert_ne!(dependency_fingerprint(&install(&changed), &deps), fp, "{i}");
        }
        assert_ne!(
            dependency_fingerprint(&install(&base[..2]), &deps),
            fp,
            "missing oracle"
        );
    }
}
