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

/// E9: one fingerprint over the canonical revision (presence, slot, revision epoch) of every
/// dependency key; any accepted write of any of them changes it, unrelated accounts never do.
/// V2-06: the epoch replaces a hash of the data (ProgramData was rehashed in full on every
/// call) and also binds same-slot owner/lamports writes. Epochs are local to this `StateCore`
/// instance: a fingerprint is compared only against the same instance, never persisted; a
/// snapshot install re-epochs every record (earlier fingerprints never match again).
pub(crate) fn dependency_fingerprint(st: &StateCore, keys: &[Pubkey]) -> Hash {
    let mut parts: Vec<Vec<u8>> = Vec::with_capacity(keys.len());
    for k in keys {
        let mut part = k.to_bytes().to_vec();
        match st.get(k) {
            Some(r) => {
                part.push(1);
                part.extend_from_slice(&r.slot.to_le_bytes());
                part.extend_from_slice(&r.last_changed_epoch.to_le_bytes());
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
    fixed: &[Pubkey],
) -> Vec<Pubkey> {
    let mut keys: Vec<Pubkey> = refreshes
        .iter()
        .flat_map(|ix| ix.accounts.iter().map(|m| m.pubkey))
        .chain(observation.iter().copied())
        .chain(fixed.iter().copied())
        .filter(|k| k != candidate)
        .collect();
    keys.sort_unstable_by_key(|k| k.to_bytes());
    keys.dedup();
    keys
}

/// E9/N3: the dependency set of a candidate's projection — the refresh instruction accounts, the
/// candidate's health observation accounts, the group and the FeeState (sizing reads the group's
/// EMODE and the FeeState) and the program's ProgramData (a program upgrade invalidates it). An undeterminable observation set is an error, never an empty set:
/// without it the projection's staleness cannot be bound (fail-closed).
#[cfg(feature = "crate_health")]
pub(crate) fn projection_dependency_set<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    program_data: &Pubkey,
    group: &Pubkey,
    candidate: &Pubkey,
    refreshes: &[solana_sdk::instruction::Instruction],
) -> Result<Vec<Pubkey>, String> {
    let observation = darkbot_project0::health::crate_impl::CrateHealth {
        state,
        group: *group,
    }
    .observation_accounts(candidate)
    .map_err(|e| e.to_string())?;
    Ok(projection_dependencies(
        candidate,
        refreshes,
        &observation,
        &[*group, protocol::fee_state_pda(program), *program_data],
    ))
}

/// E9: the newest canonical slot among the candidate and its dependencies — the oldest
/// simulation context a projection requested from them may come from.
pub(crate) fn required_context_slot(st: &StateCore, base_slot: u64, keys: &[Pubkey]) -> u64 {
    keys.iter()
        .filter_map(|k| st.get(k).map(|r| r.slot))
        .fold(base_slot, u64::max)
}

/// N6: whether a bank's dependency gap is reported — when it is new, when it changed, and once
/// when it cleared; an unchanged gap stays silent. `seen` holds exactly the banks with a current
/// gap, so it is bounded by the number of banks.
#[cfg(feature = "crate_health")]
pub(crate) fn note_gap(
    seen: &mut HashMap<Pubkey, darkbot_project0::health::DependencyGap>,
    bank: Pubkey,
    gap: Option<&darkbot_project0::health::DependencyGap>,
) -> bool {
    match gap {
        Some(g) => seen.insert(bank, g.clone()).as_ref() != Some(g),
        None => seen.remove(&bank).is_some(),
    }
}

/// [FIX-91] Bankruptcy/no-collateral is a protocol state, not a routing/sizing failure.
/// Z13: it replaces only the "no plan" follow-on causes a zero-equity account produces
/// (sizing, routing, funding, economics) and keeps that cause in the reason; an earlier,
/// independent cause (own network budget, state, ingress, health, integration, build,
/// submission …) and a candidate without any loss stay exactly as recorded.
/// V19: a health error the shadow ledger counts as fail-closed unsupported — an unsupported
/// protocol state or a layout the strict decoders refuse.
pub(crate) fn health_fails_closed(e: &darkbot_project0::health::HealthError) -> bool {
    use darkbot_project0::health::HealthError as H;
    match e {
        H::Unsupported(_) | H::UnknownLayout(_) => true,
        H::Stale(_)
        | H::IntegrationStale(_)
        | H::Crate(_)
        | H::MissingAccount(_)
        | H::Unavailable => false,
    }
}

pub(crate) fn bankruptcy_classification(
    loss: Option<crate::analytics::record::LossReason>,
    reason: Option<String>,
    bad_debt: bool,
    equity_assets: i128,
) -> (Option<crate::analytics::record::LossReason>, Option<String>) {
    use crate::analytics::record::LossReason as L;
    // F-08: the follow-on cause is bound by the pattern itself — no invariant left to a panic.
    let Some(
        cause @ (L::Sizing
        | L::Routing
        | L::Funding
        | L::Economics
        | L::EconomicSkip
        | L::ThresholdTooHigh),
    ) = loss
    else {
        return (loss, reason);
    };
    if !(bad_debt && equity_assets <= 0) {
        return (loss, reason);
    }
    (
        Some(L::Bankruptcy),
        Some(format!(
            "bad debt with zero equity assets: bankruptcy/no-collateral case, not a normal \
             receivership opportunity (reported as {:?}: {})",
            cause,
            reason.as_deref().unwrap_or("-")
        )),
    )
}

/// N4: the outcomes of the integration projections (how often E9 discards one). Every requested
/// projection ends in exactly one result counter or is still in flight, every accepted one in
/// exactly one consume counter or is still cached, so the report balances:
/// `requested = accepted + failed + stale_result + discarded_result + in_flight` and
/// `accepted = reused + invalid_at_use + discarded_consume + stale_consume + dropped + cached`.
/// Reported periodically with the owner telemetry and once at shutdown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectionStats {
    /// Handed to the projection worker.
    pub(crate) requested: u64,
    /// Not handed over: the worker queue was full (no request).
    pub(crate) queue_full: u64,
    /// Not requested: the candidate's observation set is unknown (N3, fail-closed).
    pub(crate) observation_unknown: u64,
    /// Results: cached for the candidate's next evaluation …
    pub(crate) accepted: u64,
    /// … the simulation failed …
    pub(crate) failed: u64,
    /// … the candidate itself changed while the worker projected …
    pub(crate) stale_at_result: u64,
    /// … or a dependency changed (E9).
    pub(crate) discarded_at_result: u64,
    /// Cached projections: used by the evaluation …
    pub(crate) reused: u64,
    /// … taken for the evaluation but its overlay was invalid (no evaluation on it) …
    pub(crate) invalid_at_use: u64,
    /// … discarded at use because a dependency changed (E9) …
    pub(crate) discarded_at_consume: u64,
    /// … discarded at use because the candidate changed …
    pub(crate) stale_at_consume: u64,
    /// … or replaced/removed before any use.
    pub(crate) dropped: u64,
    /// E9: a stale result or a lagging node led to a new request (bounded per candidate) …
    pub(crate) requeued: u64,
    /// … or the candidate's bound was reached and it waits for its next trigger.
    pub(crate) requeue_exhausted: u64,
    /// E9: a cached projection newer than the processed account notifications waited (parked).
    pub(crate) deferred: u64,
}

/// E9: trigger of the consumption of a candidate's cached projection (the projection's own work).
pub(crate) const TRIGGER_INTEGRATION_PROJECTION: &str = "integration_projection";
/// E9: trigger of a bounded projection requeue (the projection's own work).
pub(crate) const TRIGGER_PROJECTION_REQUEUE: &str = "integration_projection_requeue";

/// E9: whether a trigger is the projection's own work (not a new external trigger).
pub(crate) fn projection_own_trigger(trigger: &str) -> bool {
    trigger == TRIGGER_INTEGRATION_PROJECTION || trigger == TRIGGER_PROJECTION_REQUEUE
}

/// E9: the newest slot up to which the owner has every confirmed account update: one slot below
/// the newest account notification (assumption from the agave notification design, not measured
/// against the provider: one confirmed bank per pass over all subscriptions, in slot order, on
/// the one WSS connection — a notification of slot S then proves every slot before S complete,
/// S itself only once a later slot arrives), the last snapshot's
/// lower bound, or the root. (A root can precede late confirmed notifications the tracker sends
/// while catching up; accepted, stated in BUGS E9.)
pub(crate) fn processed_level(st: &StateCore) -> u64 {
    st.highest_account_slot()
        .saturating_sub(1)
        .max(st.covered_slot())
        .max(st.root().unwrap_or(0))
}

/// E9: a projection simulated at `slot` is not newer than the RAM inputs it is combined with —
/// the owner has every account update up to `slot`, and only on a tradable state (a reconnect or
/// recovery window has no complete stream until its snapshot is reconciled).
pub(crate) fn processed_through(st: &StateCore, slot: u64) -> bool {
    st.run_state().is_tradable() && processed_level(st) >= slot
}

/// E9: the detect gate before a candidate's projection path. A new external trigger supersedes
/// the candidate's parked wait. The consumption of a cached projection proceeds only once the
/// owner has processed every confirmed account notification up to the projection's simulation
/// slot — never a newer overlay combined with older RAM inputs; until then the work is parked
/// (counted `deferred`) and `false` is returned. Everything else proceeds.
#[cfg(feature = "crate_health")]
pub(crate) fn projection_consume_gate(
    st: &StateCore,
    cache: &HashMap<Pubkey, CachedProjection>,
    parked: &mut ParkedProjections,
    stats: &mut ProjectionStats,
    work: DetectWork,
) -> bool {
    let (account, _, _, trigger) = work;
    if !projection_own_trigger(trigger) {
        parked.unpark(&account);
        return true;
    }
    if trigger != TRIGGER_INTEGRATION_PROJECTION {
        return true;
    }
    match cache.get(&account).map(|c| c.projection_slot) {
        Some(projected) if !processed_through(st, projected) => {
            parked.park(projected, work);
            stats.deferred += 1;
            false
        }
        _ => true,
    }
}

/// E9: projection work waiting for the owner to process the account notifications up to a slot
/// (a cached projection's simulation slot, or one slot of backoff after a lagging node) — at
/// most one entry per candidate, released in slot order.
#[cfg(feature = "crate_health")]
#[derive(Default)]
pub(crate) struct ParkedProjections {
    by_slot: std::collections::BTreeMap<(u64, Pubkey), DetectWork>,
    at: HashMap<Pubkey, u64>,
}

#[cfg(feature = "crate_health")]
impl ParkedProjections {
    /// Parks `work` until `until_slot`, replacing the candidate's earlier entry.
    pub(crate) fn park(&mut self, until_slot: u64, work: DetectWork) {
        self.unpark(&work.0);
        self.at.insert(work.0, until_slot);
        self.by_slot.insert((until_slot, work.0), work);
    }
    /// Drops the candidate's entry (a new external trigger supersedes it).
    pub(crate) fn unpark(&mut self, account: &Pubkey) {
        if let Some(slot) = self.at.remove(account) {
            self.by_slot.remove(&(slot, *account));
        }
    }
    /// The work whose slot the owner has processed, in slot order.
    pub(crate) fn release(&mut self, st: &StateCore) -> Vec<DetectWork> {
        let mut out = Vec::new();
        while let Some((&(slot, account), _)) = self.by_slot.first_key_value() {
            if !processed_through(st, slot) {
                break;
            }
            self.at.remove(&account);
            if let Some(work) = self.by_slot.remove(&(slot, account)) {
                out.push(work);
            }
        }
        out
    }
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.at.len()
    }
}

/// E9: one more projection request that no new trigger asked for (a stale result, a lagging
/// node, a cached projection unusable at its consumption) — at most `projection_requeue_max`
/// per external trigger; at the bound it is counted, the sequence ends and the candidate waits
/// for its next trigger.
pub(crate) fn projection_retry_allowed(
    requeues: &mut HashMap<Pubkey, u8>,
    stats: &mut ProjectionStats,
    account: Pubkey,
    max: u8,
) -> bool {
    let attempts = requeues.entry(account).or_insert(0);
    if *attempts >= max {
        requeues.remove(&account);
        stats.requeue_exhausted += 1;
        return false;
    }
    *attempts += 1;
    stats.requeued += 1;
    true
}

/// E9: whether detect may request a projection for `account` on `trigger`. An external trigger
/// starts a fresh sequence; the consumption of an unusable cached projection re-requests only
/// within the bound; a requeue was counted when it was queued.
pub(crate) fn projection_request_allowed(
    requeues: &mut HashMap<Pubkey, u8>,
    stats: &mut ProjectionStats,
    account: Pubkey,
    trigger: &str,
    max: u8,
) -> bool {
    match trigger {
        TRIGGER_INTEGRATION_PROJECTION => projection_retry_allowed(requeues, stats, account, max),
        TRIGGER_PROJECTION_REQUEUE => true,
        _ => {
            requeues.remove(&account);
            true
        }
    }
}
impl ProjectionStats {
    /// A cached projection is taken for the candidate's evaluation; a usable one is counted by
    /// `used` once its overlay is built.
    #[cfg(feature = "crate_health")]
    pub(crate) fn consume(&mut self, base_matches: bool, dependencies_unchanged: bool) -> bool {
        match (base_matches, dependencies_unchanged) {
            (false, _) => self.stale_at_consume += 1,
            (true, false) => self.discarded_at_consume += 1,
            (true, true) => {}
        }
        base_matches && dependencies_unchanged
    }
    /// The taken projection's overlay was built (`reused`) or was invalid (`invalid_at_use`).
    #[cfg(feature = "crate_health")]
    pub(crate) fn used(&mut self, overlay_valid: bool) {
        if overlay_valid {
            self.reused += 1;
        } else {
            self.invalid_at_use += 1;
        }
    }
    /// A successful worker result: accepted only with the candidate and every dependency
    /// unchanged.
    #[cfg(feature = "crate_health")]
    pub(crate) fn result(&mut self, base_matches: bool, dependencies_unchanged: bool) -> bool {
        match (base_matches, dependencies_unchanged) {
            (false, _) => self.stale_at_result += 1,
            (true, false) => self.discarded_at_result += 1,
            (true, true) => self.accepted += 1,
        }
        base_matches && dependencies_unchanged
    }
    /// A cached projection left the cache without being used (replaced or removed).
    #[cfg(feature = "crate_health")]
    pub(crate) fn dropped_if<T>(&mut self, removed: Option<T>) {
        if removed.is_some() {
            self.dropped += 1;
        }
    }
    /// The report line with the two current gauges (`in_flight`, `cached`) that balance it.
    pub(crate) fn line(&self, in_flight: usize, cached: usize) -> String {
        format!(
            "projection: requested={} queue_full={} observation_unknown={} accepted={} failed={} \
             stale_result={} discarded_result={} in_flight={in_flight} reused={} \
             invalid_at_use={} discarded_consume={} stale_consume={} dropped={} cached={cached} \
             requeued={} requeue_exhausted={} deferred={}",
            self.requested,
            self.queue_full,
            self.observation_unknown,
            self.accepted,
            self.failed,
            self.stale_at_result,
            self.discarded_at_result,
            self.reused,
            self.invalid_at_use,
            self.discarded_at_consume,
            self.stale_at_consume,
            self.dropped,
            self.requeued,
            self.requeue_exhausted,
            self.deferred,
        )
    }
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

/// V2-13 (P4-03/P5b-02): the repeat gate of the threshold trigger — the sweep's forward policy
/// (`candidate_should_forward`) for a threshold hit or a bank re-check, which carries no health
/// state yet: a first admission, a forced re-arm or a newer input revision always passes; a
/// repeat only once `recheck_slots` passed and the repeat soft cap leaves room. Each pass is
/// recorded as the account's forward; a refusal is counted suppressed.
#[cfg(feature = "crate_health")]
pub(crate) struct RepeatGate<'a> {
    pub(crate) cache: &'a mut HashMap<Pubkey, CandidateCacheEntry>,
    pub(crate) forced: &'a mut HashSet<Pubkey>,
    pub(crate) recheck_slots: u64,
    pub(crate) repeat_capacity: bool,
    pub(crate) suppressed: &'a mut u64,
    pub(crate) softcap_suppressed: &'a mut u64,
}

#[cfg(feature = "crate_health")]
impl RepeatGate<'_> {
    /// The accounts that pass, each with its previous forward slot (its L2 order key).
    pub(crate) fn pass(
        &mut self,
        accounts: HashSet<Pubkey>,
        slot: u64,
        revision: u64,
    ) -> HashMap<Pubkey, u64> {
        let mut passed = HashMap::with_capacity(accounts.len());
        for account in accounts {
            let previous = self.cache.get(&account).copied();
            let forced = self.forced.remove(&account);
            let last = previous.map_or(0, |e| e.last_forwarded_slot);
            if candidate_should_forward(
                previous,
                CandidateHealthState::Liquidatable,
                slot,
                revision,
                self.recheck_slots,
                forced,
                self.repeat_capacity,
            ) {
                self.cache.insert(
                    account,
                    CandidateCacheEntry {
                        state: CandidateHealthState::Liquidatable,
                        last_forwarded_slot: slot.max(last),
                        last_forwarded_revision: previous
                            .map_or(revision, |e| e.last_forwarded_revision.max(revision)),
                    },
                );
                passed.insert(account, last);
            } else {
                *self.suppressed = self.suppressed.saturating_add(1);
                if slot.saturating_sub(last) >= self.recheck_slots {
                    *self.softcap_suppressed = self.softcap_suppressed.saturating_add(1);
                }
            }
        }
        passed
    }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateQueueAdmission {
    Enqueued,
    Coalesced,
}

/// P6-MT.5 (MT3): trigger-priority lane of a candidate before its preparation (the type lives
/// with the candidate identity since MT5a; the lanes' behaviour is here).
#[cfg(feature = "crate_health")]
pub(crate) use crate::execution::identity::Lane;
#[cfg(feature = "crate_health")]
pub(crate) const LANES: usize = 3;
#[cfg(feature = "crate_health")]
pub(crate) const LANE_LIST: [Lane; LANES] = [Lane::L0, Lane::L1, Lane::L2];

#[cfg(feature = "crate_health")]
impl Lane {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Lane::L0 => "L0",
            Lane::L1 => "L1",
            Lane::L2 => "L2",
        }
    }
    fn idx(self) -> usize {
        self as usize
    }
}

/// One trigger admitted to the lanes (what the enqueue site knows).
#[cfg(feature = "crate_health")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LaneAdmission {
    pub(crate) t_rx_ns: u64,
    pub(crate) slot: u64,
    pub(crate) trigger: &'static str,
    pub(crate) lane: Lane,
    /// Newest canonical input revision the site saw ([FIX-134] `input_revision`); 0 = unknown.
    /// Only a strictly newer revision may demote a pending entry (P6-MT.5: never by age alone).
    pub(crate) revision: u64,
    /// L2 order key: the candidate's last forwarded slot (0 = never forwarded, served first —
    /// the FIX-135 fairness of the sweep).
    pub(crate) last_forwarded_slot: u64,
}

/// A candidate's single pending entry (one per account, as before MT3).
#[cfg(feature = "crate_health")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingCandidate {
    /// The newest coalesced trigger `(account, t_rx_latest, slot, trigger)`. Its `t_rx` is the
    /// FIX-21 freshness origin handed to the detect/pipeline — unchanged by MT3 (P6-MT.6).
    pub(crate) work: DetectWork,
    /// Receive time of the first still-unserved trigger; lane order and `lane_wait_ns` only.
    pub(crate) t_rx_first: u64,
    pub(crate) lane: Lane,
    pub(crate) revision: u64,
    pub(crate) last_forwarded_slot: u64,
    /// Admission sequence: the deterministic tie-breaker inside a lane.
    seq: u64,
}

#[cfg(feature = "crate_health")]
impl PendingCandidate {
    /// Order inside the lane: L0 earliest `t_rx_first` (deadline proxy; no economics yet), L1
    /// oldest `t_rx_first`, L2 oldest last forwarded slot (FIX-135 fairness); then admission
    /// order, then the pubkey.
    fn key(&self) -> (u64, u64, Pubkey) {
        let primary = match self.lane {
            Lane::L0 | Lane::L1 => self.t_rx_first,
            Lane::L2 => self.last_forwarded_slot,
        };
        (primary, self.seq, self.work.0)
    }
}

/// Lane changes of pending entries by a later trigger (P6-MT.13 `lane_promotions/demotions/
/// cancellations`).
#[cfg(feature = "crate_health")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LaneStats {
    pub(crate) promotions: u64,
    pub(crate) demotions: u64,
    /// No cancellation rule exists today: a candidate re-evaluated Healthy by the sweep is not
    /// forwarded again but its pending entry stays for the detect's own exact evaluation (as
    /// before MT3). P6-MT.5 allows a newer revision to cancel; measured, never incremented.
    pub(crate) cancellations: u64,
}

/// P6-MT.5 (MT3): the one oracle/sweep candidate queue — three trigger-priority lanes served by
/// a weighted deficit round robin, one pending entry per account.
///
/// - Coalescing keeps `t_rx_first`, takes the newest `t_rx`/slot/trigger and the newest
///   revision; a stronger trigger promotes, only a strictly newer canonical revision may
///   demote (never age alone).
/// - Work-conserving: an empty lane yields its turn at once; L2 is starvation-free (a pending
///   L2 entry is served within one WDRR round once it is first in its lane).
/// - Bounded by the account population (one entry per account); nothing is ever dropped.
#[cfg(feature = "crate_health")]
pub(crate) struct CandidateLanes {
    /// D01: the WDRR start weights of L0/L1/L2 (`lane_weight_l0..l2`; P6-MT.5 provisional).
    weights: [u64; LANES],
    pending: HashMap<Pubkey, PendingCandidate>,
    order: [std::collections::BTreeSet<(u64, u64, Pubkey)>; LANES],
    /// Every pending entry by `t_rx_first`: the age of the oldest unserved trigger (MT0 backlog).
    by_first: std::collections::BTreeSet<(u64, Pubkey)>,
    seq: u64,
    credit: [u64; LANES],
    turn: usize,
    pub(crate) stats: LaneStats,
}

#[cfg(feature = "crate_health")]
impl Default for CandidateLanes {
    /// The lanes with the built-in default weights.
    fn default() -> Self {
        let l = darkbot_solana::runtime_config::Limits::default();
        Self::with_weights([l.lane_weight_l0, l.lane_weight_l1, l.lane_weight_l2])
    }
}

#[cfg(feature = "crate_health")]
impl CandidateLanes {
    /// The lanes with their WDRR start weights (each at least 1, validated by `Limits`).
    pub(crate) fn with_weights(weights: [u64; LANES]) -> Self {
        Self {
            weights,
            pending: HashMap::new(),
            order: Default::default(),
            by_first: Default::default(),
            seq: 0,
            credit: [0; LANES],
            turn: 0,
            stats: Default::default(),
        }
    }
    pub(crate) fn weights(&self) -> [u64; LANES] {
        self.weights
    }
    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    /// `t_rx_first` of the oldest unserved trigger (MT0 backlog age).
    pub(crate) fn oldest_t_rx_first(&self) -> Option<u64> {
        self.by_first.first().map(|(t, _)| *t)
    }
    pub(crate) fn lane_len(&self, lane: Lane) -> usize {
        self.order[lane.idx()].len()
    }
    #[cfg(test)]
    pub(crate) fn get(&self, account: &Pubkey) -> Option<&PendingCandidate> {
        self.pending.get(account)
    }

    fn unlink(&mut self, entry: &PendingCandidate) {
        self.order[entry.lane.idx()].remove(&entry.key());
        self.by_first.remove(&(entry.t_rx_first, entry.work.0));
    }
    fn link(&mut self, entry: PendingCandidate) {
        self.order[entry.lane.idx()].insert(entry.key());
        self.by_first.insert((entry.t_rx_first, entry.work.0));
        self.pending.insert(entry.work.0, entry);
    }

    /// Admits one trigger: a new entry, or the coalescing of the account's pending entry.
    pub(crate) fn admit(&mut self, account: Pubkey, a: LaneAdmission) -> CandidateQueueAdmission {
        self.admit_from(account, a, a.t_rx_ns)
    }

    /// `admit` of a trigger whose first unserved receive time is `t_rx_first` (an ownership
    /// re-arm keeps the deferred trigger's first time — P6-MT.5 coalescing keeps `t_rx_first`).
    pub(crate) fn admit_from(
        &mut self,
        account: Pubkey,
        a: LaneAdmission,
        t_rx_first: u64,
    ) -> CandidateQueueAdmission {
        let first = t_rx_first.min(a.t_rx_ns);
        let work = (account, a.t_rx_ns, a.slot, a.trigger);
        if let Some(existing) = self.pending.get(&account).copied() {
            // [FIX-137] Single-flight latest-state semantics: the queue token already exists,
            // so carry the newest causal work into that token instead of allocating/deduping
            // another one. Downstream reads canonical state again.
            self.unlink(&existing);
            let newer_revision = a.revision > existing.revision;
            // P6-MT.5: a newer canonical revision may promote or demote; the same (or an
            // unknown) revision may only promote — never a demotion by age alone.
            let lane = if newer_revision {
                a.lane
            } else {
                existing.lane.min(a.lane)
            };
            if lane < existing.lane {
                self.stats.promotions = self.stats.promotions.saturating_add(1);
            } else if lane > existing.lane {
                self.stats.demotions = self.stats.demotions.saturating_add(1);
            }
            // V2-11: an older trigger (slot, then receive time) never replaces newer work.
            let work = if (a.slot, a.t_rx_ns) < (existing.work.2, existing.work.1) {
                existing.work
            } else {
                work
            };
            self.link(PendingCandidate {
                work,
                t_rx_first: existing.t_rx_first.min(first),
                lane,
                revision: existing.revision.max(a.revision),
                // The L2 key stays the earliest: an unserved entry is never pushed back by a
                // later repeat (FIX-135 "oldest last forwarded first", as the FIFO never did).
                last_forwarded_slot: existing.last_forwarded_slot.min(a.last_forwarded_slot),
                seq: existing.seq,
            });
            CandidateQueueAdmission::Coalesced
        } else {
            self.seq = self.seq.wrapping_add(1);
            self.link(PendingCandidate {
                work,
                t_rx_first: first,
                lane: a.lane,
                revision: a.revision,
                last_forwarded_slot: a.last_forwarded_slot,
                seq: self.seq,
            });
            CandidateQueueAdmission::Enqueued
        }
    }

    /// The next entry by weighted deficit round robin (`weights`): a lane with
    /// work is served up to its weight per round, an empty lane yields its turn (and its unused
    /// credit) at once, so every non-empty lane is reached within one round.
    pub(crate) fn next(&mut self) -> Option<PendingCandidate> {
        if self.pending.is_empty() {
            return None;
        }
        // At most one full rotation finds a non-empty lane: every lane is visited once.
        for _ in 0..=LANES {
            let lane = self.turn;
            let Some(key) = self.order[lane].first().copied() else {
                self.credit[lane] = 0;
                self.turn = (lane + 1) % LANES;
                continue;
            };
            if self.credit[lane] == 0 {
                self.credit[lane] = self.weights[lane];
            }
            self.credit[lane] -= 1;
            if self.credit[lane] == 0 {
                self.turn = (lane + 1) % LANES;
            }
            let entry = self.pending.remove(&key.2)?;
            self.unlink(&entry);
            return Some(entry);
        }
        None
    }
}

/// Admits one sweep candidate; counts the physical enqueue / coalescing and the peak backlog.
#[cfg(feature = "crate_health")]
pub(crate) fn enqueue_or_coalesce_candidate(
    account: Pubkey,
    admission: LaneAdmission,
    lanes: &mut CandidateLanes,
    enqueued: &mut u64,
    coalesced: &mut u64,
    max_pending: &mut usize,
) -> CandidateQueueAdmission {
    let first = admission.t_rx_ns;
    enqueue_or_coalesce_candidate_from(
        account,
        (admission, first),
        lanes,
        enqueued,
        coalesced,
        max_pending,
    )
}

/// `enqueue_or_coalesce_candidate` with the trigger's first unserved receive time.
#[cfg(feature = "crate_health")]
pub(crate) fn enqueue_or_coalesce_candidate_from(
    account: Pubkey,
    (admission, t_rx_first): (LaneAdmission, u64),
    lanes: &mut CandidateLanes,
    enqueued: &mut u64,
    coalesced: &mut u64,
    max_pending: &mut usize,
) -> CandidateQueueAdmission {
    let outcome = lanes.admit_from(account, admission, t_rx_first);
    match outcome {
        CandidateQueueAdmission::Coalesced => *coalesced = coalesced.saturating_add(1),
        CandidateQueueAdmission::Enqueued => {
            *enqueued = enqueued.saturating_add(1);
            *max_pending = (*max_pending).max(lanes.len());
        }
    }
    outcome
}

/// Admits every distinct account of an oracle trigger (deterministic pubkey order) to `lane`;
/// `last_forwarded_slot` gives each account's L2 order key. Returns the distinct count.
#[cfg(feature = "crate_health")]
pub(crate) fn enqueue_oracle_accounts(
    accounts: impl IntoIterator<Item = Pubkey>,
    (t_rx_ns, event_slot, trigger): (u64, u64, &'static str),
    lane: Lane,
    last_forwarded_slot: impl Fn(&Pubkey) -> u64,
    lanes: &mut CandidateLanes,
    (enqueued, coalesced, max_pending): (&mut u64, &mut u64, &mut usize),
) -> usize {
    let mut unique = HashSet::new();
    unique.extend(accounts);
    let count = unique.len();
    let mut accounts: Vec<Pubkey> = unique.into_iter().collect();
    accounts.sort_unstable_by_key(|pk| pk.to_bytes());
    for account in accounts {
        let admission = LaneAdmission {
            t_rx_ns,
            slot: event_slot,
            trigger,
            lane,
            revision: 0,
            last_forwarded_slot: last_forwarded_slot(&account),
        };
        match lanes.admit(account, admission) {
            CandidateQueueAdmission::Coalesced => *coalesced = coalesced.saturating_add(1),
            CandidateQueueAdmission::Enqueued => *enqueued = enqueued.saturating_add(1),
        }
    }
    *max_pending = (*max_pending).max(lanes.len());
    count
}

#[cfg(all(test, feature = "crate_health"))]
mod mt3_lane_tests {
    use super::*;

    fn adm(t_rx_ns: u64, lane: Lane, revision: u64, last_forwarded_slot: u64) -> LaneAdmission {
        LaneAdmission {
            t_rx_ns,
            slot: 1,
            trigger: "oracle_health_ready",
            lane,
            revision,
            last_forwarded_slot,
        }
    }

    /// P6-MT.14 #10: a backlog far beyond the repeat soft-cap and the ingress capacity is never
    /// dropped — every admitted account is dispatched exactly once (one entry per account,
    /// coalesced), and the peak is the distinct population.
    #[test]
    fn mt3_10_queue_full_loses_no_candidate() {
        let mut lanes = CandidateLanes::default();
        let (mut enqueued, mut coalesced, mut max_pending) = (0u64, 0u64, 0usize);
        let accounts: Vec<Pubkey> = (0..5_000).map(|_| Pubkey::new_unique()).collect();
        for (i, account) in accounts.iter().enumerate() {
            let lane = LANE_LIST[i % LANES];
            enqueue_or_coalesce_candidate(
                *account,
                adm(i as u64, lane, 1, i as u64),
                &mut lanes,
                &mut enqueued,
                &mut coalesced,
                &mut max_pending,
            );
        }
        // A second wave over the same population coalesces, never duplicates or drops.
        let second = enqueue_oracle_accounts(
            accounts.iter().copied(),
            (10_000, 2, "oracle_threshold"),
            Lane::L2,
            |_| 0,
            &mut lanes,
            (&mut enqueued, &mut coalesced, &mut max_pending),
        );
        assert_eq!((second, enqueued, coalesced), (5_000, 5_000, 5_000));
        assert_eq!((lanes.len(), max_pending), (5_000, 5_000));
        let mut served = HashSet::new();
        while let Some(entry) = lanes.next() {
            assert!(served.insert(entry.work.0), "served twice");
            assert_eq!(entry.work.1, 10_000, "newest t_rx");
        }
        assert_eq!(served.len(), 5_000);
        assert!(served.iter().all(|a| accounts.contains(a)));
        assert!(lanes.is_empty() && lanes.oldest_t_rx_first().is_none());
    }

    /// P6-MT.14 #21: one L0 admission behind a large L2 backlog is the next dispatch (and after
    /// an L2 dispatch was served, still within one WDRR round: L2's weight is 1).
    #[test]
    fn mt3_21_l0_overtakes_a_large_l2_backlog() {
        let mut lanes = CandidateLanes::default();
        for i in 0..1_000u64 {
            lanes.admit(Pubkey::new_unique(), adm(i, Lane::L2, 1, i));
        }
        let urgent = Pubkey::new_unique();
        lanes.admit(urgent, adm(5_000, Lane::L0, 7, 0));
        let first = lanes.next().unwrap();
        assert_eq!((first.work.0, first.lane), (urgent, Lane::L0));
        // L2 is being served; a new L0 entry waits for at most L2's single dispatch.
        let served = lanes.next().unwrap();
        assert_eq!(served.lane, Lane::L2);
        let urgent2 = Pubkey::new_unique();
        lanes.admit(urgent2, adm(6_000, Lane::L0, 7, 0));
        let dispatches: Vec<Pubkey> = (0..2)
            .filter_map(|_| lanes.next())
            .map(|e| e.work.0)
            .collect();
        assert!(dispatches.contains(&urgent2));
        // Within L0: the earliest `t_rx_first` first, whatever the admission order.
        let (late, early) = (Pubkey::new_unique(), Pubkey::new_unique());
        lanes.admit(late, adm(9_000, Lane::L0, 7, 0));
        lanes.admit(early, adm(8_000, Lane::L0, 7, 0));
        while lanes.next().is_some_and(|e| e.lane != Lane::L0) {}
        assert_eq!(lanes.next().unwrap().work.0, late);
    }

    /// P6-MT.14 #22: under sustained L0 and L1 load (refilled after every dispatch) an L2 entry
    /// is served within one WDRR round (8 + 3 + 1 dispatches), and the long-run share follows
    /// the 8:3:1 start weights; an empty lane yields its turn (work-conserving).
    #[test]
    fn mt3_22_l2_stays_starvation_free_under_sustained_l0_l1_load() {
        let mut lanes = CandidateLanes::default();
        let refill = |lanes: &mut CandidateLanes, t: u64| {
            while lanes.lane_len(Lane::L0) < 4 {
                lanes.admit(Pubkey::new_unique(), adm(t, Lane::L0, 1, 0));
            }
            while lanes.lane_len(Lane::L1) < 4 {
                lanes.admit(Pubkey::new_unique(), adm(t, Lane::L1, 1, 0));
            }
        };
        let round: u64 = lanes.weights().iter().sum();
        let mut worst_wait = 0u64;
        let mut served = [0u64; LANES];
        let mut t = 0u64;
        for _ in 0..200 {
            refill(&mut lanes, t);
            let waiting = Pubkey::new_unique();
            lanes.admit(waiting, adm(t, Lane::L2, 1, 0));
            let mut dispatches = 0u64;
            loop {
                refill(&mut lanes, t);
                let entry = lanes.next().expect("work pending");
                served[entry.lane as usize] += 1;
                dispatches += 1;
                t += 1;
                if entry.work.0 == waiting {
                    break;
                }
                assert!(
                    dispatches <= round,
                    "L2 starved for {dispatches} dispatches"
                );
            }
            worst_wait = worst_wait.max(dispatches);
        }
        assert!(
            worst_wait <= round,
            "worst wait {worst_wait} > one round {round}"
        );
        assert!(served[0] > served[1] && served[1] > served[2]);
        assert_eq!(served[2], 200);
        // Work-conserving: with L0 and L1 empty, L2 is served back to back.
        while lanes.next().is_some() {}
        for i in 0..20u64 {
            lanes.admit(Pubkey::new_unique(), adm(i, Lane::L2, 1, i));
        }
        let only_l2 = (0..20)
            .map(|_| lanes.next().unwrap().lane)
            .all(|l| l == Lane::L2);
        assert!(only_l2 && lanes.is_empty());
    }

    /// P6-MT.14 #23: coalescing keeps `t_rx_first` (and the earliest L2 key), takes the newest
    /// `t_rx`/slot/trigger and the newest revision.
    #[test]
    fn mt3_23_coalescing_keeps_t_rx_first_and_takes_the_newest_work() {
        let a = Pubkey::new_unique();
        let mut lanes = CandidateLanes::default();
        lanes.admit(a, adm(100, Lane::L1, 3, 10));
        assert_eq!(
            lanes.admit(
                a,
                LaneAdmission {
                    t_rx_ns: 250,
                    slot: 12,
                    trigger: "oracle_health_error",
                    lane: Lane::L1,
                    revision: 4,
                    last_forwarded_slot: 11,
                },
            ),
            CandidateQueueAdmission::Coalesced
        );
        let e = *lanes.get(&a).unwrap();
        assert_eq!(e.work, (a, 250, 12, "oracle_health_error"));
        assert_eq!(
            (e.t_rx_first, e.revision, e.last_forwarded_slot),
            (100, 4, 10)
        );
        assert_eq!(lanes.oldest_t_rx_first(), Some(100));
        assert_eq!((lanes.len(), lanes.lane_len(Lane::L1)), (1, 1));
        assert_eq!(
            lanes.next().unwrap().work.1,
            250,
            "FIX-21: newest t_rx to the detect"
        );
    }

    /// D01: the WDRR start weights are configured, not literals — with 1:1:1 every lane is served
    /// once per round.
    #[test]
    fn d01_lane_weights_come_from_the_configuration() {
        let mut lanes = CandidateLanes::with_weights([1, 1, 1]);
        assert_eq!(lanes.weights(), [1, 1, 1]);
        for (lane, t) in [(Lane::L0, 1), (Lane::L0, 2), (Lane::L1, 3), (Lane::L2, 4)] {
            lanes.admit(Pubkey::new_unique(), adm(t, lane, 1, 0));
        }
        let served: Vec<_> = std::iter::from_fn(|| lanes.next().map(|p| p.lane)).collect();
        assert_eq!(served, vec![Lane::L0, Lane::L1, Lane::L2, Lane::L0]);
        assert_eq!(CandidateLanes::default().weights(), [8, 3, 1]);
    }

    /// V2-13: the repeat gate — a first admission and a newer revision pass; the same slot and
    /// revision is suppressed; a due repeat passes only with soft-cap room (else counted
    /// soft-cap suppressed); a forced re-arm always passes and is consumed.
    #[test]
    fn v2_13_repeat_gate_passes_new_causes_and_due_repeats_only() {
        let a = Pubkey::new_unique();
        let (mut cache, mut forced) = (HashMap::new(), HashSet::new());
        let (mut suppressed, mut softcap) = (0u64, 0u64);
        let mut pass = |slot, revision, capacity, cache: &mut _, forced: &mut _| {
            RepeatGate {
                cache,
                forced,
                recheck_slots: 4,
                repeat_capacity: capacity,
                suppressed: &mut suppressed,
                softcap_suppressed: &mut softcap,
            }
            .pass([a].into(), slot, revision)
            .contains_key(&a)
        };
        assert!(pass(10, 1, true, &mut cache, &mut forced), "first");
        assert!(!pass(11, 1, true, &mut cache, &mut forced), "no new cause");
        assert!(pass(11, 2, true, &mut cache, &mut forced), "newer revision");
        assert!(!pass(15, 2, false, &mut cache, &mut forced), "due, no room");
        assert!(pass(15, 2, true, &mut cache, &mut forced), "due repeat");
        forced.insert(a);
        assert!(pass(15, 2, false, &mut cache, &mut forced), "forced");
        assert!(forced.is_empty());
        assert_eq!((suppressed, softcap), (2, 1));
    }

    /// V2-11 (P5b-05): an older trigger coalesced into a pending entry never replaces its newer
    /// work (slot, then receive time) — `max_opportunity_age_ns` measures the newest cause; the
    /// first receive time still goes back to the oldest.
    #[test]
    fn v2_11_an_older_trigger_never_replaces_newer_work() {
        let a = Pubkey::new_unique();
        let mut lanes = CandidateLanes::default();
        let at = |t_rx_ns, slot| LaneAdmission {
            t_rx_ns,
            slot,
            trigger: "bank_projection_fallback",
            lane: Lane::L1,
            revision: 4,
            last_forwarded_slot: 0,
        };
        lanes.admit(a, at(250, 12));
        assert_eq!(
            lanes.admit(a, at(100, 10)),
            CandidateQueueAdmission::Coalesced
        );
        let e = *lanes.get(&a).unwrap();
        assert_eq!((e.work.1, e.work.2), (250, 12));
        assert_eq!(e.t_rx_first, 100);
    }

    /// P6-MT.14 #39: only a strictly newer canonical revision may demote (L0 → L1 here); the
    /// same or an unknown revision never demotes (no age-based demotion) but may promote.
    #[test]
    fn mt3_39_a_newer_revision_may_demote_the_same_revision_never() {
        let a = Pubkey::new_unique();
        let mut lanes = CandidateLanes::default();
        lanes.admit(a, adm(100, Lane::L0, 5, 0));
        // Same revision, weaker class: stays L0.
        lanes.admit(a, adm(200, Lane::L2, 5, 0));
        assert_eq!(lanes.get(&a).unwrap().lane, Lane::L0);
        // Unknown revision (threshold trigger), weaker class: stays L0.
        lanes.admit(a, adm(300, Lane::L2, 0, 0));
        assert_eq!(lanes.get(&a).unwrap().lane, Lane::L0);
        assert_eq!(lanes.stats, LaneStats::default());
        // Strictly newer revision, weaker class: demoted to L1, `t_rx_first` kept.
        lanes.admit(a, adm(400, Lane::L1, 6, 0));
        let e = *lanes.get(&a).unwrap();
        assert_eq!((e.lane, e.t_rx_first, e.revision), (Lane::L1, 100, 6));
        assert_eq!((lanes.lane_len(Lane::L0), lanes.lane_len(Lane::L1)), (0, 1));
        // Newer revision, same class: no lane change counted.
        lanes.admit(a, adm(450, Lane::L1, 7, 0));
        // A stronger class with an unknown revision promotes back to L0.
        lanes.admit(a, adm(500, Lane::L0, 0, 0));
        let e = *lanes.get(&a).unwrap();
        assert_eq!((e.lane, e.revision), (Lane::L0, 7));
        assert_eq!(
            lanes.stats,
            LaneStats {
                promotions: 1,
                demotions: 1,
                cancellations: 0
            }
        );
        assert_eq!(lanes.next().unwrap().lane, Lane::L0);
        assert!(lanes.is_empty());
    }
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

    /// One account update of the E9/V2-06 fixtures (owner `[3; 32]`, no write version).
    fn update(
        pubkey: Pubkey,
        slot: u64,
        lamports: u64,
        byte: u8,
    ) -> darkbot_solana::ingress::AccountUpdate {
        darkbot_solana::ingress::AccountUpdate {
            pubkey,
            owner: Pubkey::new_from_array([3; 32]),
            lamports,
            data: vec![byte; 8],
            slot,
            write_version: None,
            source: darkbot_solana::ingress::SourceKind::WsRpcControl,
            t_rx_ns: 0,
        }
    }

    /// V2-06 (P5b-01/P5b-04): the fingerprint binds each dependency's revision epoch on this
    /// `StateCore` instance instead of hashing its data. A same-slot write with equal data
    /// (lamports 1→2) changes it (before: equal), a same-slot data write changes it, an unrelated
    /// write keeps it, and a snapshot install of identical content re-epochs every record — no
    /// earlier fingerprint matches again (before: equal).
    #[test]
    fn v2_06_fingerprint_follows_the_revision_epoch_of_one_state() {
        use darkbot_solana::ingress::IngressEvent;
        let (bank, oracle, unrelated) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let mut deps = vec![bank, oracle];
        deps.sort_unstable_by_key(|k| k.to_bytes());
        let mut st = StateCore::new(1);
        st.install_snapshot(
            10,
            vec![
                update(bank, 10, 1, 2),
                update(oracle, 10, 1, 3),
                update(unrelated, 10, 1, 4),
            ],
        );
        let fp = dependency_fingerprint(&st, &deps);
        st.on_event(IngressEvent::Account(update(unrelated, 10, 1, 9)));
        assert_eq!(dependency_fingerprint(&st, &deps), fp, "unrelated write");
        st.on_event(IngressEvent::Account(update(oracle, 10, 2, 3)));
        let lamports = dependency_fingerprint(&st, &deps);
        assert_ne!(lamports, fp, "same slot, same data, lamports 1→2");
        st.on_event(IngressEvent::Account(update(bank, 10, 1, 7)));
        let data = dependency_fingerprint(&st, &deps);
        assert_ne!(data, lamports, "same slot, new data");
        st.install_snapshot(
            10,
            vec![
                update(bank, 10, 1, 7),
                update(oracle, 10, 2, 3),
                update(unrelated, 10, 1, 9),
            ],
        );
        let reinstalled = dependency_fingerprint(&st, &deps);
        assert!(
            ![fp, lamports, data].contains(&reinstalled),
            "a snapshot install invalidates every earlier fingerprint"
        );
    }

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
        let deps = projection_dependencies(&candidate, &[refresh], &[bank, oracle], &[]);
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
        // V2-06: revisions are written in place on one `StateCore` (epochs are instance-local).
        let write = |st: &mut StateCore, k: Pubkey, (slot, data): (u64, Vec<u8>)| {
            st.on_event(darkbot_solana::ingress::IngressEvent::Account(update(
                k, slot, 1, data[0],
            )));
        };
        let mut st = install(&base);
        let fp = dependency_fingerprint(&st, &deps);
        write(&mut st, candidate, account(11, 9));
        write(&mut st, unrelated, account(12, 9));
        assert_eq!(dependency_fingerprint(&st, &deps), fp);
        for (k, replacement) in [(bank, account(10, 7)), (oracle, account(11, 3))] {
            let before = dependency_fingerprint(&st, &deps);
            write(&mut st, k, replacement);
            assert_ne!(dependency_fingerprint(&st, &deps), before, "{k}");
        }
        let before = dependency_fingerprint(&st, &deps);
        st.install_snapshot(
            10,
            base[..2]
                .iter()
                .map(|(k, (slot, data))| update(*k, *slot, 1, data[0]))
                .collect(),
        );
        assert_ne!(dependency_fingerprint(&st, &deps), before, "missing oracle");
        // E9: the oldest acceptable simulation context is the newest canonical slot of the
        // candidate's request and its dependencies; unrelated and missing accounts do not count.
        let mut newer = base.clone();
        newer[2].1 = account(14, 3);
        newer[3].1 = account(20, 4);
        assert_eq!(required_context_slot(&install(&newer), 10, &deps), 14);
        assert_eq!(required_context_slot(&install(&base), 12, &deps), 12);
        assert_eq!(required_context_slot(&install(&base[..1]), 10, &deps), 10);
    }
}

#[cfg(all(test, feature = "crate_health"))]
mod n3_n4_tests {
    use super::*;
    use marginfi_type_crate::types::{
        Bank, BankOperationalState, MarginfiAccount, MarginfiGroup, OracleSetup, WrappedI80F48,
    };

    struct Source(HashMap<Pubkey, (Pubkey, Vec<u8>)>);
    impl AccountSource for Source {
        fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            self.0.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
        }
    }

    /// N3: the projection dependency set contains the refresh accounts, the candidate's
    /// observation accounts, the group and the FeeState; with a missing or an invalid bank the
    /// observation set is unknown and the set is an error — never a smaller set.
    #[test]
    fn n3_unknown_observation_set_is_no_dependency_set() {
        let program: Pubkey = protocol::PROGRAM_ID_MAINNET.parse().unwrap();
        let program_data = Pubkey::new_unique();
        let (group, candidate, bank, reserve) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let one = |v: i128| WrappedI80F48 {
            value: (v << 48).to_le_bytes(),
        };
        let wrap = |disc: [u8; 8], bytes: &[u8]| [disc.as_slice(), bytes].concat();
        let mut b: Bank = bytemuck::Zeroable::zeroed();
        b.group = group;
        b.mint_decimals = 6;
        b.asset_share_value = one(1);
        b.config.operational_state = BankOperationalState::Operational;
        b.config.oracle_setup = OracleSetup::Fixed;
        b.config.fixed_price = one(1);
        let mut a: MarginfiAccount = bytemuck::Zeroable::zeroed();
        a.group = group;
        a.lending_account.balances[0].active = 1;
        a.lending_account.balances[0].bank_pk = bank;
        a.lending_account.balances[0].asset_shares = one(10);
        let refresh = solana_sdk::instruction::Instruction {
            program_id: Pubkey::new_unique(),
            accounts: vec![solana_sdk::instruction::AccountMeta::new(reserve, false)],
            data: vec![],
        };
        let state = |bank_data: Option<Vec<u8>>| {
            let mut m = HashMap::from([
                (
                    candidate,
                    (
                        program,
                        wrap(protocol::DISC_MARGINFI_ACCOUNT, bytemuck::bytes_of(&a)),
                    ),
                ),
                (
                    group,
                    (
                        program,
                        wrap(
                            protocol::DISC_MARGINFI_GROUP,
                            bytemuck::bytes_of(&<MarginfiGroup as bytemuck::Zeroable>::zeroed()),
                        ),
                    ),
                ),
            ]);
            if let Some(d) = bank_data {
                m.insert(bank, (program, d));
            }
            Source(m)
        };
        let set = |src: &Source| {
            projection_dependency_set(
                src,
                &program,
                &program_data,
                &group,
                &candidate,
                std::slice::from_ref(&refresh),
            )
        };
        let valid = set(&state(Some(wrap(
            protocol::DISC_BANK,
            bytemuck::bytes_of(&b),
        ))))
        .unwrap();
        // E9: the ProgramData is a dependency — a program upgrade makes the projection stale.
        for k in [
            bank,
            reserve,
            group,
            protocol::fee_state_pda(&program),
            program_data,
        ] {
            assert!(valid.contains(&k), "{k}");
        }
        assert!(!valid.contains(&candidate));
        assert!(set(&state(None)).is_err(), "missing bank");
        assert!(set(&state(Some(vec![1; 64]))).is_err(), "invalid bank");
    }

    /// N6: repeated identical gaps are reported once, a changed gap (other code or detail) again,
    /// a cleared gap once and then nothing; the state holds only banks with a current gap.
    #[test]
    fn n6_dependency_gaps_are_reported_on_change_only() {
        use darkbot_project0::health::DependencyGap as G;
        let mut seen = HashMap::new();
        let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
        let reported = (0..100)
            .filter(|_| note_gap(&mut seen, a, Some(&G::Missing)))
            .count();
        assert_eq!(reported, 1);
        assert!(note_gap(&mut seen, a, Some(&G::NotABank)), "other code");
        assert!(!note_gap(&mut seen, a, Some(&G::NotABank)));
        let x = G::UnsupportedOracle("X".into());
        let y = G::UnsupportedOracle("Y".into());
        assert!(note_gap(&mut seen, a, Some(&x)));
        assert!(note_gap(&mut seen, a, Some(&y)), "same code, other detail");
        assert!(note_gap(&mut seen, b, Some(&G::Missing)), "per bank");
        assert_eq!(seen.len(), 2);
        assert!(note_gap(&mut seen, a, None), "cleared once");
        assert!(!note_gap(&mut seen, a, None));
        assert!(
            !note_gap(&mut seen, Pubkey::new_unique(), None),
            "never had a gap"
        );
        assert_eq!(seen.len(), 1, "bounded by banks with a current gap");
    }

    /// N4: every projection outcome lands in exactly one counter, so the report balances:
    /// requested = accepted + failed + stale_result + discarded_result + in_flight and
    /// accepted = reused + discarded_consume + stale_consume + dropped + cached.
    #[test]
    fn n4_projection_outcomes_are_counted_once() {
        let mut p = ProjectionStats {
            requested: 7,
            queue_full: 2,
            observation_unknown: 1,
            ..Default::default()
        };
        // Results of six requests (one still in flight).
        for (base, deps) in [(true, true), (true, true), (true, true), (true, true)] {
            assert!(p.result(base, deps));
        }
        assert!(!p.result(false, true), "candidate changed");
        assert!(!p.result(true, false), "dependency changed");
        // Uses of the four accepted ones: reused, E9-discarded, candidate changed, replaced;
        // none remains cached; a taken one is reused only once its overlay is built.
        assert!(p.consume(true, true));
        assert_eq!(p.reused, 0, "not yet used");
        p.used(true);
        assert!(!p.consume(true, false));
        assert!(!p.consume(false, false));
        p.dropped_if(Some(()));
        p.dropped_if(None::<()>);
        let (in_flight, cached) = (1u64, 0u64);
        assert_eq!(
            p.requested,
            p.accepted + p.failed + p.stale_at_result + p.discarded_at_result + in_flight
        );
        assert_eq!(
            p.accepted,
            p.reused
                + p.invalid_at_use
                + p.discarded_at_consume
                + p.stale_at_consume
                + p.dropped
                + cached
        );
        assert_eq!(
            p.line(1, 0),
            "projection: requested=7 queue_full=2 observation_unknown=1 accepted=4 failed=0 \
             stale_result=1 discarded_result=1 in_flight=1 reused=1 invalid_at_use=0 \
             discarded_consume=1 stale_consume=1 dropped=1 cached=0 requeued=0 \
             requeue_exhausted=0 deferred=0"
        );
    }

    /// E9: projection re-requests no new trigger asked for are bounded per external trigger: the
    /// consumption of an unusable cached projection re-requests at most `projection_requeue_max`
    /// times (then counted and refused), a queued requeue (already counted) proceeds, and an
    /// external trigger starts a fresh sequence.
    #[test]
    fn e9_projection_rerequests_are_bounded_per_external_trigger() {
        let mut requeues = HashMap::new();
        let mut stats = ProjectionStats::default();
        let a = Pubkey::new_unique();
        let max = darkbot_solana::runtime_config::Limits::default().projection_requeue_max as u8;
        let mut allowed =
            |trigger| projection_request_allowed(&mut requeues, &mut stats, a, trigger, max);
        assert!(allowed("account_update"));
        for _ in 0..max {
            assert!(allowed("integration_projection"));
        }
        assert!(!allowed("integration_projection"), "bound reached");
        assert!(allowed("integration_projection_requeue"));
        // After the bound the sequence ended; a new external trigger starts a fresh one.
        assert!(allowed("oracle_price"));
        assert!(allowed("integration_projection"));
        assert_eq!(stats.requeued, u64::from(max) + 1);
        assert_eq!(stats.requeue_exhausted, 1);
        // An external trigger resets an unfinished sequence too.
        assert!(projection_request_allowed(
            &mut requeues,
            &mut stats,
            a,
            "account_update",
            max
        ));
        assert!(!requeues.contains_key(&a));
    }

    /// E9: a projection simulated at a slot is consumable only once the owner processed the
    /// account notifications up to it (watermark or root); parked work is released in slot
    /// order, at most one entry per candidate, superseded by a new park or an unpark.
    #[test]
    fn e9_parked_projection_work_waits_for_the_processed_slot() {
        let account = |slot: u64| {
            darkbot_solana::ingress::IngressEvent::Account(darkbot_solana::ingress::AccountUpdate {
                pubkey: Pubkey::new_unique(),
                owner: Pubkey::new_unique(),
                lamports: 1,
                data: vec![1],
                slot,
                write_version: None,
                source: darkbot_solana::ingress::SourceKind::WsRpcControl,
                t_rx_ns: 0,
            })
        };
        let ready = |lower: u64| {
            let mut st = StateCore::new(16);
            st.install_snapshot(lower, vec![]);
            st.reconcile(lower, &[]);
            st.confirm_ready();
            st
        };
        // Not tradable (snapshot not reconciled): nothing is processed.
        let mut unready = StateCore::new(16);
        unready.install_snapshot(10, vec![]);
        assert!(!processed_through(&unready, 1));
        let mut st = ready(10);
        assert!(
            processed_through(&st, 10),
            "the snapshot covers its lower bound"
        );
        assert!(!processed_through(&st, 11));
        // A notification of slot 12 proves 11 complete, not 12 (same-slot pass may continue).
        st.on_event(account(12));
        assert!(processed_through(&st, 11));
        assert!(!processed_through(&st, 12));
        // A (processed-level) slot event ahead of the confirmed notifications proves nothing.
        st.on_event(darkbot_solana::ingress::IngressEvent::Slot(
            darkbot_solana::ingress::SlotUpdate {
                slot: 40,
                parent: Some(39),
                status: darkbot_solana::ingress::SlotStatus::Processed,
                source: darkbot_solana::ingress::SourceKind::WsRpcControl,
                t_rx_ns: 0,
            },
        ));
        assert!(!processed_through(&st, 12));
        let (a, b, c) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let mut parked = ParkedProjections::default();
        parked.park(13, (a, 1, 13, TRIGGER_INTEGRATION_PROJECTION));
        parked.park(12, (b, 2, 12, TRIGGER_PROJECTION_REQUEUE));
        parked.park(14, (c, 3, 14, TRIGGER_INTEGRATION_PROJECTION));
        // A new park replaces the candidate's entry; an unpark drops it.
        parked.park(15, (a, 4, 15, TRIGGER_INTEGRATION_PROJECTION));
        parked.unpark(&c);
        assert_eq!(parked.len(), 2);
        assert!(parked.release(&st).is_empty());
        st.on_event(account(13));
        assert_eq!(
            parked.release(&st),
            [(b, 2, 12, TRIGGER_PROJECTION_REQUEUE)]
        );
        st.on_event(account(20));
        assert_eq!(
            parked.release(&st),
            [(a, 4, 15, TRIGGER_INTEGRATION_PROJECTION)]
        );
        assert_eq!(parked.len(), 0);
        // A gap (reconnect) leaves the stream incomplete until the next reconciled snapshot.
        st.on_event(darkbot_solana::ingress::IngressEvent::Gap {
            source: darkbot_solana::ingress::SourceKind::WsRpcControl,
            from_slot: Some(20),
            to_slot: None,
            t_rx_ns: 0,
        });
        assert!(!processed_through(&st, 11));
        // The root reaching a slot proves it processed too.
        let mut rooted = ready(10);
        rooted.on_event(darkbot_solana::ingress::IngressEvent::Root {
            slot: 30,
            source: darkbot_solana::ingress::SourceKind::WsRpcControl,
            t_rx_ns: 0,
        });
        assert!(processed_through(&rooted, 30));
        assert!(!processed_through(&rooted, 31));
        assert!(projection_own_trigger(TRIGGER_INTEGRATION_PROJECTION));
        assert!(projection_own_trigger(TRIGGER_PROJECTION_REQUEUE));
        assert!(!projection_own_trigger("oracle_price"));
    }

    /// V19: a refused layout counts as fail-closed unsupported like an unsupported state; the
    /// other health errors do not.
    #[test]
    fn v19_unknown_layout_counts_as_fail_closed() {
        use darkbot_project0::health::HealthError as H;
        assert!(health_fails_closed(&H::UnknownLayout(
            "UNKNOWN_LAYOUT: x".into()
        )));
        assert!(health_fails_closed(&H::Unsupported(
            "CB_HALTED bank=x".into()
        )));
        for e in [
            H::Stale("x".into()),
            H::IntegrationStale("x".into()),
            H::Crate("x".into()),
            H::MissingAccount(solana_sdk::pubkey::Pubkey::new_unique()),
            H::Unavailable,
        ] {
            assert!(!health_fails_closed(&e), "{e}");
        }
    }

    /// Z13: the bankruptcy reclassification replaces only the "no plan" follow-on causes a
    /// zero-equity account produces (sizing, routing, funding, economics) and keeps their cause
    /// in the reason; an earlier, independent cause (own network budget, state, health,
    /// integration, build …) or a candidate without any loss stays as recorded.
    #[test]
    fn z13_bankruptcy_keeps_independent_causes_and_their_provenance() {
        use crate::analytics::record::LossReason as L;
        let classify = |loss: Option<L>, reason: &str| {
            bankruptcy_classification(loss, Some(reason.to_string()), true, 0)
        };
        for follow_on in [
            L::Sizing,
            L::Routing,
            L::Funding,
            L::Economics,
            L::EconomicSkip,
            L::ThresholdTooHigh,
        ] {
            let (loss, reason) = classify(Some(follow_on), "SIZING:NO_VALID_PROTOCOL_PAIR");
            assert_eq!(loss, Some(L::Bankruptcy), "{follow_on:?}");
            let reason = reason.unwrap();
            assert!(reason.contains("NO_VALID_PROTOCOL_PAIR"), "{reason}");
            assert!(reason.contains(&format!("{follow_on:?}")), "{reason}");
        }
        for independent in [
            L::RpcBudget,
            L::RpcDeadline,
            L::State,
            L::StateRace,
            L::Ingress,
            L::Health,
            L::CircuitBreaker,
            L::IntegrationRefresh,
            L::UnsupportedIntegration,
            L::MissingAta,
            L::Build,
            L::Submission,
            L::Tip,
            L::Unknown,
            L::Bankruptcy,
        ] {
            assert_eq!(
                classify(Some(independent), "original"),
                (Some(independent), Some("original".to_string())),
                "{independent:?}"
            );
        }
        assert_eq!(classify(None, "ready"), (None, Some("ready".to_string())));
        // Not bad debt, or equity left: never reclassified.
        assert_eq!(
            bankruptcy_classification(Some(L::Routing), None, true, 1),
            (Some(L::Routing), None)
        );
        assert_eq!(
            bankruptcy_classification(Some(L::Routing), None, false, 0),
            (Some(L::Routing), None)
        );
    }
}
