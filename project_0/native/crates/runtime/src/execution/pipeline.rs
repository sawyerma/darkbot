//! Die Kette. Jede Stufe setzt ihren Timing-Punkt; Fehler sind Records, keine Panics.

use crate::analytics::record::{Decision, LossReason, OpportunityRecord, Signing};
use crate::analytics::rejection::{PlanCause, PlanInvalid, PlanRejection, Rejection};
use crate::execution::one_shot::transport::{SignedPayload, SimulationBinding};
use darkbot_common::timing::{Clock, Stage, Timeline};
use darkbot_project0::economics::{self, CandidateInput, Evaluation, FailedCostMatrix, Path};
use darkbot_project0::execution::Freshness;
use darkbot_project0::health::{HealthEngine, HealthError, HealthView};
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

/// B01: the network fee of a transaction with `signatures` signers that spends its resources'
/// compute-unit limit at their priority price (the most it can pay; no message is built yet).
#[cfg(feature = "crate_health")]
pub(crate) fn planned_fee_lamports(signatures: u64, r: &Resources) -> Option<u64> {
    let priority = (u128::from(r.compute_unit_limit)
        * u128::from(r.priority_micro_lamports_per_cu))
    .div_ceil(MICRO_LAMPORTS_PER_LAMPORT);
    signatures
        .checked_mul(LAMPORTS_PER_SIGNATURE)?
        .checked_add(u64::try_from(priority).ok()?)
}
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
#[cfg_attr(test, derive(Clone))]
pub struct ReceivershipPlan {
    pub market_identity: Option<crate::analytics::record::MarketPlanIdentity>,
    /// The receivership TX. E2: its planner steps are derived from these bytes
    /// (`planner::steps_of`), never supplied separately.
    pub instructions: Vec<Instruction>,
    pub route: RouteKind,
    pub input: CandidateInput,
    pub premium_bps: u64,
    /// P2-05: the plan's growing tag premium cap; a known, not actionable plan wakes the
    /// account again at its `t_profitable`.
    pub tag_growth: Option<darkbot_project0::economics::TagGrowth>,
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
    /// B14: shared with every build of this plan.
    pub lookup_tables: std::sync::Arc<[AddressLookupTableAccount]>,
    /// [FIX-3] Vorab-TXs im Bundle (JIT_SHORTFALL_EXACT_IN: Buy-TX). Dürfen weder Start noch End enthalten;
    /// Receivership bleibt eine eigene TX mit End als letzter Ix.
    pub prerequisite_txs: Vec<Vec<Instruction>>,
    /// FE-02/B4: the SOL this plan debits besides the network fee and the tip.
    pub sol_costs: SolCosts,
    /// FE-04: every inventory the plan spends, (mint, base units) — an inventory-funded plan its
    /// liability; a JIT plan the liability inventory it uses and the funding its buy spends.
    /// The intent holds all of them. Empty: the plan spends no held inventory.
    pub inventory_holds: Vec<(Pubkey, u64)>,
    /// DEC-04: the sizing facts B is checked against (`None`: a plan without them, B
    /// unqualified).
    pub hold_facts: Option<crate::execution::hold_b::HoldFacts>,
}

/// FE-02/B4: SOL a plan's execution debits from the wallet beyond the network fee and the tip:
/// the protocol's flat liquidation fee and the rent of a liquidation record the plan creates
/// (`None`: the Rent sysvar is unknown, so the plan's SOL debit is UNKNOWN).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SolCosts {
    pub flat_fee_lamports: u64,
    pub record_rent_lamports: Option<u64>,
}

impl ReceivershipPlan {
    /// FE-02: every lamport this plan debits — the exact message's network fee (`fee_lamports`,
    /// from the fee quote), tip, flat protocol fee and record rent; `None` if any part is
    /// unknown or the sum overflows. FE-04 binds exactly this as the intent's SOL debit.
    pub fn sol_debit_lamports(&self, fee_lamports: Option<u64>) -> Option<u64> {
        fee_lamports?
            .checked_add(self.tip)?
            .checked_add(self.sol_costs.flat_fee_lamports)?
            .checked_add(self.sol_costs.record_rent_lamports?)
    }
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
    /// No health engine built in (V17: loss `Health`).
    HealthUnavailable,
    /// No route for any pair (`ROUTING:`).
    Routing,
    /// Any other planning failure (state, layout, build); V17: loss `Plan` of its own.
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

    /// Loss attribution: [P5] an unreadable FeeState is UNKNOWN, never a routing loss; V17:
    /// every other kind has its own persisted cause (before V17 `HealthUnavailable` and `Other`
    /// were ROUTING).
    pub fn loss(&self) -> LossReason {
        match self.kind {
            PlanErrorKind::FeeStateUnavailable => LossReason::Unknown,
            PlanErrorKind::Sizing => LossReason::Sizing,
            PlanErrorKind::UnsupportedIntegration => LossReason::UnsupportedIntegration,
            PlanErrorKind::MissingAta => LossReason::MissingAta,
            PlanErrorKind::IntegrationStale => LossReason::IntegrationRefresh,
            PlanErrorKind::HealthUnavailable => LossReason::Health,
            PlanErrorKind::Routing => LossReason::Routing,
            PlanErrorKind::Other => LossReason::Plan,
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
    /// DEC-04: B of `plan` from its signed instructions and the builder's state (`sol_debit`:
    /// the plan's SOL debit of its fee quote). A builder without state derives none.
    fn hold_b(
        &self,
        _plan: &ReceivershipPlan,
        _sol_debit: Option<u64>,
    ) -> crate::execution::hold_b::BHold {
        crate::execution::hold_b::BHold::Unqualified("B_BUILDER_WITHOUT_STATE".into())
    }
    /// N2: all external work of planning (route quotes, route-fit re-quotes) ends by `deadline`.
    fn plans(
        &self,
        account: &Pubkey,
        now_unix_s: u64,
        slot: u64,
        deadline: Instant,
    ) -> Result<Vec<ReceivershipPlan>, PlanError>;
    /// MT2: `plans` with the account's exact Health already computed on the builder's state at
    /// exactly (`now_unix_s`, `slot`) — the detection's one computation, carried as a value.
    /// Default: a builder that computes no Health of its own (test fixtures) plans as before.
    fn plans_with_health(
        &self,
        account: &Pubkey,
        _health: &HealthView,
        now_unix_s: u64,
        slot: u64,
        deadline: Instant,
    ) -> Result<Vec<ReceivershipPlan>, PlanError> {
        self.plans(account, now_unix_s, slot, deadline)
    }
}

/// MT2: the exact Health of a candidate as the detect consumer computed it on the evaluated
/// state at the protocol clock — the pipeline evaluates on this value and computes none.
pub type PreparedHealth<'h> = Result<&'h HealthView, &'h HealthError>;

/// DEC-01: the explicit cause of a candidate refused before planning (loss `Build`).
pub const BLOCKHASH_STALE_EARLY: &str = "BLOCKHASH_STALE: refused before planning";

/// DEC-01 (V2-09): the early blockhash refusal of one evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockhashPark {
    /// The caller cannot park a refused candidate: no early refusal (a restart is not proven),
    /// only the diagnostic counter and the late check before the build apply.
    Off,
    /// The caller parks a refused candidate: a stale or missing blockhash is refused before
    /// planning.
    Armed,
    /// Refused before planning. `generation` is the cache generation read before the check: a
    /// blockhash stored after it re-arms the parked candidate (a refresh racing the check is
    /// never missed).
    Refused { generation: u64 },
    /// The early check found a fresh blockhash (either mode): this evaluation covers a parked
    /// candidate of the account.
    Passed,
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

/// D2: one evaluation's fee quotes, keyed by the message hash of the built bytes.
type FeeQuotes = Vec<(
    solana_sdk::hash::Hash,
    Result<submit::MessageFeeQuote, submit::FeeQuoteError>,
)>;

/// DEC-11: a bundle's transactions besides the receivership TX, built once per evaluation —
/// the prerequisite TXs (signed at the boundary) and the signed tip TX.
struct BundleParts {
    prerequisites: Vec<darkbot_solana::tx::BuiltTx>,
    tip_tx: solana_sdk::transaction::VersionedTransaction,
}

/// DEC-11: a bundle plan's other transactions (or the typed rejection of building them, kept for
/// the boundary) and the sum of its transactions' exact message fees (`None`: unknown).
struct BundleQuote {
    parts: Result<BundleParts, (LossReason, Rejection)>,
    fee_lamports: Option<u64>,
}

/// A plan the evaluation may select: its built receivership TX, evaluation, cost record and, for
/// a bundle, its quote.
type Selected = (
    ReceivershipPlan,
    darkbot_solana::tx::BuiltTx,
    Evaluation,
    crate::analytics::record::CandidateCostRecord,
    Option<BundleQuote>,
);

pub struct Pipeline<'a, H: HealthEngine, B: ReceivershipBuilder, S: SubmissionAdapter> {
    pub mode: Mode,
    pub clock: &'a Clock,
    pub health: &'a H,
    pub builder: &'a B,
    pub submitter: &'a S,
    pub safety: &'a mut Safety,
    pub blockhash: &'a BlockhashCache,
    pub payer: &'a Keypair,
    /// P8-CTRL.4: the owner's ephemeral synthetic signer (never a wallet's key, never stored):
    /// the signature of a simulation without a sign permit.
    pub synthetic: &'a Keypair,
    pub allowlist: &'a Allowlist,
    pub failed_costs: &'a FailedCostMatrix,
    pub limits: TxLimits,
    pub resources: Resources,
    pub min_profit: i128,
    pub blockhash_max_age_ns: u64,
    pub p7_passed: bool,
    pub freshness: Freshness,
    /// [FIX-4]/DEC-11: the tip of a bundle (the configured account and amount), only while the
    /// bundle path is effective.
    pub bundle_tip: Option<crate::execution::bundle_path::BundleTip>,
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
    /// DEC-01: whether a stale blockhash is refused before planning (`Armed`); the evaluation
    /// leaves `Refused` for the owner to park the candidate.
    pub blockhash_park: BlockhashPark,
    /// B1/FE-04: the one durable admission of a LIVE candidate (the one-shot owner's journal and
    /// approval). `None`: LIVE stops at the send boundary, no submit call exists.
    pub fe04: Option<&'a mut crate::execution::one_shot::admission::Fe04Admission>,
    /// DEC-04: the hold mode a LIVE candidate's inventory hold and B are evaluated with.
    pub hold: &'a crate::execution::hold_b::HoldGate,
    /// P8-CTRL.7: the owner's gates an egress permit is issued from at the FE-04 handoff
    /// (`None`: no permit, no egress).
    pub egress: Option<crate::orchestration::capability_control::LiveGates<'a>>,
    /// P8-CTRL.3/.4 (review N1): the facts of a SIM_ONLY simulation's use in Shadow — the owner's
    /// (`None`: no SIM_ONLY grant in force; a LIVE candidate's are its FE-04 admission's).
    pub use_facts: Option<crate::execution::one_shot::admission::UseFacts>,
}

impl<'a, H: HealthEngine, B: ReceivershipBuilder, S: SubmissionAdapter> Pipeline<'a, H, B, S> {
    /// P8-CTRL.4: one payload transaction's signature — the wallet's key under a sign permit of
    /// this evaluation's purpose (LIVE: a new attempt; Shadow: a simulation under a SIM_ONLY
    /// grant), else in Shadow the synthetic signer's; LIVE never signs without a permit.
    fn sign(
        &self,
        tx: &mut solana_sdk::transaction::VersionedTransaction,
        bound: Option<(&crate::orchestration::fe06::Use, SubmissionPath)>,
    ) -> Result<Signing, darkbot_solana::tx::BuildError> {
        use crate::orchestration::capability_control::{SignPermit, SignPurpose};
        let purpose = match self.mode {
            Mode::Live => SignPurpose::NewAttempt,
            Mode::Shadow => SignPurpose::Simulation,
        };
        let payer = tx::payer_of(self.payer);
        let permit = self
            .egress
            .as_ref()
            .map(|g| SignPermit::issue(g, purpose, &[payer], &tx.message, bound));
        match (permit, self.mode) {
            (Some(Ok(p)), _) => p
                .sign(tx, &[self.payer])
                .map(|()| Signing::Wallet)
                .map_err(darkbot_solana::tx::BuildError::Sign),
            (refused, Mode::Live) => Err(darkbot_solana::tx::BuildError::Sign(
                refused
                    .and_then(Result::err)
                    .unwrap_or_else(|| "SIGN_NOT_PERMITTED: no authority".into()),
            )),
            (_, Mode::Shadow) => {
                tx::sign_synthetic(tx, self.synthetic).map(|()| Signing::Synthetic)
            }
        }
    }

    /// Ein Kandidat, ein Trigger → ein Record. T_rx wird vom Aufrufer gesetzt.
    ///
    /// Computes the candidate's Health itself (one computation, the `Pipeline` site): for
    /// callers without a detect consumer. The state owner's detection holds that Health
    /// already and evaluates through `evaluate_prepared` (MT2). C03: test seam only — production
    /// evaluates through `evaluate_prepared`.
    #[cfg(test)]
    pub fn evaluate(
        &mut self,
        account: Pubkey,
        trigger: &str,
        t_rx_ns: u64,
        now_unix_s: u64,
        slot: u64,
        state_valid: bool,
    ) -> OpportunityRecord {
        if !state_valid {
            // Refused before Health, as before; the value is never read.
            let unavailable = HealthError::Unavailable;
            return self.evaluate_prepared(
                account,
                trigger,
                t_rx_ns,
                now_unix_s,
                slot,
                false,
                Err(&unavailable),
            );
        }
        // Health (P3)
        darkbot_common::mt0::MT0.count_health(darkbot_common::mt0::HealthSite::Pipeline); // [MT0]
        let health = self.health.health(&account, now_unix_s as i64, slot);
        self.evaluate_prepared(
            account,
            trigger,
            t_rx_ns,
            now_unix_s,
            slot,
            true,
            health.as_ref(),
        )
    }

    /// `evaluate` on the candidate's exact Health as the caller computed it on the pipeline's
    /// state at exactly (`now_unix_s`, `slot`): no Health is computed here or below (sizing);
    /// a Health error is recorded exactly as `evaluate` records its own.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_prepared(
        &mut self,
        account: Pubkey,
        trigger: &str,
        t_rx_ns: u64,
        now_unix_s: u64,
        slot: u64,
        state_valid: bool,
        health: PreparedHealth<'_>,
    ) -> OpportunityRecord {
        let _trigger_slot = slot; // [FIX-40] Trigger-Slot ist nur Provenienz des Updates, nicht T_sign
                                  // V2-15: the monotonic instant of `now_unix_s` — the admission ages it to the handoff.
        let evaluated = Instant::now();
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
            signing: None,
        };

        if !state_valid {
            rec.loss = Some(LossReason::State);
            rec.reason = Some(Rejection::StateNotTradable);
            return rec.with_timeline(&tl);
        }

        // Health (P3): the caller's one computation (MT2).
        let hv = match health {
            Ok(h) => h,
            Err(HealthError::Unavailable) => {
                rec.decision = Decision::Unavailable;
                rec.loss = Some(LossReason::Health);
                rec.reason = Some(Rejection::HealthUnavailable);
                return rec.with_timeline(&tl);
            }
            Err(e @ HealthError::IntegrationStale(_)) => {
                rec.loss = Some(LossReason::IntegrationRefresh);
                rec.reason = Some(Rejection::Health(e.clone()));
                return rec.with_timeline(&tl);
            }
            Err(e) => {
                rec.loss = Some(LossReason::Health);
                rec.reason = Some(Rejection::Health(e.clone()));
                return rec.with_timeline(&tl);
            }
        };
        rec.progress.health_liquidatable = Some(hv.liquidatable);
        if !hv.liquidatable {
            rec.reason = Some(Rejection::NotLiquidatable);
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
            rec.reason = Some(Rejection::CircuitBreaker(e));
            return rec.with_timeline(&tl);
        }

        tl.mark(Stage::State, self.clock.now_ns());

        // P5 + P4A: Builder liefert alle technisch machbaren Funding-/Route-Klassen.
        // Es gibt bewusst **keine** feste Funding-Präferenz. Erst Protocol-Validität,
        // deterministische Economics und echte TX-Baubarkeit entscheiden.
        // [FIX-21] stale Opportunity vor teurem Quote/Build verwerfen (Drift-Muster).
        if self.freshness.is_stale(t_rx_ns, self.clock.now_ns()) {
            rec.loss = Some(LossReason::StateRace);
            rec.reason = Some(Rejection::OpportunityStale);
            return rec.with_timeline(&tl);
        }
        // V2-09 (P1-01): a blockhash already stale here is counted. DEC-01: for a caller that
        // parks the candidate it is refused here — no builder call, no network budget, the
        // routing/deadline/plan checks not run stay unrecorded (progress unset); the loss is
        // `Build` with the cause BLOCKHASH_STALE, decided before RpcBudget/RpcDeadline/Routing/
        // Plan (EXHAUSTIVE_V3). The late check before the build stays.
        let generation = self.blockhash.generation();
        if self
            .blockhash
            .fresh(self.clock.now_ns(), self.blockhash_max_age_ns)
            .is_none()
        {
            darkbot_common::mt0::MT0.add(
                darkbot_common::mt0::Counter::BlockhashStaleBeforePlanning,
                1,
            );
            if self.blockhash_park == BlockhashPark::Armed {
                rec.loss = Some(LossReason::Build);
                rec.reason = Some(Rejection::BlockhashStaleEarly);
                self.blockhash_park = BlockhashPark::Refused { generation };
                return rec.with_timeline(&tl);
            }
        } else {
            self.blockhash_park = BlockhashPark::Passed;
        }
        // B6: routes, fee quotes and simulation only within the owner's network budget.
        if self.network.remaining == 0 {
            rec.loss = Some(LossReason::RpcBudget);
            rec.reason = Some(Rejection::RpcBudgetDrain);
            return rec.with_timeline(&tl);
        }
        self.network.remaining -= 1;
        // N2: one monotonic deadline for all external work of this candidate — route quotes and
        // route-fit re-quotes, fee quote, simulation (provider permit, request, response read).
        let deadline = deadline_after(Instant::now(), self.network.deadline_ns);
        let planning_started = Instant::now();
        let mt0_jupiter_before =
            darkbot_common::mt0::MT0.remote_requests(darkbot_common::mt0::RemoteClass::Jupiter); // [MT0]
        let planning_result = self
            .builder
            .plans_with_health(&account, hv, now_unix_s, slot, deadline);
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
            rec.reason = Some(Rejection::RpcDeadlinePlanning(match planning_result {
                Ok(p) => Ok(p.len()),
                Err(e) => Err(e.text),
            }));
            return rec.with_timeline(&tl);
        }
        let plans = match planning_result {
            Ok(p) if !p.is_empty() => p,
            Ok(_) => {
                rec.loss = Some(LossReason::Routing);
                rec.reason = Some(Rejection::NoFeasiblePlans);
                return rec.with_timeline(&tl);
            }
            Err(e) => {
                rec.decision = e.decision();
                rec.loss = Some(e.loss());
                rec.reason = Some(Rejection::Plan(e.text));
                return rec.with_timeline(&tl);
            }
        };

        let Some(bh) = self
            .blockhash
            .fresh(self.clock.now_ns(), self.blockhash_max_age_ns)
        else {
            rec.loss = Some(LossReason::Build);
            rec.reason = Some(Rejection::BlockhashStaleLate);
            return rec.with_timeline(&tl);
        };

        let mut best_actionable: Option<Selected> = None;
        let mut best_probe: Option<Selected> = None;
        let mut threshold_hits: u32 = 0;
        // V9: plans the economics would act on that only the LIVE Safety preview refused.
        let mut safety_hits: u32 = 0;
        let mut rejected = Vec::new();
        // V9: the furthest technical stage a rejected plan reached (and its loss cause), and
        // whether any plan reached the economics; the loss of a candidate without a plan names it.
        let mut furthest_technical: Option<(u8, LossReason)> = None;
        let mut reached_economics = false;
        // D2: fee quotes of this evaluation (one blockhash, one slot) keyed by the message hash
        // of the built bytes: funding classes with byte-identical transactions (NoSwap and
        // Preheld inventory) share one quote; nothing is reused across evaluations.
        let mut fee_quotes: FeeQuotes = Vec::new();
        // V2-14 P1-04: plans with identical instructions, version and lookup tables (NoSwap and
        // Preheld inventory) share one compiled and signed transaction of this evaluation.
        let mut built_txs: Vec<(TxPlan, darkbot_solana::tx::BuiltTx)> = Vec::new();
        for mut plan in plans {
            // DEC-11: a bundle pays the configured tip in its own TX and is priced on the bundle
            // path, exactly as the boundary builds it.
            let bundled = self.bundles(&plan);
            if bundled {
                if let Some(tip) = self.bundle_tip {
                    plan.tip = tip.lamports;
                }
                plan.input.path = Path::JitoBundle;
            }
            // E2: the gate decides on the steps of the actually built instructions (receivership
            // TX and every prerequisite TX), immediately before they are signed.
            let table = self.builder.instruction_table();
            let checked = planner::steps_of(&plan.instructions, table).and_then(|steps| {
                planner::validate_receivership(&steps, self.allowlist, plan.route, self.p7_passed)?;
                Ok(steps)
            });
            let checked = checked.map_err(PlanInvalid::Gate).and_then(|steps| {
                if plan.prerequisite_txs.is_empty() {
                    return Ok(());
                }
                let mut txs = plan
                    .prerequisite_txs
                    .iter()
                    .map(|pre| planner::steps_of(pre, table))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(PlanInvalid::Prerequisite)?;
                txs.push(steps);
                planner::validate_bundle(&txs)
                    .map(drop)
                    .map_err(PlanInvalid::Bundle)
            });
            if let Err(e) = checked {
                note_stage(&mut furthest_technical, 1, LossReason::Build);
                rejected.push(PlanRejection {
                    funding: plan.input.funding,
                    cause: PlanCause::Invalid(e),
                });
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
                rejected.push(PlanRejection {
                    funding: plan.input.funding,
                    cause: PlanCause::ProgramNotAllowlisted(bad.program_id),
                });
                continue;
            }
            if let Some(bad) = plan
                .prerequisite_txs
                .iter()
                .flatten()
                .find(|ix| !self.prereq_allowed.contains(&ix.program_id))
            {
                note_stage(&mut furthest_technical, 2, LossReason::Build);
                rejected.push(PlanRejection {
                    funding: plan.input.funding,
                    cause: PlanCause::PrerequisiteNotAllowed(bad.program_id),
                });
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
                        rejected.push(PlanRejection {
                            funding: plan.input.funding,
                            cause: PlanCause::Safety(deny),
                        });
                        false
                    }
                },
            };

            // [FIX-3] Prerequisite-TXs nur über Bundle-Pfad; sonst ist der Kandidat nicht ausführbar.
            if !self.submitter.supports_tx_version(plan.tx_version) {
                // [FIX-38]
                note_stage(&mut furthest_technical, 3, LossReason::Submission);
                rejected.push(PlanRejection {
                    funding: plan.input.funding,
                    cause: PlanCause::TxVersion {
                        version: plan.tx_version,
                        path: self.submitter.path(),
                    },
                });
                continue;
            }
            if !plan.prerequisite_txs.is_empty() && !self.submitter.supports_bundle() {
                // [FIX-14]
                note_stage(&mut furthest_technical, 3, LossReason::Submission);
                rejected.push(PlanRejection {
                    funding: plan.input.funding,
                    cause: PlanCause::NeedsBundle,
                });
                continue;
            }
            let txplan = TxPlan {
                version: plan.tx_version,
                payer: tx::payer_of(self.payer),
                instructions: plan.instructions.clone(),
                lookup_tables: plan.lookup_tables.clone(),
                resources: self.resources,
            };
            let same = |(p, _): &&(TxPlan, darkbot_solana::tx::BuiltTx)| {
                p.version == txplan.version
                    && p.instructions == txplan.instructions
                    && *p.lookup_tables == *txplan.lookup_tables
            };
            let built = match built_txs.iter().find(same).map(|(_, b)| b.clone()) {
                Some(b) => Ok(b),
                None => tx::build(&txplan, bh.hash, &self.limits).inspect(|b| {
                    built_txs.push((txplan.clone(), b.clone()));
                }),
            };
            let built = match built {
                Ok(b) => b,
                Err(e) => {
                    note_stage(&mut furthest_technical, 4, LossReason::Build);
                    rejected.push(PlanRejection {
                        funding: plan.input.funding,
                        cause: PlanCause::Infeasible(e),
                    });
                    continue;
                }
            };

            // E2: the signed message is exactly the validated plan (with its compute budget).
            if let Err(code) = tx::message_matches_plan(&built.tx.message, &txplan) {
                note_stage(&mut furthest_technical, 5, LossReason::Build);
                rejected.push(PlanRejection {
                    funding: plan.input.funding,
                    cause: PlanCause::MessageDiffers(code),
                });
                continue;
            }
            rec.progress.transaction_built = Some(true);
            let fee_quote = if plan.input.path == Path::Rpc
                && self.submitter.path() == SubmissionPath::Rpc
                && plan.prerequisite_txs.is_empty()
                && plan.tip == 0
            {
                match self.quote_message(&built.tx, deadline, &mut fee_quotes) {
                    Ok(quote) => quote,
                    Err((loss, reason)) => {
                        rec.loss = Some(loss);
                        rec.reason = Some(reason);
                        return rec.with_timeline(&tl);
                    }
                }
            } else {
                None
            };
            // DEC-11: the bundle fee model — every transaction's exact message fee, summed, plus
            // the tip; an unknown part leaves the bundle's cost unknown (never 0).
            let bundle = if bundled {
                match self.bundle_quote(&plan, &built, bh.hash, deadline, &mut fee_quotes) {
                    Ok(quote) => Some(quote),
                    Err((loss, reason)) => {
                        rec.loss = Some(loss);
                        rec.reason = Some(reason);
                        return rec.with_timeline(&tl);
                    }
                }
            } else {
                None
            };
            let fee_lamports = match &bundle {
                Some(b) => b.fee_lamports,
                None => fee_quote.as_ref().map(|q| q.lamports),
            };
            plan.input.transaction_fee_usd_micro =
                fee_lamports.and_then(|l| self.builder.native_cost_usd_micro(l));
            if bundled {
                match self.builder.native_cost_usd_micro(plan.tip) {
                    Some(tip) => plan.input.jito_tip_usd_micro = tip,
                    None => plan.input.transaction_fee_usd_micro = None,
                }
            }
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
            if !safety_valid
                && economics::actionable(eval, self.min_profit, true, state_valid, true)
            {
                safety_hits += 1;
            }
            // [MT0] avoidable remote work, classified only from decisions already made above.
            if fee_lamports.is_some() && matches!(eval, Evaluation::Known { .. }) && !actionable {
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
                    // P2-05: the earliest second a growing tag premium could make a plan
                    // actionable; the owner re-evaluates the account then.
                    if let (Some(growth), Some(gross)) = (
                        plan.tag_growth,
                        plan.input.gross_seized_realizable_usd_micro,
                    ) {
                        let wake = economics::tag_wakeup(
                            now_unix_s,
                            growth,
                            plan.premium_bps,
                            plan.input.repay_usd_micro,
                            gross,
                            net,
                            self.min_profit,
                        );
                        rec.t_profitable_unix_s = match (rec.t_profitable_unix_s, wake) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (a, b) => a.or(b),
                        };
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
                rejected.push(PlanRejection {
                    funding: plan.input.funding,
                    cause: PlanCause::Economics(eval),
                });
                continue;
            }
            // V2-04: T_detect = valid and economic (PHASENPLAN §6) — the first plan that passes the
            // economic gate; a plan the gate rejects never marks it (its intervals stay None).
            if tl.get(Stage::Detect).is_none() {
                tl.mark(Stage::Detect, self.clock.now_ns());
                rec.detect_slot = Some(self.slot_watch.get()).filter(|s| *s > 0);
            }

            if actionable {
                let replace = best_actionable
                    .as_ref()
                    .is_none_or(|(_, _, current, _, _)| {
                        economics::compare_evaluations(eval, *current).is_gt()
                    });
                if replace {
                    best_actionable = Some((plan, built, eval, cost_record, bundle));
                }
            } else {
                let candidate_key = (
                    built.serialized_len,
                    built.locked_accounts,
                    plan.input.funding.as_str(),
                );
                let replace = best_probe
                    .as_ref()
                    .is_none_or(|(cur_plan, cur_built, _, _, _)| {
                        candidate_key
                            < (
                                cur_built.serialized_len,
                                cur_built.locked_accounts,
                                cur_plan.input.funding.as_str(),
                            )
                    });
                if replace {
                    best_probe = Some((plan, built, eval, cost_record, bundle));
                }
            }
        }

        if best_actionable.is_some() {
            rec.t_profitable_unix_s = None;
        }
        let Some((plan, mut built, eval, selected_costs, bundle)) = best_actionable.or(best_probe)
        else {
            // V9: a plan the economics would have acted on was refused by Safety: its own cause.
            if safety_hits > 0 {
                rec.decision = Decision::DeniedBySafety;
            }
            // [FIX-43] ThresholdTooHigh nur, wenn mindestens ein Kandidat positiv, aber unter MIN_PROFIT lag.
            rec.loss = Some(if safety_hits > 0 {
                LossReason::Safety
            } else if threshold_hits > 0 {
                LossReason::ThresholdTooHigh
            } else if reached_economics {
                LossReason::EconomicSkip
            } else if let Some((_, technical)) = furthest_technical {
                // V9: no plan reached the economics; the furthest technical stage is the cause.
                technical
            } else {
                LossReason::EconomicSkip
            });
            rec.reason = Some(Rejection::PlansRejected(rejected));
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
            rec.reason = Some(Rejection::TechnicalProbe);
        }

        rec.serialized_bytes = Some(built.serialized_len);
        rec.locked_accounts = Some(built.locked_accounts);
        rec.tx_version = Some(built.version.record_name().into());
        tl.mark(Stage::Ready, self.clock.now_ns());

        // [FIX-4/FIX-15] Bundle = [prerequisite TXs…, receivership TX, tip TX], only on a bundle
        // path; otherwise this single TX is the payload. P8-CTRL.3 (review H1): the whole
        // payload exists before any signature — its every fact is bound by the sign permit, so
        // no signed byte ever leaves for a simulation or a send its grant's scope would refuse.
        let use_bundle = self.bundles(&plan);
        // DEC-11: a bundle's fee is the sum quoted for exactly its transactions, a single TX's
        // its message quote; the worst-case loss follows from it (FE-02).
        let fee_lamports = if use_bundle {
            bundle.as_ref().and_then(|b| b.fee_lamports)
        } else {
            rec.selected_costs
                .as_ref()
                .and_then(|c| c.message_fee_quote.as_ref())
                .map(|q| q.lamports)
        };
        let max_loss_usd_micro = plan
            .sol_debit_lamports(fee_lamports)
            .and_then(|l| self.builder.native_cost_usd_micro(l))
            .and_then(|usd| usd.checked_add(plan.input.required_capital_usd_micro));
        let parts = if use_bundle {
            // The parts quoted with the plan (or, for a plan priced as a single TX, built now).
            let parts = match bundle {
                Some(quoted) => quoted.parts,
                None => self.bundle_parts(&plan, bh.hash),
            };
            match parts {
                Ok(parts) => Some(parts),
                Err((loss, reason)) => {
                    rec.loss = Some(loss);
                    rec.reason = Some(reason);
                    return rec.with_timeline(&tl);
                }
            }
        } else {
            None
        };
        let path = if use_bundle {
            SubmissionPath::JitoBundle
        } else {
            self.submitter.path()
        };
        let use_ = {
            let payload: Vec<&solana_sdk::transaction::VersionedTransaction> = match &parts {
                Some(p) => p
                    .prerequisites
                    .iter()
                    .map(|b| &b.tx)
                    .chain([&built.tx, &p.tip_tx])
                    .collect(),
                None => vec![&built.tx],
            };
            let priority = payload.iter().try_fold(0u64, |sum, tx| {
                sum.checked_add(tx::requested_priority_lamports(tx)?)
            });
            self.egress.as_ref().and_then(|g| {
                let grant = g.fe06.map(|a| a.digest());
                let facts = match self.fe04.as_deref() {
                    Some(fe04) => fe04.use_facts(grant),
                    None => self.use_facts?,
                };
                Some(crate::execution::one_shot::admission::attempt_use(
                    &plan,
                    &tx::payer_of(self.payer),
                    fee_lamports,
                    max_loss_usd_micro,
                    priority,
                    g.source.cloned(),
                    facts,
                ))
            })
        };
        let bound = use_.as_ref().map(|u| (u, path));
        // P8-CTRL.4: the wallet's own signature only under a permit of the candidate's purpose
        // (LIVE: a new attempt; Shadow: a SIM_ONLY simulation); else a synthetic one — the same
        // work and wire bytes, restricted: simulated without verification, never sent.
        let signing = match self.sign(&mut built.tx, bound) {
            Ok(signing) => signing,
            Err(e) => {
                rec.loss = Some(LossReason::Build);
                rec.reason = Some(Rejection::Sign(e));
                return rec.with_timeline(&tl);
            }
        };
        rec.signing = Some(signing);
        // B18: T_sign and its slot exactly once, when the payload is completely signed — here for
        // the single TX (also with a tip but no bundle path), after the bundle otherwise.
        // [FIX-40] the slot is read at T_sign, lock-free.
        if !use_bundle {
            tl.mark(Stage::Sign, self.clock.now_ns());
            rec.sign_slot = Some(self.slot_watch.get()).filter(|s| *s > 0);
        }

        // Drift-Muster: stale Opportunity / Deadline → nicht senden (Record bleibt für P6).
        // V2-04: the deadline counts from T_rx — externally defined, it never moves with the
        // T_detect mark (stricter than the former T_detect anchor).
        if self.freshness.deadline_passed(t_rx_ns, self.clock.now_ns()) {
            rec.loss = Some(LossReason::Build);
            rec.reason = Some(Rejection::ExecutionDeadline);
            return rec.with_timeline(&tl);
        }
        // [M1] Shadow baut und simuliert identisch zu LIVE. B1: this pipeline never reserves and
        // never sends; a value-bearing handoff exists only in the FE-04 one-shot owner.
        // [FIX-6] Simulate-first auf genau dieser signierten TX. Nur Ok sendet; Fehler/Unavailable → SKIP,
        // Kapital deterministisch freigeben (kein Fehlversuch im Tagesstatus, es wurde nichts gesendet).
        // [FIX-15] Bundle-Kandidaten werden als Bundle simuliert, Einzel-TX als Einzel-TX — nie gemischt.
        // [FIX-4] Bundle = [prerequisite TXs…, receivership TX, tip TX]; Tip nie in der Receivership-TX.
        let mut bundle_txs: Vec<solana_sdk::transaction::VersionedTransaction> = Vec::new();
        if let Some(BundleParts {
            prerequisites,
            tip_tx,
        }) = parts
        {
            for mut b in prerequisites {
                if let Err(e) = self.sign(&mut b.tx, bound) {
                    rec.loss = Some(LossReason::Build);
                    rec.reason = Some(Rejection::PrerequisiteSign(e));
                    return rec.with_timeline(&tl);
                }
                bundle_txs.push(b.tx);
            }
            bundle_txs.push(built.tx.clone());
            let mut tip_tx = tip_tx;
            if let Err(e) = self.sign(&mut tip_tx, bound) {
                rec.loss = Some(LossReason::Build);
                rec.reason = Some(Rejection::Sign(e));
                return rec.with_timeline(&tl);
            }
            rec.submitted_tip =
                tip_tx
                    .signatures
                    .first()
                    .map(|s| crate::execution::pending::SubmittedTip {
                        signature: s.to_string(),
                        lamports: plan.tip,
                    });
            bundle_txs.push(tip_tx);
        }
        if use_bundle {
            tl.mark(Stage::Sign, self.clock.now_ns());
            rec.sign_slot = Some(self.slot_watch.get()).filter(|s| *s > 0);
            rec.path = Some(SubmissionPath::JitoBundle.as_str().into());
        }
        if Instant::now() >= deadline {
            rec.loss = Some(LossReason::RpcDeadline);
            rec.reason = Some(Rejection::RpcDeadlineBeforeSimulation);
            return rec.with_timeline(&tl);
        }
        // DEC-02: a LIVE candidate with an FE-04 admission simulates its exact signed payload
        // once — the typed token the admission reuses at the handoff while it stays bound. A
        // payload that cannot be formed is simulated as before (the admission refuses it).
        let live_payload = match (self.mode, self.fe04.as_deref()) {
            (Mode::Live, Some(fe04)) => {
                let (path, txs) = if use_bundle {
                    (SubmissionPath::JitoBundle, bundle_txs.clone())
                } else {
                    (self.submitter.path(), vec![built.tx.clone()])
                };
                SignedPayload::new(txs, path, tx::payer_of(self.payer))
                    .ok()
                    .map(|payload| {
                        let binding = SimulationBinding {
                            state_generation: fe04.state_generation,
                            program_data_hash: fe04.program_data_hash.clone(),
                            state_valid,
                        };
                        (payload, binding)
                    })
            }
            _ => None,
        };
        let mut simulated_payload = None;
        let simulation_started = Instant::now();
        let sim = match live_payload
            .map(|(payload, binding)| payload.simulate_bound(self.submitter, deadline, binding))
        {
            Some(Ok((result, token))) => {
                simulated_payload = token;
                result
            }
            _ if use_bundle => self.submitter.simulate_bundle(&bundle_txs, deadline),
            // P8-CTRL.4: a synthetic signature is simulated without verification.
            _ if signing == Signing::Synthetic => {
                self.submitter.simulate_unverified(&built.tx, deadline)
            }
            _ => self.submitter.simulate(&built.tx, deadline),
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
            SimResult::Ok { units_consumed, .. } => {
                rec.simulation_result = Some("PASS".into());
                rec.simulated_cu = units_consumed;
            }
            // N2: no simulation result exists; the cause is our own network bound.
            SimResult::Deadline => {
                rec.loss = Some(LossReason::RpcDeadline);
                rec.reason = Some(Rejection::RpcDeadlineSimulation);
                return rec.with_timeline(&tl);
            }
            SimResult::BudgetRejected(code) => {
                rec.loss = Some(LossReason::RpcBudget);
                rec.reason = Some(Rejection::RpcBudgetSimulation(code));
                return rec.with_timeline(&tl);
            }
            other => {
                rec.simulation_result = Some(other.record_text());
                if let SimResult::Failed(f) = &other {
                    self.simulation_failure = Some(f.clone());
                }
                rec.loss = Some(LossReason::Build);
                rec.reason = Some(Rejection::Simulation(other.clone()));
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
            rec.loss = Some(LossReason::Safety);
            rec.reason = Some(Rejection::Safety(d));
            return rec.with_timeline(&tl);
        }
        let Some(fe04) = self.fe04.as_deref_mut() else {
            // Not a Safety denial: the send capability is unavailable on this path.
            rec.decision = Decision::Unavailable;
            rec.loss = Some(LossReason::Submission);
            rec.reason = Some(Rejection::LiveSendRequiresFe04 { bundle: use_bundle });
            return rec.with_timeline(&tl);
        };
        // DEC-11/FE-02: the fee and the worst-case loss bound before the signature.
        // FE-02: the day's loss limit holds with this attempt's worst case (an unknown worst case
        // is refused by the admission).
        if let Some(max_loss) = max_loss_usd_micro {
            if let Err(d) = self.safety.admit_loss(now_unix_s, max_loss) {
                rec.decision = Decision::DeniedBySafety;
                rec.loss = Some(LossReason::Safety);
                rec.reason = Some(Rejection::Safety(d));
                return rec.with_timeline(&tl);
            }
        }
        // DEC-04: B of the signed plan (not in OFF); the inventory hold is A's unless an
        // effective ACTIVE qualifies B — the same value is reserved and released.
        let hold_b = (self.hold.mode != crate::execution::hold_b::HoldMode::Off).then(|| {
            self.builder
                .hold_b(&plan, plan.sol_debit_lamports(fee_lamports))
        });
        let inventory_hold = self.hold.inventory_hold(
            plan.inventory_hold_usd_micro,
            hold_b.as_ref(),
            plan.input.funding.as_str(),
        );
        // B1/B15: capital and the worst-case inventory are reserved before the durable admission.
        if let Err(d) = self.safety.check(
            now_unix_s,
            plan.exposure,
            inventory_hold,
            plan.tip,
            plan.slippage_bps,
            &plan.assets,
            "longtail",
        ) {
            rec.decision = Decision::DeniedBySafety;
            rec.loss = Some(LossReason::Safety);
            rec.reason = Some(Rejection::Safety(d));
            return rec.with_timeline(&tl);
        }
        let (path, txs) = if use_bundle {
            (SubmissionPath::JitoBundle, bundle_txs)
        } else {
            (self.submitter.path(), vec![built.tx.clone()])
        };
        // DEC-13b (review L1): the attempt is valid no longer than the blockhash it is signed with.
        if let Some(valid) = bh.valid_until_slot() {
            fe04.valid_until_slot = fe04.valid_until_slot.min(valid);
        }
        let t_handoff_ns = self.clock.now_ns();
        let admission = fe04.admit(
            crate::execution::one_shot::admission::Candidate {
                plan: &plan,
                target: account,
                txs,
                path,
                payer: tx::payer_of(self.payer),
                fee_lamports,
                net_usd_micro: rec.net_profit.unwrap_or(i128::MIN),
                max_loss_usd_micro,
                slot,
                last_valid_block_height: bh.last_valid_block_height,
                now_unix_ms: now_unix_s.saturating_mul(1_000),
                evaluated,
                slot_now: &|| self.slot_watch.get(),
                state_valid,
                deadline,
                t_handoff_ns,
                simulated: simulated_payload,
                hold: hold_b.map(|b| crate::execution::hold_b::HoldEvidence {
                    mode: self.hold.mode,
                    a_inventory_usd_micro: plan.inventory_hold_usd_micro,
                    b,
                    reserved_inventory_usd_micro: inventory_hold,
                }),
            },
            self.submitter,
            self.egress.as_ref(),
        );
        use crate::execution::one_shot::admission::Admission;
        match admission {
            Admission::Submitted(ack) => {
                // DEC-13b: the reservation stays until the journal settlement books it.
                fe04.reserved = Some(crate::execution::one_shot::admission::SafetyReserved {
                    capital: plan.exposure,
                    inventory: inventory_hold,
                });
                tl.mark(Stage::Handoff, ack.t_handoff_ns);
                rec.decision = Decision::Submitted;
                rec.signature = Some(ack.signature.to_string());
                rec.all_signatures = ack.signatures.iter().map(ToString::to_string).collect();
                rec.bundle_id = ack.submission_id.clone();
                rec.exposure = Some(plan.exposure);
            }
            Admission::Refused(reason) => {
                // Nothing durable exists and nothing was sent: the reservation is released.
                if let Err(v) = self
                    .safety
                    .release_without_outcome(plan.exposure, inventory_hold)
                {
                    rec.reason = Some(Rejection::SubmissionRefused {
                        reason,
                        release: Some(v.code()),
                    });
                } else {
                    rec.reason = Some(Rejection::SubmissionRefused {
                        reason,
                        release: None,
                    });
                }
                rec.decision = Decision::Unavailable;
                rec.loss = Some(LossReason::Submission);
            }
            Admission::Unknown(reason) => {
                // A durable reservation/handoff exists: UNKNOWN keeps every hold and halts.
                fe04.reserved = Some(crate::execution::one_shot::admission::SafetyReserved {
                    capital: plan.exposure,
                    inventory: inventory_hold,
                });
                self.safety
                    .halt(darkbot_common::error::HaltReason::UnknownEconomics);
                rec.decision = Decision::AmbiguousSubmission;
                rec.loss = Some(LossReason::Submission);
                rec.exposure = Some(plan.exposure);
                rec.reason = Some(Rejection::SubmissionUnknown(reason));
            }
        }
        rec.with_timeline(&tl)
    }

    /// [FIX-4/FIX-15]/DEC-11: `plan` goes as a bundle — prerequisite TXs or a tip, on a bundle
    /// path; the one rule the pricing and the boundary share.
    fn bundles(&self, plan: &ReceivershipPlan) -> bool {
        self.submitter.supports_bundle() && (!plan.prerequisite_txs.is_empty() || plan.tip > 0)
    }

    /// N2/D2: the network fee of one exact message, quoted once per evaluation (`fee_quotes`,
    /// keyed by the message hash) and bound to that hash. `Ok(None)`: unknown — an unavailable
    /// quote, a transport failure or another hash, never 0; `Err`: the evaluation ends with its
    /// own cause (the deadline or a refused call).
    fn quote_message(
        &self,
        tx: &solana_sdk::transaction::VersionedTransaction,
        deadline: Instant,
        fee_quotes: &mut FeeQuotes,
    ) -> Result<Option<submit::MessageFeeQuote>, (LossReason, Rejection)> {
        if Instant::now() >= deadline {
            return Err((
                LossReason::RpcDeadline,
                Rejection::RpcDeadlineBeforeFeeQuote,
            ));
        }
        let message_hash = tx.message.hash();
        let cached = fee_quotes
            .iter()
            .find(|(h, _)| *h == message_hash)
            .map(|(_, q)| q.clone());
        let fee = match cached {
            Some(quote) => quote,
            None => {
                let fee_started = Instant::now();
                let quote = self.submitter.fee_for_message(tx, deadline);
                // [MT0] H6: would a local fee model have matched the RPC fee?
                // (measurement only, once per RPC quote)
                if let Ok(q) = &quote {
                    match local_fee_estimate_lamports(tx) {
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
        match fee {
            Err(submit::FeeQuoteError::Deadline) => {
                Err((LossReason::RpcDeadline, Rejection::RpcDeadlineInFeeQuote))
            }
            Err(submit::FeeQuoteError::BudgetRejected(code)) => {
                Err((LossReason::RpcBudget, Rejection::RpcBudgetFeeQuote(code)))
            }
            Err(_) => Ok(None),
            // V2-14 P1-02: compared with the hash computed above, typed (an unreadable quote
            // hash is no match); the quote's persisted text form stays (layout).
            Ok(q) => Ok(Some(q).filter(|q| q.message_hash == message_hash)),
        }
    }

    /// DEC-11: the bundle fee model of `plan` — its other transactions from the evaluated
    /// blockhash and the sum of every transaction's exact message fee (prerequisite TXs, the
    /// receivership TX `built`, the tip TX; the tip itself is the plan's). Parts that cannot be
    /// built are kept for the boundary (their typed rejection) with the fee unknown.
    fn bundle_quote(
        &self,
        plan: &ReceivershipPlan,
        built: &darkbot_solana::tx::BuiltTx,
        blockhash: solana_sdk::hash::Hash,
        deadline: Instant,
        fee_quotes: &mut FeeQuotes,
    ) -> Result<BundleQuote, (LossReason, Rejection)> {
        let parts = self.bundle_parts(plan, blockhash);
        let mut fee_lamports = None;
        if let Ok(p) = &parts {
            let mut sum = Some(0u64);
            let txs = p.prerequisites.iter().map(|b| &b.tx);
            for tx in txs.chain([&built.tx, &p.tip_tx]) {
                match self.quote_message(tx, deadline, fee_quotes)? {
                    Some(q) => sum = sum.and_then(|s| s.checked_add(q.lamports)),
                    None => {
                        sum = None;
                        break;
                    }
                }
            }
            fee_lamports = sum;
        }
        Ok(BundleQuote {
            parts,
            fee_lamports,
        })
    }

    /// [FIX-4]/DEC-11: a bundle's transactions besides the receivership TX, from `blockhash` —
    /// every prerequisite TX built and checked against its plan (signed at the boundary) and the
    /// signed tip TX to the configured tip account, checked against its plan (E2). The tip is
    /// never in the receivership TX.
    fn bundle_parts(
        &self,
        plan: &ReceivershipPlan,
        blockhash: solana_sdk::hash::Hash,
    ) -> Result<BundleParts, (LossReason, Rejection)> {
        let Some(tip) = self.bundle_tip else {
            return Err((LossReason::Submission, Rejection::BundleWithoutTip));
        };
        let mut prerequisites = Vec::with_capacity(plan.prerequisite_txs.len());
        for pre in &plan.prerequisite_txs {
            let pre_plan = TxPlan {
                version: BuildVersion::V0,
                payer: tx::payer_of(self.payer),
                instructions: pre.clone(),
                lookup_tables: plan.lookup_tables.clone(),
                resources: self.resources,
            };
            let b = tx::build(&pre_plan, blockhash, &self.limits)
                .map_err(|e| (LossReason::Build, Rejection::PrerequisiteBuild(e)))?;
            tx::message_matches_plan(&b.tx.message, &pre_plan)
                .map_err(|code| (LossReason::Build, Rejection::PrerequisiteMessage(code)))?;
            prerequisites.push(b);
        }
        let tip_tx =
            submit::build_tip_tx(&tx::payer_of(self.payer), &tip.account, plan.tip, blockhash)
                .map_err(|e| (LossReason::Build, Rejection::TipBuild(e)))?;
        submit::tip_matches(&tip_tx, &tx::payer_of(self.payer), &tip.account, plan.tip)
            .map_err(|code| (LossReason::Build, Rejection::TipMismatch(code)))?;
        Ok(BundleParts {
            prerequisites,
            tip_tx,
        })
    }
}

/// The tests' synthetic signer of every evaluation (one per test process).
#[cfg(test)]
pub(crate) fn test_synthetic() -> &'static Keypair {
    static KEY: std::sync::OnceLock<Keypair> = std::sync::OnceLock::new();
    KEY.get_or_init(Keypair::new)
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
            lookup_tables: vec![].into(),
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
            // V17: every kind has its own persisted cause (before: both ROUTING).
            K::HealthUnavailable => (Decision::Unavailable, LossReason::Health),
            K::Routing => (Decision::Skip, LossReason::Routing),
            K::Other => (Decision::Skip, LossReason::Plan),
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
        // An untyped string is an unclassified planning failure (V17: PLAN, never ROUTING).
        let e = PlanError::from("FEE_STATE_UNAVAILABLE:looks typed but is not");
        assert_eq!(e.kind, K::Other);
        assert_eq!((e.decision(), e.loss()), (Decision::Skip, LossReason::Plan));
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
            program_id: const { Pubkey::from_str_const(COMPUTE_BUDGET_PROGRAM_ID) },
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
