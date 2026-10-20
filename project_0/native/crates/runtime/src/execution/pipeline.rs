//! Die Kette. Jede Stufe setzt ihren Timing-Punkt; Fehler sind Records, keine Panics.

use crate::analytics::record::{Decision, LossReason, OpportunityRecord};
use darkbot_common::timing::{Clock, Stage, Timeline};
use darkbot_project0::economics::{self, CandidateInput, Evaluation, FailedCostMatrix, Path};
use darkbot_project0::execution::Freshness;
use darkbot_project0::health::{HealthEngine, HealthError};
use darkbot_project0::planner::{self, Allowlist, InstructionTable, RouteKind};
use darkbot_project0::safety::Safety;
use darkbot_solana::state::blockhash::BlockhashCache;
use darkbot_solana::submit::{self as submit, SimResult, SubmissionAdapter, SubmissionPath};
use darkbot_solana::tx::{self, BuildVersion, Resources, TxLimits, TxPlan};
use solana_message::AddressLookupTableAccount; /* [FIX-13] */
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub struct PipelineTimingSnapshot {
    pub planning_calls: u64,
    pub planning_ns_total: u64,
    pub planning_ns_max: u64,
    pub fee_calls: u64,
    pub fee_ns_total: u64,
    pub fee_ns_max: u64,
    pub simulation_calls: u64,
    pub simulation_ns_total: u64,
    pub simulation_ns_max: u64,
}

static PLANNING_CALLS: AtomicU64 = AtomicU64::new(0);
static PLANNING_NS_TOTAL: AtomicU64 = AtomicU64::new(0);
static PLANNING_NS_MAX: AtomicU64 = AtomicU64::new(0);
static FEE_CALLS: AtomicU64 = AtomicU64::new(0);
static FEE_NS_TOTAL: AtomicU64 = AtomicU64::new(0);
static FEE_NS_MAX: AtomicU64 = AtomicU64::new(0);
static SIMULATION_CALLS: AtomicU64 = AtomicU64::new(0);
static SIMULATION_NS_TOTAL: AtomicU64 = AtomicU64::new(0);
static SIMULATION_NS_MAX: AtomicU64 = AtomicU64::new(0);

fn record_timing(calls: &AtomicU64, total: &AtomicU64, max: &AtomicU64, elapsed_ns: u64) {
    calls.fetch_add(1, Ordering::Relaxed);
    total.fetch_add(elapsed_ns, Ordering::Relaxed);
    max.fetch_max(elapsed_ns, Ordering::Relaxed);
}

pub fn pipeline_timing_snapshot() -> PipelineTimingSnapshot {
    PipelineTimingSnapshot {
        planning_calls: PLANNING_CALLS.load(Ordering::Relaxed),
        planning_ns_total: PLANNING_NS_TOTAL.load(Ordering::Relaxed),
        planning_ns_max: PLANNING_NS_MAX.load(Ordering::Relaxed),
        fee_calls: FEE_CALLS.load(Ordering::Relaxed),
        fee_ns_total: FEE_NS_TOTAL.load(Ordering::Relaxed),
        fee_ns_max: FEE_NS_MAX.load(Ordering::Relaxed),
        simulation_calls: SIMULATION_CALLS.load(Ordering::Relaxed),
        simulation_ns_total: SIMULATION_NS_TOTAL.load(Ordering::Relaxed),
        simulation_ns_max: SIMULATION_NS_MAX.load(Ordering::Relaxed),
    }
}

const LAMPORTS_PER_SIGNATURE: u64 = 5_000;
const COMPUTE_BUDGET_PROGRAM_ID: &str = "ComputeBudget111111111111111111111111111111";
const DEFAULT_CU_PER_INSTRUCTION: u64 = 200_000;
const MAX_CU_LIMIT: u64 = 1_400_000;
const MICRO_LAMPORTS_PER_LAMPORT: u128 = 1_000_000;

/// [MT0] H6 measurement only (never used for a decision): local fee estimate for legacy/v0
/// messages = signatures x 5_000 + ceil(CU limit x CU price / 10^6). `None` for other formats.
fn local_fee_estimate_lamports(tx: &solana_sdk::transaction::VersionedTransaction) -> Option<u64> {
    use solana_sdk::message::VersionedMessage;
    if !matches!(
        tx.message,
        VersionedMessage::Legacy(_) | VersionedMessage::V0(_)
    ) {
        return None;
    }
    let keys = tx.message.static_account_keys();
    let (mut limit, mut price, mut other_ixs) = (None, None, 0u64);
    for ix in tx.message.instructions() {
        let is_cb = keys
            .get(usize::from(ix.program_id_index))
            .is_some_and(|k| k.to_string() == COMPUTE_BUDGET_PROGRAM_ID);
        if !is_cb {
            other_ixs += 1;
            continue;
        }
        match ix.data.first() {
            Some(2) if ix.data.len() >= 5 => {
                limit = Some(u64::from(u32::from_le_bytes([
                    ix.data[1], ix.data[2], ix.data[3], ix.data[4],
                ])));
            }
            Some(3) if ix.data.len() >= 9 => {
                let mut a = [0u8; 8];
                a.copy_from_slice(&ix.data[1..9]);
                price = Some(u64::from_le_bytes(a));
            }
            _ => {}
        }
    }
    let base = u64::from(tx.message.header().num_required_signatures)
        .checked_mul(LAMPORTS_PER_SIGNATURE)?;
    let priority = match price {
        None | Some(0) => 0,
        Some(p) => {
            let cu =
                limit.unwrap_or_else(|| (other_ixs * DEFAULT_CU_PER_INSTRUCTION).min(MAX_CU_LIMIT));
            u64::try_from((u128::from(cu) * u128::from(p)).div_ceil(MICRO_LAMPORTS_PER_LAMPORT))
                .ok()?
        }
    };
    base.checked_add(priority)
}

/// [P1-C] Economic gate for one built plan (used by `Pipeline::evaluate`). LIVE acts only on
/// `Known` economics above the threshold; `Unknown` (e.g. missing executable floor) can at most
/// become a Shadow technical probe. Returns `(evaluation, actionable, technical_probe)`.
pub(crate) fn plan_economic_gate(
    input: &CandidateInput,
    failed: &FailedCostMatrix,
    min_profit: i128,
    mode: Mode,
    state_valid: bool,
    safety_valid: bool,
) -> (Evaluation, bool, bool) {
    let eval = economics::evaluate(input, failed);
    let actionable = economics::actionable(eval, min_profit, true, state_valid, safety_valid);
    let technical_probe =
        mode == Mode::Shadow && matches!(eval, Evaluation::Unknown) && state_valid && safety_valid;
    (eval, actionable, technical_probe)
}

/// V9: keeps the furthest technical stage a rejected plan reached: plan gate (1), allowlists (2),
/// path capability (3), transaction build (4), compiled-message check (5).
fn note_stage(furthest: &mut Option<(u8, LossReason)>, stage: u8, loss: LossReason) {
    if furthest.is_none_or(|(reached, _)| stage > reached) {
        *furthest = Some((stage, loss));
    }
}

/// N2: `now` plus the candidate's network time; an unrepresentable bound (an unbounded budget of
/// tools and tests) is one year.
pub(crate) fn deadline_after(now: Instant, deadline_ns: u64) -> Instant {
    now.checked_add(std::time::Duration::from_nanos(deadline_ns))
        .unwrap_or_else(|| now + std::time::Duration::from_secs(365 * 24 * 3600))
}

/// B1: reason code of a LIVE candidate that reached the send boundary of this pipeline.
pub const LIVE_SEND_REQUIRES_FE04: &str = "LIVE_SEND_REQUIRES_FE04_ONE_SHOT";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Shadow,
    Live,
}

/// Was der Receivership-Builder (P5-06) liefern muss. Implementierung gegen das
/// marginfi-Crate/IDL; hier nur der Contract.
pub struct ReceivershipPlan {
    pub market_identity: Option<crate::analytics::record::MarketPlanIdentity>,
    /// The receivership TX. E2: its planner steps are derived from these bytes
    /// (`planner::steps_of`), never supplied separately.
    pub instructions: Vec<Instruction>,
    pub route: RouteKind,
    pub input: CandidateInput,
    pub premium_bps: u64,
    pub exposure: u64,
    pub tip: u64,
    pub slippage_bps: u64,
    /// Beteiligte Asset-Mints (Safety-Allowlist).
    pub assets: Vec<Pubkey>,
    /// B15: collateral this plan leaves in the wallet (USD micro at the protocol valuation of
    /// the exact seized amount, from the plan's sizing); the Safety inventory hold.
    pub inventory_hold_usd_micro: u64,
    /// TX-Klasse ist Bestandteil des Execution-Candidates; v0 kann Route-LUTs nutzen.
    pub tx_version: BuildVersion,
    pub lookup_tables: Vec<AddressLookupTableAccount>,
    /// [FIX-3] Vorab-TXs im Bundle (JIT_SHORTFALL: Buy-TX). Dürfen weder Start noch End enthalten;
    /// Receivership bleibt eine eigene TX mit End als letzter Ix.
    pub prerequisite_txs: Vec<Vec<Instruction>>,
}

/// Q5: why a builder produced no plan, typed where it arises. The text is the former message
/// (the record's `reason`, unchanged); decision and loss follow from the kind alone, never from
/// the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanErrorKind {
    /// The protocol FeeState is unreadable: economics UNKNOWN (P5).
    FeeStateUnavailable,
    /// Nothing protocol-valid to size (`SIZING:`).
    Sizing,
    /// Sizing stopped at an integration bank this build cannot size.
    UnsupportedIntegration,
    /// A prewarmed associated token account is missing (`ATA:`).
    MissingAta,
    /// C10: a stale integration reserve, from the pinned program's error code.
    IntegrationStale,
    /// No health engine built in.
    HealthUnavailable,
    /// No route for any pair (`ROUTING:`).
    Routing,
    /// Any other planning failure (state, layout, build), attributed to routing as before Q5.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanError {
    pub kind: PlanErrorKind,
    pub text: String,
}

impl PlanError {
    pub fn new(kind: PlanErrorKind, text: impl Into<String>) -> Self {
        PlanError {
            kind,
            text: text.into(),
        }
    }

    /// [P5] An unreadable FeeState (or no health engine) leaves the candidate unjudged
    /// (UNAVAILABLE); every other planning failure is a judged skip.
    pub fn decision(&self) -> Decision {
        match self.kind {
            PlanErrorKind::FeeStateUnavailable | PlanErrorKind::HealthUnavailable => {
                Decision::Unavailable
            }
            PlanErrorKind::Sizing
            | PlanErrorKind::UnsupportedIntegration
            | PlanErrorKind::MissingAta
            | PlanErrorKind::IntegrationStale
            | PlanErrorKind::Routing
            | PlanErrorKind::Other => Decision::Skip,
        }
    }

    /// Loss attribution: [P5] an unreadable FeeState is UNKNOWN, never a routing loss.
    pub fn loss(&self) -> LossReason {
        match self.kind {
            PlanErrorKind::FeeStateUnavailable => LossReason::Unknown,
            PlanErrorKind::Sizing => LossReason::Sizing,
            PlanErrorKind::UnsupportedIntegration => LossReason::UnsupportedIntegration,
            PlanErrorKind::MissingAta => LossReason::MissingAta,
            PlanErrorKind::IntegrationStale => LossReason::IntegrationRefresh,
            PlanErrorKind::HealthUnavailable | PlanErrorKind::Routing | PlanErrorKind::Other => {
                LossReason::Routing
            }
        }
    }
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

impl From<String> for PlanError {
    fn from(text: String) -> Self {
        PlanError::new(PlanErrorKind::Other, text)
    }
}

impl From<&str> for PlanError {
    fn from(text: &str) -> Self {
        PlanError::new(PlanErrorKind::Other, text)
    }
}

pub trait ReceivershipBuilder {
    /// Liefert **alle** technisch machbaren Funding-/Route-Candidates. Keine harte
    /// Funding-Präferenz im Builder; P4A + TX-Feasibility entscheiden danach.
    fn native_cost_usd_micro(&self, _lamports: u64) -> Option<u64> {
        None
    }
    /// E2: the pinned instruction table the plans' instructions are classified with.
    fn instruction_table(&self) -> &InstructionTable;
    /// N2: all external work of planning (route quotes, route-fit re-quotes) ends by `deadline`.
    fn plans(
        &self,
        account: &Pubkey,
        now_unix_s: u64,
        slot: u64,
        deadline: Instant,
    ) -> Result<Vec<ReceivershipPlan>, PlanError>;
}

/// B6: the Shadow owner's bound on synchronous network work (routes, fee quotes, simulation)
/// while its ingress queue is behind. At most `per_drain` candidates use the network until the
/// owner has drained its queue again (`drained`); each stops calling the provider once
/// `deadline_ns` has passed since it reached the network stage. Other candidates are recorded
/// with `RpcBudget`/`RpcDeadline`, never dropped silently and never a HALT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkBudget {
    pub remaining: usize,
    pub per_drain: usize,
    pub deadline_ns: u64,
}
impl NetworkBudget {
    pub fn new(per_drain: usize, deadline_ns: u64) -> Self {
        Self {
            remaining: per_drain,
            per_drain,
            deadline_ns,
        }
    }
    /// No bound (tools and tests outside the owner).
    pub fn unbounded() -> Self {
        Self::new(usize::MAX, u64::MAX)
    }
    /// The owner's ingress queue is empty again: a new drain window.
    pub fn drained(&mut self) {
        self.remaining = self.per_drain;
    }
}

pub struct Pipeline<'a, H: HealthEngine, B: ReceivershipBuilder, S: SubmissionAdapter> {
    pub mode: Mode,
    pub clock: &'a Clock,
    pub health: &'a H,
    pub builder: &'a B,
    pub submitter: &'a S,
    pub safety: &'a mut Safety,
    pub blockhash: &'a BlockhashCache,
    pub payer: &'a Keypair,
    pub allowlist: &'a Allowlist,
    pub failed_costs: &'a FailedCostMatrix,
    pub limits: TxLimits,
    pub resources: Resources,
    pub min_profit: i128,
    pub blockhash_max_age_ns: u64,
    pub p7_passed: bool,
    pub freshness: Freshness,
    /// [FIX-4] Tip-Account (aus `JitoAdapter::tip_accounts()`, rotierend) — nur für Bundle-Pfad.
    pub tip_account: Option<Pubkey>,
    /// [FIX-22] Globale P0-Top-Level-Allowlist (protocol::receivership_allowed_programs).
    /// C8: borrowed from the owner (immutable during the evaluation), never copied per candidate.
    pub global_allowed: &'a [Pubkey],
    /// [FIX-30] Allowlist für Prerequisite-TXs (Swap-Programme, System, Token, Token-2022, ATA, ComputeBudget).
    pub prereq_allowed: &'a [Pubkey],
    /// [FIX-40] Zuletzt beobachteter Chain-Slot (vom Ingress aktualisiert).
    pub slot_watch: &'a darkbot_solana::ingress::SlotWatch,
    /// B6: bound on the synchronous network work of the calling owner.
    pub network: &'a mut NetworkBudget,
    /// F1: logs of the last failed simulation, taken by the owner for its journal.
    pub simulation_failure: Option<darkbot_solana::submit::SimulationFailure>,
}

impl<'a, H: HealthEngine, B: ReceivershipBuilder, S: SubmissionAdapter> Pipeline<'a, H, B, S> {
    /// Ein Kandidat, ein Trigger → ein Record. T_rx wird vom Aufrufer gesetzt.
    pub fn evaluate(
        &mut self,
        account: Pubkey,
        trigger: &str,
        t_rx_ns: u64,
        now_unix_s: u64,
        slot: u64,
        state_valid: bool,
    ) -> OpportunityRecord {
        let _trigger_slot = slot; // [FIX-40] Trigger-Slot ist nur Provenienz des Updates, nicht T_sign
        let mut tl = Timeline::new();
        tl.mark(Stage::Rx, t_rx_ns);
        let mut rec = OpportunityRecord {
            progress: Default::default(),
            market_plan_identity: None,
            timing: Default::default(),
            rx_slot: Some(slot),
            detect_slot: None,
            selected_costs: None,
            simulation_result: None,
            account: account.to_string(),
            trigger: trigger.to_string(),
            decision: Decision::Skip,
            net_profit: None,
            economics_known: false,
            premium_bps: None,
            t_profitable_unix_s: None,
            tx_version: None,
            serialized_bytes: None,
            locked_accounts: None,
            path: None,
            funding: None,
            signature: None,
            sign_slot: None,
            simulated_cu: None,
            candidate_costs: vec![],
            exposure: None,
            all_signatures: vec![],
            bundle_id: None,
            submitted_tip: None,
            intervals_ns: vec![],
            reason: None,
            loss: None,
        };

        if !state_valid {
            rec.loss = Some(LossReason::State);
            rec.reason = Some("state not tradable".into());
            return rec.with_timeline(&tl);
        }

        // Health (P3)
        darkbot_common::mt0::MT0.count_health(darkbot_common::mt0::HealthSite::Pipeline); // [MT0]
        let hv = match self.health.health(&account, now_unix_s as i64, slot) {
            Ok(h) => h,
            Err(HealthError::Unavailable) => {
                rec.decision = Decision::Unavailable;
                rec.loss = Some(LossReason::Health);
                rec.reason = Some("health engine unavailable".into());
                return rec.with_timeline(&tl);
            }
            Err(e @ HealthError::IntegrationStale(_)) => {
                rec.loss = Some(LossReason::IntegrationRefresh);
                rec.reason = Some(e.to_string());
                return rec.with_timeline(&tl);
            }
            Err(e) => {
                rec.loss = Some(LossReason::Health);
                rec.reason = Some(e.to_string());
                return rec.with_timeline(&tl);
            }
        };
        rec.progress.health_liquidatable = Some(hv.liquidatable);
        if !hv.liquidatable {
            rec.reason = Some("not liquidatable".into());
            return rec.with_timeline(&tl);
        }

        // [FIX-88] Exact receivership eligibility beyond health: pinned marginfi circuit-breaker
        // state + CB price gate. This is part of the shared pipeline, so Shadow/LIVE/replay do not
        // diverge at the protocol boundary.
        if let Err(e) = self
            .health
            .receivership_eligibility(&account, now_unix_s as i64, slot)
        {
            rec.loss = Some(LossReason::CircuitBreaker);
            rec.reason = Some(e.to_string());
            return rec.with_timeline(&tl);
        }

        tl.mark(Stage::State, self.clock.now_ns());

        // P5 + P4A: Builder liefert alle technisch machbaren Funding-/Route-Klassen.
        // Es gibt bewusst **keine** feste Funding-Präferenz. Erst Protocol-Validität,
        // deterministische Economics und echte TX-Baubarkeit entscheiden.
        // [FIX-21] stale Opportunity vor teurem Quote/Build verwerfen (Drift-Muster).
        if self.freshness.is_stale(t_rx_ns, self.clock.now_ns()) {
            rec.loss = Some(LossReason::StateRace);
            rec.reason = Some("opportunity stale before planning".into());
            return rec.with_timeline(&tl);
        }
        // B6: routes, fee quotes and simulation only within the owner's network budget.
        if self.network.remaining == 0 {
            rec.loss = Some(LossReason::RpcBudget);
            rec.reason = Some(
                "RPC_BUDGET: owner ingress queue behind; no network evaluation in this drain window"
                    .into(),
            );
            return rec.with_timeline(&tl);
        }
        self.network.remaining -= 1;
        // N2: one monotonic deadline for all external work of this candidate — route quotes and
        // route-fit re-quotes, fee quote, simulation (provider permit, request, response read).
        let deadline = deadline_after(Instant::now(), self.network.deadline_ns);
        let planning_started = Instant::now();
        let mt0_jupiter_before =
            darkbot_common::mt0::MT0.remote_requests(darkbot_common::mt0::RemoteClass::Jupiter); // [MT0]
        let planning_result = self.builder.plans(&account, now_unix_s, slot, deadline);
        let mt0_jupiter = darkbot_common::mt0::MT0
            .remote_requests(darkbot_common::mt0::RemoteClass::Jupiter)
            .saturating_sub(mt0_jupiter_before);
        if mt0_jupiter > 0 {
            darkbot_common::mt0::MT0.add(darkbot_common::mt0::Counter::RoutedEvaluations, 1);
            darkbot_common::mt0::MT0.add(
                darkbot_common::mt0::Counter::JupiterRequestsRouted,
                mt0_jupiter,
            );
        }
        record_timing(
            &PLANNING_CALLS,
            &PLANNING_NS_TOTAL,
            &PLANNING_NS_MAX,
            planning_started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
        );
        rec.progress.candidates_created = planning_result.as_ref().ok().map(|p| !p.is_empty());
        // N2: planning that reached the deadline leaves no time for fee quote and simulation;
        // a planning error at the deadline is the deadline, not a routing verdict.
        if Instant::now() >= deadline {
            rec.loss = Some(LossReason::RpcDeadline);
            rec.reason = Some(format!(
                "RPC_DEADLINE: candidate network deadline during planning ({})",
                match &planning_result {
                    Ok(p) => format!("{} plan(s)", p.len()),
                    Err(e) => e.text.clone(),
                }
            ));
            return rec.with_timeline(&tl);
        }
        let plans = match planning_result {
            Ok(p) if !p.is_empty() => p,
            Ok(_) => {
                rec.loss = Some(LossReason::Routing);
                rec.reason = Some("no feasible execution candidates".into());
                return rec.with_timeline(&tl);
            }
            Err(e) => {
                rec.decision = e.decision();
                rec.loss = Some(e.loss());
                rec.reason = Some(e.text);
                return rec.with_timeline(&tl);
            }
        };

        let Some(bh) = self
            .blockhash
            .fresh(self.clock.now_ns(), self.blockhash_max_age_ns)
        else {
            rec.loss = Some(LossReason::Build);
            rec.reason = Some("blockhash stale".into());
            return rec.with_timeline(&tl);
        };

        let mut best_actionable: Option<(
            ReceivershipPlan,
            darkbot_solana::tx::BuiltTx,
            Evaluation,
            crate::analytics::record::CandidateCostRecord,
        )> = None;
        let mut best_probe: Option<(
            ReceivershipPlan,
            darkbot_solana::tx::BuiltTx,
            Evaluation,
            crate::analytics::record::CandidateCostRecord,
        )> = None;
        let mut threshold_hits: u32 = 0;
        let mut rejected = Vec::new();
        // V9: the furthest technical stage a rejected plan reached (and its loss cause), and
        // whether any plan reached the economics; the loss of a candidate without a plan names it.
        let mut furthest_technical: Option<(u8, LossReason)> = None;
        let mut reached_economics = false;
        // D2: fee quotes of this evaluation (one blockhash, one slot) keyed by the message hash
        // of the built bytes: funding classes with byte-identical transactions (NoSwap and
        // Preheld inventory) share one quote; nothing is reused across evaluations.
        let mut fee_quotes: Vec<(
            solana_sdk::hash::Hash,
            Result<submit::MessageFeeQuote, submit::FeeQuoteError>,
        )> = Vec::new();
        for mut plan in plans {
            // E2: the gate decides on the steps of the actually built instructions (receivership
            // TX and every prerequisite TX), immediately before they are signed.
            let table = self.builder.instruction_table();
            let checked = planner::steps_of(&plan.instructions, table).and_then(|steps| {
                planner::validate_receivership(&steps, self.allowlist, plan.route, self.p7_passed)?;
                Ok(steps)
            });
            let checked = checked.map_err(|e| format!("{e:?}")).and_then(|steps| {
                if plan.prerequisite_txs.is_empty() {
                    return Ok(());
                }
                let mut txs = plan
                    .prerequisite_txs
                    .iter()
                    .map(|pre| planner::steps_of(pre, table))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("prerequisite {e:?}"))?;
                txs.push(steps);
                planner::validate_bundle(&txs)
                    .map(drop)
                    .map_err(str::to_owned)
            });
            if let Err(e) = checked {
                note_stage(&mut furthest_technical, 1, LossReason::Build);
                rejected.push(format!(
                    "{}: plan invalid: {}",
                    plan.input.funding.as_str(),
                    e
                ));
                continue;
            }

            rec.progress.plan_validated = Some(true);

            // [FIX-22][FIX-30] P0-Top-Level-Allowlist gilt nur für die Receivership-TX; Prerequisite-TXs
            // (JIT-Buy) liegen außerhalb Start/End und haben ihre eigene Allowlist (`prereq_allowed`).
            if let Some(bad) = plan
                .instructions
                .iter()
                .find(|ix| !self.global_allowed.contains(&ix.program_id))
            {
                note_stage(&mut furthest_technical, 2, LossReason::Build);
                rejected.push(format!(
                    "{}: receivership program not allowlisted: {}",
                    plan.input.funding.as_str(),
                    bad.program_id
                ));
                continue;
            }
            if let Some(bad) = plan
                .prerequisite_txs
                .iter()
                .flatten()
                .find(|ix| !self.prereq_allowed.contains(&ix.program_id))
            {
                note_stage(&mut furthest_technical, 2, LossReason::Build);
                rejected.push(format!(
                    "{}: prerequisite program not allowed: {}",
                    plan.input.funding.as_str(),
                    bad.program_id
                ));
                continue;
            }
            // Shadow muss den finalen Pfad messen können, auch wenn LIVE absichtlich deaktiviert
            // ist. Safety ist dort Evidence, kein Send-Gate. Erst LIVE previewt die Limits
            // nicht-reservierend, damit bei einem Safety-Verstoß eine andere Funding-Klasse
            // derselben Opportunity gewählt werden kann.
            let safety_valid = match self.mode {
                Mode::Shadow => true,
                Mode::Live => match self.safety.preview(
                    now_unix_s,
                    plan.exposure,
                    plan.inventory_hold_usd_micro,
                    plan.tip,
                    plan.slippage_bps,
                    &plan.assets,
                    "longtail",
                ) {
                    Ok(()) => true,
                    Err(deny) => {
                        rejected.push(format!(
                            "{}: safety {:?}",
                            plan.input.funding.as_str(),
                            deny
                        ));
                        false
                    }
                },
            };

            // [FIX-3] Prerequisite-TXs nur über Bundle-Pfad; sonst ist der Kandidat nicht ausführbar.
            if !self.submitter.supports_tx_version(plan.tx_version) {
                // [FIX-38]
                note_stage(&mut furthest_technical, 3, LossReason::Submission);
                rejected.push(format!(
                    "{}: tx version {} not supported on {}",
                    plan.input.funding.as_str(),
                    plan.tx_version.record_name(),
                    self.submitter.path().as_str()
                ));
                continue;
            }
            if !plan.prerequisite_txs.is_empty() && !self.submitter.supports_bundle() {
                // [FIX-14]
                note_stage(&mut furthest_technical, 3, LossReason::Submission);
                rejected.push(format!(
                    "{}: needs bundle path",
                    plan.input.funding.as_str()
                ));
                continue;
            }
            if tl.get(Stage::Detect).is_none() {
                tl.mark(Stage::Detect, self.clock.now_ns());
                rec.detect_slot = Some(self.slot_watch.get()).filter(|s| *s > 0);
            }
            let txplan = TxPlan {
                version: plan.tx_version,
                payer: tx::payer_of(self.payer),
                instructions: plan.instructions.clone(),
                lookup_tables: plan.lookup_tables.clone(),
                resources: self.resources,
            };
            let built = match tx::build(&txplan, bh.hash, &self.limits) {
                Ok(b) => b,
                Err(e) => {
                    note_stage(&mut furthest_technical, 4, LossReason::Build);
                    rejected.push(format!(
                        "{}: tx infeasible: {}",
                        plan.input.funding.as_str(),
                        e
                    ));
                    continue;
                }
            };

            // E2: the signed message is exactly the validated plan (with its compute budget).
            if let Err(code) = tx::message_matches_plan(&built.tx.message, &txplan) {
                note_stage(&mut furthest_technical, 5, LossReason::Build);
                rejected.push(format!(
                    "{}: compiled message differs from the validated instructions ({code})",
                    plan.input.funding.as_str()
                ));
                continue;
            }
            rec.progress.transaction_built = Some(true);
            let fee_quote = if plan.input.path == Path::Rpc
                && self.submitter.path() == SubmissionPath::Rpc
                && plan.prerequisite_txs.is_empty()
                && plan.tip == 0
            {
                if Instant::now() >= deadline {
                    rec.loss = Some(LossReason::RpcDeadline);
                    rec.reason = Some(
                        "RPC_DEADLINE: candidate network deadline before the fee quote".into(),
                    );
                    return rec.with_timeline(&tl);
                }
                let message_hash = built.tx.message.hash();
                let cached = fee_quotes
                    .iter()
                    .find(|(h, _)| *h == message_hash)
                    .map(|(_, q)| q.clone());
                let fee = match cached {
                    Some(quote) => quote,
                    None => {
                        let fee_started = Instant::now();
                        let quote = self.submitter.fee_for_message(&built.tx, deadline);
                        // [MT0] H6: would a local fee model have matched the RPC fee?
                        // (measurement only, once per RPC quote)
                        if let Ok(q) = &quote {
                            match local_fee_estimate_lamports(&built.tx) {
                                Some(local) if local == q.lamports => darkbot_common::mt0::MT0
                                    .add(darkbot_common::mt0::Counter::FeeParityMatch, 1),
                                Some(_) => darkbot_common::mt0::MT0
                                    .add(darkbot_common::mt0::Counter::FeeParityMismatch, 1),
                                None => darkbot_common::mt0::MT0
                                    .add(darkbot_common::mt0::Counter::FeeParityUnsupported, 1),
                            }
                        }
                        record_timing(
                            &FEE_CALLS,
                            &FEE_NS_TOTAL,
                            &FEE_NS_MAX,
                            fee_started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                        );
                        fee_quotes.push((message_hash, quote.clone()));
                        quote
                    }
                };
                // N2: a deadline or a refused call ends the evaluation with its own cause; an
                // unavailable quote or a transport failure leaves the fee UNKNOWN (never 0).
                match fee {
                    Err(submit::FeeQuoteError::Deadline) => {
                        rec.loss = Some(LossReason::RpcDeadline);
                        rec.reason = Some(
                            "RPC_DEADLINE: candidate network deadline in the fee quote".into(),
                        );
                        return rec.with_timeline(&tl);
                    }
                    Err(submit::FeeQuoteError::BudgetRejected(code)) => {
                        rec.loss = Some(LossReason::RpcBudget);
                        rec.reason = Some(format!("RPC_BUDGET: fee quote refused ({code})"));
                        return rec.with_timeline(&tl);
                    }
                    Err(_) => None,
                    Ok(q) => {
                        Some(q).filter(|q| q.message_hash == built.tx.message.hash().to_string())
                    }
                }
            } else {
                None
            };
            plan.input.transaction_fee_usd_micro = fee_quote
                .as_ref()
                .and_then(|q| self.builder.native_cost_usd_micro(q.lamports));
            let candidate_failed = if fee_quote.is_some() {
                economics::rpc_message_failure_costs(plan.input.transaction_fee_usd_micro)
            } else {
                *self.failed_costs
            };
            let failed_reserve = candidate_failed.conservative(plan.input.path);
            let (eval, actionable, technical_probe) = plan_economic_gate(
                &plan.input,
                &candidate_failed,
                self.min_profit,
                self.mode,
                state_valid,
                safety_valid,
            );
            // [MT0] avoidable remote work, classified only from decisions already made above.
            if fee_quote.is_some() && matches!(eval, Evaluation::Known { .. }) && !actionable {
                darkbot_common::mt0::MT0
                    .add(darkbot_common::mt0::Counter::FeeRpcForKnownNonActionable, 1);
            }
            if technical_probe && plan.input.transaction_fee_usd_micro.is_none() {
                darkbot_common::mt0::MT0.add(
                    darkbot_common::mt0::Counter::ProbeCandidatesWithFeeUnknown,
                    1,
                );
            }
            let cost_record = crate::analytics::record::CandidateCostRecord::from_input(
                &plan.input,
                eval,
                failed_reserve,
                fee_quote.clone(),
            );
            rec.candidate_costs.push(cost_record.clone());
            if !actionable && !technical_probe {
                if let Evaluation::Known { net, .. } = eval {
                    if net > 0 && net <= self.min_profit {
                        threshold_hits += 1;
                    }
                    if rec.net_profit.is_none_or(|current| net > current) {
                        rec.net_profit = Some(net);
                        rec.market_plan_identity = plan.market_identity.clone();
                        rec.selected_costs = Some(cost_record);
                        rec.economics_known = true;
                        rec.premium_bps = Some(plan.premium_bps);
                        rec.funding = Some(plan.input.funding.as_str().into());
                    }
                }
                reached_economics = true;
                rejected.push(format!(
                    "{}: economics {:?}",
                    plan.input.funding.as_str(),
                    eval
                ));
                continue;
            }

            if actionable {
                let replace = best_actionable.as_ref().is_none_or(|(_, _, current, _)| {
                    economics::compare_evaluations(eval, *current).is_gt()
                });
                if replace {
                    best_actionable = Some((plan, built, eval, cost_record));
                }
            } else {
                let candidate_key = (
                    built.serialized_len,
                    built.locked_accounts,
                    plan.input.funding.as_str(),
                );
                let replace = best_probe
                    .as_ref()
                    .is_none_or(|(cur_plan, cur_built, _, _)| {
                        candidate_key
                            < (
                                cur_built.serialized_len,
                                cur_built.locked_accounts,
                                cur_plan.input.funding.as_str(),
                            )
                    });
                if replace {
                    best_probe = Some((plan, built, eval, cost_record));
                }
            }
        }

        let Some((plan, mut built, eval, selected_costs)) = best_actionable.or(best_probe) else {
            // [FIX-43] ThresholdTooHigh nur, wenn mindestens ein Kandidat positiv, aber unter MIN_PROFIT lag.
            rec.loss = Some(if threshold_hits > 0 {
                LossReason::ThresholdTooHigh
            } else if reached_economics {
                LossReason::EconomicSkip
            } else if let Some((_, technical)) = furthest_technical {
                // V9: no plan reached the economics; the furthest technical stage is the cause.
                technical
            } else {
                LossReason::EconomicSkip
            });
            rec.reason = Some(if rejected.is_empty() {
                "no actionable execution candidate".into()
            } else {
                rejected.join(" | ")
            });
            return rec.with_timeline(&tl);
        };

        rec.market_plan_identity = plan.market_identity.clone();
        rec.selected_costs = Some(selected_costs);
        rec.path = Some(self.submitter.path().as_str().into());
        rec.premium_bps = Some(plan.premium_bps);
        rec.funding = Some(plan.input.funding.as_str().to_string());
        rec.economics_known = matches!(eval, Evaluation::Known { .. });
        rec.net_profit = None;
        if let Evaluation::Known { net, .. } = eval {
            rec.net_profit = Some(net);
        } else if self.mode == Mode::Shadow {
            rec.reason =
                Some("technical shadow probe: cost or realization evidence incomplete".into());
        }

        rec.serialized_bytes = Some(built.serialized_len);
        rec.locked_accounts = Some(built.locked_accounts);
        rec.tx_version = Some(built.version.record_name().into());
        tl.mark(Stage::Ready, self.clock.now_ns());

        if let Err(e) = tx::sign(&mut built, &[self.payer]) {
            rec.loss = Some(LossReason::Build);
            rec.reason = Some(e.to_string());
            return rec.with_timeline(&tl);
        }
        // [FIX-4/FIX-15] Bundle = [prerequisite TXs…, receivership TX, tip TX], only on a bundle
        // path; otherwise this single signed TX is the payload.
        let use_bundle =
            self.submitter.supports_bundle() && (!plan.prerequisite_txs.is_empty() || plan.tip > 0);
        // B18: T_sign and its slot exactly once, when the payload is completely signed — here for
        // the single TX (also with a tip but no bundle path), after the bundle otherwise.
        // [FIX-40] the slot is read at T_sign, lock-free.
        if !use_bundle {
            tl.mark(Stage::Sign, self.clock.now_ns());
            rec.sign_slot = Some(self.slot_watch.get()).filter(|s| *s > 0);
        }

        // Drift-Muster: stale Opportunity / Deadline → nicht senden (Record bleibt für P6).
        let t_detect = tl.get(Stage::Detect).unwrap_or(t_rx_ns);
        if self
            .freshness
            .deadline_passed(t_detect, self.clock.now_ns())
        {
            rec.loss = Some(LossReason::Build);
            rec.reason = Some("execution deadline passed".into());
            return rec.with_timeline(&tl);
        }
        // [M1] Shadow baut und simuliert identisch zu LIVE. B1: this pipeline never reserves and
        // never sends; a value-bearing handoff exists only in the FE-04 one-shot owner.
        // [FIX-6] Simulate-first auf genau dieser signierten TX. Nur Ok sendet; Fehler/Unavailable → SKIP,
        // Kapital deterministisch freigeben (kein Fehlversuch im Tagesstatus, es wurde nichts gesendet).
        // [FIX-15] Bundle-Kandidaten werden als Bundle simuliert, Einzel-TX als Einzel-TX — nie gemischt.
        // [FIX-4] Bundle = [prerequisite TXs…, receivership TX, tip TX]; Tip nie in der Receivership-TX.
        let mut bundle_txs: Vec<solana_sdk::transaction::VersionedTransaction> = Vec::new();
        if use_bundle {
            let Some(tip_account) = self.tip_account else {
                rec.loss = Some(LossReason::Submission);
                rec.reason = Some("bundle path without tip account".into());
                return rec.with_timeline(&tl);
            };
            for pre in &plan.prerequisite_txs {
                let pre_plan = TxPlan {
                    version: BuildVersion::V0,
                    payer: tx::payer_of(self.payer),
                    instructions: pre.clone(),
                    lookup_tables: plan.lookup_tables.clone(),
                    resources: self.resources,
                };
                let mut b = match tx::build(&pre_plan, bh.hash, &self.limits) {
                    Ok(b) => b,
                    Err(e) => {
                        rec.loss = Some(LossReason::Build);
                        rec.reason = Some(format!("prerequisite build: {}", e));
                        return rec.with_timeline(&tl);
                    }
                };
                if let Err(code) = tx::message_matches_plan(&b.tx.message, &pre_plan) {
                    rec.loss = Some(LossReason::Build);
                    rec.reason = Some(format!(
                        "prerequisite compiled message differs from the validated one ({code})"
                    ));
                    return rec.with_timeline(&tl);
                }
                if let Err(e) = tx::sign(&mut b, &[self.payer]) {
                    rec.loss = Some(LossReason::Build);
                    rec.reason = Some(e.to_string());
                    return rec.with_timeline(&tl);
                }
                bundle_txs.push(b.tx);
            }
            bundle_txs.push(built.tx.clone());
            match submit::build_tip_tx(self.payer, &tip_account, plan.tip, bh.hash) {
                Ok(t) => {
                    rec.submitted_tip =
                        t.signatures
                            .first()
                            .map(|s| crate::execution::pending::SubmittedTip {
                                signature: s.to_string(),
                                lamports: plan.tip,
                            });
                    bundle_txs.push(t);
                }
                Err(e) => {
                    rec.loss = Some(LossReason::Build);
                    rec.reason = Some(format!("tip tx: {}", e));
                    return rec.with_timeline(&tl);
                }
            }
        }
        if use_bundle {
            tl.mark(Stage::Sign, self.clock.now_ns());
            rec.sign_slot = Some(self.slot_watch.get()).filter(|s| *s > 0);
            rec.path = Some(SubmissionPath::JitoBundle.as_str().into());
        }
        if Instant::now() >= deadline {
            rec.loss = Some(LossReason::RpcDeadline);
            rec.reason =
                Some("RPC_DEADLINE: candidate network deadline before the simulation".into());
            return rec.with_timeline(&tl);
        }
        let simulation_started = Instant::now();
        let sim = if use_bundle {
            self.submitter.simulate_bundle(&bundle_txs, deadline)
        } else {
            self.submitter.simulate(&built.tx, deadline)
        };
        record_timing(
            &SIMULATION_CALLS,
            &SIMULATION_NS_TOTAL,
            &SIMULATION_NS_MAX,
            simulation_started
                .elapsed()
                .as_nanos()
                .min(u64::MAX as u128) as u64,
        );
        match sim {
            SimResult::Ok { units_consumed } => {
                rec.simulation_result = Some("PASS".into());
                rec.simulated_cu = units_consumed;
            }
            // N2: no simulation result exists; the cause is our own network bound.
            SimResult::Deadline => {
                rec.loss = Some(LossReason::RpcDeadline);
                rec.reason =
                    Some("RPC_DEADLINE: candidate network deadline in the simulation".into());
                return rec.with_timeline(&tl);
            }
            SimResult::BudgetRejected(code) => {
                rec.loss = Some(LossReason::RpcBudget);
                rec.reason = Some(format!("RPC_BUDGET: simulation refused ({code})"));
                return rec.with_timeline(&tl);
            }
            other => {
                rec.simulation_result = Some(other.record_text());
                if let SimResult::Failed(f) = &other {
                    self.simulation_failure = Some(f.clone());
                }
                rec.loss = Some(LossReason::Build);
                rec.reason = Some(format!("simulation not ok: {}", other.record_text()));
                rec.decision = if self.mode == Mode::Shadow {
                    if rec.economics_known {
                        Decision::ShadowSimFailed
                    } else {
                        Decision::ShadowTechnicalSimFailed
                    }
                } else {
                    rec.decision
                };
                return rec.with_timeline(&tl);
            }
        }
        // [M1] Send-Boundary: Shadow endet hier — gebaut, signiert, simuliert, nicht gesendet.
        if self.mode == Mode::Shadow {
            rec.decision = if rec.economics_known {
                Decision::ShadowReady
            } else {
                Decision::ShadowTechnicalReady
            };
            return rec.with_timeline(&tl);
        }
        // B1: LIVE ends at the same boundary. A send needs a persisted, fsync-confirmed FE-04
        // approval/intent/reservation bound to these signed bytes, and only the one-shot owner
        // (`execution::one_shot`) holds one; this pipeline has no submit call at all.
        if let Err(d) = self.safety.preview(
            now_unix_s,
            plan.exposure,
            plan.inventory_hold_usd_micro,
            plan.tip,
            plan.slippage_bps,
            &plan.assets,
            "longtail",
        ) {
            rec.decision = Decision::DeniedBySafety;
            rec.reason = Some(format!("{:?}", d));
            return rec.with_timeline(&tl);
        }
        // Not a Safety denial: the send capability is unavailable on this path.
        rec.decision = Decision::Unavailable;
        rec.loss = Some(LossReason::Submission);
        rec.reason = Some(format!(
            "{LIVE_SEND_REQUIRES_FE04}: simulated {} candidate is handed off only by the FE-04 one-shot owner",
            if use_bundle { "bundle" } else { "single-TX" }
        ));
        rec.with_timeline(&tl)
    }

    /// Landing-Reconciliation (S3-06) → Safety. Muss für **jede** Submission genau einmal
    /// aufgerufen werden, sonst bleibt `outstanding` belegt.
    /// Q2: a ledger violation halts the safety layer and is returned.
    pub fn reconcile_outcome(
        &mut self,
        now_unix_s: u64,
        landed_success: bool,
        realized_pnl: Option<i128>,
        exposure_released: u64,
        inventory_delta: i128,
    ) -> Result<(), darkbot_project0::safety::LedgerViolation> {
        self.safety.record_outcome(
            now_unix_s,
            landed_success,
            realized_pnl,
            exposure_released,
            inventory_delta,
            0,
        )
    }
}

#[cfg(test)]
mod plan_error_tests {
    use super::*;

    /// E2: the compiled message must carry exactly the validated instructions (ComputeBudget
    /// prefix allowed); any other instruction, order or data byte is a mismatch.
    #[test]
    fn e2_compiled_message_is_checked_against_the_validated_instructions() {
        let payer = solana_sdk::signature::Keypair::new();
        let ix = |program: Pubkey, data: Vec<u8>| Instruction {
            program_id: program,
            accounts: vec![],
            data,
        };
        let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
        let validated = vec![ix(a, vec![1; 8]), ix(b, vec![2; 8])];
        let plan = TxPlan {
            version: BuildVersion::V0,
            payer: tx::payer_of(&payer),
            instructions: validated.clone(),
            lookup_tables: vec![],
            resources: Resources {
                compute_unit_limit: 200_000,
                priority_micro_lamports_per_cu: 1,
                loaded_accounts_data_size: None,
            },
        };
        let built = tx::build(
            &plan,
            solana_sdk::hash::Hash::new_unique(),
            &TxLimits::default(),
        )
        .unwrap();
        assert!(
            built.tx.message.instructions().len() > validated.len(),
            "CU prefix added"
        );
        assert_eq!(tx::message_matches_plan(&built.tx.message, &plan), Ok(()));
        let with = |instructions: Vec<Instruction>| TxPlan {
            instructions,
            ..plan.clone()
        };
        let mut other_data = validated.clone();
        other_data[1].data[0] = 3;
        assert!(tx::message_matches_plan(&built.tx.message, &with(other_data)).is_err());
        let reordered = vec![validated[1].clone(), validated[0].clone()];
        assert!(tx::message_matches_plan(&built.tx.message, &with(reordered)).is_err());
        assert!(
            tx::message_matches_plan(&built.tx.message, &with(validated[..1].to_vec())).is_err()
        );
    }

    /// Q5 (with C10/P5): decision and loss of a planning failure follow from its typed kind
    /// alone. The text — the record's reason, unchanged — never decides: a routing failure whose
    /// remote message says "unavailable" or names "INTEGRATION_STALE:"/"SIZING:"/"ATA:" stays a
    /// routing skip, and a typed stale reserve is IntegrationRefresh whatever its wording.
    #[test]
    fn q5_plan_errors_are_classified_by_kind_not_text() {
        use PlanErrorKind as K;
        let expected = |kind| match kind {
            K::FeeStateUnavailable => (Decision::Unavailable, LossReason::Unknown),
            K::Sizing => (Decision::Skip, LossReason::Sizing),
            K::UnsupportedIntegration => (Decision::Skip, LossReason::UnsupportedIntegration),
            K::MissingAta => (Decision::Skip, LossReason::MissingAta),
            K::IntegrationStale => (Decision::Skip, LossReason::IntegrationRefresh),
            K::HealthUnavailable => (Decision::Unavailable, LossReason::Routing),
            K::Routing | K::Other => (Decision::Skip, LossReason::Routing),
        };
        let texts = [
            "ROUTING:exact-in a→b: provider temporarily unavailable",
            "INTEGRATION_STALE:oracle adapter: Kamino reserve not refreshed in this slot",
            "SIZING:NO_LIABILITY",
            "ATA:PREWARM_REQUIRED: asset ATA=x",
            "FEE_STATE_UNAVAILABLE:FeeState account missing",
            "route provider: ReserveStale upstream",
        ];
        for kind in [
            K::FeeStateUnavailable,
            K::Sizing,
            K::UnsupportedIntegration,
            K::MissingAta,
            K::IntegrationStale,
            K::HealthUnavailable,
            K::Routing,
            K::Other,
        ] {
            for text in texts {
                let e = PlanError::new(kind, text);
                assert_eq!((e.decision(), e.loss()), expected(kind), "{kind:?} {text}");
                assert_eq!(e.to_string(), text);
            }
        }
        // An untyped string is an unclassified failure: a routing skip, as before Q5.
        let e = PlanError::from("FEE_STATE_UNAVAILABLE:looks typed but is not");
        assert_eq!(e.kind, K::Other);
        assert_eq!(
            (e.decision(), e.loss()),
            (Decision::Skip, LossReason::Routing)
        );
    }
}

#[cfg(test)]
mod mt0_tests {
    use super::*;
    use solana_sdk::instruction::Instruction;
    use solana_sdk::message::{Message, VersionedMessage};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::signature::Signature;
    use solana_sdk::transaction::VersionedTransaction;
    use std::str::FromStr;

    fn tx_with(ixs: Vec<Instruction>) -> VersionedTransaction {
        let payer = Pubkey::new_unique();
        let message = VersionedMessage::Legacy(Message::new(&ixs, Some(&payer)));
        VersionedTransaction {
            signatures: vec![Signature::default()],
            message,
        }
    }

    fn cb(data: Vec<u8>) -> Instruction {
        Instruction {
            program_id: Pubkey::from_str(COMPUTE_BUDGET_PROGRAM_ID).unwrap(),
            accounts: vec![],
            data,
        }
    }

    /// [MT0/H6] The local estimator (measurement only) reproduces the FIX-129 observations:
    /// 5_000 without priority and 7_800 = 5_000 + 1_400_000 CU x 2_000 uLamports / 10^6.
    #[test]
    fn local_fee_estimate_reproduces_fix129_observations() {
        let plain = Instruction {
            program_id: Pubkey::new_unique(),
            accounts: vec![],
            data: vec![1],
        };
        assert_eq!(
            local_fee_estimate_lamports(&tx_with(vec![plain.clone()])),
            Some(5_000)
        );
        let mut limit = vec![2u8];
        limit.extend_from_slice(&1_400_000u32.to_le_bytes());
        let mut price = vec![3u8];
        price.extend_from_slice(&2_000u64.to_le_bytes());
        assert_eq!(
            local_fee_estimate_lamports(&tx_with(vec![cb(limit), cb(price), plain])),
            Some(7_800)
        );
    }
}
