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
}

/// FE-02: the economics an intent binds are complete — a realizable value and the rent/token
/// cost (fee and max loss are checked separately) — never assumed by the admission.
pub(crate) fn economics_known(input: &darkbot_project0::economics::CandidateInput) -> bool {
    input.gross_seized_realizable_usd_micro.is_some() && input.rent_and_token_usd_micro.is_some()
}

/// Everything the pipeline hands over for one actionable, simulated candidate.
pub struct Candidate<'a> {
    pub plan: &'a ReceivershipPlan,
    pub target: Pubkey,
    pub txs: Vec<VersionedTransaction>,
    pub path: SubmissionPath,
    pub payer: Pubkey,
    /// The exact message's network fee (fee quote); `None` is unknown.
    pub fee_lamports: Option<u64>,
    pub net_usd_micro: i128,
    /// Worst-case loss of the attempt (USD micro); `None` is unknown.
    pub max_loss_usd_micro: Option<u64>,
    pub slot: u64,
    pub now_unix_ms: u64,
    pub state_valid: bool,
    pub deadline: std::time::Instant,
    pub t_handoff_ns: u64,
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
    pub fn admit(&mut self, c: Candidate<'_>, adapter: &dyn SubmissionAdapter) -> Admission {
        let refuse = |s: &str| Admission::Refused(s.into());
        let a = &self.approval;
        if !c.plan.prerequisite_txs.is_empty() {
            return refuse("FE04_BUNDLE_FEES_UNQUOTED");
        }
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
        if let Some((mint, units)) = c.plan.liability_hold {
            holds.insert(mint.to_string(), units);
        }
        let payload = match SignedPayload::new(c.txs, c.path, c.payer) {
            Ok(p) => p,
            Err(e) => return Admission::Refused(e),
        };
        let (signatures, wire_sha256) = (
            payload.signatures().to_vec(),
            payload.wire_sha256().to_string(),
        );
        let simulated = match payload.simulate(adapter, c.deadline) {
            Ok(s) => s,
            Err(e) => return Admission::Refused(e),
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
            exact_signed_payload_simulated: true,
            bundle_simulated: c.path == SubmissionPath::JitoBundle,
        };
        if let Err(e) = self
            .journal
            .reserve_payload(a.clone(), intent, &current, &simulated)
        {
            // A failed write may still have reached the disk: never treated as nothing.
            return if e.starts_with("JOURNAL") {
                Admission::Unknown(e)
            } else {
                Admission::Refused(e)
            };
        }
        match self
            .journal
            .submit_payload(&simulated, &current, c.t_handoff_ns)
        {
            Ok(ack) => Admission::Submitted(ack),
            Err(e) => Admission::Unknown(e),
        }
    }
}
