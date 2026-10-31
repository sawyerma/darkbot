//! FE-04 exit admission: the one durable admission of an exit of the open inventory. The exit
//! route (`JupiterV2RouteProvider::exit_to_sol`) is re-validated against the approval's mint and
//! the payer (`exit_route_terms`), must sell exactly the open position and close an empty WSOL
//! account (anything already in it would be paid out as proceeds); a WSOL position's exit is its
//! unwrap (`route::unwrap_exit_route`: the close of a WSOL account holding exactly the position
//! and its re-creation, no swap); the transaction is built from
//! exactly those instructions, signed, quoted for its exact message fee and simulated through the
//! production adapter; the exit is reserved durably against the approval and the current wallet
//! and the adapter is called exactly once, after its UNKNOWN handoff is durable. Nothing here
//! enables LIVE or creates an approval.
use super::admission::Admission;
use super::exit::{ExitApproval, ExitIntent};
use super::transport::SignedPayload;
use super::{Current, Journal};
use crate::execution::builder::p0::Route;
use crate::execution::route::{exit_route_terms, unwrap_route_shape, SwapTerms, WSOL_MINT};
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
    /// The monotonic instant at which `now_unix_ms` was read (review M4: the handoff's time is
    /// that time advanced by the monotonic time since).
    pub evaluated: std::time::Instant,
    pub state_valid: bool,
    /// Reconciled wallet balances (`"SOL"` lamports, mint base units; the WSOL mint's must be 0).
    pub balances: BTreeMap<String, u64>,
    pub reserved_elsewhere: BTreeMap<String, u64>,
    pub policy_reserve: BTreeMap<String, u64>,
    pub deadline: std::time::Instant,
    /// S4-04: the clock `T_handoff` is read on, immediately before the adapter call.
    pub clock: &'a darkbot_common::timing::Clock,
}

/// P8-CTRL.5: a new exit within its grant's exit limit — the exits booked against the grant in
/// the wallet's journals (archived and active), against the scope's `max_exits_grant` (an
/// unreadable journal refuses).
pub(super) fn within_exit_limit(
    journal: &Journal,
    booking: &super::Fe06Attempt,
    scope: &crate::orchestration::fe06::Scope,
) -> Result<(), String> {
    let used = super::Fe06Quota::Grant(&booking.grant).used(
        journal
            .attempt_records()?
            .iter()
            .flat_map(|r| r.exits.iter())
            .filter_map(|e| e.fe06.as_ref())
            .collect::<Vec<_>>(),
    );
    if used >= scope.max_exits_grant {
        return Err(format!(
            "FE06_EXIT_LIMIT_GRANT: {used} of {} exits used",
            scope.max_exits_grant
        ));
    }
    Ok(())
}

/// P8-CTRL.3/.5: the exit as its grant's scope bounds it — the one construction of its sign and
/// egress permit: its position, mint, payer, fee (its whole SOL cost) and priority fee, the
/// wallet's free SOL, what the grant has booked, its slippage, route class and source.
#[allow(clippy::too_many_arguments)]
fn exit_use(
    journal: &Journal,
    approval: &ExitApproval,
    payer: &solana_sdk::pubkey::Pubkey,
    fee_lamports: u64,
    priority_fee_lamports: Option<u64>,
    slippage_bps: u64,
    c: &ExitCandidate<'_>,
    g: &crate::orchestration::capability_control::LiveGates<'_>,
) -> crate::orchestration::fe06::Use {
    use crate::orchestration::fe06;
    let grant = g.fe06.map(|a| a.digest());
    let booked_grant = journal.attempt_booking().map(|b| b.grant.as_str());
    fe06::Use::Exit(fe06::ExitUse {
        position: format!("{}:{}", approval.attempt_approval_id, approval.mint),
        mint: approval.mint.clone(),
        fee_payer: payer.to_string(),
        fee_lamports,
        priority_fee_lamports,
        sol_free_lamports: super::free_sol(&c.balances, &c.reserved_elsewhere, &c.policy_reserve),
        booked: grant.and_then(|g| {
            let records = journal.attempt_records().ok()?;
            super::fe06_grant_totals(&records, g)
        }),
        slippage_bps,
        // A router's route: the class the builder gives a plan with one; a WSOL position's
        // unwrap swaps nothing.
        route: fe06::route_class(if approval.mint == WSOL_MINT {
            darkbot_project0::planner::RouteKind::NoSwap
        } else {
            darkbot_project0::planner::RouteKind::FreePublicAllowed
        }),
        data_source: g.source.cloned(),
        booked_under_grant: booked_grant.is_some() && booked_grant == grant,
    })
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
    // A WSOL position is unwrapped (one to one, no slippage); every other mint is sold.
    let unwrap = approval.mint == WSOL_MINT;
    let terms = if unwrap {
        unwrap_route_shape(&r.instructions, &payer).map(|()| SwapTerms {
            in_amount: r.amount_in,
            quoted_out: r.amount_in,
            slippage_bps: 0,
            minimum_out: r.amount_in,
        })
    } else {
        exit_route_terms(&r.instructions, &payer, &mint)
    };
    let terms = match terms {
        Ok(t) => t,
        Err(e) => return Admission::Refused(e),
    };
    if terms.in_amount != r.amount_in
        || Some(terms.minimum_out) != r.minimum_executable_out
        || r.swap_program.to_string() != approval.route_program
    {
        return refuse("EXIT_ROUTE_NOT_BOUND");
    }
    // The close pays out every WSOL unit the account holds: a sale's account must be empty, an
    // unwrap's must hold exactly the position (nothing foreign is ever paid out).
    if unwrap {
        if c.balances.get(WSOL_MINT) != Some(&r.amount_in) {
            return refuse("EXIT_WSOL_ACCOUNT_NOT_THE_POSITION_OR_UNKNOWN");
        }
    } else if c.balances.get(WSOL_MINT) != Some(&0) {
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
    // P8-CTRL.3 (review H1): the exit's fee is quoted on its unsigned message, and every fact of
    // its use is bound by the sign permit before the wallet signs it.
    let Ok(fee) = adapter.fee_for_message(&built.tx, c.deadline) else {
        return refuse("EXIT_FEE_UNKNOWN");
    };
    let priority = tx::requested_priority_lamports(&built.tx);
    let slippage_bps = u64::from(terms.slippage_bps);
    let use_ = gates.map(|g| {
        exit_use(
            journal,
            &approval,
            &payer,
            fee.lamports,
            priority,
            slippage_bps,
            &c,
            g,
        )
    });
    // P8-CTRL.4/.7: the exit is signed with the wallet's key only under its sign permit.
    let permit = match gates.map(|g| {
        crate::orchestration::capability_control::SignPermit::issue(
            g,
            crate::orchestration::capability_control::SignPurpose::Exit,
            &[payer],
            &built.tx.message,
            use_.as_ref().map(|u| (u, adapter.path())),
        )
    }) {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Admission::Refused(e),
        None => return refuse("SIGN_NOT_PERMITTED: no authority"),
    };
    if permit.sign(&mut built.tx, &[c.payer]).is_err() {
        return refuse("EXIT_SIGN");
    }
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
        slippage_bps,
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
    // P8-CTRL.7: the exit's egress permit, before anything is reserved — P8-CTRL.3/.5: within
    // its grant's scope with the same use its signature was bound to.
    let permit = match gates.zip(use_.as_ref()).map(|(g, use_)| {
        crate::orchestration::capability_control::EgressPermit::issue(
            g,
            use_,
            simulated.signatures(),
            simulated.path(),
            c.deadline,
        )
    }) {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Admission::Refused(e),
        None => return Admission::Refused("EGRESS_NOT_PERMITTED: no egress authority".into()),
    };
    // P8-CTRL.5: the grant's exit limit — every exit booked against it, UNKNOWN or sent,
    // counts; the booking is written with the reservation, before any dispatch.
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
            if let Err(e) = within_exit_limit(journal, &booking, scope) {
                return Admission::Refused(e);
            }
            Some(booking)
        }
        None => None,
    };
    if let Err(e) = journal.reserve_exit_payload(approval, intent, &current, &simulated, fe06) {
        // A failed write may still have reached the disk: never treated as nothing.
        return if e.starts_with("JOURNAL") {
            Admission::Unknown(e)
        } else {
            Admission::Refused(e)
        };
    }
    // Review M4: the handoff is checked at its own time — the evaluation's time advanced by the
    // monotonic time since (one clock domain), as an attempt's is.
    let elapsed_ms = u64::try_from(c.evaluated.elapsed().as_millis()).unwrap_or(u64::MAX);
    let at_handoff = Current {
        now_unix_ms: c.now_unix_ms.saturating_add(elapsed_ms),
        ..current
    };
    match journal.submit_exit_payload(&simulated, &at_handoff, c.clock, c.deadline, permit) {
        Ok((ack, handoff)) => Admission::Submitted(ack, handoff),
        Err(e) => Admission::Unknown(e),
    }
}
