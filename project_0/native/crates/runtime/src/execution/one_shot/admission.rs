//! B1/FE-04: the one durable admission of a LIVE pipeline candidate. The pipeline reaches a
//! submitter only through here: the exact signed payload is simulated, bound into an intent
//! (signatures, wire digest, wallet, program/deployment pin, funding class, fee, tip, record
//! rent, the full SOL debit, capital, net, slippage, blockhash lifetime, path) and reserved
//! durably (fsync) in the FE-04 wallet journal against the explicit approval and the current
//! wallet facts; the adapter is called exactly once, after the UNKNOWN handoff is durable.
//! Nothing here enables LIVE: without an admission the pipeline still stops at its boundary.
use super::transport::SignedPayload;
use super::{Approval, Current, Intent, Journal};
use crate::execution::pipeline::ReceivershipPlan;
use darkbot_solana::submit::{SubmissionAdapter, SubmissionPath, SubmitAck};
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};
use std::collections::BTreeMap;

/// The FE-04 one-shot owner's journal with its explicit approval and the current wallet facts.
pub struct Fe04Admission {
    pub journal: Journal,
    pub approval: Approval,
    pub state_generation: u64,
    /// The currently observed deployment (ProgramData) hash.
    pub program_data_hash: String,
    /// Reconciled wallet balances (`"SOL"` lamports, mint base units).
    pub balances: BTreeMap<String, u64>,
    /// What other attempts hold (`WalletReservations::held_by_others`).
    pub reserved_elsewhere: BTreeMap<String, u64>,
    /// Never-spent policy reserve (`inventory::reserves`).
    pub policy_reserve: BTreeMap<String, u64>,
    /// The last slot at which the signed blockhash is valid.
    pub valid_until_slot: u64,
    /// DEC-02: what became of the pipeline's simulations at the handoff.
    pub simulation: SimulationStats,
    /// DEC-13b: what the pipeline's `check()` reserved in Safety for the attempt it handed off
    /// (submitted or UNKNOWN); released only by the journal settlement's booking (C3-Rest).
    pub reserved: Option<SafetyReserved>,
}

/// DEC-13b: the Safety reservation of an admitted attempt — its capital and worst-case inventory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SafetyReserved {
    pub capital: u64,
    pub inventory: u64,
}

/// DEC-02: the pipeline simulation reused, a new simulation, or a refusal without one (no
/// time left), and the condition that last prevented a reuse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimulationStats {
    pub reused: u64,
    pub resimulated: u64,
    pub refused: u64,
    pub last_reuse_refusal: Option<&'static str>,
}

/// FE-02: the economics an intent binds are complete — a realizable value and the rent/token
/// cost (fee and max loss are checked separately) — never assumed by the admission.
pub(crate) fn economics_known(input: &darkbot_project0::economics::CandidateInput) -> bool {
    input.gross_seized_realizable_usd_micro.is_some() && input.rent_and_token_usd_micro.is_some()
}

/// P8-CTRL.5: a new attempt within its grant's limits — the attempts of this UTC day, of the
/// grant's campaign and of the grant itself, each against its ceiling of the scope (an unreadable
/// journal refuses).
pub(super) fn within_limits(
    journal: &Journal,
    booking: &super::Fe06Attempt,
    scope: &crate::orchestration::fe06::Scope,
) -> Result<(), String> {
    use super::Fe06Quota;
    let records = journal.attempt_records()?;
    let booked = || records.iter().filter_map(|r| r.fe06.as_ref());
    for (quota, limit) in [
        (
            Fe06Quota::UtcDay(booking.utc_day),
            scope.max_attempts_per_utc_day,
        ),
        (
            Fe06Quota::Campaign(&booking.campaign),
            scope.max_attempts_campaign,
        ),
        (Fe06Quota::Grant(&booking.grant), scope.max_attempts_grant),
    ] {
        let used = quota.used(booked());
        if used >= limit {
            return Err(format!(
                "FE06_ATTEMPT_LIMIT_{}: {used} of {limit} attempts used",
                quota.name()
            ));
        }
    }
    Ok(())
}

/// Everything the pipeline hands over for one actionable, simulated candidate.
pub struct Candidate<'a> {
    pub plan: &'a ReceivershipPlan,
    pub target: Pubkey,
    pub txs: Vec<VersionedTransaction>,
    pub path: SubmissionPath,
    pub payer: Pubkey,
    /// The network fee of the payload: the exact message's quote, a bundle's the sum of every
    /// transaction's quote (DEC-11); `None` is unknown.
    pub fee_lamports: Option<u64>,
    pub net_usd_micro: i128,
    /// Worst-case loss of the attempt (USD micro); `None` is unknown.
    pub max_loss_usd_micro: Option<u64>,
    pub slot: u64,
    /// C12: `lastValidBlockHeight` of the blockhash the payload is signed with.
    pub last_valid_block_height: u64,
    pub now_unix_ms: u64,
    /// V2-15: the monotonic instant at which `now_unix_ms` was read (the evaluation).
    pub evaluated: std::time::Instant,
    /// V2-15: the newest observed slot, read at the handoff.
    pub slot_now: &'a dyn Fn() -> u64,
    pub state_valid: bool,
    pub deadline: std::time::Instant,
    pub t_handoff_ns: u64,
    /// DEC-02: the pipeline's simulation of exactly `txs`, when it passed (`None`: none).
    pub simulated: Option<super::transport::SimulatedPayload<'a>>,
    /// DEC-04: the mode, A's inventory hold and B the attempt is admitted with (`None`: OFF).
    pub hold: Option<crate::execution::hold_b::HoldEvidence>,
}

#[derive(Debug)]
pub enum Admission {
    /// Durably handed off and submitted once; the outcome is UNKNOWN until settled.
    Submitted(SubmitAck),
    /// Refused before anything durable: no hold exists, nothing was sent.
    Refused(String),
    /// A durable reservation or handoff exists but the submission's fate is unknown: every hold
    /// stays, the attempt is UNKNOWN (never released here).
    Unknown(String),
}

impl Fe04Admission {
    /// P8-CTRL.7: `gates` issue the egress permit of exactly this payload before anything is
    /// reserved (`None` or a refusal: nothing durable, nothing sent).
    pub fn admit(
        &mut self,
        c: Candidate<'_>,
        adapter: &dyn SubmissionAdapter,
        gates: Option<&crate::orchestration::capability_control::LiveGates<'_>>,
    ) -> Admission {
        let refuse = |s: &str| Admission::Refused(s.into());
        let a = &self.approval;
        // DEC-11: a bundle is admitted like a single TX — its summed fee, the tip and every
        // transaction's signature are bound into the intent and its SOL/inventory holds; it is
        // reserved only with a bundle simulation of exactly these bytes (`bundle_simulated`).
        if c.plan.input.funding.as_str() != a.funding_class {
            return refuse("FE04_FUNDING_CLASS_MISMATCH");
        }
        if c.payer.to_string() != a.wallet {
            return refuse("FE04_WALLET_MISMATCH");
        }
        let Some(fee) = c.fee_lamports else {
            return refuse("FE04_FEE_UNKNOWN");
        };
        let Some(rent) = c.plan.sol_costs.record_rent_lamports else {
            return refuse("FE04_RENT_UNKNOWN");
        };
        // The full SOL debit includes the flat protocol fee, which the intent's fee/tip/rent
        // fields do not name: it is bound into the SOL hold and checked against the approval.
        let Some(sol_debit) = c.plan.sol_debit_lamports(Some(fee)) else {
            return refuse("FE04_SOL_DEBIT_UNKNOWN");
        };
        if sol_debit > a.max_total_sol_debit_lamports {
            return refuse("APPROVAL_BUDGET_EXCEEDED:SOL_DEBIT");
        }
        let Some(max_loss) = c.max_loss_usd_micro else {
            return refuse("FE04_MAX_LOSS_UNKNOWN");
        };
        let mut holds: BTreeMap<String, u64> = [("SOL".to_string(), sol_debit)].into();
        for (mint, units) in &c.plan.inventory_holds {
            let held = holds.entry(mint.to_string()).or_insert(0);
            *held = held.saturating_add(*units);
        }
        // P8-CTRL.3: the priority fee the signed payload itself requests (every transaction).
        let priority = c.txs.iter().try_fold(0u64, |sum, tx| {
            sum.checked_add(darkbot_solana::tx::requested_priority_lamports(tx)?)
        });
        let payload = match SignedPayload::new(c.txs, c.path, c.payer) {
            Ok(p) => p,
            Err(e) => return Admission::Refused(e),
        };
        let (signatures, wire_sha256) = (
            payload.signatures().to_vec(),
            payload.wire_sha256().to_string(),
        );
        // DEC-02: the pipeline's simulation stands for the handoff only while every binding
        // condition holds; otherwise a new one within the deadline, or a refusal without a hold.
        let binding = super::transport::SimulationBinding {
            state_generation: self.state_generation,
            program_data_hash: self.program_data_hash.clone(),
            state_valid: c.state_valid,
        };
        let max_age = std::time::Duration::from_millis(a.max_state_age_ms);
        let refusal = c.simulated.as_ref().map_or(Some("SIM_REUSE_NONE"), |s| {
            s.reuse_refusal(&payload, c.slot, &binding, max_age)
        });
        let simulated = match (refusal, c.simulated) {
            (None, Some(simulated)) => {
                self.simulation.reused = self.simulation.reused.saturating_add(1);
                simulated
            }
            (refusal, _) => {
                self.simulation.last_reuse_refusal = refusal;
                if std::time::Instant::now() >= c.deadline {
                    self.simulation.refused = self.simulation.refused.saturating_add(1);
                    return refuse("FE04_SIMULATION_NOT_REUSABLE_NO_TIME");
                }
                self.simulation.resimulated = self.simulation.resimulated.saturating_add(1);
                let simulated = match payload.simulate(adapter, c.deadline, binding) {
                    Ok(s) => s,
                    Err(e) => return Admission::Refused(e),
                };
                // The new simulation is bound like a reused one: a node still behind the state
                // slot is refused before anything is reserved.
                if !simulated.context_reaches(c.slot) {
                    self.simulation.refused = self.simulation.refused.saturating_add(1);
                    return refuse("FE04_SIMULATION_CONTEXT_BEHIND");
                }
                simulated
            }
        };
        // DEC-12: the handoff is bounded by the candidate's deadline; past it nothing is reserved.
        if std::time::Instant::now() >= c.deadline {
            return refuse("FE04_HANDOFF_DEADLINE");
        }
        // P8-CTRL.7: the egress permit of exactly this payload, before anything is reserved —
        // P8-CTRL.3/.5: within its grant's scope with every fact of the attempt.
        let permit = match gates.map(|g| {
            use crate::orchestration::fe06;
            let market = c.plan.market_identity.as_ref();
            let use_ = fe06::Use::Attempt(fe06::AttemptUse {
                execution_class: market.map(|m| m.liquidation_type.clone()),
                funding_class: a.funding_class.clone(),
                fee_payer: c.payer.to_string(),
                banks: market.map(|m| [m.asset_bank.clone(), m.liability_bank.clone()]),
                mints: c.plan.assets.iter().map(ToString::to_string).collect(),
                input_usd_micro: c.plan.input.required_capital_usd_micro,
                exposure_usd_micro: c.plan.exposure,
                max_loss_usd_micro: max_loss,
                fee_lamports: fee,
                priority_fee_lamports: priority,
                tip_lamports: c.plan.tip,
                rent_lamports: rent,
                protocol_fee_lamports: c.plan.sol_costs.flat_fee_lamports,
                spend_lamports: sol_debit,
                sol_balance_lamports: self.balances.get("SOL").copied(),
                slippage_bps: c.plan.slippage_bps,
                route: fe06::route_class(c.plan.route),
                data_source: g.source.cloned(),
            });
            crate::orchestration::capability_control::EgressPermit::issue(
                g,
                &use_,
                simulated.signatures(),
                simulated.path(),
                c.deadline,
            )
        }) {
            Some(Ok(p)) => p,
            Some(Err(e)) => return Admission::Refused(e),
            None => return refuse("EGRESS_NOT_PERMITTED: no egress authority"),
        };
        // P8-CTRL.5: the grant's new-attempt limits — every attempt booked against it, UNKNOWN or
        // sent, counts; the booking is written with the reservation, before any dispatch.
        let fe06 = match gates
            .and_then(|g| g.fe06)
            .and_then(|a| a.scope().map(|s| (a, s)))
        {
            Some((grant, scope)) => {
                let booking = super::Fe06Attempt {
                    grant: grant.digest().to_string(),
                    campaign: scope.campaign_id.clone(),
                    utc_day: c.now_unix_ms / 86_400_000,
                    owner_epoch: permit.epoch(),
                };
                if let Err(e) = within_limits(&self.journal, &booking, scope) {
                    return Admission::Refused(e);
                }
                Some(booking)
            }
            None => None,
        };
        let intent = Intent {
            code_sha: a.code_sha.clone(),
            binary_sha256: a.binary_sha256.clone(),
            wallet: a.wallet.clone(),
            program: a.program.clone(),
            program_data_hash: a.program_data_hash.clone(),
            funding_class: a.funding_class.clone(),
            target: c.target.to_string(),
            signatures,
            signed_wire_sha256: wire_sha256,
            state_generation: self.state_generation,
            state_slot: c.slot,
            valid_until_slot: self.valid_until_slot,
            evaluated_unix_ms: c.now_unix_ms,
            capital_usd_micro: c.plan.input.required_capital_usd_micro,
            transaction_fee_lamports: fee,
            tip_lamports: c.plan.tip,
            rent_lamports: rent,
            max_loss_usd_micro: max_loss,
            conservative_net_usd_micro: c.net_usd_micro,
            slippage_bps: c.plan.slippage_bps,
            holds,
            expected_wallet_assets: std::iter::once("SOL".to_string())
                .chain(c.plan.assets.iter().map(ToString::to_string))
                .collect(),
            last_valid_block_height: Some(c.last_valid_block_height),
        };
        let current = Current {
            now_unix_ms: c.now_unix_ms,
            slot: c.slot,
            state_generation: self.state_generation,
            program_data_hash: self.program_data_hash.clone(),
            balances: self.balances.clone(),
            reserved_elsewhere: self.reserved_elsewhere.clone(),
            policy_reserve: self.policy_reserve.clone(),
            health_funding_route_fee_exit_known: economics_known(&c.plan.input),
            canonical_fresh_no_gap_or_fork: c.state_valid,
            exact_signed_payload_simulated: simulated.exact_signed_payload_simulated(),
            bundle_simulated: simulated.bundle_simulated(),
        };
        if let Err(e) =
            self.journal
                .reserve_payload(a.clone(), intent, &current, &simulated, c.hold, fe06)
        {
            // A failed write may still have reached the disk: never treated as nothing.
            return if e.starts_with("JOURNAL") {
                Admission::Unknown(e)
            } else {
                Admission::Refused(e)
            };
        }
        // V2-15 (P9-01b): the handoff is validated at the handoff — the evaluation's time advanced
        // by the monotonic time since (one clock domain) and the newest observed slot; a state
        // that aged past the approval during the simulation is never sent.
        let elapsed_ms = u64::try_from(c.evaluated.elapsed().as_millis()).unwrap_or(u64::MAX);
        let at_handoff = Current {
            now_unix_ms: c.now_unix_ms.saturating_add(elapsed_ms),
            slot: c.slot.max((c.slot_now)()),
            ..current
        };
        match self.journal.submit_payload(
            &simulated,
            &at_handoff,
            c.t_handoff_ns,
            c.deadline,
            permit,
        ) {
            Ok(ack) => Admission::Submitted(ack),
            Err(e) => Admission::Unknown(e),
        }
    }
}
