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
    t_rx_ns: u64,
    event_slot: u64,
    trigger: &'static str,
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
    t_rx_ns: u64,
    event_slot: u64,
    trigger: &'static str,
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
