//! FE-04/P8-INV exit: the controlled realization of a settled attempt's open inventory, in the same
//! wallet journal (the one financial record). An exit is its own explicitly approved one-shot
//! action bound to the settled attempt: it sells the whole open position of one mint ExactIn for
//! native SOL with a floor enforced by the signed swap, is durably reserved before its single
//! handoff and UNKNOWN until finalized receipts or a strict non-landing proof settle it.
//!
//! Only a settled successful exit realizes: its SOL proceeds become realized net and the
//! position's mark leaves the open inventory. A failed exit realizes its fee as a loss and the
//! position stays open; a proven non-landing pays nothing. Any other outcome — a partial or
//! mismatched sale, foreign wallet activity, proceeds below the floor, an incomplete observation —
//! is refused at finalization: the exit stays UNKNOWN with every hold.
use super::receipts::FinalizedObservation;
use super::settlement::{NonLandingProof, SettlementEvidence};
use super::{
    cause, err, hex, key, Current, Journal, Phase, Receipt, Saved, Transition, Version,
    EXIT_LIFECYCLE_VERSIONS, MAX_JOURNAL_BYTES,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The native mint (wrapped SOL); an exit's proceeds are native SOL, never WSOL.
pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// The explicit one-shot approval of one exit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExitApproval {
    pub schema: u32,
    pub authorized: bool,
    pub approval_id: String,
    /// The settled attempt (its approval id) whose open inventory this exit realizes.
    pub attempt_approval_id: String,
    pub code_sha: String,
    pub binary_sha256: String,
    pub wallet: String,
    pub mint: String,
    /// The swap program the signed exit routes through.
    pub route_program: String,
    pub expires_unix_s: u64,
    pub max_state_age_ms: u64,
    /// The least native SOL (lamports, before fees) the whole position may be sold for.
    pub min_out_lamports: u64,
    pub max_slippage_bps: u64,
    pub max_fee_lamports: u64,
}

/// What the exit binds: the exact signed swap of the whole open position.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExitIntent {
    pub code_sha: String,
    pub binary_sha256: String,
    pub wallet: String,
    pub mint: String,
    pub route_program: String,
    /// The whole open position of `mint` (base units), sold ExactIn.
    pub amount_in: u64,
    /// The floor of native SOL (lamports, before fees) the signed swap enforces on chain.
    pub minimum_out_lamports: u64,
    pub slippage_bps: u64,
    pub transaction_fee_lamports: u64,
    pub signatures: Vec<String>,
    pub signed_wire_sha256: String,
    pub state_slot: u64,
    pub evaluated_unix_ms: u64,
    /// The signed blockhash's `lastValidBlockHeight` (the non-landing proof's domain).
    pub last_valid_block_height: u64,
}

impl ExitIntent {
    /// Held while the exit is open: the position's units and the SOL fee.
    pub fn holds(&self) -> BTreeMap<String, u64> {
        [
            (self.mint.clone(), self.amount_in),
            ("SOL".to_string(), self.transaction_fee_lamports),
        ]
        .into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExitPhase {
    Reserved,
    HandoffUnknown,
    /// Receipts (or the non-landing) are final and consistent; the booking may still wait for
    /// a SOL value.
    Finalized,
    Settled(ExitSettlement),
}

/// The booked outcome of one exit (USD micro where named so).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExitSettlement {
    pub landed: bool,
    pub landed_success: bool,
    pub fee_lamports: u64,
    pub wallet_deltas: BTreeMap<String, i128>,
    /// Units of the mint sold (the whole position on success, else 0).
    pub sold_units: u64,
    /// The conservative value of the SOL delta (proceeds net of fees; a fee alone is a loss).
    pub realized_usd_micro: i128,
    /// The mark of the sold position, leaving the open inventory.
    pub released_mark_usd_micro: i128,
    /// The second whose UTC day the exit is booked into.
    pub booked_unix_s: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExitAttempt {
    pub(super) approval: ExitApproval,
    pub(super) intent: ExitIntent,
    pub(super) phase: ExitPhase,
    pub(super) observations: BTreeMap<String, FinalizedObservation>,
    pub(super) finalized: Option<Vec<Receipt>>,
}

/// The outcome of one exit settlement step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExitStep {
    Settled(ExitSettlement),
    /// Still UNKNOWN: every hold stays; the reasons of this step.
    Unknown(Vec<String>),
}

/// One open position: its units and its conservative mark (USD micro).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub units: i128,
    /// F-07: `None` when the settlement holds no valuation of the mint (Unknown, never 0).
    pub mark_usd_micro: Option<i128>,
}

/// The exit not yet settled (at most one: single-flight).
pub(super) fn active(s: &Saved) -> Option<&ExitAttempt> {
    s.exits
        .last()
        .filter(|e| !matches!(e.phase, ExitPhase::Settled(_)))
}

/// The open inventory of a settled attempt after its settled exits.
pub(super) fn open_positions(s: &Saved) -> BTreeMap<String, Position> {
    let Phase::Settled(done) = &s.phase else {
        return BTreeMap::new();
    };
    let mut open: BTreeMap<String, Position> = done
        .open_inventory
        .iter()
        .map(|(mint, units)| {
            let mark = done.valuation.get(mint).copied();
            (
                mint.clone(),
                Position {
                    units: *units,
                    mark_usd_micro: mark,
                },
            )
        })
        .collect();
    for e in &s.exits {
        if let ExitPhase::Settled(x) = &e.phase {
            if x.landed_success {
                open.remove(&e.intent.mint);
            }
        }
    }
    open
}

/// Replay legality of an exit transition: the attempt is settled and unchanged; earlier exits are
/// unchanged; the last exit advances exactly as the automaton does.
pub(super) fn legal(p: &Saved, t: Transition, next: &Saved) -> bool {
    if p.approval != next.approval
        || p.intent != next.intent
        || p.phase != next.phase
        || !matches!(p.phase, Phase::Settled(_))
        || p.finalized != next.finalized
        || p.observations != next.observations
    {
        return false;
    }
    let (before, after) = (&p.exits, &next.exits);
    if t == Transition::ExitReserve {
        return after.len() == before.len() + 1
            && after[..before.len()] == before[..]
            && active(p).is_none()
            && after.last().is_some_and(|e| {
                e.phase == ExitPhase::Reserved && e.observations.is_empty() && e.finalized.is_none()
            });
    }
    let (Some(old), Some(new)) = (before.last(), after.last()) else {
        return false;
    };
    if after.len() != before.len()
        || after[..after.len() - 1] != before[..before.len() - 1]
        || old.approval != new.approval
        || old.intent != new.intent
    {
        return false;
    }
    match t {
        Transition::ExitHandoff | Transition::ExitReopenUnknown => {
            old.phase == ExitPhase::Reserved
                && new.phase == ExitPhase::HandoffUnknown
                && new.observations == old.observations
                && new.finalized.is_none()
        }
        Transition::ExitObserve => {
            old.phase == ExitPhase::HandoffUnknown
                && new.phase == ExitPhase::HandoffUnknown
                && new.finalized.is_none()
        }
        Transition::ExitFinalize => {
            old.phase == ExitPhase::HandoffUnknown
                && new.phase == ExitPhase::Finalized
                && new.finalized.as_ref().is_some_and(|r| !r.is_empty())
                && new.observations == old.observations
        }
        Transition::ExitNotLanded => {
            old.phase == ExitPhase::HandoffUnknown
                && new.phase == ExitPhase::Finalized
                && new.finalized.as_ref().is_some_and(Vec::is_empty)
                && old.observations.is_empty()
                && new.observations.is_empty()
        }
        Transition::ExitSettle => {
            old.phase == ExitPhase::Finalized
                && matches!(new.phase, ExitPhase::Settled(_))
                && new.finalized == old.finalized
                && new.observations == old.observations
        }
        _ => false,
    }
}

impl ExitApproval {
    /// FE-04: the exit approval alone, before any attempt or intent — schema 1, authorized, its
    /// ids, keys and bounds well formed, and not expired at `now_unix_s`.
    pub fn check(&self, now_unix_s: u64) -> Result<(), String> {
        if !well_formed(self) {
            return Err(err("INVALID_OR_UNAUTHORIZED_EXIT_APPROVAL"));
        }
        if now_unix_s >= self.expires_unix_s {
            return Err(err("APPROVAL_EXPIRED"));
        }
        Ok(())
    }
}

fn well_formed(a: &ExitApproval) -> bool {
    !(a.schema != 1
        || !a.authorized
        || a.approval_id.is_empty()
        || a.attempt_approval_id.is_empty()
        || !key(&a.wallet)
        || !hex(&a.code_sha, 40)
        || !hex(&a.binary_sha256, 64)
        || !key(&a.mint)
        || !key(&a.route_program)
        || a.max_state_age_ms == 0
        || a.max_slippage_bps > 10_000
        || a.min_out_lamports == 0)
}

/// The exit's approval, binding, position, freshness, evidence, budget and wallet checks.
fn validate(s: &Saved, a: &ExitApproval, i: &ExitIntent, c: &Current) -> Result<(), String> {
    if !well_formed(a)
        || a.attempt_approval_id != s.approval.approval_id
        || a.wallet != s.approval.wallet
    {
        return Err(err("INVALID_OR_UNAUTHORIZED_EXIT_APPROVAL"));
    }
    if (
        i.code_sha.as_str(),
        i.binary_sha256.as_str(),
        i.wallet.as_str(),
        i.mint.as_str(),
        i.route_program.as_str(),
    ) != (
        a.code_sha.as_str(),
        a.binary_sha256.as_str(),
        a.wallet.as_str(),
        a.mint.as_str(),
        a.route_program.as_str(),
    ) || !hex(&i.signed_wire_sha256, 64)
    {
        return Err(err("EXIT_BINDING_MISMATCH"));
    }
    let unique: BTreeSet<_> = i.signatures.iter().collect();
    if unique.is_empty()
        || unique.len() > 5
        || unique.len() != i.signatures.len()
        || i.signatures
            .iter()
            .any(|s| s.parse::<solana_sdk::signature::Signature>().is_err())
    {
        return Err(err("SIGNATURE_BINDING_INVALID"));
    }
    if open_positions(s)
        .get(&i.mint)
        .is_none_or(|p| p.units != i128::from(i.amount_in))
    {
        return Err(err("EXIT_AMOUNT_NOT_THE_OPEN_POSITION"));
    }
    // Review 9 (F-07): an unvalued position is refused before any handoff; its settlement could
    // never release a mark (the hold would stay after the trade).
    if open_positions(s)
        .get(&i.mint)
        .is_some_and(|p| p.mark_usd_micro.is_none())
    {
        return Err(err("EXIT_POSITION_MARK_UNKNOWN"));
    }
    if c.now_unix_ms / 1000 >= a.expires_unix_s
        || c.now_unix_ms < i.evaluated_unix_ms
        || c.now_unix_ms - i.evaluated_unix_ms > a.max_state_age_ms
        || c.slot < i.state_slot
        || !c.canonical_fresh_no_gap_or_fork
    {
        return Err(err("STALE_FORK_GAP_OR_UPGRADE"));
    }
    if !c.exact_signed_payload_simulated || (i.signatures.len() > 1 && !c.bundle_simulated) {
        return Err(err("CURRENT_EXECUTION_EVIDENCE_UNKNOWN"));
    }
    if i.minimum_out_lamports < a.min_out_lamports
        || i.slippage_bps > a.max_slippage_bps
        || i.transaction_fee_lamports > a.max_fee_lamports
    {
        return Err(err("EXIT_APPROVAL_BUDGET_EXCEEDED"));
    }
    if i.holds().iter().any(|(asset, n)| {
        c.balances.get(asset).is_none_or(|balance| {
            let elsewhere = c.reserved_elsewhere.get(asset).copied().unwrap_or(0);
            let reserve = c.policy_reserve.get(asset).copied().unwrap_or(0);
            balance.saturating_sub(elsewhere.saturating_add(reserve)) < *n
        })
    }) {
        return Err(err("WALLET_UNDERFUNDED_OR_HOLD_MISSING"));
    }
    Ok(())
}

/// Finalized receipts consistent with the exit: no foreign delta; a success sold exactly the whole
/// position for at least the floor; a failure sold nothing.
fn consistent(
    i: &ExitIntent,
    receipts: &[Receipt],
) -> Result<(BTreeMap<String, i128>, u64), String> {
    let overflow = || err("EXIT_OVERFLOW");
    let mut deltas: BTreeMap<String, i128> = BTreeMap::new();
    let mut fee = 0u64;
    for r in receipts {
        fee = fee
            .checked_add(r.fee_lamports.ok_or_else(|| err("EXIT_FEE_UNKNOWN"))?)
            .ok_or_else(overflow)?;
        for (asset, d) in r
            .wallet_deltas
            .as_ref()
            .ok_or_else(|| err("EXIT_DELTAS_UNKNOWN"))?
        {
            let total = deltas.entry(asset.clone()).or_default();
            *total = total.checked_add(*d).ok_or_else(overflow)?;
        }
    }
    if deltas
        .iter()
        .any(|(asset, d)| *d != 0 && asset != "SOL" && *asset != i.mint)
    {
        return Err(err("EXIT_FOREIGN_DELTA_HOLD_RETAINED"));
    }
    let sold = deltas.get(&i.mint).copied().unwrap_or(0);
    let sol = deltas.get("SOL").copied().unwrap_or(0);
    let success = !receipts.is_empty() && receipts.iter().all(|r| r.success);
    if success {
        if sold != -i128::from(i.amount_in) {
            return Err(err("EXIT_PARTIAL_OR_MISMATCHED_SALE_HOLD_RETAINED"));
        }
        if sol.checked_add(i128::from(fee)).ok_or_else(overflow)?
            < i128::from(i.minimum_out_lamports)
        {
            return Err(err("EXIT_BELOW_FLOOR_HOLD_RETAINED"));
        }
    } else if sold != 0 {
        return Err(err("EXIT_FAILED_BUT_SOLD_HOLD_RETAINED"));
    }
    Ok((deltas, fee))
}

impl Journal {
    /// The settled attempt's open inventory after its settled exits.
    pub fn open_inventory(&self) -> BTreeMap<String, Position> {
        self.saved.as_ref().map(open_positions).unwrap_or_default()
    }

    /// The open exit's signatures and its proof bounds (`last_valid_block_height`, state slot)
    /// with the already observed signatures, for the settlement worker; `None` otherwise.
    pub fn exit_job(&self) -> Option<(ExitIntent, BTreeSet<String>)> {
        self.saved
            .as_ref()
            .and_then(active)
            .filter(|e| e.phase == ExitPhase::HandoffUnknown)
            .map(|e| (e.intent.clone(), e.observations.keys().cloned().collect()))
    }

    /// The reserved (not yet handed off) exit: its intent and approval.
    pub(super) fn exit_reserved(&self) -> Option<(ExitIntent, ExitApproval)> {
        self.saved
            .as_ref()
            .and_then(active)
            .filter(|e| e.phase == ExitPhase::Reserved)
            .map(|e| (e.intent.clone(), e.approval.clone()))
    }

    /// The open exit's approval id (reserved, UNKNOWN or finalized but not booked).
    pub fn open_exit_id(&self) -> Option<String> {
        self.saved
            .as_ref()
            .and_then(active)
            .map(|e| e.approval.approval_id.clone())
    }

    /// The open exit is finalized but not yet booked (its SOL value was unknown): settled from
    /// the journal alone.
    pub fn exit_awaits_booking(&self) -> bool {
        self.saved
            .as_ref()
            .and_then(active)
            .is_some_and(|e| e.phase == ExitPhase::Finalized)
    }

    /// Something to settle: the attempt UNKNOWN or unbooked, or its open exit UNKNOWN or unbooked.
    pub fn needs_settlement(&self) -> bool {
        self.settlement_job().is_some()
            || self.awaits_booking()
            || self.exit_job().is_some()
            || self.exit_awaits_booking()
    }

    /// Reserves one exit durably: validated against the approval, the open position and the
    /// current wallet; one approval is consumed once; at most one exit is open.
    pub fn reserve_exit(
        &mut self,
        approval: ExitApproval,
        intent: ExitIntent,
        now: &Current,
    ) -> Result<(), String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        if !matches!(s.phase, Phase::Settled(_)) {
            return Err(err("EXIT_REQUIRES_SETTLED_ATTEMPT"));
        }
        if active(&s).is_some() {
            return Err(err("EXIT_ALREADY_OPEN"));
        }
        if s.exits
            .iter()
            .any(|e| e.approval.approval_id == approval.approval_id)
        {
            return Err(err("EXIT_APPROVAL_ALREADY_CONSUMED"));
        }
        validate(&s, &approval, &intent, now)?;
        s.exits.push(ExitAttempt {
            approval,
            intent,
            phase: ExitPhase::Reserved,
            observations: BTreeMap::new(),
            finalized: None,
        });
        // A23-07: a new exit only while its whole lifecycle keeps the wallet journal reopenable
        // (the open check refuses a larger file); beyond, it is refused and the position stays
        // held — nothing truncated, nothing released.
        let version = crate::analytics::payload::encoded_len(&Version {
            transition: Transition::ExitReserve,
            state: s.clone(),
        })
        .map_err(|e| cause("JOURNAL_WRITE", &e))?;
        let lifecycle = (crate::analytics::journal::FRAME_WIRE as u64)
            .saturating_add(version)
            .saturating_mul(EXIT_LIFECYCLE_VERSIONS);
        if self.writer.position().saturating_add(lifecycle) > MAX_JOURNAL_BYTES {
            return Err(err("JOURNAL_RUNWAY_EXHAUSTED"));
        }
        self.persist(s, Transition::ExitReserve)
    }

    /// The exit's single handoff: durable UNKNOWN before `send` (the adapter's one submit over
    /// exactly the bound bytes); no retry.
    pub fn handoff_exit<T>(
        &mut self,
        wire_sha256: &str,
        now: &Current,
        send: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        let Some(e) = s.exits.last() else {
            return Err(err("EXIT_NOT_RESERVED"));
        };
        if e.phase != ExitPhase::Reserved {
            return Err(err("EXIT_HALTED"));
        }
        if e.intent.signed_wire_sha256 != wire_sha256 {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        let mut probe = s.clone();
        probe.exits.pop();
        validate(&probe, &e.approval, &e.intent, now)?;
        if let Some(e) = s.exits.last_mut() {
            e.phase = ExitPhase::HandoffUnknown;
        }
        self.persist(s, Transition::ExitHandoff)?;
        send()
    }

    /// Reopening an exit reserved before a crash makes it UNKNOWN; it is never handed off again.
    pub(super) fn reopen_exit(&mut self) -> Result<(), String> {
        let Some(mut s) = self.saved.clone() else {
            return Ok(());
        };
        match s.exits.last_mut() {
            Some(e) if e.phase == ExitPhase::Reserved => e.phase = ExitPhase::HandoffUnknown,
            _ => return Ok(()),
        }
        self.persist(s, Transition::ExitReopenUnknown)
    }

    /// Finalizes the open exit from durable observations of every bound signature, without
    /// unknown reasons and consistent with the exit; anything less keeps it UNKNOWN.
    pub fn reconcile_exit_observed(&mut self) -> Result<(), String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        let e = s
            .exits
            .last_mut()
            .filter(|e| e.phase == ExitPhase::HandoffUnknown)
            .ok_or_else(|| err("EXIT_NOT_UNKNOWN"))?;
        let mut receipts = Vec::with_capacity(e.intent.signatures.len());
        for signature in &e.intent.signatures {
            let o = e
                .observations
                .get(signature)
                .ok_or_else(|| err("EXIT_SIGNATURE_UNOBSERVED_HOLD_RETAINED"))?;
            if !o.unknown_reasons.is_empty() {
                return Err(err("EXIT_OBSERVATION_UNKNOWN_HOLD_RETAINED"));
            }
            let mut deltas = o
                .wallet_token_deltas
                .clone()
                .ok_or_else(|| err("EXIT_TOKEN_DELTAS_UNKNOWN_HOLD_RETAINED"))?;
            deltas.insert("SOL".into(), o.wallet_sol_delta_lamports);
            receipts.push(Receipt {
                signature: signature.clone(),
                finalized: true,
                success: o.success,
                fee_lamports: Some(o.fee_lamports),
                wallet_deltas: Some(deltas),
                exit_complete: true,
            });
        }
        consistent(&e.intent, &receipts)?;
        e.phase = ExitPhase::Finalized;
        e.finalized = Some(receipts);
        self.persist(s, Transition::ExitFinalize)
    }

    /// Finalizes a proven non-landing of the open exit: nothing landed, nothing was paid.
    pub fn exit_not_landed(&mut self, proof: &NonLandingProof) -> Result<(), String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        let e = s
            .exits
            .last_mut()
            .filter(|e| e.phase == ExitPhase::HandoffUnknown)
            .ok_or_else(|| err("EXIT_NOT_UNKNOWN"))?;
        let bound: BTreeSet<String> = e.intent.signatures.iter().cloned().collect();
        if proof.finalized_block_height <= e.intent.last_valid_block_height
            || proof.history_from_slot > e.intent.state_slot
            || proof.absent_signatures != bound
            || !e.observations.is_empty()
        {
            return Err(err("NON_LANDING_NOT_PROVEN_HOLD_RETAINED"));
        }
        e.phase = ExitPhase::Finalized;
        e.finalized = Some(Vec::new());
        self.persist(s, Transition::ExitNotLanded)
    }

    /// Books the finalized exit exactly once at `booked_unix_s`: realized the conservative value
    /// of its SOL delta (`value`), released the sold position's mark. A booked exit is returned
    /// unchanged.
    pub fn settle_exit(
        &mut self,
        value: &dyn Fn(&str, i128) -> Option<i128>,
        booked_unix_s: u64,
    ) -> Result<ExitSettlement, String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        let positions = {
            let mut before = s.clone();
            before.exits.pop();
            open_positions(&before)
        };
        let e = s.exits.last_mut().ok_or_else(|| err("NO_EXIT"))?;
        let receipts = match &e.phase {
            ExitPhase::Settled(done) => return Ok(done.clone()),
            ExitPhase::Finalized => e.finalized.clone().ok_or_else(|| err("NOT_FINALIZED"))?,
            _ => return Err(err("EXIT_NOT_FINALIZED_HOLD_RETAINED")),
        };
        let (deltas, fee) = consistent(&e.intent, &receipts)?;
        let landed = !receipts.is_empty();
        let success = landed && receipts.iter().all(|r| r.success);
        let sol = deltas.get("SOL").copied().unwrap_or(0);
        let realized = if sol == 0 {
            0
        } else {
            let v = value("SOL", sol).ok_or_else(|| err("SETTLEMENT_VALUE_UNKNOWN:SOL"))?;
            if v.signum() * sol.signum() < 0 {
                return Err(err("SETTLEMENT_VALUE_SIGN:SOL"));
            }
            v
        };
        let mark = positions
            .get(&e.intent.mint)
            .map(|p| p.mark_usd_micro)
            .ok_or_else(|| err("EXIT_POSITION_GONE"))?;
        let done = ExitSettlement {
            landed,
            landed_success: success,
            fee_lamports: fee,
            wallet_deltas: deltas,
            sold_units: if success { e.intent.amount_in } else { 0 },
            realized_usd_micro: realized,
            // F-07: a sold position's mark leaves the inventory; an unvalued one is not booked.
            released_mark_usd_micro: if success {
                mark.ok_or_else(|| err("EXIT_POSITION_MARK_UNKNOWN"))?
            } else {
                0
            },
            booked_unix_s,
        };
        e.phase = ExitPhase::Settled(done.clone());
        self.persist(s, Transition::ExitSettle)?;
        Ok(done)
    }

    /// FE-04 exit settlement step from the worker's evidence (as `apply_settlement_evidence` for
    /// the attempt): observations are durable when changed; all observed and consistent ⇒ finalize
    /// and book; nothing observed and a strict proof ⇒ finalized not landed and booked; anything
    /// else stays UNKNOWN with its reasons. Idempotent across steps and restarts.
    pub fn apply_exit_evidence(
        &mut self,
        evidence: SettlementEvidence,
        value: &dyn Fn(&str, i128) -> Option<i128>,
        now_unix_s: u64,
    ) -> Result<ExitStep, String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        let Some(e) = s.exits.last_mut() else {
            return Err(err("NO_EXIT"));
        };
        match &e.phase {
            ExitPhase::Settled(done) => return Ok(ExitStep::Settled(done.clone())),
            ExitPhase::Finalized => {
                return self.settle_exit(value, now_unix_s).map(ExitStep::Settled)
            }
            ExitPhase::Reserved => return Ok(ExitStep::Unknown(vec![err("NOT_HANDED_OFF")])),
            ExitPhase::HandoffUnknown => {}
        }
        let before = e.observations.clone();
        let mut reasons = Vec::new();
        for (signature, read) in evidence.observations {
            if !e.intent.signatures.contains(&signature) {
                reasons.push(format!("{signature}:SETTLEMENT_SIGNATURE_FOREIGN"));
                continue;
            }
            match read {
                Ok(o) => {
                    e.observations.insert(signature, o);
                }
                Err(reason) => reasons.push(format!("{signature}:{reason}")),
            }
        }
        for (signature, o) in &e.observations {
            reasons.extend(o.unknown_reasons.iter().map(|r| format!("{signature}:{r}")));
        }
        let complete = e.observations.len() == e.intent.signatures.len();
        let none = e.observations.is_empty();
        if e.observations != before {
            self.persist(s, Transition::ExitObserve)?;
        }
        if complete && reasons.is_empty() {
            return match self.reconcile_exit_observed() {
                Ok(()) => self.settle_exit(value, now_unix_s).map(ExitStep::Settled),
                Err(reason) => Ok(ExitStep::Unknown(vec![reason])),
            };
        }
        if none {
            match evidence.proof {
                Some(Ok(proof)) => {
                    self.exit_not_landed(&proof)?;
                    return self.settle_exit(value, now_unix_s).map(ExitStep::Settled);
                }
                Some(Err(reason)) => reasons.push(reason),
                None => reasons.push(err("NON_LANDING_NOT_ATTEMPTED")),
            }
        }
        Ok(ExitStep::Unknown(reasons))
    }
}

/// The hold of the open exit (reserved or UNKNOWN), for the reservation book.
pub(super) fn exit_hold(s: &Saved) -> Option<super::ActiveHold> {
    active(s)
        .filter(|e| matches!(e.phase, ExitPhase::Reserved | ExitPhase::HandoffUnknown))
        .map(|e| super::ActiveHold {
            attempt: e.approval.approval_id.clone(),
            target: e.intent.mint.clone(),
            unknown_outcome: e.phase == ExitPhase::HandoffUnknown,
            holds: e.intent.holds(),
            signature: e.intent.signatures.first().cloned(),
            last_valid_block_height: Some(e.intent.last_valid_block_height),
        })
}

/// What the safety layer books for every settled exit, in its own booking day: realized net,
/// the outcome and the released mark (a negative inventory delta).
pub(super) fn exit_bookings(s: &Saved) -> Vec<darkbot_project0::safety::SettledAttempt> {
    s.exits
        .iter()
        .filter_map(|e| match &e.phase {
            ExitPhase::Settled(x) => Some(darkbot_project0::safety::SettledAttempt {
                unix_s: x.booked_unix_s,
                realized_net_usd_micro: x.realized_usd_micro,
                landed_success: x.landed_success,
                open_inventory_usd_micro: -x.released_mark_usd_micro,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::tests::{path_of, setup};
    use super::super::*;
    use super::*;
    use std::path::PathBuf;

    /// USD micro: 1 lamport = 0.1, 1 token base unit = 1.
    fn value(asset: &str, d: i128) -> Option<i128> {
        Some(if asset == "SOL" { d / 10 } else { d })
    }

    fn observation(
        sig: &str,
        success: bool,
        sol: i128,
        tokens: &[(&str, i128)],
    ) -> FinalizedObservation {
        FinalizedObservation {
            signature: sig.into(),
            slot: 85,
            success,
            fee_lamports: 5_000,
            wallet_sol_delta_lamports: sol,
            wallet_token_deltas: Some(tokens.iter().map(|(m, d)| (m.to_string(), *d)).collect()),
            unknown_reasons: vec![],
        }
    }

    /// A settled attempt that left `units` of a fresh mint in the wallet (mark = units).
    fn settled(units: i128) -> (PathBuf, Journal, String, Approval) {
        let (path, a, i, c) = setup();
        let mint = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
        let mut s = j.saved.clone().unwrap();
        let sig = &i.signatures[0];
        s.observations.insert(
            sig.clone(),
            observation(sig, true, -5_000, &[(&mint, units)]),
        );
        j.persist(s, Transition::Observe).unwrap();
        j.reconcile_observed().unwrap();
        j.settle(&value, 1_000).unwrap();
        (path, j, mint, a)
    }

    /// An exit of `amount` of `mint` for the attempt approved by `a`, and a current wallet that
    /// holds exactly it.
    fn exit_for(
        a: &Approval,
        mint: &str,
        amount: u64,
        id: &str,
    ) -> (ExitApproval, ExitIntent, Current) {
        let route = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let approval = ExitApproval {
            schema: 1,
            authorized: true,
            approval_id: id.into(),
            attempt_approval_id: a.approval_id.clone(),
            code_sha: a.code_sha.clone(),
            binary_sha256: a.binary_sha256.clone(),
            wallet: a.wallet.clone(),
            mint: mint.into(),
            route_program: route.clone(),
            expires_unix_s: 100,
            max_state_age_ms: 500,
            min_out_lamports: 9_000,
            max_slippage_bps: 100,
            max_fee_lamports: 10_000,
        };
        let intent = ExitIntent {
            code_sha: a.code_sha.clone(),
            binary_sha256: a.binary_sha256.clone(),
            wallet: a.wallet.clone(),
            mint: mint.into(),
            route_program: route,
            amount_in: amount,
            minimum_out_lamports: 9_500,
            slippage_bps: 50,
            transaction_fee_lamports: 5_000,
            signatures: vec![solana_sdk::signature::Signature::new_unique().to_string()],
            signed_wire_sha256: "e".repeat(64),
            state_slot: 80,
            evaluated_unix_ms: 2_000,
            last_valid_block_height: 90,
        };
        let current = Current {
            now_unix_ms: 2_001,
            slot: 81,
            state_generation: 1,
            program_data_hash: "deployed-pinned".into(),
            balances: [("SOL".to_string(), 100_000), (mint.to_string(), amount)].into(),
            reserved_elsewhere: BTreeMap::new(),
            policy_reserve: BTreeMap::new(),
            health_funding_route_fee_exit_known: true,
            canonical_fresh_no_gap_or_fork: true,
            exact_signed_payload_simulated: true,
            bundle_simulated: false,
        };
        (approval, intent, current)
    }

    /// Test seam: `wallet`'s journal in `dir` with a settled attempt (fee −500 realized, 1000
    /// units open, mark 1000) and its settled successful exit (+500 realized, mark released),
    /// both booked at `booked`; closed again.
    #[cfg(feature = "crate_health")]
    pub(crate) fn exited(dir: &std::path::Path, wallet: &str, booked: u64) {
        let (mut j, ei, mint) = open_exit(dir, wallet, booked);
        let sig = &ei.signatures[0];
        let sold = evidence(sig, observation(sig, true, 5_000, &[(&mint, -1_000)]));
        assert!(matches!(
            j.apply_exit_evidence(sold, &value, booked).unwrap(),
            ExitStep::Settled(_)
        ));
    }

    /// Test seam: `wallet`'s journal in `dir` with a settled attempt (fee −500 realized, 1000
    /// units of a fresh mint open, mark 1000, booked at `booked`) and its exit `exit-1` handed off
    /// (UNKNOWN); closed again. The exit's intent.
    #[cfg(feature = "crate_health")]
    pub(crate) fn handed_off_exit_journal(
        dir: &std::path::Path,
        wallet: &str,
        booked: u64,
    ) -> ExitIntent {
        open_exit(dir, wallet, booked).1
    }

    /// Test seam: `wallet`'s journal in `dir` with a settled attempt that left 1000 units of a
    /// fresh mint open (mark 1000); closed again. The mint.
    #[cfg(feature = "crate_health")]
    pub(crate) fn settled_with_position(
        dir: &std::path::Path,
        wallet: &str,
        booked: u64,
    ) -> String {
        settled_in(dir, wallet, booked).2
    }

    /// Test seam: `wallet`'s journal in `dir` with a settled attempt that left nothing (only its
    /// fee: −500 realized, booked at `booked`), archived; returns the archived path.
    #[cfg(feature = "crate_health")]
    pub(crate) fn archived_fee_only_attempt(
        dir: &std::path::Path,
        wallet: &str,
        booked: u64,
    ) -> std::path::PathBuf {
        let (_, mut a, mut i, c) = setup();
        a.wallet = wallet.into();
        i.wallet = wallet.into();
        let mut j = Journal::open(dir, wallet).unwrap();
        j.reserve(a, i.clone(), &c).unwrap();
        j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
        let mut s = j.saved.clone().unwrap();
        let sig = &i.signatures[0];
        let assets: Vec<(String, i128)> = i
            .expected_wallet_assets
            .iter()
            .filter(|k| k.as_str() != "SOL")
            .map(|k| (k.clone(), 0))
            .collect();
        let refs: Vec<(&str, i128)> = assets.iter().map(|(k, d)| (k.as_str(), *d)).collect();
        s.observations
            .insert(sig.clone(), observation(sig, true, -5_000, &refs));
        j.persist(s, Transition::Observe).unwrap();
        j.reconcile_observed().unwrap();
        j.settle(&value, booked).unwrap();
        j.archive().unwrap()
    }

    /// Test seam: an exit approval of that attempt (`exit_for`'s limits: floor 9 000 lamports,
    /// fee 10 000, slippage 100 bps; no expiry within the tests) for `mint` via `route_program`.
    #[cfg(feature = "crate_health")]
    pub(crate) fn exit_approval(wallet: &str, mint: &str, route_program: &str) -> ExitApproval {
        let (_, mut a, _, _) = setup();
        a.wallet = wallet.into();
        let (mut approval, _, _) = exit_for(&a, mint, 1_000, "exit-1");
        approval.route_program = route_program.into();
        approval.expires_unix_s = u64::MAX / 1_000;
        approval
    }

    #[cfg(feature = "crate_health")]
    fn settled_in(dir: &std::path::Path, wallet: &str, booked: u64) -> (Journal, Approval, String) {
        let (_, mut a, mut i, c) = setup();
        a.wallet = wallet.into();
        i.wallet = wallet.into();
        let mint = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let mut j = Journal::open(dir, wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
        let mut s = j.saved.clone().unwrap();
        let sig = &i.signatures[0];
        s.observations.insert(
            sig.clone(),
            observation(sig, true, -5_000, &[(&mint, 1_000)]),
        );
        j.persist(s, Transition::Observe).unwrap();
        j.reconcile_observed().unwrap();
        j.settle(&value, booked).unwrap();
        (j, a, mint)
    }

    #[cfg(feature = "crate_health")]
    fn open_exit(
        dir: &std::path::Path,
        wallet: &str,
        booked: u64,
    ) -> (Journal, ExitIntent, String) {
        let (mut j, a, mint) = settled_in(dir, wallet, booked);
        let ei = handed_off_exit(&mut j, &a, &mint, "exit-1");
        (j, ei, mint)
    }

    fn handed_off_exit(j: &mut Journal, a: &Approval, mint: &str, id: &str) -> ExitIntent {
        let (ea, ei, c) = exit_for(a, mint, 1_000, id);
        j.reserve_exit(ea, ei.clone(), &c).unwrap();
        j.handoff_exit(&ei.signed_wire_sha256, &c, || Ok(()))
            .unwrap();
        ei
    }

    fn evidence(sig: &str, o: FinalizedObservation) -> SettlementEvidence {
        SettlementEvidence {
            observations: vec![(sig.to_string(), Ok(o))],
            proof: None,
        }
    }

    /// FE-04 archive + FE-07 accounting over attempts. One journal holds one attempt; a finished
    /// attempt (settled, exit settled, no position) is sealed into `attempts/`, a fresh journal
    /// takes the next separately approved attempt and the same approval is refused; the
    /// accounting reads the archive and the active journal: a profitable sale is
    /// FIRST_REALIZED_PROFIT_EVIDENCED, a new unsettled attempt makes the verdict UNKNOWN again.
    /// Unfinished attempts are never archived; a foreign archive file fails closed.
    #[cfg(feature = "crate_health")]
    #[test]
    fn fe04_archive_keeps_the_attempt_history_and_approvals_single_use() {
        use crate::execution::accounting::{Ledger, Verdict};
        let dir = std::env::temp_dir().join(format!(
            "p0-fe04-archive-{}",
            solana_sdk::pubkey::Pubkey::new_unique()
        ));
        // `new_unique` counts per process: a run aborted mid-test leaves this name behind.
        let _ = std::fs::remove_dir_all(&dir);
        let wallet = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        assert_eq!(
            Ledger::load(&dir, &wallet).unwrap().verdict(),
            Verdict::NoTrade
        );
        // Attempt 1: −500 realized (fee), its 1000 units sold for 20 000 lamports (+2 000).
        let (mut j, ei, mint) = open_exit(&dir, &wallet, 1_000);
        let sig = &ei.signatures[0];
        let sold = evidence(sig, observation(sig, true, 20_000, &[(&mint, -1_000)]));
        assert!(matches!(
            j.apply_exit_evidence(sold, &value, 1_100).unwrap(),
            ExitStep::Settled(_)
        ));
        drop(j);
        let ledger = Ledger::load(&dir, &wallet).unwrap();
        let t = ledger.totals();
        assert_eq!(
            (
                t.attempts,
                t.landed_attempts,
                t.settled_exits,
                t.realized_net_usd_micro
            ),
            (1, 1, 1, Some(1_500))
        );
        assert_eq!(ledger.verdict(), Verdict::FirstRealizedProfitEvidenced);
        // Archive it: sealed, moved, the history unchanged.
        let archived = Journal::open(&dir, &wallet).unwrap().archive().unwrap();
        assert!(archived.ends_with(format!("attempts/{wallet}/UNIT_TEST_ONLY.p0j")));
        assert!(!dir.join(format!("{wallet}.p0j")).exists());
        let ledger = Ledger::load(&dir, &wallet).unwrap();
        assert_eq!(ledger.totals().archived, 1);
        assert_eq!(ledger.verdict(), Verdict::FirstRealizedProfitEvidenced);
        // A fresh journal: the archived approval is spent, a new one starts attempt 2.
        let (_, mut a, mut i, c) = setup();
        a.wallet.clone_from(&wallet);
        i.wallet.clone_from(&wallet);
        let mut j = Journal::open(&dir, &wallet).unwrap();
        assert_eq!(
            j.reserve(a.clone(), i.clone(), &c).unwrap_err(),
            "APPROVAL_ALREADY_CONSUMED"
        );
        a.approval_id = "approval 2".into();
        assert_eq!(
            j.reserve(a.clone(), i.clone(), &c).unwrap_err(),
            "APPROVAL_ID_INVALID"
        );
        a.approval_id = "approval-2".into();
        j.reserve(a, i, &c).unwrap();
        assert_eq!(
            Ledger::load(&dir, &wallet).unwrap().verdict(),
            Verdict::Unknown(vec!["ATTEMPT_RESERVED:approval-2".into()])
        );
        assert_eq!(j.archive().unwrap_err(), "ARCHIVE_NOT_SETTLED");
        // A foreign or unsealed file in the archive is never read as history.
        std::fs::write(dir.join(format!("attempts/{wallet}/forged.p0j")), b"P0J").unwrap();
        assert!(Ledger::load(&dir, &wallet).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        // An open position or an open exit is not finished.
        for (open_position, code) in [
            (true, "ARCHIVE_POSITION_OPEN"),
            (false, "ARCHIVE_EXIT_OPEN"),
        ] {
            let dir = std::env::temp_dir().join(format!(
                "p0-fe04-unfinished-{}",
                solana_sdk::pubkey::Pubkey::new_unique()
            ));
            let j = if open_position {
                settled_in(&dir, &wallet, 1_000).0
            } else {
                open_exit(&dir, &wallet, 1_000).0
            };
            let verdict = Ledger::load(&dir, &wallet).unwrap().verdict();
            assert!(
                matches!(&verdict, Verdict::Unknown(r) if !r.is_empty()),
                "{verdict:?}"
            );
            assert_eq!(j.archive().unwrap_err(), code);
            // Copied by hand into the archive, an unsealed journal is no archived attempt.
            let archive = dir.join(format!("attempts/{wallet}"));
            std::fs::create_dir_all(&archive).unwrap();
            std::fs::copy(
                dir.join(format!("{wallet}.p0j")),
                archive.join("UNIT_TEST_ONLY.p0j"),
            )
            .unwrap();
            assert_eq!(
                Ledger::load(&dir, &wallet).unwrap_err(),
                "ARCHIVE_NOT_SEALED"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// FE-07 verdicts from settled attempts: a sale below the attempt's cost is a LOSS; an
    /// attempt proven not landed paid nothing and is NO_TRADE.
    #[cfg(feature = "crate_health")]
    #[test]
    fn fe07_verdict_is_loss_or_no_trade_from_settled_attempts() {
        use crate::execution::accounting::{Ledger, Verdict};
        let dir = std::env::temp_dir().join(format!(
            "p0-fe07-loss-{}",
            solana_sdk::pubkey::Pubkey::new_unique()
        ));
        let wallet = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        exited(&dir, &wallet, 1_000);
        let ledger = Ledger::load(&dir, &wallet).unwrap();
        assert_eq!(ledger.totals().realized_net_usd_micro, Some(0));
        assert_eq!(ledger.verdict(), Verdict::Loss);
        assert!(ledger.report().last().unwrap().starts_with("verdict=LOSS"));
        let _ = std::fs::remove_dir_all(&dir);
        // Not landed: the non-landing proof settles the attempt with nothing paid.
        let (path, a, i, c) = setup();
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
        let proven = SettlementEvidence {
            observations: vec![],
            proof: Some(Ok(NonLandingProof {
                finalized_block_height: i.last_valid_block_height.unwrap() + 1,
                history_from_slot: i.state_slot,
                absent_signatures: i.signatures.iter().cloned().collect(),
            })),
        };
        j.apply_settlement_evidence(proven, &value, 1_000).unwrap();
        drop(j);
        let ledger = Ledger::load(&path, &a.wallet).unwrap();
        assert_eq!(ledger.totals().landed_attempts, 0);
        assert_eq!(ledger.verdict(), Verdict::NoTrade);
        let _ = std::fs::remove_dir_all(&path);
    }

    /// F-07 (P0-PROD-CTRL-08): an open position without a valuation is unvalued (Unknown), never
    /// marked 0.
    #[test]
    fn f07_unvalued_position_is_unknown_never_zero() {
        let (path, j, mint, _) = settled(1_000);
        let mut s = j.saved.clone().unwrap();
        if let Phase::Settled(done) = &mut s.phase {
            done.valuation.remove(&mint);
        }
        let p = open_positions(&s)[&mint];
        assert_eq!((p.units, p.mark_usd_micro), (1_000, None));
        let _ = std::fs::remove_dir_all(&path);
    }

    /// Review 9 (F-07): an exit of an unvalued position is refused before any reservation or
    /// handoff (its settlement could never release a mark), not after the trade.
    #[test]
    fn exit_of_an_unvalued_position_is_refused_before_handoff() {
        let (path, j, mint, a) = settled(1_000);
        let mut s = j.saved.clone().unwrap();
        if let Phase::Settled(done) = &mut s.phase {
            done.valuation.remove(&mint);
        }
        let (ea, ei, c) = exit_for(&a, &mint, 1_000, "exit-1");
        assert_eq!(
            super::validate(&s, &ea, &ei, &c).unwrap_err(),
            err("EXIT_POSITION_MARK_UNKNOWN")
        );
        let _ = std::fs::remove_dir_all(&path);
    }

    /// FE-04 exit: the whole open position is sold once — durably reserved and held before its
    /// single handoff, settled from its finalized receipt: the SOL proceeds net of the fee are
    /// realized (low value), the position's mark leaves the open inventory. A repeated answer
    /// books nothing new; a restart replays the same; the attempt and the exit together book the
    /// day's realized net and an empty inventory.
    #[test]
    fn exit_realizes_the_whole_open_position_once_and_survives_a_restart() {
        let (path, mut j, mint, a) = settled(1_000);
        assert_eq!(
            j.open_inventory(),
            [(
                mint.clone(),
                Position {
                    units: 1_000,
                    mark_usd_micro: Some(1_000)
                }
            )]
            .into()
        );
        let (ea, ei, c) = exit_for(&a, &mint, 1_000, "exit-1");
        j.reserve_exit(ea, ei.clone(), &c).unwrap();
        let hold = j.active_hold().unwrap();
        assert_eq!((hold.holds, hold.unknown_outcome), (ei.holds(), false));
        j.handoff_exit(&ei.signed_wire_sha256, &c, || Ok(()))
            .unwrap();
        assert!(j.active_hold().unwrap().unknown_outcome);
        assert_eq!(j.exit_job().unwrap().0, ei);
        let sig = &ei.signatures[0];
        let sold = evidence(sig, observation(sig, true, 5_000, &[(&mint, -1_000)]));
        let ExitStep::Settled(x) = j.apply_exit_evidence(sold.clone(), &value, 2_000).unwrap()
        else {
            panic!("settled");
        };
        assert_eq!(
            (
                x.landed_success,
                x.sold_units,
                x.realized_usd_micro,
                x.released_mark_usd_micro
            ),
            (true, 1_000, 500, 1_000)
        );
        assert!(j.open_inventory().is_empty() && j.active_hold().is_none());
        assert_eq!(
            j.apply_exit_evidence(sold, &value, 9_000).unwrap(),
            ExitStep::Settled(x.clone())
        );
        let replay = j.safety_replay().unwrap();
        assert_eq!(
            replay.exits,
            vec![darkbot_project0::safety::SettledAttempt {
                unix_s: 2_000,
                realized_net_usd_micro: 500,
                landed_success: true,
                open_inventory_usd_micro: -1_000,
            }]
        );
        drop(j);
        let mut reopened = Journal::open(&path, &a.wallet).unwrap();
        assert_eq!(reopened.safety_replay(), Some(replay.clone()));
        let (ea, ei, c) = exit_for(&a, &mint, 1_000, "exit-2");
        assert_eq!(
            reopened.reserve_exit(ea, ei, &c).unwrap_err(),
            "EXIT_AMOUNT_NOT_THE_OPEN_POSITION",
            "nothing left to sell"
        );
        // The attempt (fee −500, inventory +1000) and the exit (+500, −1000) on one day.
        let mut safety = darkbot_project0::safety::Safety::new(
            darkbot_project0::safety::Limits {
                max_total_capital: u64::MAX,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: 9,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss: u64::MAX,
                max_failed_attempts_per_day: 9,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: 9,
            },
            darkbot_project0::safety::Allow::default(),
            true,
        );
        safety.restore_attempt(3_000, None, replay.settled).unwrap();
        for e in replay.exits {
            safety.restore_attempt(3_000, None, Some(e)).unwrap();
        }
        assert_eq!(
            (safety.day().realized_pnl, safety.inventory_outstanding()),
            (0, 0)
        );
        drop(reopened);
        fs::remove_dir_all(path).unwrap();
    }

    /// FE-04 exit: a landed failure realizes its fee as a loss and sells nothing; a proven
    /// non-landing pays nothing — the position stays open in both; one approval is consumed once,
    /// a new one may exit again. A proof that does not reach the exit's expiry proves nothing.
    #[test]
    fn failed_and_not_landed_exits_keep_the_position_open() {
        let (path, mut j, mint, a) = settled(1_000);
        let ei = handed_off_exit(&mut j, &a, &mint, "exit-1");
        let sig = &ei.signatures[0];
        let failed = evidence(sig, observation(sig, false, -5_000, &[(&mint, 0)]));
        let ExitStep::Settled(x) = j.apply_exit_evidence(failed, &value, 2_000).unwrap() else {
            panic!("settled");
        };
        assert_eq!(
            (
                x.landed,
                x.landed_success,
                x.realized_usd_micro,
                x.sold_units,
                x.released_mark_usd_micro
            ),
            (true, false, -500, 0, 0)
        );
        assert_eq!(j.open_inventory()[&mint].units, 1_000);
        let (ea, ei2, c) = exit_for(&a, &mint, 1_000, "exit-1");
        assert_eq!(
            j.reserve_exit(ea, ei2, &c).unwrap_err(),
            "EXIT_APPROVAL_ALREADY_CONSUMED"
        );
        let ei = handed_off_exit(&mut j, &a, &mint, "exit-2");
        let proof = |height| NonLandingProof {
            finalized_block_height: height,
            history_from_slot: ei.state_slot,
            absent_signatures: ei.signatures.iter().cloned().collect(),
        };
        let short = SettlementEvidence {
            observations: vec![],
            proof: Some(Ok(proof(ei.last_valid_block_height))),
        };
        assert_eq!(
            j.apply_exit_evidence(short, &value, 2_100).unwrap_err(),
            "NON_LANDING_NOT_PROVEN_HOLD_RETAINED"
        );
        assert!(j.active_hold().is_some());
        let proven = SettlementEvidence {
            observations: vec![],
            proof: Some(Ok(proof(ei.last_valid_block_height + 1))),
        };
        let ExitStep::Settled(x) = j.apply_exit_evidence(proven, &value, 2_200).unwrap() else {
            panic!("settled");
        };
        assert_eq!((x.landed, x.realized_usd_micro), (false, 0));
        assert_eq!(j.open_inventory()[&mint].units, 1_000);
        assert_eq!(j.safety_replay().unwrap().exits.len(), 2);
        drop(j);
        fs::remove_dir_all(path).unwrap();
    }

    /// A23-07: the wallet journal's real growth, measured with the production writer: one
    /// attempt per journal (one-shot), then not-landed exits of its position until the runway
    /// guard refuses the next one. Every version carries all exits, so the growth per exit rises;
    /// the refusal comes before the file can outgrow what `Journal::open` reopens, keeps the
    /// position held and leaves a journal that reopens with every exit.
    #[test]
    fn a23_07_wallet_journal_runway_is_measured_and_stays_reopenable() {
        let (path, mut j, mint, a) = settled(1_000);
        let attempt_bytes = j.writer.position();
        let mut sizes = vec![attempt_bytes];
        let refused = loop {
            let id = format!("exit-{}", sizes.len());
            let (ea, ei, c) = exit_for(&a, &mint, 1_000, &id);
            if let Err(e) = j.reserve_exit(ea, ei.clone(), &c) {
                break e;
            }
            j.handoff_exit(&ei.signed_wire_sha256, &c, || Ok(()))
                .unwrap();
            let proven = SettlementEvidence {
                observations: vec![],
                proof: Some(Ok(NonLandingProof {
                    finalized_block_height: ei.last_valid_block_height + 1,
                    history_from_slot: ei.state_slot,
                    absent_signatures: ei.signatures.iter().cloned().collect(),
                })),
            };
            assert!(matches!(
                j.apply_exit_evidence(proven, &value, 2_200).unwrap(),
                ExitStep::Settled(_)
            ));
            sizes.push(j.writer.position());
        };
        assert_eq!(refused, "JOURNAL_RUNWAY_EXHAUSTED");
        let exits = sizes.len() - 1;
        let (first, last) = (sizes[1] - sizes[0], sizes[exits] - sizes[exits - 1]);
        eprintln!(
            "A23_07_RUNWAY attempt_bytes={attempt_bytes} first_exit_bytes={first} \
             last_exit_bytes={last} exits={exits} final_bytes={} limit={MAX_JOURNAL_BYTES}",
            sizes[exits]
        );
        assert!(last > first, "every version carries all exits");
        assert!(exits >= 50, "{exits} exits");
        assert!(sizes[exits] <= MAX_JOURNAL_BYTES);
        // The refusal changed nothing: the position stays held, every exit replays.
        assert_eq!(j.open_inventory()[&mint].units, 1_000);
        assert_eq!(j.safety_replay().unwrap().exits.len(), exits);
        let wallet = a.wallet.clone();
        drop(j);
        let j = Journal::open(&path, &wallet).unwrap();
        assert_eq!(j.safety_replay().unwrap().exits.len(), exits);
        assert_eq!(j.open_inventory()[&mint].units, 1_000);
        drop(j);
        fs::remove_dir_all(path).unwrap();
    }

    /// FE-04 exit ("unvollständiger Exit"): a partial or mismatched sale, foreign wallet activity
    /// (also WSOL the unwrap paid out), proceeds below the signed floor, a failure that sold, or an
    /// observation with unknown reasons never finalizes — the exit stays UNKNOWN with every hold,
    /// nothing is booked.
    #[test]
    fn inconsistent_exit_receipts_stay_unknown_with_every_hold() {
        let other = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        for (name, success, sol, sold, foreign, unknown) in [
            ("partial", true, 5_000, -500, 0, false),
            ("foreign", true, 5_000, -1_000, 1, false),
            ("below_floor", true, 4_000, -1_000, 0, false),
            ("failed_but_sold", false, -5_000, -1_000, 0, false),
            ("unknown_reason", true, 5_000, -1_000, 0, true),
        ] {
            let (path, mut j, mint, a) = settled(1_000);
            let ei = handed_off_exit(&mut j, &a, &mint, "exit-1");
            let sig = &ei.signatures[0];
            let mut o = observation(sig, success, sol, &[(&mint, sold), (&other, foreign)]);
            if unknown {
                o.unknown_reasons = vec!["ARCHIVE_GAP".into()];
            }
            let step = j
                .apply_exit_evidence(evidence(sig, o), &value, 2_000)
                .unwrap();
            assert!(matches!(step, ExitStep::Unknown(_)), "{name}: {step:?}");
            let hold = j.active_hold().unwrap();
            assert!(hold.unknown_outcome && hold.holds == ei.holds(), "{name}");
            assert!(j.safety_replay().unwrap().exits.is_empty(), "{name}");
            assert_eq!(j.open_inventory()[&mint].units, 1_000, "{name}");
            drop(j);
            fs::remove_dir_all(path).unwrap();
        }
        // The unwrap paid out WSOL that was already in the wallet: observed, foreign, UNKNOWN.
        let (path, mut j, mint, a) = settled(1_000);
        let ei = handed_off_exit(&mut j, &a, &mint, "exit-1");
        let sig = &ei.signatures[0];
        let o = observation(sig, true, 7_000, &[(&mint, -1_000), (WSOL_MINT, -2_000)]);
        assert!(matches!(
            j.apply_exit_evidence(evidence(sig, o), &value, 2_000)
                .unwrap(),
            ExitStep::Unknown(_)
        ));
        assert!(j.active_hold().unwrap().unknown_outcome);
        drop(j);
        fs::remove_dir_all(path).unwrap();
    }

    /// FE-04 exit: refused before anything durable — no settled attempt, an amount other than the
    /// whole position, an approval of another attempt, expired or unsimulated, a floor below the
    /// approval, an underfunded wallet, a second open exit, a changed payload at the handoff.
    #[test]
    fn exit_reservation_refusals_write_nothing() {
        let (path, a, i, c) = setup();
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        let mint = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let (ea, ei, ec) = exit_for(&a, &mint, 1_000, "exit-1");
        assert_eq!(
            j.reserve_exit(ea, ei, &ec).unwrap_err(),
            "EXIT_REQUIRES_SETTLED_ATTEMPT"
        );
        drop(j);
        fs::remove_dir_all(path).unwrap();

        let (path, mut j, mint, a) = settled(1_000);
        let file = path_of(&j);
        let frames = || {
            let mut r = crate::analytics::journal::Reader::open(&file).unwrap();
            std::iter::from_fn(|| r.next_frame().unwrap()).count()
        };
        let before = frames();
        let base = exit_for(&a, &mint, 1_000, "exit-1");
        type Case = fn(&mut ExitApproval, &mut ExitIntent, &mut Current);
        let cases: [(&str, Case); 7] = [
            ("EXIT_AMOUNT_NOT_THE_OPEN_POSITION", |_, i, c| {
                i.amount_in = 999;
                c.balances.insert(i.mint.clone(), 999);
            }),
            ("INVALID_OR_UNAUTHORIZED_EXIT_APPROVAL", |a, _, _| {
                a.attempt_approval_id = "another-attempt".into()
            }),
            ("INVALID_OR_UNAUTHORIZED_EXIT_APPROVAL", |a, _, _| {
                a.authorized = false
            }),
            ("STALE_FORK_GAP_OR_UPGRADE", |_, _, c| {
                c.now_unix_ms = 100_000
            }),
            ("CURRENT_EXECUTION_EVIDENCE_UNKNOWN", |_, _, c| {
                c.exact_signed_payload_simulated = false
            }),
            ("EXIT_APPROVAL_BUDGET_EXCEEDED", |_, i, _| {
                i.minimum_out_lamports = 8_999
            }),
            ("WALLET_UNDERFUNDED_OR_HOLD_MISSING", |_, i, c| {
                c.balances.insert(i.mint.clone(), 999);
            }),
        ];
        for (code, edit) in cases {
            let (mut ea, mut ei, mut ec) = base.clone();
            edit(&mut ea, &mut ei, &mut ec);
            assert_eq!(j.reserve_exit(ea, ei, &ec).unwrap_err(), code);
        }
        assert_eq!(frames(), before, "no refusal wrote a frame");
        let (ea, ei, ec) = base.clone();
        j.reserve_exit(ea, ei.clone(), &ec).unwrap();
        let (ea, ei2, ec2) = exit_for(&a, &mint, 1_000, "exit-2");
        assert_eq!(
            j.reserve_exit(ea, ei2, &ec2).unwrap_err(),
            "EXIT_ALREADY_OPEN"
        );
        assert_eq!(
            j.handoff_exit(&"f".repeat(64), &ec, || Ok(())).unwrap_err(),
            "SIGNED_PAYLOAD_CHANGED"
        );
        drop(j);
        fs::remove_dir_all(path).unwrap();
    }

    /// FE-04 exit: a crash after the durable reserve reopens the exit UNKNOWN with its hold — it
    /// is never handed off again; a frame the automaton would not write halts the replay.
    #[test]
    fn crashed_exit_reopens_unknown_and_an_illegal_frame_halts() {
        let (path, mut j, mint, a) = settled(1_000);
        let (ea, ei, c) = exit_for(&a, &mint, 1_000, "exit-1");
        j.reserve_exit(ea, ei.clone(), &c).unwrap();
        drop(j); // crash before the handoff
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        assert!(j.active_hold().unwrap().unknown_outcome);
        assert_eq!(
            j.handoff_exit(&ei.signed_wire_sha256, &c, || Ok(()))
                .unwrap_err(),
            "EXIT_HALTED"
        );
        // A settled exit written straight from UNKNOWN (no finalization) is foreign.
        let mut s = j.saved.clone().unwrap();
        s.exits.last_mut().unwrap().phase = ExitPhase::Settled(ExitSettlement {
            landed: true,
            landed_success: true,
            fee_lamports: 0,
            wallet_deltas: BTreeMap::new(),
            sold_units: 1_000,
            realized_usd_micro: 1_000_000,
            released_mark_usd_micro: 1_000,
            booked_unix_s: 2_000,
        });
        j.persist(s, Transition::ExitSettle).unwrap();
        drop(j);
        assert_eq!(
            Journal::open(&path, &a.wallet).err().unwrap(),
            "JOURNAL_TRANSITION_INVALID"
        );
        fs::remove_dir_all(path).unwrap();
    }
}
