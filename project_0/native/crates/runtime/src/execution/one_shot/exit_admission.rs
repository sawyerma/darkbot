//! FE-04 exit admission: the one durable admission of an exit of the open inventory. The exit
//! route (`JupiterV2RouteProvider::exit_to_sol`) is re-validated against the approval's mint and
//! the payer (`exit_route_terms`), must sell exactly the open position and close an empty WSOL
//! account (anything already in it would be paid out as proceeds); the transaction is built from
//! exactly those instructions, signed, quoted for its exact message fee and simulated through the
//! production adapter; the exit is reserved durably against the approval and the current wallet
//! and the adapter is called exactly once, after its UNKNOWN handoff is durable. Nothing here
//! enables LIVE or creates an approval.
use super::admission::Admission;
use super::exit::{ExitApproval, ExitIntent};
use super::transport::SignedPayload;
use super::{Current, Journal};
use crate::execution::builder::p0::Route;
use crate::execution::route::{exit_route_terms, WSOL_MINT};
use darkbot_solana::submit::SubmissionAdapter;
use darkbot_solana::tx::{self, BuildVersion, Resources, TxLimits, TxPlan};
use solana_sdk::{hash::Hash, signature::Keypair, signer::Signer};
use std::collections::BTreeMap;

/// Everything one exit admission needs besides the journal, the approval and the adapter.
pub struct ExitCandidate<'a> {
    pub route: &'a Route,
    pub payer: &'a Keypair,
    pub blockhash: Hash,
    /// `lastValidBlockHeight` of `blockhash`.
    pub last_valid_block_height: u64,
    pub resources: Resources,
    pub limits: TxLimits,
    pub slot: u64,
    pub now_unix_ms: u64,
    pub state_valid: bool,
    /// Reconciled wallet balances (`"SOL"` lamports, mint base units; the WSOL mint's must be 0).
    pub balances: BTreeMap<String, u64>,
    pub reserved_elsewhere: BTreeMap<String, u64>,
    pub policy_reserve: BTreeMap<String, u64>,
    pub deadline: std::time::Instant,
    pub t_handoff_ns: u64,
}

/// P8-CTRL.7: `gates` issue the exit's egress permit before anything is reserved.
pub fn admit_exit(
    journal: &mut Journal,
    approval: ExitApproval,
    c: ExitCandidate<'_>,
    adapter: &dyn SubmissionAdapter,
    gates: Option<&crate::orchestration::capability_control::LiveGates<'_>>,
) -> Admission {
    let refuse = |s: &str| Admission::Refused(s.into());
    let payer = c.payer.pubkey();
    let r = c.route;
    let Ok(mint) = approval.mint.parse() else {
        return refuse("INVALID_OR_UNAUTHORIZED_EXIT_APPROVAL");
    };
    let terms = match exit_route_terms(&r.instructions, &payer, &mint) {
        Ok(t) => t,
        Err(e) => return Admission::Refused(e),
    };
    if terms.in_amount != r.amount_in
        || Some(terms.minimum_out) != r.minimum_executable_out
        || r.swap_program.to_string() != approval.route_program
    {
        return refuse("EXIT_ROUTE_NOT_BOUND");
    }
    if c.balances.get(WSOL_MINT) != Some(&0) {
        return refuse("EXIT_WSOL_ACCOUNT_NOT_EMPTY_OR_UNKNOWN");
    }
    let plan = TxPlan {
        version: BuildVersion::V0,
        payer,
        instructions: r.instructions.clone(),
        lookup_tables: r.lookup_tables.clone().into(),
        resources: c.resources,
    };
    let mut built = match tx::build(&plan, c.blockhash, &c.limits) {
        Ok(b) => b,
        Err(e) => return Admission::Refused(format!("EXIT_BUILD: {e}")),
    };
    if tx::message_matches_plan(&built.tx.message, &plan).is_err() {
        return refuse("EXIT_MESSAGE_MISMATCH");
    }
    // P8-CTRL.4/.7: the exit is signed with the wallet's key only under its sign permit.
    let permit = match gates.map(|g| {
        crate::orchestration::capability_control::SignPermit::issue(
            g,
            crate::orchestration::capability_control::SignPurpose::Exit,
            &[payer],
            &built.tx.message,
        )
    }) {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Admission::Refused(e),
        None => return refuse("SIGN_NOT_PERMITTED: no authority"),
    };
    if permit.sign(&mut built.tx, &[c.payer]).is_err() {
        return refuse("EXIT_SIGN");
    }
    let Ok(fee) = adapter.fee_for_message(&built.tx, c.deadline) else {
        return refuse("EXIT_FEE_UNKNOWN");
    };
    let payload = match SignedPayload::new(vec![built.tx], adapter.path(), payer) {
        Ok(p) => p,
        Err(e) => return Admission::Refused(e),
    };
    let (signatures, wire) = (
        payload.signatures().to_vec(),
        payload.wire_sha256().to_string(),
    );
    // The exit simulates its own payload once (no pipeline simulation exists for it).
    let binding = super::transport::SimulationBinding {
        state_generation: 0,
        program_data_hash: String::new(),
        state_valid: c.state_valid,
    };
    let simulated = match payload.simulate(adapter, c.deadline, binding) {
        Ok(s) => s,
        Err(e) => return Admission::Refused(e),
    };
    let intent = ExitIntent {
        code_sha: approval.code_sha.clone(),
        binary_sha256: approval.binary_sha256.clone(),
        wallet: payer.to_string(),
        mint: approval.mint.clone(),
        route_program: r.swap_program.to_string(),
        amount_in: terms.in_amount,
        minimum_out_lamports: terms.minimum_out,
        slippage_bps: u64::from(terms.slippage_bps),
        transaction_fee_lamports: fee.lamports,
        signatures,
        signed_wire_sha256: wire,
        state_slot: c.slot,
        evaluated_unix_ms: c.now_unix_ms,
        last_valid_block_height: c.last_valid_block_height,
    };
    let current = Current {
        now_unix_ms: c.now_unix_ms,
        slot: c.slot,
        state_generation: 0,
        program_data_hash: String::new(),
        balances: c.balances,
        reserved_elsewhere: c.reserved_elsewhere,
        policy_reserve: c.policy_reserve,
        health_funding_route_fee_exit_known: true,
        canonical_fresh_no_gap_or_fork: c.state_valid,
        exact_signed_payload_simulated: simulated.exact_signed_payload_simulated(),
        bundle_simulated: false,
    };
    // DEC-12: the handoff is bounded by the deadline; past it nothing is reserved.
    if std::time::Instant::now() >= c.deadline {
        return Admission::Refused("FE04_HANDOFF_DEADLINE".into());
    }
    // P8-CTRL.7: the exit's egress permit, before anything is reserved.
    let permit = match gates.map(|g| {
        crate::orchestration::capability_control::EgressPermit::issue(
            g,
            crate::orchestration::capability_control::Purpose::Exit,
            simulated.signatures(),
            simulated.path(),
            c.deadline,
        )
    }) {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Admission::Refused(e),
        None => return Admission::Refused("EGRESS_NOT_PERMITTED: no egress authority".into()),
    };
    if let Err(e) = journal.reserve_exit_payload(approval, intent, &current, &simulated) {
        // A failed write may still have reached the disk: never treated as nothing.
        return if e.starts_with("JOURNAL") {
            Admission::Unknown(e)
        } else {
            Admission::Refused(e)
        };
    }
    match journal.submit_exit_payload(&simulated, &current, c.t_handoff_ns, c.deadline, permit) {
        Ok(ack) => Admission::Submitted(ack),
        Err(e) => Admission::Unknown(e),
    }
}
