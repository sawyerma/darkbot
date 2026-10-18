//! Final market-driven analytics over the existing production evaluation journal.
//! No RPC, price model, candidate selection or execution is implemented here.
use super::*;
use crate::analytics::record::LossReason;
use crate::market_observer::truth::TruthEvent;

/// One canonical market event joined with the Shadow evidence for it; also the typed
/// `ECON_CANONICAL` payload (JFI-FIX-05). Its JSONL view is rendered by `joined_value`.
#[derive(Clone, Debug, Serialize)]
pub struct Joined {
    pub market: TruthEvent,
    pub target_seen: bool,
    pub target_seen_without_slot: bool,
    pub health_liquidatable: Option<bool>,
    pub candidates_created: Option<bool>,
    pub plan_validated: Option<bool>,
    pub built: Option<bool>,
    pub identity_status: &'static str,
    pub our: Option<OpportunityRecord>,
    pub diagnostic: Option<OpportunityRecord>,
}
impl crate::analytics::payload::Payload for Joined {
    const KIND: u16 = journal::kind::ECON_CANONICAL;
}
/// The closed set of join identity outcomes; anything else fails to decode.
const IDENTITY_STATUS: [&str; 6] = [
    "NO_MATCH",
    "AMBIGUOUS_SAME_SLOT_EVENTS",
    "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT",
    "PLAN_IDENTITY_UNKNOWN",
    "MARKET_BANK_PAIR_UNKNOWN",
    "PLAN_TYPE_OR_BANK_PAIR_MISMATCH",
];
/// Wire form of `Joined` (owned status string, mapped back onto the closed set).
#[derive(Deserialize)]
struct JoinedWire {
    market: TruthEvent,
    target_seen: bool,
    target_seen_without_slot: bool,
    health_liquidatable: Option<bool>,
    candidates_created: Option<bool>,
    plan_validated: Option<bool>,
    built: Option<bool>,
    identity_status: String,
    our: Option<OpportunityRecord>,
    diagnostic: Option<OpportunityRecord>,
}
impl<'de> Deserialize<'de> for Joined {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = JoinedWire::deserialize(d)?;
        let identity_status = IDENTITY_STATUS
            .into_iter()
            .find(|k| *k == w.identity_status)
            .ok_or_else(|| serde::de::Error::custom("IDENTITY_STATUS_UNKNOWN"))?;
        Ok(Self {
            market: w.market,
            target_seen: w.target_seen,
            target_seen_without_slot: w.target_seen_without_slot,
            health_liquidatable: w.health_liquidatable,
            candidates_created: w.candidates_created,
            plan_validated: w.plan_validated,
            built: w.built,
            identity_status,
            our: w.our,
            diagnostic: w.diagnostic,
        })
    }
}
/// Historical `canonical_market_shadow.jsonl` row of a typed `ECON_CANONICAL` frame.
pub fn canonical_legacy_view(row: &Joined) -> Value {
    joined_value(row)
}

/// Typed derived columns of one joined row (native report, B7): the same selector, race,
/// loss and economics functions as the `canonical_market_shadow.jsonl` render. Hypothetical
/// Shadow evidence only — never a chain fact, winner or realized PnL.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinedFacts {
    pub identity_status: String,
    pub race_class: String,
    pub local_observed_race_margin_ns: Option<i128>,
    pub loss_reason: String,
    pub loss_provenance: String,
    pub signed: Option<bool>,
    pub simulation_result: Option<String>,
    pub economics_known: Option<bool>,
    /// Executable Shadow net edge: exact plan identity, technical PASS, signed, not handed off
    /// and known economics only; otherwise UNKNOWN.
    pub executable_net_usd_micro: Option<i128>,
    pub required_working_capital_usd_micro: Option<u64>,
    pub required_total_budget_usd_micro: Option<u64>,
    /// `sha256(bincode(record))` of the exact-match evaluation (`our`).
    pub record_digest: Option<[u8; 32]>,
}

/// Content digest of one Shadow evaluation (identity across joins and opportunity rows).
pub fn record_digest(rec: &OpportunityRecord) -> Option<[u8; 32]> {
    crate::analytics::payload::encode(rec)
        .ok()
        .map(|b| solana_sdk::hash::hash(&b).to_bytes())
}

pub fn joined_facts(row: &Joined) -> JoinedFacts {
    let our = row.our.as_ref();
    let costs = our.and_then(|r| r.selected_costs.as_ref());
    JoinedFacts {
        identity_status: row.identity_status.into(),
        race_class: race_class(row).into(),
        local_observed_race_margin_ns: margin(row),
        loss_reason: loss(row).into(),
        loss_provenance: if row.our.is_some() {
            "EXACT_PLAN_MATCH"
        } else if row.diagnostic.is_some() {
            "TARGET_EPISODE_DIAGNOSTIC; executable identity unproven"
        } else {
            "NO_TARGET_EPISODE_RECORD"
        }
        .into(),
        signed: our.map(|r| r.timing.t_sign_ns.is_some()),
        simulation_result: our.and_then(|r| r.simulation_result.clone()),
        economics_known: our.map(|r| r.economics_known),
        executable_net_usd_micro: our
            .filter(|r| technical(r) && r.economics_known)
            .and(costs)
            .and_then(|c| c.net_profit_usd_micro),
        required_working_capital_usd_micro: costs
            .and_then(|c| c.required_working_capital_usd_micro),
        required_total_budget_usd_micro: costs.and_then(CandidateCostRecord::total_budget),
        record_digest: our.and_then(record_digest),
    }
}
fn any_evidence(old: Option<bool>, new: Option<bool>) -> Option<bool> {
    match (old, new) {
        (Some(a), Some(b)) => Some(a || b),
        (a, None) => a,
        (None, b) => b,
    }
}
fn identity_matches(event: &TruthEvent, rec: &OpportunityRecord) -> bool {
    let Some(id) = &rec.market_plan_identity else {
        return false;
    };
    rec.account == event.target
        && id.liquidation_type == event.liquidation_type.as_str()
        && event.asset_bank.as_deref() == Some(id.asset_bank.as_str())
        && event.liability_bank.as_deref() == Some(id.liability_bank.as_str())
}
fn technical(rec: &OpportunityRecord) -> bool {
    rec.timing.t_sign_ns.is_some()
        && rec.simulation_result.as_deref() == Some("PASS")
        && matches!(
            rec.decision,
            Decision::ShadowReady | Decision::ShadowTechnicalReady
        )
        && rec.timing.t_handoff_ns.is_none()
}
fn net(rec: Option<&OpportunityRecord>) -> Option<f64> {
    let rec = rec?;
    if !rec.economics_known {
        return None;
    }
    rec.selected_costs
        .as_ref()?
        .net_profit_usd_micro
        .map(|n| n as f64 / 1e6)
}
fn preferable(new: &OpportunityRecord, old: &OpportunityRecord) -> bool {
    match (technical(new), technical(old)) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => new.timing.t_sign_ns < old.timing.t_sign_ns,
        (false, false) => new.timing.t_rx_ns > old.timing.t_rx_ns,
    }
}
fn race_class(row: &Joined) -> &'static str {
    match row
        .our
        .as_ref()
        .filter(|r| r.timing.t_sign_ns.is_some())
        .and_then(|r| r.sign_slot)
    {
        Some(s) if s < row.market.slot => "PRE_WINNER_SLOT_PROVEN",
        Some(s) if s == row.market.slot => "SAME_SLOT_UNKNOWN",
        Some(_) => "POST_WINNER_SLOT_PROVEN_LATE",
        None => "UNRESOLVED",
    }
}
fn margin(row: &Joined) -> Option<i128> {
    Some(i128::from(row.market.live_t_rx_ns?) - i128::from(row.our.as_ref()?.timing.t_sign_ns?))
}
fn loss(row: &Joined) -> &'static str {
    if !row.market.permissionless_profit_opportunity {
        return "ADMINISTRATIVE_SETTLEMENT";
    }
    if !row.target_seen {
        return if row.target_seen_without_slot {
            "UNKNOWN"
        } else {
            "NOT_DETECTED"
        };
    }
    let rec = row.our.as_ref().or(row.diagnostic.as_ref());
    if let Some(rec) = rec {
        if matches!(
            rec.decision,
            Decision::ShadowSimFailed | Decision::ShadowTechnicalSimFailed
        ) {
            return "SIMULATION";
        }
        if let Some(reason) = rec.loss {
            return match reason {
                LossReason::Health => "HEALTH_ERROR",
                LossReason::StateRace => "STATE_RACE",
                LossReason::MissingAta => "MISSING_ATA",
                LossReason::Routing => "ROUTING",
                LossReason::Funding => "FUNDING",
                LossReason::Bankruptcy => "BANKRUPTCY",
                LossReason::Economics | LossReason::EconomicSkip | LossReason::ThresholdTooHigh => {
                    "ECONOMICS"
                }
                LossReason::Build => "BUILD",
                _ => "UNKNOWN",
            };
        }
        if row.our.is_some() && technical(rec) {
            if !rec.economics_known {
                return "ECONOMICS";
            }
            if race_class(row) == "POST_WINNER_SLOT_PROVEN_LATE" {
                return "WINNER_TOO_EARLY";
            }
            return "NO_PROVEN_TECHNICAL_LOSS";
        }
        if rec.timing.t_ready_ns.is_some() && rec.timing.t_sign_ns.is_none() {
            return "SIGNING";
        }
    }
    "UNKNOWN"
}

impl Worker {
    /// Replay each relevant target once, so old coalesced representatives cannot hide
    /// a pre-winner plan. Selection uses canonical chain episodes + exact plan identity.
    pub(super) fn finalize_canonical(&mut self) -> Result<(), String> {
        let mut grouped: BTreeMap<String, Vec<TruthEvent>> = BTreeMap::new();
        for event in &self.canonical_market {
            grouped
                .entry(event.target.clone())
                .or_default()
                .push(event.clone());
        }
        let mut result = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(7200);
        let mut replayed = 0u64;
        for (target, mut events) in grouped {
            events.sort_by_key(|e| (e.slot, e.transaction_index, e.event_index));
            let mut rows: Vec<_> = events
                .into_iter()
                .map(|market| Joined {
                    market,
                    target_seen: false,
                    target_seen_without_slot: false,
                    health_liquidatable: None,
                    candidates_created: None,
                    plan_validated: None,
                    built: None,
                    identity_status: "NO_MATCH",
                    our: None,
                    diagnostic: None,
                })
                .collect();
            let mut cursor = self.shadow_heads.get(&target).copied();
            while let Some(at) = cursor {
                replayed += 1;
                if replayed > self.shadow_evaluations_streamed
                    || std::time::Instant::now() > deadline
                {
                    return Err("canonical replay budget".into());
                }
                let (_, previous, rec) = self.chain_frame(&target, at)?;
                assign_record(&mut rows, &rec);
                cursor = previous;
            }
            result.extend(rows);
        }
        result.sort_by_key(|r| {
            (
                r.market.slot,
                r.market.transaction_index,
                r.market.event_index,
            )
        });
        for row in &result {
            self.append_typed_report(row)?;
        }
        self.canonical_joined = result;
        self.canonical_finalized = true;
        Ok(())
    }
    pub(super) fn canonical_metrics(&self, from: usize, through: usize) -> CanonicalSummary {
        let start = self
            .h
            .clock
            .unix_ns(self.h.shared.start_ns.load(Ordering::Acquire)) as i128;
        let hour = |r: &Joined| {
            r.market.block_time.and_then(|s| {
                let delta = i128::from(s) * 1_000_000_000 - start;
                u64::try_from(delta)
                    .ok()
                    .and_then(|ns| self.h.cfg.plan.chain_bucket(ns))
            })
        };
        let final_total = from == 0 && through == SNAPSHOT_COUNT;
        let rows: Vec<_> = self
            .canonical_joined
            .iter()
            .filter(|r| final_total || hour(r).is_some_and(|h| h >= from && h < through))
            .collect();
        let unassigned = self
            .canonical_joined
            .iter()
            .filter(|r| hour(r).is_none())
            .count();
        let seconds = self.h.cfg.plan.span_seconds(from, through);
        summarize(&rows, seconds, unassigned, self.canonical_finalized)
    }
}

fn assign_record(rows: &mut [Joined], rec: &OpportunityRecord) {
    let Some(rx) = rec.rx_slot else {
        for row in rows {
            row.target_seen_without_slot = true;
        }
        return;
    };
    // First next chain event closes this target's episode. Multiple events in that
    // slot are not ordered by wall time or guessed transaction send ordering.
    let Some(first) = rows.iter().position(|r| r.market.slot >= rx) else {
        return;
    };
    let slot = rows[first].market.slot;
    let end = first
        + rows[first..]
            .iter()
            .take_while(|r| r.market.slot == slot)
            .count();
    let matches = rows[first..end]
        .iter()
        .filter(|r| identity_matches(&r.market, rec))
        .count();
    for row in &mut rows[first..end] {
        row.target_seen = true;
        row.health_liquidatable =
            any_evidence(row.health_liquidatable, rec.progress.health_liquidatable);
        row.candidates_created =
            any_evidence(row.candidates_created, rec.progress.candidates_created);
        row.plan_validated = any_evidence(row.plan_validated, rec.progress.plan_validated);
        row.built = any_evidence(row.built, rec.progress.transaction_built);
        if row
            .diagnostic
            .as_ref()
            .is_none_or(|old| rec.timing.t_rx_ns > old.timing.t_rx_ns)
        {
            row.diagnostic = Some(rec.clone());
        }
        if identity_matches(&row.market, rec) {
            if matches != 1 {
                row.identity_status = "AMBIGUOUS_SAME_SLOT_EVENTS";
                row.our = None;
                continue;
            }
            row.identity_status = "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT";
            if row.our.as_ref().is_none_or(|old| preferable(rec, old)) {
                row.our = Some(rec.clone());
            }
        } else if row.our.is_none() && row.identity_status != "AMBIGUOUS_SAME_SLOT_EVENTS" {
            row.identity_status = if rec.market_plan_identity.is_none() {
                "PLAN_IDENTITY_UNKNOWN"
            } else if row.market.asset_bank.is_none() || row.market.liability_bank.is_none() {
                "MARKET_BANK_PAIR_UNKNOWN"
            } else {
                "PLAN_TYPE_OR_BANK_PAIR_MISMATCH"
            };
        }
    }
}

/// Known values of one field over a group of events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Values {
    pub count_known: u64,
    pub count_unknown: u64,
    pub distribution: Distribution,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalGroup {
    pub events: u64,
    pub transactions: u64,
    pub events_per_hour: Option<f64>,
    pub seized_usd: Values,
    pub repaid_usd: Values,
    pub gross_edge_usd: Values,
    pub winner_net_edge_usd: Values,
    pub native_amounts: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalMatching {
    pub events: u64,
    pub target_seen: u64,
    pub exact_plan_match: u64,
    pub target_seen_without_slot: u64,
    pub identity_unresolved: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetworkFees {
    pub transactions: u64,
    pub known_transactions: u64,
    pub unknown_transactions: u64,
    pub known_total_lamports: Option<u64>,
    pub priority_included_in_meta_fee: bool,
    pub unknown_never_zero: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalCapital {
    pub budget_usd: u64,
    pub market_opportunities: u64,
    pub shadow_detected: u64,
    pub executable: u64,
    pub fundable: u64,
    pub known_profitable: u64,
    pub unknown_economics: u64,
    pub known_net_edge_usd: Distribution,
    pub lost_only_because_capital: u64,
    pub lost_edge_only_because_capital_usd: Distribution,
    pub only_capital_semantics: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LossBucket {
    pub count: u64,
    pub known_market_gross_edge_usd: Values,
    pub known_our_executable_net_edge_usd: Distribution,
    pub our_edge_unknown_count: u64,
    pub interpretation: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StageBucket {
    pub events: u64,
    pub known_market_gross_edge_usd: Values,
    pub claim: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RaceBucket {
    pub count: u64,
    pub known_net_edge_usd: Distribution,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeedupBucket {
    pub ms: u64,
    pub local_boundaries_crossed: u64,
    pub claim: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TopShare {
    pub top: u64,
    pub event_share: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalCompetition {
    /// `(liquidator, events, known gross edge distribution)`, most events first.
    pub liquidators: Vec<(String, u64, Distribution)>,
    pub top_shares: Vec<TopShare>,
}
/// The canonical (all-path, finalized) market/Shadow analytics of one window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalSummary {
    pub finalized: bool,
    pub market: BTreeMap<String, CanonicalGroup>,
    pub authoritative_scope: String,
    pub market_window_hour_assignment: String,
    pub hour_unassigned_events_full_window: u64,
    pub unassigned_included_in_final_total: bool,
    pub matching: CanonicalMatching,
    pub network_fees: NetworkFees,
    pub capital: Vec<CanonicalCapital>,
    pub loss_attribution: BTreeMap<String, LossBucket>,
    pub largest_measured_stage: BTreeMap<String, StageBucket>,
    pub race: BTreeMap<String, RaceBucket>,
    pub matched_stage_ms: BTreeMap<String, Distribution>,
    pub local_margin_ms: Distribution,
    pub required_local_speedup_ms: Distribution,
    pub speedup_sensitivity: Vec<SpeedupBucket>,
    pub competition: CanonicalCompetition,
    pub true_competitor_submission_deadline: String,
    pub live_ready: bool,
}

fn values(rows: &[&Joined], f: impl Fn(&TruthEvent) -> Option<f64>) -> Values {
    let known: Vec<_> = rows.iter().filter_map(|r| f(&r.market)).collect();
    Values {
        count_known: known.len() as u64,
        count_unknown: (rows.len() - known.len()) as u64,
        distribution: distribution(known),
    }
}

fn summarize(
    rows: &[&Joined],
    seconds: f64,
    unassigned: usize,
    finalized: bool,
) -> CanonicalSummary {
    let mut market = BTreeMap::new();
    let mut fees = BTreeMap::new();
    for row in rows {
        fees.entry(&row.market.signature)
            .or_insert(row.market.transaction_fee_lamports);
    }
    for kind in [
        "ALL",
        "CLASSIC",
        "RECEIVERSHIP",
        "FORCED_DELEVERAGE",
        "BANKRUPTCY_SETTLEMENT",
    ] {
        let group: Vec<_> = rows
            .iter()
            .copied()
            .filter(|r| kind == "ALL" || r.market.liquidation_type.as_str() == kind)
            .collect();
        let tx: BTreeSet<_> = group.iter().map(|r| &r.market.signature).collect();
        market.insert(
            kind.to_string(),
            CanonicalGroup {
                events: group.len() as u64,
                transactions: tx.len() as u64,
                events_per_hour: if seconds > 0. {
                    Some(group.len() as f64 * 3600. / seconds)
                } else {
                    None
                },
                seized_usd: values(&group, |e| e.seized_value_usd),
                repaid_usd: values(&group, |e| e.repaid_value_usd),
                gross_edge_usd: values(&group, |e| e.gross_edge_usd),
                winner_net_edge_usd: values(&group, |e| e.winner_net_edge_usd),
                native_amounts: "canonical_market_shadow.jsonl join.market; per mint/venue units, never summed across denominations".into(),
            },
        );
    }
    let tradable: Vec<_> = rows
        .iter()
        .copied()
        .filter(|r| r.market.permissionless_profit_opportunity)
        .collect();
    let capital:Vec<_>=CAPITAL_USD.iter().map(|budget| {
        let mut detected=0;let mut exec=0;let mut funded=0;let mut unknown=0;let mut blocked=0;
        let mut nets=Vec::new();let mut lost=Vec::new();
        for row in &tradable {
            detected+=usize::from(row.target_seen);
            let rec=row.our.as_ref();
            let technical=rec.is_some_and(technical);exec+=usize::from(technical);
            let capacity=rec.and_then(|r|r.selected_costs.as_ref()).and_then(CandidateCostRecord::total_budget);
            let n=net(rec);
            if n.is_none() || capacity.is_none(){unknown+=1;}
            if technical {
                match capacity {
                    Some(v) if v<=budget*1_000_000 => {funded+=1;if let Some(n)=n.filter(|n|*n>0.){nets.push(n);}},
                    Some(_) if n.is_some_and(|n|n>0.) && race_class(row)=="PRE_WINNER_SLOT_PROVEN"=>{blocked+=1;lost.push(n.unwrap());},
                    _=>{},
                }
            }
        }
        CanonicalCapital {
            budget_usd: *budget,
            market_opportunities: tradable.len() as u64,
            shadow_detected: detected as u64,
            executable: exec as u64,
            fundable: funded,
            known_profitable: nets.len() as u64,
            unknown_economics: unknown,
            known_net_edge_usd: distribution(nets),
            lost_only_because_capital: blocked,
            lost_edge_only_because_capital_usd: distribution(lost),
            only_capital_semantics: "otherwise signed/simulated, positive known net, PRE winner slot; still not guaranteed capture".into(),
        }
    }).collect();
    let mut reasons: BTreeMap<&str, Vec<&Joined>> = [
        "NOT_DETECTED",
        "HEALTH_ERROR",
        "STATE_RACE",
        "MISSING_ATA",
        "ROUTING",
        "FUNDING",
        "CAPITAL",
        "ECONOMICS",
        "BANKRUPTCY",
        "SIMULATION",
        "BUILD",
        "SIGNING",
        "WINNER_TOO_EARLY",
        "UNKNOWN",
    ]
    .into_iter()
    .map(|k| (k, vec![]))
    .collect();
    let mut race: BTreeMap<&str, Vec<&Joined>> = BTreeMap::new();
    let mut stages: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let mut bottlenecks: BTreeMap<&str, Vec<&Joined>> = BTreeMap::new();
    let mut margins = Vec::new();
    let mut speed = Vec::new();
    for row in &tradable {
        reasons.entry(loss(row)).or_default().push(row);
        race.entry(race_class(row)).or_default().push(row);
        if let Some(rec) = &row.our {
            let intervals = stage_intervals(rec);
            // Exclude the overlapping end-to-end total from stage ranking.
            if let Some((stage, _)) = intervals
                .iter()
                .filter(|(k, _)| {
                    matches!(**k, "rx_to_detect" | "detect_to_ready" | "ready_to_sign")
                })
                .filter_map(|(k, v)| v.map(|n| (*k, n)))
                .max_by_key(|(_, n)| *n)
            {
                bottlenecks.entry(stage).or_default().push(row);
            }
            for (k, v) in intervals {
                if let Some(n) = v {
                    stages.entry(k).or_default().push(n as f64 / 1e6);
                }
            }
        }
        if let Some(n) = margin(row) {
            margins.push(n as f64 / 1e6);
            if n < 0 {
                speed.push((-n) as f64 / 1e6);
            }
        }
    }
    let loss_dollars: BTreeMap<_, _> = reasons
        .into_iter()
        .map(|(reason, group)| {
            let ours: Vec<_> = group
                .iter()
                .filter_map(|r| {
                    r.our
                        .as_ref()
                        .filter(|r| technical(r))
                        .and_then(|r| net(Some(r)))
                })
                .collect();
            (
                reason.to_string(),
                LossBucket {
                    count: group.len() as u64,
                    known_market_gross_edge_usd: values(&group, |e| e.gross_edge_usd),
                    our_edge_unknown_count: (group.len() - ours.len()) as u64,
                    known_our_executable_net_edge_usd: distribution(ours),
                    interpretation: "opportunity attribution; not realized monetary loss".into(),
                },
            )
        })
        .collect();
    let race: BTreeMap<String, RaceBucket> = race
        .into_iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                RaceBucket {
                    count: v.len() as u64,
                    known_net_edge_usd: distribution(
                        v.iter().filter_map(|r| net(r.our.as_ref())).collect(),
                    ),
                },
            )
        })
        .collect();
    let mut actors: BTreeMap<&str, (u64, Vec<f64>)> = BTreeMap::new();
    for row in &tradable {
        let a = actors.entry(&row.market.liquidator).or_default();
        a.0 += 1;
        if let Some(g) = row.market.gross_edge_usd {
            a.1.push(g);
        }
    }
    let mut actors: Vec<_> = actors
        .into_iter()
        .map(|(id, (count, edges))| (id.to_string(), count, distribution(edges)))
        .collect();
    actors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let event_share = |n: usize| {
        (!tradable.is_empty())
            .then(|| actors.iter().take(n).map(|a| a.1).sum::<u64>() as f64 / tradable.len() as f64)
    };
    let top_shares = [1, 3, 5]
        .iter()
        .map(|n| TopShare {
            top: *n as u64,
            event_share: event_share(*n),
        })
        .collect();
    CanonicalSummary {
        finalized,
        market,
        authoritative_scope: "FINALIZED_ALL_PATH_MARKET_JOIN; production selected_costs only".into(),
        market_window_hour_assignment: "blockTime against shared-clock UTC capture boundaries; boundary/out-of-window or unknown timestamps reported separately".into(),
        hour_unassigned_events_full_window: unassigned as u64,
        unassigned_included_in_final_total: true,
        matching: CanonicalMatching {
            events: rows.len() as u64,
            target_seen: rows.iter().filter(|r| r.target_seen).count() as u64,
            exact_plan_match: rows.iter().filter(|r| r.our.is_some()).count() as u64,
            target_seen_without_slot: rows.iter().filter(|r| r.target_seen_without_slot).count()
                as u64,
            identity_unresolved: rows
                .iter()
                .filter(|r| r.target_seen && r.our.is_none())
                .count() as u64,
        },
        network_fees: NetworkFees {
            transactions: fees.len() as u64,
            known_transactions: fees.values().filter(|v| v.is_some()).count() as u64,
            unknown_transactions: fees.values().filter(|v| v.is_none()).count() as u64,
            known_total_lamports: fees
                .values()
                .copied()
                .flatten()
                .try_fold(None, |sum: Option<u64>, n| Some(Some(sum.unwrap_or(0).checked_add(n)?)))
                .flatten(),
            priority_included_in_meta_fee: true,
            unknown_never_zero: true,
        },
        capital,
        loss_attribution: loss_dollars,
        largest_measured_stage: bottlenecks
            .into_iter()
            .map(|(k, rows)| {
                (
                    k.to_string(),
                    StageBucket {
                        events: rows.len() as u64,
                        known_market_gross_edge_usd: values(&rows, |e| e.gross_edge_usd),
                        claim: "correlation with largest measured nonoverlapping stage; not proof of causal loss".into(),
                    },
                )
            })
            .collect(),
        race,
        matched_stage_ms: stages
            .into_iter()
            .map(|(k, v)| (k.to_string(), distribution(v)))
            .collect(),
        local_margin_ms: distribution(margins),
        required_local_speedup_ms: distribution(speed.clone()),
        speedup_sensitivity: SPEEDUP_MS
            .iter()
            .map(|ms| SpeedupBucket {
                ms: *ms,
                local_boundaries_crossed: speed.iter().filter(|s| **s <= *ms as f64).count() as u64,
                claim: "COUNTERFACTUAL LOCAL BOUNDARY ANALYSIS".into(),
            })
            .collect(),
        competition: CanonicalCompetition {
            liquidators: actors,
            top_shares,
        },
        true_competitor_submission_deadline: "UNKNOWN".into(),
        live_ready: false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::view;
    use super::*;
    #[test]
    fn missing_values_have_no_zero_total() {
        let s = view(&summarize(&[], 3600., 0, true));
        assert!(s["network_fees"]["known_total_lamports"].is_null());
        assert_eq!(s["capital"].as_array().unwrap().len(), 6);
    }
}

#[cfg(test)]
mod regression {
    use super::super::tests::{costs, rec, view, worker};
    use super::*;
    use crate::analytics::record::MarketPlanIdentity;
    use crate::market_observer::truth::LiquidationType;
    /// JFI-FIX-05: a typed `ECON_CANONICAL` frame round-trips losslessly and renders the
    /// identical historical JSONL row; an identity status outside the closed set is refused.
    #[test]
    fn typed_canonical_row_renders_identical_view() {
        use crate::analytics::payload::{self, Payload};
        let mut j = row(&rec(), 7, 2);
        j.our = Some(rec());
        j.identity_status = "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT";
        let bytes = payload::encode(&j).unwrap();
        let back: Joined = payload::decode(Joined::KIND, Joined::SCHEMA, &bytes).unwrap();
        assert_eq!(canonical_legacy_view(&back), canonical_legacy_view(&j));
        assert_eq!(back.identity_status, j.identity_status);
        let mut text = serde_json::to_value(&j).unwrap();
        text["identity_status"] = "FORGED".into();
        assert!(serde_json::from_value::<Joined>(text).is_err());
    }
    fn row(r: &OpportunityRecord, slot: u64, index: u32) -> Joined {
        Joined {
            market: TruthEvent {
                signature: format!("tx-{slot}"),
                slot,
                transaction_index: 0,
                event_index: index,
                liquidation_type: LiquidationType::Receivership,
                target: r.account.clone(),
                liquidator: "winner".into(),
                liquidator_marginfi_account: None,
                asset_bank: Some("asset".into()),
                liability_bank: Some("liability".into()),
                asset_mint: None,
                liability_mint: None,
                seized_asset_native_exact: None,
                native_evidence: None,
                seized_value_usd: Some(105.),
                repaid_value_usd: Some(100.),
                gross_edge_usd: Some(5.),
                protocol_fee_lamports: Some(5000),
                transaction_fee_lamports: Some(7000),
                fee_payer: Some("payer".into()),
                visible_payments: Some(vec![]),
                incremental_rent_lamports: None,
                priority_fee_lamports: None,
                winner_net_edge_usd: None,
                winner_net_edge_complete: false,
                economic_provenance: "FIXTURE".into(),
                block_time: None,
                live_t_rx_ns: Some(50),
                discovery_source: "LIVE".into(),
                permissionless_profit_opportunity: true,
            },
            target_seen: false,
            target_seen_without_slot: false,
            health_liquidatable: None,
            candidates_created: None,
            plan_validated: None,
            built: None,
            identity_status: "NO_MATCH",
            our: None,
            diagnostic: None,
        }
    }
    fn identified() -> OpportunityRecord {
        let mut r = rec();
        r.market_plan_identity = Some(MarketPlanIdentity {
            liquidation_type: "RECEIVERSHIP".into(),
            asset_bank: "asset".into(),
            liability_bank: "liability".into(),
        });
        r.progress.health_liquidatable = Some(true);
        r.progress.candidates_created = Some(true);
        r.progress.plan_validated = Some(true);
        r.progress.transaction_built = Some(true);
        r
    }
    #[test]
    fn offline_replay_uses_same_selector_and_exact_odd_plan_boundaries() {
        let mut w = worker();
        let r = identified();
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
        let mut event = row(&r, 10, 0).market;
        event.block_time = Some(101);
        let input = w.h.cfg.run_dir.join("replay_market.jsonl");
        fs::write(
            &input,
            format!("{}\n", serde_json::to_string(&event).unwrap()),
        )
        .unwrap();
        let out = w.h.cfg.run_dir.join("offline_replay");
        let plan = MeasurementPlan::new(61).unwrap();
        replay(
            &input,
            &w.h.cfg.run_dir.join("shadow_evaluations.jsonl"),
            &out,
            &plan,
            100_000_000_000,
            30,
        )
        .unwrap();
        let got: Value = serde_json::from_str(
            fs::read_to_string(out.join("canonical_market_shadow.jsonl"))
                .unwrap()
                .trim(),
        )
        .unwrap();
        let mut expected = vec![row(&r, 10, 0)];
        expected[0].market = event;
        assign_record(&mut expected, &r);
        assert_eq!(
            got["economic_net_usd"],
            joined_value(&expected[0])["economic_net_usd"]
        );
        assert_eq!(
            got["join"]["identity_status"],
            "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT"
        );
        for i in 1..=4 {
            let v: Value =
                serde_json::from_reader(File::open(out.join(format!("hour_{i:02}.json"))).unwrap())
                    .unwrap();
            assert_eq!(v["cutoff_unix_ns"], 100_000_000_000u64 + plan.offset_ns(i));
            assert_eq!(v["cumulative"]["market"]["ALL"]["events"], 1);
            assert_eq!(v["master_pass"], false);
        }
        assert!(replay(
            &input,
            &w.h.cfg.run_dir.join("shadow_evaluations.jsonl"),
            &out,
            &plan,
            100_000_000_000,
            30
        )
        .is_err());
        // Runs before PERSIST-01 keep their JSONL stream: same replay, byte-identical renders.
        let legacy_dir = w.h.cfg.run_dir.join("legacy");
        fs::create_dir(&legacy_dir).unwrap();
        let legacy = legacy_dir.join("shadow_evaluations.jsonl");
        fs::write(&legacy, format!("{}\n", serde_json::to_string(&r).unwrap())).unwrap();
        let from_legacy = w.h.cfg.run_dir.join("offline_replay_legacy");
        replay(&input, &legacy, &from_legacy, &plan, 100_000_000_000, 30).unwrap();
        for name in ["canonical_market_shadow.jsonl", "economics_final.json"] {
            assert_eq!(
                fs::read(out.join(name)).unwrap(),
                fs::read(from_legacy.join(name)).unwrap(),
                "{name}"
            );
        }
        // Both sources at once are ambiguous, never merged or guessed.
        fs::copy(
            journal::binary_path(&w.h.cfg.run_dir.join("shadow_evaluations.jsonl")),
            journal::binary_path(&legacy),
        )
        .unwrap();
        assert_eq!(
            replay(
                &input,
                &legacy,
                &w.h.cfg.run_dir.join("ambiguous"),
                &plan,
                100_000_000_000,
                30
            )
            .err()
            .as_deref(),
            Some("AMBIGUOUS_SHADOW_SOURCE")
        );
    }
    #[test]
    fn exact_identity_and_chain_episode_prevent_cross_event_allocation() {
        let r = identified();
        let mut rows = vec![row(&r, 10, 0), row(&r, 20, 0)];
        assign_record(&mut rows, &r);
        assert!(rows[0].our.is_some());
        assert!(!rows[1].target_seen);
        let mut later = r.clone();
        later.rx_slot = Some(11);
        assign_record(&mut rows, &later);
        assert!(rows[1].our.is_some());
        let mut wrong = identified();
        wrong.account = r.account.clone();
        wrong.market_plan_identity.as_mut().unwrap().asset_bank = "wrong".into();
        let mut rows = vec![row(&r, 10, 0)];
        assign_record(&mut rows, &wrong);
        assert!(rows[0].target_seen);
        assert!(rows[0].our.is_none());
        assert_eq!(rows[0].identity_status, "PLAN_TYPE_OR_BANK_PAIR_MISMATCH");
    }
    #[test]
    fn same_slot_same_pair_ambiguous_but_distinct_pair_resolves() {
        let r = identified();
        let mut rows = vec![row(&r, 10, 0), row(&r, 10, 1)];
        assign_record(&mut rows, &r);
        assert!(rows
            .iter()
            .all(|r| r.our.is_none() && r.identity_status == "AMBIGUOUS_SAME_SLOT_EVENTS"));
        rows[1].market.asset_bank = Some("other".into());
        assign_record(&mut rows, &r);
        assert!(rows[0].our.is_some());
        assert!(rows[1].our.is_none());
    }
    #[test]
    fn unknown_identity_and_old_records_never_become_known_economics() {
        let r = rec();
        let mut rows = vec![row(&r, 10, 0)];
        assign_record(&mut rows, &r);
        let s = view(&summarize(&[&rows[0]], 3600., 0, true));
        assert_eq!(s["matching"]["exact_plan_match"], 0);
        assert_eq!(s["capital"][0]["unknown_economics"], 1);
        assert!(s["capital"][0]["known_net_edge_usd"]["total"].is_null());
        let mut v = serde_json::to_value(r).unwrap();
        v.as_object_mut().unwrap().remove("progress");
        v.as_object_mut().unwrap().remove("market_plan_identity");
        let old: OpportunityRecord = serde_json::from_value(v).unwrap();
        assert!(old.progress.health_liquidatable.is_none());
        assert!(old.market_plan_identity.is_none());
        let mut no_slot = old;
        no_slot.rx_slot = None;
        let mut rows = vec![row(&no_slot, 10, 0)];
        assign_record(&mut rows, &no_slot);
        assert_eq!(loss(&rows[0]), "UNKNOWN");
        assert!(rows[0].our.is_none());
    }
    #[test]
    fn capital_uses_full_production_budget_and_excludes_admin() {
        let mut r = identified();
        r.selected_costs = Some(costs(200_000_000));
        let mut rows = vec![row(&r, 10, 0)];
        assign_record(&mut rows, &r);
        let s = view(&summarize(&[&rows[0]], 3600., 0, true));
        assert_eq!(s["capital"][0]["fundable"], 0); // fee reserve makes exactly $200 insufficient
        assert_eq!(s["capital"][0]["lost_only_because_capital"], 1);
        assert_eq!(s["capital"][1]["fundable"], 1);
        assert_eq!(s["capital"][1]["known_profitable"], 1);
        rows[0].market.permissionless_profit_opportunity = false;
        rows[0].market.liquidation_type = LiquidationType::ForcedDeleverage;
        let s = view(&summarize(&[&rows[0]], 3600., 0, true));
        assert_eq!(s["market"]["ALL"]["events"], 1);
        assert_eq!(s["capital"][0]["market_opportunities"], 0);
    }
    #[test]
    fn transaction_fee_dedup_unknown_costs_and_loss_dollars() {
        let r = identified();
        let a = row(&r, 10, 0);
        let mut b = row(&r, 10, 1);
        b.market.gross_edge_usd = None;
        let s = view(&summarize(&[&a, &b], 3600., 0, true));
        assert_eq!(s["network_fees"]["transactions"], 1);
        assert_eq!(s["network_fees"]["known_total_lamports"], 7000);
        assert_eq!(s["loss_attribution"]["NOT_DETECTED"]["count"], 2);
        assert_eq!(
            s["loss_attribution"]["NOT_DETECTED"]["known_market_gross_edge_usd"]["distribution"]
                ["total"],
            5.
        );
        assert_eq!(
            s["loss_attribution"]["NOT_DETECTED"]["known_market_gross_edge_usd"]["count_unknown"],
            1
        );
        assert!(s["market"]["ALL"]["winner_net_edge_usd"]["distribution"]["total"].is_null());
        assert!(s["loss_attribution"]["SIMULATION"].is_object());
    }
    #[test]
    fn race_same_slot_never_proves_win_and_backfill_has_no_fake_margin() {
        let mut r = identified();
        r.sign_slot = Some(10);
        let mut rows = vec![row(&r, 10, 0)];
        assign_record(&mut rows, &r);
        assert_eq!(race_class(&rows[0]), "SAME_SLOT_UNKNOWN");
        assert_eq!(margin(&rows[0]), Some(10));
        rows[0].market.live_t_rx_ns = None;
        assert_eq!(margin(&rows[0]), None);
    }
    #[test]
    fn journal_replay_preserves_earliest_signed_record_and_hourly_final_reconcile() {
        let mut w = worker();
        let mut r = identified();
        let start = (w.h.clock.unix_ns(1) / 1_000_000_000) as i64 + 1;
        w.h.cfg.plan = MeasurementPlan::new(14400).unwrap();
        for i in 0..4 {
            let mut e = row(&r, 10 + i, 0).market;
            e.block_time = Some(start + i as i64 * 3600);
            w.canonical_market.push(e);
        }
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
        r.timing.t_sign_ns = Some(45);
        r.timing.t_rx_ns = Some(11);
        w.accept(Evidence::Shadow(Box::new(r))).unwrap();
        w.finalize_canonical().unwrap();
        assert_eq!(
            w.canonical_joined[0].our.as_ref().unwrap().timing.t_sign_ns,
            Some(40)
        );
        let sum: u64 = (1..=4)
            .map(|i| {
                view(&w.canonical_metrics(i - 1, i))["market"]["ALL"]["events"]
                    .as_u64()
                    .unwrap()
            })
            .sum();
        assert_eq!(sum, 4);
        assert_eq!(
            view(&w.canonical_metrics(0, 4))["market"]["ALL"]["events"],
            4
        );
        // The joined rows are typed report frames; no JSONL view is written by the run.
        assert!(!w
            .h
            .cfg
            .run_dir
            .join("canonical_market_shadow.jsonl")
            .exists());
        assert_eq!(w.canonical_joined.len(), 4);
    }

    /// JFI-FIX-05 parity: the snapshot and final report documents of a rich analytics state
    /// (live and backfilled markets, unique and ambiguous winners, executable, simulation-failed,
    /// economics-unknown and routing-loss evaluations, capture counters, canonical join, real
    /// market summaries) render exactly as recorded. Fixed keys only.
    #[test]
    fn report_documents_match_golden() {
        use crate::market::winner::SameSlotCandidate;
        use crate::market_observer::{
            EnrichConfig, MarketSource, MarketStats, ObserverCore, SummaryContext,
        };
        use solana_sdk::pubkey::Pubkey;
        use std::str::FromStr;
        let mut w = worker();
        let key = |n: u8| Pubkey::new_from_array([n; 32]);
        let program = Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap();
        let summary = |secs: f64, done: bool| {
            let mut core = ObserverCore::new(program, MarketSource::Logs, EnrichConfig::DEFAULT);
            core.start_observation(0, u64::MAX);
            core.summary(&SummaryContext {
                run_id: "golden".into(),
                commitment: "finalized".into(),
                stream_ready: true,
                full_duration_observed: done,
                drain_complete: done,
                actual_observation_seconds: secs,
                configured_duration_seconds: 4.0,
                min_sample_liquidations: 1,
                ..Default::default()
            })
        };
        const S: u64 = 1_000_000_000;
        let mk = |n: u8, hour: u64| {
            let mut r = identified();
            r.account = key(n).to_string();
            r.timing.t_rx_ns = Some(hour * S + 10 + u64::from(n));
            r.timing.t_sign_ns = Some(hour * S + 40 + u64::from(n));
            r.rx_slot = Some(10 + hour);
            r.detect_slot = Some(10 + hour);
            r.sign_slot = Some(10 + hour);
            r
        };
        let market = |r: &OpportunityRecord, slot, rx, source, receiver: u8| MarketEvent {
            signature: format!("win-{slot}-{receiver}"),
            slot,
            t_rx_ns: rx,
            source,
            transaction_index: Some(3),
            event: ReceiverEvent {
                event_index: 0,
                target: r.account.parse().unwrap(),
                receiver: key(receiver),
                seized_equity_usd: 105.0 + f64::from(receiver),
                repaid_equity_usd: 100.0,
                protocol_flat_fee_lamports: 5000,
            },
        };
        let unique = |slot, receiver: u8| Winner {
            slot,
            identity: WinnerIdentity::Unique {
                signature: format!("win-{slot}-{receiver}"),
                receiver: key(receiver).to_string(),
                tx_version: "0".into(),
            },
        };
        // A: executable, unique live winner in a later slot.
        let a = mk(1, 0);
        w.accept(Evidence::Market(market(
            &a,
            12,
            Some(25),
            MarketEvidenceSource::Live,
            101,
        )))
        .unwrap();
        w.accept(Evidence::Slot {
            slot: 12,
            t_rx_ns: 24,
        })
        .unwrap();
        w.accept(Evidence::Shadow(Box::new(a.clone()))).unwrap();
        w.accept(Evidence::Winner {
            target: a.account.clone(),
            rx_slot: 10,
            winner: unique(12, 101),
        })
        .unwrap();
        // B: executable, winner in its own sign slot (same slot, order unknown).
        let b = mk(2, 1);
        w.accept(Evidence::Market(market(
            &b,
            11,
            Some(S + 20),
            MarketEvidenceSource::Live,
            102,
        )))
        .unwrap();
        w.accept(Evidence::Shadow(Box::new(b.clone()))).unwrap();
        w.accept(Evidence::Winner {
            target: b.account.clone(),
            rx_slot: 11,
            winner: unique(11, 102),
        })
        .unwrap();
        // C: simulation failed; D: economics unknown; E: routing loss.
        let mut c = mk(3, 1);
        c.decision = Decision::ShadowSimFailed;
        c.simulation_result = Some("FAIL".into());
        w.accept(Evidence::Shadow(Box::new(c))).unwrap();
        let mut d = mk(4, 2);
        d.economics_known = false;
        d.net_profit = None;
        d.selected_costs = None;
        d.decision = Decision::ShadowTechnicalReady;
        w.accept(Evidence::Shadow(Box::new(d))).unwrap();
        let mut e = mk(5, 2);
        e.decision = Decision::Skip;
        e.loss = Some(LossReason::Routing);
        e.timing.t_sign_ns = None;
        w.accept(Evidence::Shadow(Box::new(e))).unwrap();
        // F: executable without winner; G: same-slot ambiguous winner; H: backfilled market.
        let f = mk(6, 3);
        w.accept(Evidence::Shadow(Box::new(f.clone()))).unwrap();
        let g = mk(7, 3);
        w.accept(Evidence::Shadow(Box::new(g.clone()))).unwrap();
        w.accept(Evidence::Winner {
            target: g.account.clone(),
            rx_slot: 13,
            winner: Winner {
                slot: 13,
                identity: WinnerIdentity::SameSlotAmbiguous {
                    candidates: vec![SameSlotCandidate {
                        signature: "amb".into(),
                        receiver: key(107).to_string(),
                        tx_version: "0".into(),
                        transaction_index: Some(1),
                    }],
                },
            },
        })
        .unwrap();
        let h = mk(8, 3);
        w.accept(Evidence::Market(market(
            &h,
            13,
            None,
            MarketEvidenceSource::GapBackfill,
            108,
        )))
        .unwrap();
        // Capture counters, Shadow owner counters, market summaries.
        w.accept(Evidence::MarketCounters {
            at_ns: S / 2,
            before: Box::default(),
            after: Box::new(MarketStats {
                transactions_received: 5,
                unique_transactions: 4,
                duplicate_transactions: 1,
                liquidation_events: 2,
                first_observed_slot: Some(10),
                last_observed_slot: Some(12),
                max_stream_silence_ms: 70,
                ..Default::default()
            }),
        })
        .unwrap();
        w.accept(Evidence::ShadowStats(ShadowStats {
            winner_watch_current: 0,
            winner_watch_max: 3,
            winner_watch_overflow: 0,
            winner_watch_errors: 0,
            winner_watch_resolved: 3,
            winner_watch_expired: 1,
            winner_queue_full_backoffs: 0,
            ingress_overflow: 0,
            winner_pending_at_deadline: 0,
            shadow_state_ready_at_shutdown: true,
            remote_winner_queue_depth: 0,
            remote_projection_queue_depth: 0,
        }))
        .unwrap();
        for index in 1..=3 {
            w.accept(Evidence::MarketCheckpoint {
                index,
                summary: Box::new(summary(index as f64, false)),
            })
            .unwrap();
        }
        let last = crate::market_observer::MarketFinal {
            summary: summary(4.0, true),
            econ: Some((w.h.cfg.plan.clone(), None)),
            evidence_journal: None,
        };
        w.accept(Evidence::MarketFinal(Box::new(last))).unwrap();
        // Canonical: one exact-identity event per quarter (A's target), one admin settlement.
        let start = (w.h.clock.unix_ns(1) / 1_000_000_000) as i64 + 1;
        for i in 0..3u64 {
            let mut ev = row(&a, 10 + i, 0).market;
            ev.signature = format!("canon-{i}");
            ev.block_time = Some(start + i as i64);
            w.canonical_market.push(ev);
        }
        let mut admin = row(&f, 13, 1).market;
        admin.signature = "canon-admin".into();
        admin.liquidation_type = LiquidationType::BankruptcySettlement;
        admin.permissionless_profit_opportunity = false;
        admin.block_time = Some(start + 3);
        w.canonical_market.push(admin);
        w.h.shared.shadow_ready_ns.store(1, Ordering::Release);
        w.h.shared.shadow_done.store(true, Ordering::Relaxed);
        w.h.shared.market_done.store(true, Ordering::Relaxed);
        w.snapshots = SNAPSHOT_COUNT;
        // Quarter 2 while capturing (checkpoint delta 2-1, live telemetry) and the final report
        // (market final summary, canonical join, frozen telemetry).
        let provisional = view(&w.report(2, false));
        w.finalize_canonical().unwrap();
        w.final_technical = Some(w.technical(true));
        let doc =
            json!({"provisional_2": provisional, "final": view(&w.report(SNAPSHOT_COUNT, true))});
        crate::analytics::golden::check(
            "econ_reports",
            &doc,
            &[
                "run_id",
                "started_at_utc",
                "ended_at_utc",
                "capture_window",
                "actual_elapsed_seconds",
                "remote_and_backlog_telemetry",
                "provider_budget",
            ],
        );
    }
}

fn joined_value(row: &Joined) -> Value {
    json!({"join":row,"race_class":race_class(row),"local_observed_race_margin_ns":margin(row),"loss_reason":loss(row),
                    "signed":row.our.as_ref().map(|r|r.timing.t_sign_ns.is_some()),
                    "simulation_result":row.our.as_ref().and_then(|r|r.simulation_result.as_deref()),
                    "economic_net_usd":net(row.our.as_ref()),"economics_known":row.our.as_ref().map(|r|r.economics_known),
                    "fundable_by_budget":CAPITAL_USD.iter().map(|budget|json!({"budget_usd":budget,"fundable":row.our.as_ref().and_then(|r|r.selected_costs.as_ref()).and_then(CandidateCostRecord::total_budget).map(|n|n<=budget*1_000_000)})).collect::<Vec<_>>(),
                    "loss_provenance":if row.our.is_some(){"EXACT_PLAN_MATCH"}else if row.diagnostic.is_some(){"TARGET_EPISODE_DIAGNOSTIC; executable identity unproven"}else{"NO_TARGET_EPISODE_RECORD"}})
}

/// Result of the canonical join over finalized events.
pub struct JoinOutcome {
    /// One row per event, in chain order.
    pub rows: Vec<Joined>,
    /// Whether an evaluation journal existed (absence with events present is an error).
    pub shadow_available: bool,
}

/// The production episode/plan selector over finalized events and the run's evaluation stream:
/// the binary journal (`shadow_evaluations.p0j` and its linked segments) or, for runs before
/// PERSIST-01, their JSONL file. One bounded streaming pass; no RPC repricing/re-simulation and
/// no synthetic evaluation when the original capture lost data. Used by the offline replay and
/// the native report (B7).
pub fn join(
    events: Vec<TruthEvent>,
    shadow_path: &Path,
    deadline: std::time::Instant,
) -> Result<JoinOutcome, String> {
    use std::io::BufRead;
    let mut grouped: BTreeMap<String, Vec<Joined>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let count = events.len();
    if count > 100_000 {
        return Err("REPLAY_MARKET_CAPACITY".into());
    }
    for market in events {
        if !seen.insert((market.signature.clone(), market.slot, market.event_index)) {
            return Err("DUPLICATE_MARKET_EVENT".into());
        }
        grouped
            .entry(market.target.clone())
            .or_default()
            .push(Joined {
                market,
                target_seen: false,
                target_seen_without_slot: false,
                health_liquidatable: None,
                candidates_created: None,
                plan_validated: None,
                built: None,
                identity_status: "NO_MATCH",
                our: None,
                diagnostic: None,
            });
    }
    for rows in grouped.values_mut() {
        rows.sort_by_key(|r| {
            (
                r.market.slot,
                r.market.transaction_index,
                r.market.event_index,
            )
        });
    }
    let binary = if shadow_path.extension().is_some_and(|e| e == "p0j") {
        shadow_path.to_path_buf()
    } else {
        journal::binary_path(shadow_path)
    };
    let legacy = binary != shadow_path && shadow_path.exists();
    if binary.exists() && legacy {
        return Err("AMBIGUOUS_SHADOW_SOURCE".into());
    }
    let available = binary.exists() || legacy;
    if count > 0 && !available {
        return Err("SHADOW_JOURNAL_MISSING".into());
    }
    if count > 0 && binary.exists() {
        // SK-07: the evaluation journal is replayed across its whole segment chain.
        let mut reader =
            crate::storage::segments::ChainReader::open(&binary).map_err(|_| "SHADOW_INPUT")?;
        while let Some(frame) = reader.next_frame().map_err(|_| "SHADOW_READ")? {
            if std::time::Instant::now() >= deadline {
                return Err("REPLAY_TIME_BUDGET".into());
            }
            match frame.kind {
                journal::kind::ECON_SHADOW_EVALUATION => {
                    // Typed frames (JFI-FIX-05); historical schema-1 value trees stay readable.
                    let rec: OpportunityRecord = if frame.schema == 1 {
                        serde_json::from_value(
                            frame.legacy_value().map_err(|_| "SHADOW_DECODE")?["record"].take(),
                        )
                        .map_err(|_| "SHADOW_DECODE")?
                    } else {
                        frame
                            .typed::<super::ShadowEvaluation>()
                            .map_err(|_| "SHADOW_DECODE")?
                            .record
                    };
                    if let Some(rows) = grouped.get_mut(&rec.account) {
                        assign_record(rows, &rec);
                    }
                }
                journal::MANIFEST => {}
                _ => return Err("SHADOW_DECODE".into()),
            }
        }
    } else if count > 0 {
        let mut reader = BufReader::new(File::open(shadow_path).map_err(|_| "SHADOW_INPUT")?);
        loop {
            if std::time::Instant::now() >= deadline {
                return Err("REPLAY_TIME_BUDGET".into());
            }
            let mut line = Vec::new();
            let len = (&mut reader)
                .take(16 * 1024 * 1024 + 1)
                .read_until(b'\n', &mut line)
                .map_err(|_| "SHADOW_READ")?;
            if len == 0 {
                break;
            }
            if len > 16 * 1024 * 1024 || line.last() != Some(&b'\n') {
                return Err("REPLAY_ROW_BUDGET".into());
            }
            let rec: OpportunityRecord =
                serde_json::from_slice(&line).map_err(|_| "SHADOW_DECODE")?;
            if let Some(rows) = grouped.get_mut(&rec.account) {
                assign_record(rows, &rec);
            }
        }
    }
    let mut rows: Vec<_> = grouped.into_values().flatten().collect();
    rows.sort_by_key(|r| {
        (
            r.market.slot,
            r.market.transaction_index,
            r.market.event_index,
        )
    });
    Ok(JoinOutcome {
        rows,
        shadow_available: available,
    })
}

/// Offline replay uses the identical production episode/plan selector and
/// economics/attribution functions. It cannot call a provider or submit a TX.
pub(super) fn replay(
    market_path: &Path,
    shadow_path: &Path,
    out: &Path,
    plan: &MeasurementPlan,
    start_ns: u64,
    max_seconds: u64,
) -> Result<(), String> {
    use std::io::BufRead;
    plan.validate()?;
    if max_seconds == 0 || max_seconds > 7200 {
        return Err("REPLAY_TIME_BUDGET".into());
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(max_seconds);
    let mut events = Vec::new();
    let mut reader = BufReader::new(File::open(market_path).map_err(|_| "MARKET_INPUT")?);
    loop {
        if std::time::Instant::now() >= deadline {
            return Err("REPLAY_TIME_BUDGET".into());
        }
        let mut line = Vec::new();
        let len = (&mut reader)
            .take(16 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| "MARKET_READ")?;
        if len == 0 {
            break;
        }
        if len > 16 * 1024 * 1024 || line.last() != Some(&b'\n') {
            return Err("REPLAY_ROW_BUDGET".into());
        }
        let mut market: TruthEvent = serde_json::from_slice(&line).map_err(|_| "MARKET_DECODE")?;
        market.discovery_source = "OFFLINE_FINALIZED_EVIDENCE".into();
        market.economic_provenance = "PERSISTED_PROTOCOL_EVENT_AND_ACTUAL_TX_META".into();
        events.push(market);
        if events.len() > 100_000 {
            return Err("REPLAY_MARKET_CAPACITY".into());
        }
    }
    let JoinOutcome {
        rows,
        shadow_available: available,
    } = join(events, shadow_path, deadline)?;
    fs::create_dir_all(out).map_err(|_| "REPLAY_OUTPUT")?;
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out.join("canonical_market_shadow.jsonl"))
        .map_err(|_| "REPLAY_OUTPUT_EXISTS")?;
    let mut writer = BufWriter::new(file);
    for row in &rows {
        line(&mut writer, &joined_value(row))?;
    }
    writer.flush().map_err(|_| "REPLAY_WRITE")?;
    let bucket = |r: &Joined| {
        r.market
            .block_time
            .and_then(|t| u64::try_from((t as i128) * 1_000_000_000 - start_ns as i128).ok())
            .and_then(|n| plan.chain_bucket(n))
    };
    for i in 1..=SNAPSHOT_COUNT {
        let cumulative: Vec<_> = rows
            .iter()
            .filter(|r| bucket(r).is_some_and(|b| b < i))
            .collect();
        let delta: Vec<_> = rows.iter().filter(|r| bucket(r) == Some(i - 1)).collect();
        let unassigned = rows.iter().filter(|r| bucket(r).is_none()).count();
        let value = json!({"schema":"C0-E1/OfflineCanonicalEconomics/1","scope":"EXPLICIT_FINALIZED_EVENT_INPUT; verification coverage is separate",
            "start_unix_ns":start_ns,"cutoff_unix_ns":start_ns+plan.offset_ns(i),"plan":plan,
            "cumulative":summarize(&cumulative,plan.span_seconds(0,i),unassigned,true),
            "window_delta":summarize(&delta,plan.span_seconds(i-1,i),unassigned,true),
            "raw_shadow_available":available,"live_enabled":false,"master_pass":false});
        atomic_json(&out.join(format!("hour_{i:02}.json")), &value)?;
        if i == SNAPSHOT_COUNT {
            atomic_json(&out.join("economics_final.json"), &value)?;
        }
    }
    Ok(())
}
