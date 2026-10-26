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
    /// A2: commitment of the market capture (decides whether the live-receipt margin is valid).
    pub market_commitment: String,
    /// A2: first in-window local receipt of the winner slot (the capture's `SlotBoundary` row).
    pub winner_slot_first_seen_ns: Option<u64>,
    /// A6: capital scenarios of the run (USD), as on the opportunity rows.
    pub capital: Vec<u64>,
}
impl crate::analytics::payload::Payload for Joined {
    const KIND: u16 = journal::kind::ECON_CANONICAL;
    /// DEC-12: 5 — the records with the typed rejection and the native message hash.
    const SCHEMA: u16 = 5;
    /// Schema 2 (before A2) and 3 (before A6) — their views used the default capital list —
    /// and 4; all three carry the records before DEC-12.
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<Self>> {
        use crate::analytics::payload::decode_wire;
        use crate::analytics::record::OpportunityRecordV2 as V2;
        if schema == 4 {
            return Some(
                decode_wire::<JoinedWire<V2>>(bytes)
                    .and_then(JoinedWire::<V2>::current)
                    .and_then(|v| {
                        v.into_joined()
                            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                    }),
            );
        }
        let wire = match schema {
            2 => decode_wire::<JoinedWireV2<V2>>(bytes).map(|v| JoinedWireV3 {
                market: v.market,
                target_seen: v.target_seen,
                target_seen_without_slot: v.target_seen_without_slot,
                health_liquidatable: v.health_liquidatable,
                candidates_created: v.candidates_created,
                plan_validated: v.plan_validated,
                built: v.built,
                identity_status: v.identity_status,
                our: v.our,
                diagnostic: v.diagnostic,
                market_commitment: "UNKNOWN".into(),
                winner_slot_first_seen_ns: None,
            }),
            3 => decode_wire::<JoinedWireV3<V2>>(bytes),
            _ => return None,
        };
        Some(wire.and_then(|v| {
            JoinedWire::<V2> {
                market: v.market,
                target_seen: v.target_seen,
                target_seen_without_slot: v.target_seen_without_slot,
                health_liquidatable: v.health_liquidatable,
                candidates_created: v.candidates_created,
                plan_validated: v.plan_validated,
                built: v.built,
                identity_status: v.identity_status,
                our: v.our,
                diagnostic: v.diagnostic,
                market_commitment: v.market_commitment,
                winner_slot_first_seen_ns: v.winner_slot_first_seen_ns,
                capital: CAPITAL_USD.to_vec(),
            }
            .current()?
            .into_joined()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        }))
    }
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
/// Wire form of `Joined` (owned status string, mapped back onto the closed set); `R` is the
/// record layout (DEC-12: `OpportunityRecordV2` in schemas ≤ 4).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct JoinedWire<R = OpportunityRecord> {
    market: TruthEvent,
    target_seen: bool,
    target_seen_without_slot: bool,
    health_liquidatable: Option<bool>,
    candidates_created: Option<bool>,
    plan_validated: Option<bool>,
    built: Option<bool>,
    identity_status: String,
    our: Option<R>,
    diagnostic: Option<R>,
    market_commitment: String,
    winner_slot_first_seen_ns: Option<u64>,
    capital: Vec<u64>,
}
/// Layout of schema 3 (A2, before A6): no capital list.
#[derive(Deserialize)]
struct JoinedWireV3<R> {
    market: TruthEvent,
    target_seen: bool,
    target_seen_without_slot: bool,
    health_liquidatable: Option<bool>,
    candidates_created: Option<bool>,
    plan_validated: Option<bool>,
    built: Option<bool>,
    identity_status: String,
    our: Option<R>,
    diagnostic: Option<R>,
    market_commitment: String,
    winner_slot_first_seen_ns: Option<u64>,
}
/// Layout of schema 2 (before A2): no commitment, no winner-slot receipt.
#[derive(Deserialize)]
struct JoinedWireV2<R> {
    market: TruthEvent,
    target_seen: bool,
    target_seen_without_slot: bool,
    health_liquidatable: Option<bool>,
    candidates_created: Option<bool>,
    plan_validated: Option<bool>,
    built: Option<bool>,
    identity_status: String,
    our: Option<R>,
    diagnostic: Option<R>,
}
impl JoinedWire<crate::analytics::record::OpportunityRecordV2> {
    /// DEC-12: the records of an earlier layout in the current one.
    fn current(self) -> std::io::Result<JoinedWire> {
        Ok(JoinedWire {
            market: self.market,
            target_seen: self.target_seen,
            target_seen_without_slot: self.target_seen_without_slot,
            health_liquidatable: self.health_liquidatable,
            candidates_created: self.candidates_created,
            plan_validated: self.plan_validated,
            built: self.built,
            identity_status: self.identity_status,
            our: self.our.map(TryInto::try_into).transpose()?,
            diagnostic: self.diagnostic.map(TryInto::try_into).transpose()?,
            market_commitment: self.market_commitment,
            winner_slot_first_seen_ns: self.winner_slot_first_seen_ns,
            capital: self.capital,
        })
    }
}
impl JoinedWire {
    fn into_joined(self) -> Result<Joined, &'static str> {
        let identity_status = IDENTITY_STATUS
            .into_iter()
            .find(|k| *k == self.identity_status)
            .ok_or("IDENTITY_STATUS_UNKNOWN")?;
        Ok(Joined {
            market: self.market,
            target_seen: self.target_seen,
            target_seen_without_slot: self.target_seen_without_slot,
            health_liquidatable: self.health_liquidatable,
            candidates_created: self.candidates_created,
            plan_validated: self.plan_validated,
            built: self.built,
            identity_status,
            our: self.our,
            diagnostic: self.diagnostic,
            market_commitment: self.market_commitment,
            winner_slot_first_seen_ns: self.winner_slot_first_seen_ns,
            capital: self.capital,
        })
    }
}
impl<'de> Deserialize<'de> for Joined {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JoinedWire::deserialize(d)?
            .into_joined()
            .map_err(serde::de::Error::custom)
    }
}
#[cfg(feature = "historical")]
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
    /// A2: margin to the first local receipt of the winner slot.
    pub local_slot_race_margin_ns: Option<i128>,
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
        local_slot_race_margin_ns: slot_margin(row),
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
            .and_then(executable_costs)
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
/// The selected candidate's costs of an executable evaluation: technically executable (signed,
/// simulation PASS, Shadow ready, not handed off) with known economics; else `None`.
pub fn executable_costs(rec: &OpportunityRecord) -> Option<&CandidateCostRecord> {
    rec.selected_costs
        .as_ref()
        .filter(|_| technical(rec) && rec.economics_known)
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
/// Margin to the winner's live receipt; A2: only where that receipt is a race observation.
fn margin(row: &Joined) -> Option<i128> {
    if !rx_margin_valid(&row.market_commitment) {
        return None;
    }
    Some(i128::from(row.market.live_t_rx_ns?) - i128::from(row.our.as_ref()?.timing.t_sign_ns?))
}
/// A2: margin to the first local receipt of the winner slot (positive: signed before it). The
/// same function live and offline; only the receipt map differs in origin (Worker / journal).
fn slot_margin(row: &Joined) -> Option<i128> {
    Some(
        i128::from(row.winner_slot_first_seen_ns?)
            - i128::from(row.our.as_ref()?.timing.t_sign_ns?),
    )
}
/// N1: the loss-attribution model of a report: one bucket per `LossReason`, UNKNOWN only for an
/// undetermined cause. Reports of ECON schema ≤ 3 carry `LOSS_ATTRIBUTION_PRE_N1`; a report
/// keeps the model it was made under (before V9/V17: "EXHAUSTIVE_V1: one bucket per
/// LossReason; UNKNOWN only for an undetermined cause").
pub const LOSS_ATTRIBUTION_MODEL: &str = "EXHAUSTIVE_V3: one bucket per LossReason; PLAN and \
     SAFETY are causes of their own; a stale blockhash is BUILD (cause BLOCKHASH_STALE), decided \
     before RPC_BUDGET/RPC_DEADLINE/ROUTING/PLAN for a lane candidate (DEC-01) and in the late \
     check before the build for any other detection; UNKNOWN only for an undetermined cause";
/// V9/V17, DEC-01: an offline replay that joined shadow evaluations written under an earlier
/// attribution (schema ≤ 3): their causes and priority cannot be redone afterwards.
pub const LOSS_ATTRIBUTION_EARLIER_RECORDS: &str = "EARLIER_ATTRIBUTION_RECORDS: evaluations \
     written under an earlier attribution joined — before V9/V17 (EXHAUSTIVE_V1) their ROUTING \
     includes planning failures without a cause of their own and a missing health engine, their \
     ECONOMIC_SKIP/THRESHOLD_TOO_HIGH include LIVE Safety preview denials; before DEC-01 \
     (EXHAUSTIVE_V2) a stale blockhash is BUILD only after RPC_BUDGET/RPC_DEADLINE/ROUTING/PLAN";
pub const LOSS_ATTRIBUTION_PRE_N1: &str = "PRE_N1 (ECON schema <= 3): UNKNOWN includes the \
     LossReasons Ingress, State, CircuitBreaker, IntegrationRefresh, UnsupportedIntegration, \
     Sizing, Submission, Tip, RpcBudget and RpcDeadline; ECONOMICS includes EconomicSkip, \
     ThresholdTooHigh and technical matches with unknown economics";

/// N1: every bucket the canonical loss attribution can report, pre-seeded with zero rows.
/// (`ADMINISTRATIVE_SETTLEMENT` rows are not tradable and never attributed.)
pub(crate) const LOSS_BUCKETS: [&str; 30] = [
    "NOT_DETECTED",
    "INGRESS",
    "STATE",
    "STATE_RACE",
    "HEALTH_ERROR",
    "CIRCUIT_BREAKER",
    "BANKRUPTCY",
    "INTEGRATION_REFRESH",
    "UNSUPPORTED_INTEGRATION",
    "SIZING",
    "MISSING_ATA",
    "ROUTING",
    "PLAN",
    "FUNDING",
    "CAPITAL",
    "SAFETY",
    "ECONOMICS",
    "ECONOMIC_SKIP",
    "THRESHOLD_TOO_HIGH",
    "ECONOMICS_UNKNOWN",
    "SIMULATION",
    "BUILD",
    "SIGNING",
    "SUBMISSION",
    "TIP",
    "RPC_BUDGET",
    "RPC_DEADLINE",
    "WINNER_TOO_EARLY",
    "NO_PROVEN_TECHNICAL_LOSS",
    "UNKNOWN",
];

/// N1: exhaustive — a new `LossReason` is a compile error here, never a silent UNKNOWN.
pub(crate) fn loss_bucket(reason: LossReason) -> &'static str {
    match reason {
        LossReason::Ingress => "INGRESS",
        LossReason::State => "STATE",
        LossReason::Health => "HEALTH_ERROR",
        LossReason::CircuitBreaker => "CIRCUIT_BREAKER",
        LossReason::Bankruptcy => "BANKRUPTCY",
        LossReason::IntegrationRefresh => "INTEGRATION_REFRESH",
        LossReason::Funding => "FUNDING",
        LossReason::MissingAta => "MISSING_ATA",
        LossReason::UnsupportedIntegration => "UNSUPPORTED_INTEGRATION",
        LossReason::Sizing => "SIZING",
        LossReason::Economics => "ECONOMICS",
        LossReason::Routing => "ROUTING",
        LossReason::Build => "BUILD",
        LossReason::Submission => "SUBMISSION",
        LossReason::Tip => "TIP",
        LossReason::StateRace => "STATE_RACE",
        LossReason::EconomicSkip => "ECONOMIC_SKIP",
        LossReason::ThresholdTooHigh => "THRESHOLD_TOO_HIGH",
        // The recording stage itself could not classify the cause.
        LossReason::Unknown => "UNKNOWN",
        LossReason::RpcBudget => "RPC_BUDGET",
        LossReason::RpcDeadline => "RPC_DEADLINE",
        LossReason::Plan => "PLAN",
        LossReason::Safety => "SAFETY",
    }
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
            return loss_bucket(reason);
        }
        if row.our.is_some() && technical(rec) {
            if !rec.economics_known {
                return "ECONOMICS_UNKNOWN";
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
                    winner_slot_first_seen_ns: self.slots.get(&market.slot).copied(),
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
                    market_commitment: self.commitment.clone(),
                    capital: self.h.cfg.capital.clone(),
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
    /// The canonical rows of a window and the count of rows without an hour.
    fn canonical_rows(&self, from: usize, through: usize) -> (Vec<&Joined>, usize) {
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
        (rows, unassigned)
    }
    pub(super) fn canonical_metrics(&self, from: usize, through: usize) -> CanonicalSummary {
        let (rows, unassigned) = self.canonical_rows(from, through);
        let seconds = self.h.cfg.plan.span_seconds(from, through);
        summarize(
            &rows,
            seconds,
            unassigned,
            self.canonical_finalized,
            &self.h.cfg.capital,
        )
    }
    /// A2: race figures of the same canonical rows (live slot receipts of this run).
    pub(super) fn race_margin_metrics(&self, from: usize, through: usize) -> RaceMargins {
        let (rows, _) = self.canonical_rows(from, through);
        race_margins(&rows, &self.commitment, true)
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
            // DEC-01: a re-armed evaluation keeps its trigger's T_rx; the later one wins.
            .is_none_or(|old| rec.timing.t_rx_ns >= old.timing.t_rx_ns)
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

/// A2: the race figures of a window. Main figure: the canonical margin to the first local
/// receipt of the winner slot. Margins against the winner's live receipt (`canonical.local_*`,
/// `race_timing.*_rx_*`) exist only under `confirmed`; otherwise their status says why not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RaceMargins {
    pub market_commitment: String,
    pub winner_rx_margin_status: String,
    pub winner_slot_margin_status: String,
    /// Population `_all`: every exact-plan match with a signing time, whatever its simulation.
    pub canonical_slot_margin_ms: Distribution,
    pub required_canonical_slot_speedup_ms: Distribution,
    pub canonical_slot_speedup_sensitivity: Vec<SpeedupBucket>,
    /// N5: how the `_technical` figures below were obtained (or why they are absent).
    pub technical_status: String,
    /// N5: population `_technical` — exact-plan matches that were signed and simulated PASS
    /// (`technical`); the main race figure. `None`: not recorded (report before N5).
    pub canonical_slot_margin_ms_technical: Option<Distribution>,
    pub required_canonical_slot_speedup_ms_technical: Option<Distribution>,
}
/// N5: status of the technical race figures.
pub const TECHNICAL_COMPUTED: &str =
    "COMPUTED: exact-plan matches signed and simulated PASS (ShadowReady/ShadowTechnicalReady)";
pub const TECHNICAL_NOT_RECORDED: &str =
    "NOT_RECORDED: report of ECON schema <= 3 (before N5); no technical figures exist";
impl RaceMargins {
    /// A report written before A2 (no race figures recorded).
    pub fn earlier_schema() -> Self {
        RaceMarginsV3::from_layout(race_margins(&[], "UNKNOWN", false)).into()
    }
}
/// Layout of `RaceMargins` in ECON schema 3 (A2, before N5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RaceMarginsV3 {
    market_commitment: String,
    winner_rx_margin_status: String,
    winner_slot_margin_status: String,
    canonical_slot_margin_ms: Distribution,
    required_canonical_slot_speedup_ms: Distribution,
    canonical_slot_speedup_sensitivity: Vec<SpeedupBucket>,
}
impl RaceMarginsV3 {
    fn from_layout(m: RaceMargins) -> Self {
        Self {
            market_commitment: m.market_commitment,
            winner_rx_margin_status: m.winner_rx_margin_status,
            winner_slot_margin_status: m.winner_slot_margin_status,
            canonical_slot_margin_ms: m.canonical_slot_margin_ms,
            required_canonical_slot_speedup_ms: m.required_canonical_slot_speedup_ms,
            canonical_slot_speedup_sensitivity: m.canonical_slot_speedup_sensitivity,
        }
    }
}
impl From<RaceMarginsV3> for RaceMargins {
    /// The recorded figures unchanged; the technical figures did not exist.
    fn from(v: RaceMarginsV3) -> Self {
        Self {
            market_commitment: v.market_commitment,
            winner_rx_margin_status: v.winner_rx_margin_status,
            winner_slot_margin_status: v.winner_slot_margin_status,
            canonical_slot_margin_ms: v.canonical_slot_margin_ms,
            required_canonical_slot_speedup_ms: v.required_canonical_slot_speedup_ms,
            canonical_slot_speedup_sensitivity: v.canonical_slot_speedup_sensitivity,
            technical_status: TECHNICAL_NOT_RECORDED.into(),
            canonical_slot_margin_ms_technical: None,
            required_canonical_slot_speedup_ms_technical: None,
        }
    }
}

/// N5: margin samples (ms) and the required speedups of the negative ones.
fn margin_samples<'a>(
    rows: impl Iterator<Item = &'a &'a Joined>,
    margin: impl Fn(&Joined) -> Option<i128>,
) -> (Vec<f64>, Vec<f64>) {
    let mut margins = Vec::new();
    let mut speed = Vec::new();
    for row in rows {
        if let Some(n) = margin(row) {
            margins.push(n as f64 / 1e6);
            if n < 0 {
                speed.push((-n) as f64 / 1e6);
            }
        }
    }
    (margins, speed)
}

/// N5: the technically executable rows: exact-plan matches signed and simulated PASS.
fn technical_row(row: &Joined) -> bool {
    row.our.as_ref().is_some_and(technical)
}

/// A2: the race figures over canonical rows (live and offline the same function).
pub fn race_margins(rows: &[&Joined], market_commitment: &str, slot_receipts: bool) -> RaceMargins {
    let tradable = || {
        rows.iter()
            .filter(|r| r.market.permissionless_profit_opportunity)
    };
    let (margins, speed) = margin_samples(tradable(), slot_margin);
    let (technical_margins, technical_speed) =
        margin_samples(tradable().filter(|r| technical_row(r)), slot_margin);
    RaceMargins {
        market_commitment: market_commitment.into(),
        winner_rx_margin_status: rx_margin_status(market_commitment),
        winner_slot_margin_status: if slot_receipts {
            "COMPUTED"
        } else {
            "NOT_AVAILABLE: the input has no first local slot receipts (SlotBoundary rows of the market journal)"
        }
        .into(),
        canonical_slot_margin_ms: distribution(margins),
        required_canonical_slot_speedup_ms: distribution(speed.clone()),
        canonical_slot_speedup_sensitivity: SPEEDUP_MS
            .iter()
            .map(|ms| SpeedupBucket {
                ms: *ms,
                local_boundaries_crossed: speed.iter().filter(|s| **s <= *ms as f64).count() as u64,
                claim: "COUNTERFACTUAL LOCAL BOUNDARY ANALYSIS (first local winner-slot receipt)"
                    .into(),
            })
            .collect(),
        technical_status: TECHNICAL_COMPUTED.into(),
        canonical_slot_margin_ms_technical: Some(distribution(technical_margins)),
        required_canonical_slot_speedup_ms_technical: Some(distribution(technical_speed)),
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
    /// Population `_all`: every exact-plan match with a signing time, whatever its simulation.
    pub local_margin_ms: Distribution,
    pub required_local_speedup_ms: Distribution,
    pub speedup_sensitivity: Vec<SpeedupBucket>,
    pub competition: CanonicalCompetition,
    pub true_competitor_submission_deadline: String,
    pub live_ready: bool,
    /// N1: the model `loss_attribution` was attributed under.
    pub loss_attribution_model: String,
    /// N5: population `_technical` (signed, simulation PASS); `None`: not recorded (before N5).
    pub local_margin_ms_technical: Option<Distribution>,
    pub required_local_speedup_ms_technical: Option<Distribution>,
}
/// Layout of `CanonicalSummary` in ECON schema 2 and 3 (before N1/N5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalSummaryV3 {
    finalized: bool,
    market: BTreeMap<String, CanonicalGroup>,
    authoritative_scope: String,
    market_window_hour_assignment: String,
    hour_unassigned_events_full_window: u64,
    unassigned_included_in_final_total: bool,
    matching: CanonicalMatching,
    network_fees: NetworkFees,
    capital: Vec<CanonicalCapital>,
    loss_attribution: BTreeMap<String, LossBucket>,
    largest_measured_stage: BTreeMap<String, StageBucket>,
    race: BTreeMap<String, RaceBucket>,
    matched_stage_ms: BTreeMap<String, Distribution>,
    local_margin_ms: Distribution,
    required_local_speedup_ms: Distribution,
    speedup_sensitivity: Vec<SpeedupBucket>,
    competition: CanonicalCompetition,
    true_competitor_submission_deadline: String,
    live_ready: bool,
}
impl From<CanonicalSummaryV3> for CanonicalSummary {
    /// The recorded figures unchanged, labelled with the attribution model they were made
    /// under; the technical figures did not exist.
    fn from(v: CanonicalSummaryV3) -> Self {
        Self {
            finalized: v.finalized,
            market: v.market,
            authoritative_scope: v.authoritative_scope,
            market_window_hour_assignment: v.market_window_hour_assignment,
            hour_unassigned_events_full_window: v.hour_unassigned_events_full_window,
            unassigned_included_in_final_total: v.unassigned_included_in_final_total,
            matching: v.matching,
            network_fees: v.network_fees,
            capital: v.capital,
            loss_attribution: v.loss_attribution,
            largest_measured_stage: v.largest_measured_stage,
            race: v.race,
            matched_stage_ms: v.matched_stage_ms,
            local_margin_ms: v.local_margin_ms,
            required_local_speedup_ms: v.required_local_speedup_ms,
            speedup_sensitivity: v.speedup_sensitivity,
            competition: v.competition,
            true_competitor_submission_deadline: v.true_competitor_submission_deadline,
            live_ready: v.live_ready,
            loss_attribution_model: LOSS_ATTRIBUTION_PRE_N1.into(),
            local_margin_ms_technical: None,
            required_local_speedup_ms_technical: None,
        }
    }
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
    capital: &[u64],
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
    let capital:Vec<_>=capital.iter().map(|budget| {
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
    let mut reasons: BTreeMap<&str, Vec<&Joined>> =
        LOSS_BUCKETS.into_iter().map(|k| (k, vec![])).collect();
    let mut race: BTreeMap<&str, Vec<&Joined>> = BTreeMap::new();
    let mut stages: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let mut bottlenecks: BTreeMap<&str, Vec<&Joined>> = BTreeMap::new();
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
    }
    let (margins, speed) = margin_samples(tradable.iter(), margin);
    let (technical_margins, technical_speed) =
        margin_samples(tradable.iter().filter(|r| technical_row(r)), margin);
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
        loss_attribution_model: LOSS_ATTRIBUTION_MODEL.into(),
        local_margin_ms_technical: Some(distribution(technical_margins)),
        required_local_speedup_ms_technical: Some(distribution(technical_speed)),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::view;
    use super::*;
    #[test]
    fn missing_values_have_no_zero_total() {
        let s = view(&summarize(&[], 3600., 0, true, &CAPITAL_USD));
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
    use std::fs;
    include!("econ_a2_tests.rs");
    include!("econ_a3_tests.rs");
    include!("econ_a6_tests.rs");
    /// V9/V17: shadow evaluations of the earlier writer stay readable and keep their
    /// attribution. The pinned schema-2 journal (written by the writer before V9/V17) decodes
    /// to its record; an offline replay over it labels its summaries `EXHAUSTIVE_V1_RECORDS`;
    /// a schema-2 frame or a JSONL line with a cause added since (PLAN/SAFETY) is forged and
    /// refused, while the same bytes are valid at schema 3.
    #[test]
    fn v17_earlier_writer_evaluations_keep_their_attribution() {
        use crate::analytics::payload;
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/analytics/fixtures/shadow_evaluations_schema2.p0j");
        assert_eq!(
            journal::hex(&journal::file_sha256(&fixture).unwrap()),
            "8cf15a2863346a9f51dff108a8726339d7b3c498b8e3591148df9e9110757b3d"
        );
        let chain = crate::storage::segments::verify_chain(&fixture).unwrap();
        assert!(chain.sealed && chain.complete);
        let mut reader = journal::Reader::open(&fixture).unwrap();
        let mut records = Vec::new();
        while let Some(f) = reader.next_frame().unwrap() {
            if f.kind == journal::MANIFEST {
                continue;
            }
            assert_eq!(
                (f.kind, f.schema),
                (journal::kind::ECON_SHADOW_EVALUATION, 2)
            );
            let v: super::super::ShadowEvaluation =
                payload::decode(f.kind, f.schema, &f.payload).unwrap();
            // The earlier layout (DEC-12: schemas 2–4) forged in place: the record's loss is
            // the last field of the frame.
            for cause in [LossReason::Plan, LossReason::Safety] {
                let bytes = with_loss(&f.payload, cause);
                assert!(
                    payload::decode::<super::super::ShadowEvaluation>(f.kind, 2, &bytes).is_err()
                );
                for schema in [3, 4] {
                    let back: super::super::ShadowEvaluation =
                        payload::decode(f.kind, schema, &bytes).unwrap();
                    assert_eq!(back.record.loss, Some(cause));
                }
            }
            records.push(v.record);
        }
        assert_eq!(records.len(), 1);
        let r = records.remove(0);
        assert_eq!(
            (r.loss, r.reason_text().as_deref()),
            (Some(LossReason::Routing), Some("mint not in state"))
        );
        let run = std::env::temp_dir().join(format!("p0-v17-earlier-{}", std::process::id()));
        let _ = fs::remove_dir_all(&run);
        fs::create_dir_all(&run).unwrap();
        let shadow = run.join("shadow_evaluations.p0j");
        fs::copy(&fixture, &shadow).unwrap();
        let mut event = row(&r, 10, 0).market;
        event.block_time = Some(101);
        let ctx = JoinContext {
            market_commitment: "UNKNOWN",
            slot_first_seen: None,
            capture: None,
            capital: &CAPITAL_USD,
        };
        let deadline = || std::time::Instant::now() + std::time::Duration::from_secs(10);
        let joined = join(vec![event.clone()], &shadow, deadline(), &ctx).unwrap();
        assert_eq!(joined.earlier_attribution_records, 1);
        // The native backtest labels the summary with the earlier model.
        let rows: Vec<_> = joined
            .rows
            .iter()
            .map(crate::analytics::backtest::row_of)
            .collect();
        let refs: Vec<_> = rows.iter().collect();
        let params = crate::analytics::backtest::Params {
            min_profit_usd_micro: 0,
            capital_usd_micro: vec![1_000_000],
            funding: vec![],
        };
        let c = crate::analytics::backtest::counterfactual(
            &refs,
            &[],
            &params,
            crate::analytics::backtest::loss_model(joined.earlier_attribution_records),
        );
        assert_eq!(c.loss_attribution_model, LOSS_ATTRIBUTION_EARLIER_RECORDS);
        assert_eq!(c.loss_attribution.get("ROUTING"), Some(&1));
        assert_eq!(c.loss_attribution.get("PLAN"), None);
        // A JSONL evaluation file (before PERSIST-01) is historical evidence: never a join input.
        let legacy = run.join("legacy.jsonl");
        fs::write(&legacy, b"{}\n").unwrap();
        assert_eq!(
            join(vec![event], &legacy, deadline(), &ctx)
                .err()
                .as_deref(),
            Some("SHADOW_HISTORICAL_JSONL_INPUT")
        );
        let _ = fs::remove_dir_all(run);
    }
    /// An earlier-layout frame (`ShadowEvaluation` before DEC-12) with another loss: the record
    /// ends with its `Option<LossReason>` (tag 1, then the variant index as u32 LE).
    fn with_loss(payload: &[u8], loss: LossReason) -> Vec<u8> {
        let index = LossReason::ALL.iter().position(|l| *l == loss).unwrap() as u32;
        let mut bytes = payload.to_vec();
        let at = bytes.len() - 5;
        assert_eq!(bytes[at], 1, "Some(loss) ends the frame");
        bytes[at + 1..].copy_from_slice(&index.to_le_bytes());
        bytes
    }
    /// DEC-01: the loss statistic carries its policy version and never mixes them. A shadow
    /// evaluation of the writer before DEC-01 (schema 3, EXHAUSTIVE_V2: a stale blockhash is
    /// BUILD only after RPC_BUDGET/RPC_DEADLINE/ROUTING/PLAN) stays readable with every cause
    /// and a replay joining it carries the earlier label; the same record written now
    /// (schema 4, EXHAUSTIVE_V3) carries the current model.
    #[test]
    fn dec01_evaluations_of_both_loss_policies_are_never_mixed() {
        use crate::analytics::payload;
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/analytics/fixtures/shadow_evaluations_schema2.p0j");
        let mut reader = journal::Reader::open(&fixture).unwrap();
        let raw = loop {
            let f = reader.next_frame().unwrap().unwrap();
            if f.kind != journal::MANIFEST {
                break f.payload;
            }
        };
        let mut v: super::super::ShadowEvaluation =
            payload::decode(journal::kind::ECON_SHADOW_EVALUATION, 2, &raw).unwrap();
        v.record.loss = Some(LossReason::Build);
        v.record.reason = Some(crate::analytics::rejection::Rejection::BlockhashStaleLate);
        // Schemas 3 and 4 carry the record layout before DEC-12, schema 5 the current one.
        let earlier_layout = with_loss(&raw, LossReason::Build);
        let current_layout = payload::encode_wire(&v).unwrap();
        let bytes = |schema: u16| {
            if schema < 5 {
                earlier_layout.clone()
            } else {
                current_layout.clone()
            }
        };
        let mut event = row(&v.record, 10, 0).market;
        event.block_time = Some(101);
        let ctx = JoinContext {
            market_commitment: "UNKNOWN",
            slot_first_seen: None,
            capture: None,
            capital: &CAPITAL_USD,
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for (schema, earlier, model) in [
            (3u16, 1u64, LOSS_ATTRIBUTION_EARLIER_RECORDS),
            (4, 0, LOSS_ATTRIBUTION_MODEL),
            (5, 0, LOSS_ATTRIBUTION_MODEL),
        ] {
            let run = std::env::temp_dir()
                .join(format!("p0-dec01-policy-{}-{schema}", std::process::id()));
            let _ = fs::remove_dir_all(&run);
            fs::create_dir_all(&run).unwrap();
            let shadow = run.join("shadow_evaluations.p0j");
            let mut w = journal::Writer::create(&shadow).unwrap();
            w.test_append_raw(
                journal::kind::ECON_SHADOW_EVALUATION,
                schema,
                &bytes(schema),
            )
            .unwrap();
            w.seal(true).unwrap();
            let joined = join(vec![event.clone()], &shadow, deadline, &ctx).unwrap();
            assert_eq!(
                joined.rows[0].our.as_ref().unwrap().loss,
                Some(LossReason::Build)
            );
            assert_eq!(
                joined.earlier_attribution_records, earlier,
                "schema {schema}"
            );
            assert_eq!(
                crate::analytics::backtest::loss_model(joined.earlier_attribution_records),
                model
            );
            let _ = fs::remove_dir_all(run);
        }
        // Both policies in one journal are refused, never added into the same buckets; two
        // layouts of the same policy (DEC-12: schemas 4 and 5) join.
        for (schemas, mixed) in [([5u16, 3], true), ([4, 3], true), ([4, 5], false)] {
            let run = std::env::temp_dir().join(format!(
                "p0-dec01-mixed-{}-{}{}",
                std::process::id(),
                schemas[0],
                schemas[1]
            ));
            let _ = fs::remove_dir_all(&run);
            fs::create_dir_all(&run).unwrap();
            let shadow = run.join("shadow_evaluations.p0j");
            let mut w = journal::Writer::create(&shadow).unwrap();
            for schema in schemas {
                w.test_append_raw(
                    journal::kind::ECON_SHADOW_EVALUATION,
                    schema,
                    &bytes(schema),
                )
                .unwrap();
            }
            w.seal(true).unwrap();
            let joined = join(vec![event.clone()], &shadow, deadline, &ctx);
            assert_eq!(
                joined.err().as_deref(),
                mixed.then_some("SHADOW_LOSS_POLICY_MIXED"),
                "{schemas:?}"
            );
            let _ = fs::remove_dir_all(run);
        }
        assert!(LOSS_ATTRIBUTION_MODEL.starts_with("EXHAUSTIVE_V3"));
        assert!(LOSS_ATTRIBUTION_MODEL.contains("for a lane candidate"));
        assert!(LOSS_ATTRIBUTION_EARLIER_RECORDS.contains("before DEC-01 (EXHAUSTIVE_V2)"));
    }
    /// DEC-12: canonical rows written before DEC-12 (schema 4, the record layout of the earlier
    /// writer) stay readable with their records' texts; the current schema round-trips them.
    #[test]
    fn dec12_canonical_rows_of_the_earlier_layout_stay_readable() {
        use crate::analytics::payload::{self, Payload};
        use crate::analytics::record::OpportunityRecordV2;
        let bytes = crate::analytics::record::earlier_writer_record_bytes();
        let rec: OpportunityRecord =
            payload::decode(journal::kind::OWNER_OPPORTUNITY, 2, &bytes).unwrap();
        let wire = JoinedWire::<OpportunityRecordV2> {
            market: row(&rec, 10, 0).market,
            target_seen: true,
            target_seen_without_slot: false,
            health_liquidatable: Some(true),
            candidates_created: None,
            plan_validated: None,
            built: None,
            identity_status: "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT".into(),
            our: Some(payload::decode_wire(&bytes).unwrap()),
            diagnostic: Some(payload::decode_wire(&bytes).unwrap()),
            market_commitment: "confirmed".into(),
            winner_slot_first_seen_ns: Some(5),
            capital: vec![1_000],
        };
        let earlier = payload::encode_wire(&wire).unwrap();
        let j: Joined = payload::decode(Joined::KIND, 4, &earlier).unwrap();
        assert_eq!(
            j.our.as_ref().unwrap().reason_text().as_deref(),
            Some("mint not in state")
        );
        assert_eq!(
            (j.capital.as_slice(), j.market_commitment.as_str()),
            (&[1_000][..], "confirmed")
        );
        let current = payload::encode(&j).unwrap();
        let back: Joined = payload::decode(Joined::KIND, Joined::SCHEMA, &current).unwrap();
        assert_eq!(back.our.unwrap().reason, j.our.unwrap().reason);
    }
    /// DEC-01: a re-armed evaluation keeps its trigger's T_rx; of two evaluations of the same
    /// trigger the later one is the episode's diagnostic (before: the refusal stayed).
    #[test]
    fn dec01_the_later_evaluation_of_the_same_trigger_is_the_diagnostic() {
        let refused = OpportunityRecord {
            loss: Some(LossReason::Build),
            reason: Some(crate::analytics::rejection::Rejection::BlockhashStaleEarly),
            market_plan_identity: None,
            ..rec()
        };
        let later = OpportunityRecord {
            loss: Some(LossReason::StateRace),
            reason: Some(crate::analytics::rejection::Rejection::OpportunityStale),
            market_plan_identity: None,
            ..rec()
        };
        assert_eq!(refused.timing.t_rx_ns, later.timing.t_rx_ns);
        let mut rows = vec![row(&refused, 10, 0)];
        assign_record(&mut rows, &refused);
        assign_record(&mut rows, &later);
        assert_eq!(
            rows[0].diagnostic.as_ref().map(|d| d.loss),
            Some(Some(LossReason::StateRace))
        );
    }
    #[cfg(feature = "historical")]
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
            market_commitment: "confirmed".into(),
            winner_slot_first_seen_ns: None,
            capital: CAPITAL_USD.to_vec(),
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
    /// N1: every `LossReason` has its own pre-seeded bucket (UNKNOWN only for `Unknown`), and a
    /// detected target lost for a reason the former mapping dropped into UNKNOWN is reported
    /// under that reason.
    #[test]
    fn n1_every_loss_reason_has_its_own_bucket() {
        use LossReason as L;
        let reasons = [
            L::Ingress,
            L::State,
            L::Health,
            L::CircuitBreaker,
            L::Bankruptcy,
            L::IntegrationRefresh,
            L::Funding,
            L::MissingAta,
            L::UnsupportedIntegration,
            L::Sizing,
            L::Economics,
            L::Routing,
            L::Build,
            L::Submission,
            L::Tip,
            L::StateRace,
            L::EconomicSkip,
            L::ThresholdTooHigh,
            L::Unknown,
            L::RpcBudget,
            L::RpcDeadline,
            L::Plan,
            L::Safety,
        ];
        // Exhaustive like `loss_bucket`: a new variant must be added to `reasons` above.
        for r in reasons {
            match r {
                L::Ingress | L::State | L::Health | L::CircuitBreaker | L::Bankruptcy => {}
                L::IntegrationRefresh | L::Funding | L::MissingAta => {}
                L::UnsupportedIntegration | L::Sizing | L::Economics | L::Routing => {}
                L::Build | L::Submission | L::Tip | L::StateRace | L::EconomicSkip => {}
                L::ThresholdTooHigh | L::Unknown | L::RpcBudget | L::RpcDeadline => {}
                L::Plan | L::Safety => {}
            }
        }
        let mut buckets: Vec<_> = reasons.iter().map(|r| loss_bucket(*r)).collect();
        for (r, b) in reasons.iter().zip(&buckets) {
            assert!(LOSS_BUCKETS.contains(b), "{b} pre-seeded");
            assert_eq!(*b == "UNKNOWN", *r == L::Unknown, "{r:?}");
        }
        buckets.sort_unstable();
        buckets.dedup();
        assert_eq!(buckets.len(), reasons.len(), "one bucket per reason");
        for (reason, bucket) in [
            (L::RpcBudget, "RPC_BUDGET"),
            (L::RpcDeadline, "RPC_DEADLINE"),
            (L::Tip, "TIP"),
            (L::Submission, "SUBMISSION"),
            (L::CircuitBreaker, "CIRCUIT_BREAKER"),
            (L::IntegrationRefresh, "INTEGRATION_REFRESH"),
            (L::UnsupportedIntegration, "UNSUPPORTED_INTEGRATION"),
            (L::Ingress, "INGRESS"),
            (L::State, "STATE"),
            (L::Sizing, "SIZING"),
            (L::EconomicSkip, "ECONOMIC_SKIP"),
            (L::ThresholdTooHigh, "THRESHOLD_TOO_HIGH"),
            (L::Plan, "PLAN"),
            (L::Safety, "SAFETY"),
        ] {
            let mut r = identified();
            r.loss = Some(reason);
            let mut rows = vec![row(&r, 10, 0)];
            assign_record(&mut rows, &r);
            let s = summarize(&[&rows[0]], 3600., 0, true, &CAPITAL_USD);
            assert_eq!(s.loss_attribution[bucket].count, 1, "{reason:?}");
            assert_eq!(s.loss_attribution["UNKNOWN"].count, 0, "{reason:?}");
            assert_eq!(s.loss_attribution.len(), LOSS_BUCKETS.len(), "{reason:?}");
            assert_eq!(s.loss_attribution_model, LOSS_ATTRIBUTION_MODEL);
        }
    }

    /// N5: the `_technical` race figures count only exact-plan matches that were signed and
    /// simulated PASS; the `_all` figures keep every exact-plan match with a signing time; no
    /// technical sample is an empty distribution (UNKNOWN values), a report before N5 has none.
    #[test]
    fn n5_technical_race_figures_count_only_signed_simulated_matches() {
        let pass = identified();
        let mut failed = identified();
        failed.simulation_result = Some("FAIL".into());
        let mut rows = vec![row(&pass, 10, 0), row(&failed, 10, 1)];
        assign_record(&mut rows[..1], &pass);
        assign_record(&mut rows[1..], &failed);
        for r in &mut rows {
            assert!(r.our.is_some());
            r.winner_slot_first_seen_ns = Some(100);
        }
        let both = [&rows[0], &rows[1]];
        let m = race_margins(&both, "confirmed", true);
        assert_eq!(m.canonical_slot_margin_ms.count_known, 2, "_all unchanged");
        let tech = m.canonical_slot_margin_ms_technical.as_ref().unwrap();
        assert_eq!(tech.count_known, 1);
        assert_eq!(m.technical_status, TECHNICAL_COMPUTED);
        let s = summarize(&both, 3600., 0, true, &CAPITAL_USD);
        assert_eq!(s.local_margin_ms.count_known, 2, "_all unchanged");
        assert_eq!(s.local_margin_ms_technical.as_ref().unwrap().count_known, 1);
        let only_failed = race_margins(&[&rows[1]], "confirmed", true);
        let none = only_failed.canonical_slot_margin_ms_technical.unwrap();
        assert_eq!((none.count_known, none.median), (0, None));
        let earlier = RaceMargins::earlier_schema();
        assert!(earlier.canonical_slot_margin_ms_technical.is_none());
        assert!(earlier
            .required_canonical_slot_speedup_ms_technical
            .is_none());
        assert_eq!(earlier.technical_status, TECHNICAL_NOT_RECORDED);
    }

    /// The native join over the live writer's typed evaluation journal selects exactly what the
    /// live selector selected (identical row facts); a JSONL evaluation file — alone or beside a
    /// binary journal — is refused, never merged, guessed or read; events without any
    /// evaluation journal fail closed.
    #[test]
    fn native_join_uses_the_live_selector_and_refuses_jsonl() {
        let mut w = worker();
        let r = identified();
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
        let mut event = row(&r, 10, 0).market;
        event.block_time = Some(101);
        let shadow = journal::binary_path(&w.h.cfg.run_dir.join("shadow_evaluations.jsonl"));
        let commitment = row(&r, 10, 0).market_commitment;
        let ctx = JoinContext {
            market_commitment: &commitment,
            slot_first_seen: None,
            capture: None,
            capital: &CAPITAL_USD,
        };
        let deadline = || std::time::Instant::now() + std::time::Duration::from_secs(10);
        let joined = join(vec![event.clone()], &shadow, deadline(), &ctx).unwrap();
        let mut expected = vec![row(&r, 10, 0)];
        expected[0].market = event.clone();
        assign_record(&mut expected, &r);
        assert_eq!(joined_facts(&joined.rows[0]), joined_facts(&expected[0]));
        assert_eq!(
            joined.rows[0].identity_status,
            "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT"
        );
        assert_eq!(joined.earlier_attribution_records, 0);
        let legacy_dir = w.h.cfg.run_dir.join("legacy");
        fs::create_dir(&legacy_dir).unwrap();
        let legacy = legacy_dir.join("shadow_evaluations.jsonl");
        fs::write(&legacy, b"{}\n").unwrap();
        for _ in 0..2 {
            assert_eq!(
                join(vec![event.clone()], &legacy, deadline(), &ctx)
                    .err()
                    .as_deref(),
                Some("SHADOW_HISTORICAL_JSONL_INPUT")
            );
            // Beside a binary journal it is refused as well.
            let _ = fs::copy(&shadow, journal::binary_path(&legacy));
        }
        assert_eq!(
            join(
                vec![event],
                &legacy_dir.join("absent.p0j"),
                deadline(),
                &ctx
            )
            .err()
            .as_deref(),
            Some("SHADOW_JOURNAL_MISSING")
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
        let s = view(&summarize(&[&rows[0]], 3600., 0, true, &CAPITAL_USD));
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
        let s = view(&summarize(&[&rows[0]], 3600., 0, true, &CAPITAL_USD));
        assert_eq!(s["capital"][0]["fundable"], 0); // fee reserve makes exactly $200 insufficient
        assert_eq!(s["capital"][0]["lost_only_because_capital"], 1);
        assert_eq!(s["capital"][1]["fundable"], 1);
        assert_eq!(s["capital"][1]["known_profitable"], 1);
        rows[0].market.permissionless_profit_opportunity = false;
        rows[0].market.liquidation_type = LiquidationType::ForcedDeleverage;
        let s = view(&summarize(&[&rows[0]], 3600., 0, true, &CAPITAL_USD));
        assert_eq!(s["market"]["ALL"]["events"], 1);
        assert_eq!(s["capital"][0]["market_opportunities"], 0);
    }
    #[test]
    fn transaction_fee_dedup_unknown_costs_and_loss_dollars() {
        let r = identified();
        let a = row(&r, 10, 0);
        let mut b = row(&r, 10, 1);
        b.market.gross_edge_usd = None;
        let s = view(&summarize(&[&a, &b], 3600., 0, true, &CAPITAL_USD));
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

    #[cfg(feature = "historical")]
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
            observed_slots: Some((10, 12)),
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

#[cfg(feature = "historical")]
fn joined_value(row: &Joined) -> Value {
    json!({"join":row,"race_class":race_class(row),"local_observed_race_margin_ns":margin(row),"local_slot_race_margin_ns":slot_margin(row),"loss_reason":loss(row),
                    "signed":row.our.as_ref().map(|r|r.timing.t_sign_ns.is_some()),
                    "simulation_result":row.our.as_ref().and_then(|r|r.simulation_result.as_deref()),
                    "economic_net_usd":net(row.our.as_ref()),"economics_known":row.our.as_ref().map(|r|r.economics_known),
                    "fundable_by_budget":row.capital.iter().map(|budget|json!({"budget_usd":budget,"fundable":row.our.as_ref().and_then(|r|r.selected_costs.as_ref()).and_then(CandidateCostRecord::total_budget).map(|n|n<=budget*1_000_000)})).collect::<Vec<_>>(),
                    "loss_provenance":if row.our.is_some(){"EXACT_PLAN_MATCH"}else if row.diagnostic.is_some(){"TARGET_EPISODE_DIAGNOSTIC; executable identity unproven"}else{"NO_TARGET_EPISODE_RECORD"}})
}

/// Result of the canonical join over finalized events.
pub struct JoinOutcome {
    /// One row per event, in chain order.
    pub rows: Vec<Joined>,
    /// Whether an evaluation journal existed (absence with events present is an error).
    pub shadow_available: bool,
    /// V9/V17: joined evaluations of an earlier typed writer (schema 2 frames), attributed
    /// under the earlier model.
    pub earlier_attribution_records: u64,
}

/// The production episode/plan selector over finalized events and the run's evaluation stream:
/// the typed binary journal (`shadow_evaluations.p0j` and its linked segments) only — a JSONL
/// evaluation file or a schema-1 value frame is refused (JFI-01/03), never read. One bounded
/// streaming pass; no RPC repricing/re-simulation and no synthetic evaluation when the original
/// capture lost data. Used by the native report (B7) and the native backtest.
/// A3: the capture window an evaluation must lie in to join, the rule of the live chain
/// (`Worker::hour`): monotone `capture_start_ns`/`capture_cutoff_ns` and the plan.
#[derive(Clone, Copy)]
pub struct CaptureFilter<'a> {
    pub plan: &'a MeasurementPlan,
    pub start_ns: u64,
    pub cutoff_ns: u64,
}
impl CaptureFilter<'_> {
    pub(crate) fn admits(&self, rec: &OpportunityRecord) -> bool {
        rec.timing
            .t_rx_ns
            .and_then(|t| live_hour(self.plan, self.start_ns, self.cutoff_ns, t))
            .is_some()
    }
}

/// Run context of the offline join. A2: the market commitment (capture start record) and the
/// first in-window receipt per slot (`SlotBoundary` rows of the market journal), if read. A3:
/// the capture window of the evaluations (`None`: every evaluation frame joins).
pub struct JoinContext<'a> {
    pub market_commitment: &'a str,
    pub slot_first_seen: Option<&'a BTreeMap<u64, u64>>,
    pub capture: Option<CaptureFilter<'a>>,
    /// A6: capital scenarios of the run (USD).
    pub capital: &'a [u64],
}

pub fn join(
    events: Vec<TruthEvent>,
    shadow_path: &Path,
    deadline: std::time::Instant,
    ctx: &JoinContext,
) -> Result<JoinOutcome, String> {
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
                winner_slot_first_seen_ns: ctx
                    .slot_first_seen
                    .and_then(|m| m.get(&market.slot).copied()),
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
                market_commitment: ctx.market_commitment.into(),
                capital: ctx.capital.to_vec(),
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
    if binary != shadow_path && shadow_path.exists() {
        // A pre-PERSIST-01 JSONL evaluation file is historical evidence, never a join input.
        return Err("SHADOW_HISTORICAL_JSONL_INPUT".into());
    }
    let available = binary.exists();
    // V9/V17: a writer before them never wrote a cause added since; such a record is forged.
    let earlier = |rec: &OpportunityRecord, schema: u16| {
        if schema <= 2 && rec.loss.is_some_and(LossReason::since_exhaustive_v2) {
            Err("SHADOW_DECODE".to_string())
        } else {
            Ok(())
        }
    };
    let mut earlier_attribution_records = 0u64;
    // DEC-01: evaluations of different loss policies are never joined together; a journal
    // mixing the current schema with an earlier one is refused (earlier ones alone are labelled).
    let (mut current_frames, mut earlier_frames) = (0u64, 0u64);
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
                    // Typed frames only (JFI-FIX-05); a schema-1 value tree is historical.
                    if frame.schema == 1 {
                        return Err("SHADOW_HISTORICAL_SCHEMA_1".into());
                    }
                    let rec: OpportunityRecord = frame
                        .typed::<super::ShadowEvaluation>()
                        .map_err(|_| "SHADOW_DECODE")?
                        .record;
                    let from_earlier_writer = frame.schema < super::SHADOW_POLICY_V3_SCHEMA;
                    if from_earlier_writer {
                        earlier(&rec, frame.schema)?;
                        earlier_frames += 1;
                    } else {
                        current_frames += 1;
                    }
                    if current_frames > 0 && earlier_frames > 0 {
                        return Err("SHADOW_LOSS_POLICY_MIXED".into());
                    }
                    if ctx.capture.is_some_and(|c| !c.admits(&rec)) {
                        continue;
                    }
                    if let Some(rows) = grouped.get_mut(&rec.account) {
                        assign_record(rows, &rec);
                        if from_earlier_writer {
                            earlier_attribution_records += 1;
                        }
                    }
                }
                journal::MANIFEST => {}
                _ => return Err("SHADOW_DECODE".into()),
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
        earlier_attribution_records,
    })
}
