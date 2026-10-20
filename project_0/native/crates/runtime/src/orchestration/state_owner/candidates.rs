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
/// EMODE and the FeeState). An undeterminable observation set is an error, never an empty set:
/// without it the projection's staleness cannot be bound (fail-closed).
#[cfg(feature = "crate_health")]
pub(crate) fn projection_dependency_set<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
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
        &[*group, protocol::fee_state_pda(program)],
    ))
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
    let follow_on = matches!(
        loss,
        Some(
            L::Sizing
                | L::Routing
                | L::Funding
                | L::Economics
                | L::EconomicSkip
                | L::ThresholdTooHigh
        )
    );
    if !(bad_debt && equity_assets <= 0 && follow_on) {
        return (loss, reason);
    }
    (
        Some(L::Bankruptcy),
        Some(format!(
            "bad debt with zero equity assets: bankruptcy/no-collateral case, not a normal \
             receivership opportunity (reported as {:?}: {})",
            loss.expect("follow-on cause"),
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
             invalid_at_use={} discarded_consume={} stale_consume={} dropped={} cached={cached}",
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
        for k in [bank, reserve, group, protocol::fee_state_pda(&program)] {
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
             discarded_consume=1 stale_consume=1 dropped=1 cached=0"
        );
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
