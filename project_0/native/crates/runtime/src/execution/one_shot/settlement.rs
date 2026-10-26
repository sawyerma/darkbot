//! C3/P8-05: finalized settlement of the one durable FE-04 attempt, in the same wallet journal
//! (no second financial store). Receipts come from the durable finalized observations of every
//! bound signature (`reconcile_observed`) or a DEC-03/N-2 non-landing read that proves it
//! (`reconcile_not_landed`: B+C or C alone, the class persisted with the attempt);
//! the realized i128 net PnL is booked exactly once as a journal transition (`Phase::Settled`).
//! An incomplete bundle, an observation with unknown reasons, a missing proof or any delta without
//! a conservative value stays UNKNOWN: holds and HALT remain.
//!
//! Realized is only what left or reached the wallet as SOL (fees, tip, rent, protocol fee, SOL
//! proceeds) and every token outflow (spent). A token inflow — collateral or surplus left in the
//! wallet — is open inventory: marked at its conservative (lower-bound) value, unrealized, never
//! realized net PnL, since no receipt is evidence of its exit (sale). The attempt's expected edge
//! stays the intent's `conservative_net_usd_micro`.
use super::non_landing::{NonLandingRead, NonLandingSources, Subject, Verdict};
use super::{err, Intent, Journal, Phase, Receipt, Transition};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use std::collections::{BTreeMap, BTreeSet};

/// The booked outcome of one attempt (USD micro where named so; lamports and base units else).
/// Journal schema 4; earlier layouts read through `SettlementV3`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settlement {
    /// Some own transaction landed (false: a proven non-landing, nothing was paid).
    pub landed: bool,
    /// Every own transaction landed and succeeded.
    pub landed_success: bool,
    /// Network fees of all own transactions; already inside the SOL delta.
    pub fee_lamports: u64,
    /// Wallet deltas summed over all own transactions; SOL includes fees, tip and rent.
    pub wallet_deltas: BTreeMap<String, i128>,
    /// The conservative USD-micro value of every non-zero delta.
    pub valuation: BTreeMap<String, i128>,
    /// The SOL delta and every token outflow; never a token inflow.
    pub realized_net_usd_micro: i128,
    /// Token inflows left in the wallet (base units, positive): open inventory.
    pub open_inventory: BTreeMap<String, i128>,
    /// Their conservative mark (≥ 0): unrealized until an exit is evidenced.
    pub unrealized_inventory_usd_micro: i128,
    /// The second whose UTC day the settlement is booked into, persisted with it: the running
    /// owner and a restart book the same day. `None`: settled before schema 4 (booked into the
    /// evaluation day, as then).
    pub booked_unix_s: Option<u64>,
}

/// Schema ≤ 3 layout of `Settlement`: one net that included token inflows.
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
pub(super) struct SettlementV3 {
    pub(super) landed: bool,
    pub(super) landed_success: bool,
    pub(super) fee_lamports: u64,
    pub(super) wallet_deltas: BTreeMap<String, i128>,
    pub(super) valuation: BTreeMap<String, i128>,
    pub(super) realized_net_usd_micro: i128,
}
impl TryFrom<SettlementV3> for Settlement {
    type Error = String;
    /// Reclassified from its own deltas and valuation, as `compute` books today; the parts must
    /// sum to the net it booked then (else the frame is damaged).
    fn try_from(v: SettlementV3) -> Result<Self, String> {
        let (realized, open, unrealized) = classify(&v.wallet_deltas, &v.valuation)?;
        if realized.checked_add(unrealized) != Some(v.realized_net_usd_micro) {
            return Err(err("SETTLEMENT_LEGACY_NET_MISMATCH"));
        }
        Ok(Self {
            landed: v.landed,
            landed_success: v.landed_success,
            fee_lamports: v.fee_lamports,
            wallet_deltas: v.wallet_deltas,
            valuation: v.valuation,
            realized_net_usd_micro: realized,
            open_inventory: open,
            unrealized_inventory_usd_micro: unrealized,
            booked_unix_s: None,
        })
    }
}

/// Realized net (SOL and token outflows), open inventory (token inflows) and its mark.
fn classify(
    deltas: &BTreeMap<String, i128>,
    valuation: &BTreeMap<String, i128>,
) -> Result<(i128, BTreeMap<String, i128>, i128), String> {
    let overflow = || err("SETTLEMENT_OVERFLOW");
    let (mut realized, mut open, mut unrealized) = (0i128, BTreeMap::new(), 0i128);
    for (asset, v) in valuation {
        let d = deltas
            .get(asset)
            .copied()
            .ok_or_else(|| format!("SETTLEMENT_VALUE_UNKNOWN:{asset}"))?;
        if asset != "SOL" && d > 0 {
            open.insert(asset.clone(), d);
            unrealized = unrealized.checked_add(*v).ok_or_else(overflow)?;
        } else {
            realized = realized.checked_add(*v).ok_or_else(overflow)?;
        }
    }
    Ok((realized, open, unrealized))
}

/// What one settlement read is about — the attempt's or an exit's: the wallet and the assets its
/// receipts carry, the bound signatures and the bounds of their non-landing proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementSubject {
    pub wallet: String,
    pub signatures: Vec<String>,
    pub expected_wallet_assets: BTreeSet<String>,
    /// `None` (an intent before C12): never provably absent.
    pub last_valid_block_height: Option<u64>,
    pub state_slot: u64,
}
impl From<&Intent> for SettlementSubject {
    fn from(i: &Intent) -> Self {
        Self {
            wallet: i.wallet.clone(),
            signatures: i.signatures.clone(),
            expected_wallet_assets: i.expected_wallet_assets.clone(),
            last_valid_block_height: i.last_valid_block_height,
            state_slot: i.state_slot,
        }
    }
}
impl From<&super::exit::ExitIntent> for SettlementSubject {
    /// An exit's receipts carry SOL, its mint and the wrapped-SOL mint: the unwrap closes the
    /// wallet's WSOL account, so any WSOL delta is observed (and, being foreign, never proceeds).
    fn from(i: &super::exit::ExitIntent) -> Self {
        Self {
            wallet: i.wallet.clone(),
            signatures: i.signatures.clone(),
            expected_wallet_assets: [
                "SOL".to_string(),
                i.mint.clone(),
                super::exit::WSOL_MINT.to_string(),
            ]
            .into(),
            last_valid_block_height: Some(i.last_valid_block_height),
            state_slot: i.state_slot,
        }
    }
}
impl SettlementSubject {
    /// DEC-03: what its non-landing read proves; an unknown expiry (an intent before C12) is
    /// never provable.
    pub fn non_landing(&self) -> Result<Subject, String> {
        Ok(Subject {
            signatures: self.signatures.clone(),
            last_valid_block_height: self
                .last_valid_block_height
                .ok_or_else(|| err("NON_LANDING_EXPIRY_UNKNOWN"))?,
            state_slot: self.state_slot,
        })
    }
}

/// FE-04/C3: the conservative value of a signed settlement delta from a low and a high value of
/// a native amount: a gain at its lower bound, a loss at its upper bound (negative). Unknown (no
/// value, an amount beyond `u64`) is `None`: the settlement stays UNKNOWN.
pub fn conservative_value(
    low: &dyn Fn(&str, u64) -> Option<u64>,
    high: &dyn Fn(&str, u64) -> Option<u64>,
    asset: &str,
    delta: i128,
) -> Option<i128> {
    let amount = u64::try_from(delta.unsigned_abs()).ok()?;
    if delta >= 0 {
        low(asset, amount).map(i128::from)
    } else {
        high(asset, amount).map(|v| -i128::from(v))
    }
}

/// What the settlement worker read for one UNKNOWN attempt, off the owner: each not yet observed
/// bound signature's finalized observation or why it is missing, and — only while nothing at all
/// is observed — the DEC-03 non-landing read (B and C) or why none was made. Evidence only; the
/// journal decides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettlementEvidence {
    pub observations: Vec<(
        String,
        Result<super::receipts::FinalizedObservation, String>,
    )>,
    pub non_landing: Option<Result<NonLandingRead, String>>,
}

/// V2-01: the signatures whose observation is complete (final; never read again).
pub(super) fn complete_signatures(
    observations: &BTreeMap<String, super::receipts::FinalizedObservation>,
) -> Observed {
    Observed {
        complete: observations
            .iter()
            .filter(|(_, o)| o.is_complete())
            .map(|(signature, _)| signature.clone())
            .collect(),
        stored: !observations.is_empty(),
    }
}

/// What a settlement read starts from: the signatures with a complete observation (not read
/// again, V2-01) and whether any observation is stored at all — then no non-landing proof is
/// read (it could never be accepted while an observation exists).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Observed {
    pub complete: BTreeSet<String>,
    pub stored: bool,
}

/// V2-01: a read replaces the stored observation unless that one is complete and the read is
/// not (complete evidence is never weakened); an identical read changes nothing.
pub(super) fn keep_strongest(
    observations: &mut BTreeMap<String, super::receipts::FinalizedObservation>,
    signature: String,
    read: super::receipts::FinalizedObservation,
) {
    if observations
        .get(&signature)
        .is_some_and(|old| old.is_complete() && !read.is_complete())
    {
        return;
    }
    observations.insert(signature, read);
}

/// The settlement worker's reads for `intent` (signatures with a complete observation are not
/// read again); the non-landing read over `sources` (unavailable: why).
pub fn fetch_settlement_evidence(
    rpc: &RpcClient,
    sources: Result<&NonLandingSources, &str>,
    intent: &SettlementSubject,
    observed: &Observed,
) -> SettlementEvidence {
    let wallet = intent.wallet.parse();
    let observations: Vec<_> = intent
        .signatures
        .iter()
        .filter(|s| !observed.complete.contains(*s))
        .map(|signature| {
            let read = match (&wallet, signature.parse()) {
                (Ok(wallet), Ok(sig)) => super::receipts::fetch_finalized(
                    rpc,
                    &sig,
                    wallet,
                    intent.state_slot,
                    &intent.expected_wallet_assets,
                ),
                (Err(_), _) => Err(err("WALLET_INVALID")),
                (_, Err(_)) => Err(err("SIGNATURE_BINDING_INVALID")),
            };
            (signature.clone(), read)
        })
        .collect();
    let none_observed = !observed.stored && observations.iter().all(|(_, r)| r.is_err());
    SettlementEvidence {
        non_landing: none_observed.then(|| {
            let subject = intent.non_landing()?;
            Ok(sources.map_err(str::to_string)?.read(&subject))
        }),
        observations,
    }
}

/// Why a non-landing read releases nothing: its class, then C's result and each B source that
/// is not usable.
pub(super) fn non_landing_reasons(read: &NonLandingRead) -> Vec<String> {
    std::iter::once(format!("NON_LANDING_{}", read.verdict().code()))
        .chain(std::iter::once(format!("NON_LANDING_C:{}", read.c.code())))
        .chain(
            read.b
                .iter()
                .filter(|s| s.class != super::non_landing::BClass::Usable)
                .map(|s| format!("NON_LANDING_B:{}:{}", s.operator, s.class.code())),
        )
        .collect()
}

/// DEC-03 (N-2 §6): the result class of a subject from its journal state and the latest read
/// made for it. Reserved (nothing handed off): none. Finalized from receipts: a sighting with
/// the landing confirmed; finalized not landed: its persisted release class (none before N-2).
/// UNKNOWN: any stored observation is a sighting without a confirmed landing (an open
/// contradiction); else the read's class — a proving class the journal did not release, an
/// unreadable subject or no read is incomplete.
pub(super) fn non_landing_class(
    handed_off: bool,
    finalized: Option<(&[Receipt], Option<&super::non_landing::NonLandingRelease>)>,
    observed: bool,
    subject: Result<Subject, String>,
    read: Option<&NonLandingRead>,
) -> Option<Verdict> {
    if let Some((receipts, release)) = finalized {
        return if receipts.is_empty() {
            release.map(|r| r.class)
        } else {
            Some(Verdict::VetoLandingConfirmed)
        };
    }
    if !handed_off {
        return None;
    }
    if observed {
        return Some(Verdict::VetoContradiction);
    }
    Some(match (subject, read) {
        (Ok(s), Some(r)) if r.subject == s => r.verdict(),
        _ => Verdict::UnknownIncomplete,
    })
    .map(|v| {
        if v.proves_non_landing() {
            Verdict::UnknownIncomplete
        } else {
            v
        }
    })
}

/// DEC-03 (N-2 §6): the non-landing status of the journal's latest subject — its last exit,
/// else the attempt: its result class and, while its hold is held, the evaluation it was
/// reserved at (the hold duration's start).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonLandingStatus {
    /// The exit's approval id; `None`: the attempt.
    pub exit: Option<String>,
    pub class: Option<Verdict>,
    pub held_since_unix_ms: Option<u64>,
    /// The persisted release of a released non-landing (what proved it, after a restart too).
    pub release: Option<super::non_landing::NonLandingRelease>,
}

/// The outcome of one settlement step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettleStep {
    /// Booked once (also when it already was): holds are released by the finalized phase.
    Settled(Settlement),
    /// Still UNKNOWN: every hold stays; the reasons of this step.
    Unknown(Vec<String>),
}

impl Journal {
    /// Finalizes from the durable observations: every bound signature (prerequisites,
    /// receivership, tip) observed finalized, without unknown reasons and with its token deltas.
    /// Anything less keeps the attempt UNKNOWN (partial bundle, missing archive, 429).
    pub fn reconcile_observed(&mut self) -> Result<(), String> {
        let saved = self.saved.as_ref().ok_or_else(|| err("NO_ATTEMPT"))?;
        if saved.phase != Phase::HandoffUnknown {
            return Err(err("NOT_UNKNOWN"));
        }
        let mut receipts = Vec::with_capacity(saved.intent.signatures.len());
        for signature in &saved.intent.signatures {
            let o = saved
                .observations
                .get(signature)
                .ok_or_else(|| err("SETTLEMENT_SIGNATURE_UNOBSERVED_HOLD_RETAINED"))?;
            if !o.unknown_reasons.is_empty() {
                return Err(err("SETTLEMENT_OBSERVATION_UNKNOWN_HOLD_RETAINED"));
            }
            let mut deltas = o
                .wallet_token_deltas
                .clone()
                .ok_or_else(|| err("SETTLEMENT_TOKEN_DELTAS_UNKNOWN_HOLD_RETAINED"))?;
            deltas.insert("SOL".into(), o.wallet_sol_delta_lamports);
            receipts.push(Receipt {
                signature: signature.clone(),
                finalized: true,
                success: o.success,
                fee_lamports: Some(o.fee_lamports),
                wallet_deltas: Some(deltas),
                // The receipt is complete (every bound transaction, all deltas). It is no
                // evidence of an inventory exit: token inflows stay open inventory (`classify`).
                exit_complete: true,
            });
        }
        self.reconcile(receipts)
    }

    /// Finalizes a proven non-landing (DEC-03/N-2: a read of exactly this intent that is B+C
    /// confirmed or C alone qualified, nothing observed): nothing landed, nothing was paid; the
    /// release class is persisted with the attempt.
    pub fn reconcile_not_landed(&mut self, read: &NonLandingRead) -> Result<(), String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        if s.phase != Phase::HandoffUnknown {
            return Err(err("NOT_UNKNOWN"));
        }
        // C12: an intent without its blockhash expiry (schema 2) can never be proven absent.
        let release = SettlementSubject::from(&s.intent)
            .non_landing()
            .ok()
            .and_then(|subject| read.release(&subject))
            .filter(|_| s.observations.is_empty())
            .ok_or_else(|| err("NON_LANDING_NOT_PROVEN_HOLD_RETAINED"))?;
        s.phase = Phase::HaltFinalized;
        s.finalized = Some(Vec::new());
        s.non_landing = Some(release);
        self.persist(s, Transition::NotLanded)
    }

    /// DEC-03: the non-landing status of the latest subject; `read`: the latest non-landing read
    /// with the exit it was made for (`None`: the attempt), used only for that subject.
    pub fn non_landing_status(
        &self,
        read: Option<(Option<&str>, &NonLandingRead)>,
    ) -> Option<NonLandingStatus> {
        use super::exit::ExitPhase;
        let s = self.saved.as_ref()?;
        let latest = |exit: Option<&str>| read.filter(|(e, _)| *e == exit).map(|(_, r)| r);
        Some(match s.exits.last() {
            Some(e) => {
                let id = e.approval.approval_id.as_str();
                let finalized = matches!(e.phase, ExitPhase::Finalized | ExitPhase::Settled(_));
                NonLandingStatus {
                    exit: Some(id.to_string()),
                    class: non_landing_class(
                        e.phase != ExitPhase::Reserved,
                        finalized.then(|| {
                            (
                                e.finalized.as_deref().unwrap_or_default(),
                                e.non_landing.as_ref(),
                            )
                        }),
                        !e.observations.is_empty(),
                        SettlementSubject::from(&e.intent).non_landing(),
                        latest(Some(id)),
                    ),
                    held_since_unix_ms: (!finalized).then_some(e.intent.evaluated_unix_ms),
                    release: e.non_landing.clone(),
                }
            }
            None => {
                let finalized = s.phase.finalized();
                NonLandingStatus {
                    exit: None,
                    class: non_landing_class(
                        s.phase != Phase::Reserved,
                        finalized.then(|| {
                            (
                                s.finalized.as_deref().unwrap_or_default(),
                                s.non_landing.as_ref(),
                            )
                        }),
                        !s.observations.is_empty(),
                        SettlementSubject::from(&s.intent).non_landing(),
                        latest(None),
                    ),
                    held_since_unix_ms: (!finalized).then_some(s.intent.evaluated_unix_ms),
                    release: s.non_landing.clone(),
                }
            }
        })
    }

    /// The settlement worker's job: the intent and the already observed signatures of an
    /// UNKNOWN attempt; `None` otherwise.
    pub fn settlement_job(&self) -> Option<(Intent, Observed)> {
        self.saved
            .as_ref()
            .filter(|s| s.phase == Phase::HandoffUnknown)
            // V2-01: only complete observations are final; an incomplete one is read again.
            .map(|s| (s.intent.clone(), complete_signatures(&s.observations)))
    }

    /// C02/F-04: the settled attempt's landing telemetry — the one reconcile path. Its primary
    /// signature with the finalized slot, outcome and fees of the settlement; compute units and
    /// the paid tip are not read here (`None`, never 0). `released`: the capital the booking of
    /// this settlement released.
    pub fn landing_record(
        &self,
        released: u64,
    ) -> Option<crate::execution::pending::ReconcileRecord> {
        let s = self.saved.as_ref()?;
        let Phase::Settled(done) = &s.phase else {
            return None;
        };
        let signature = s.intent.signatures.first()?.clone();
        let landed_slot = s.observations.get(&signature).map(|o| o.slot);
        Some(crate::execution::pending::ReconcileRecord {
            signature,
            path: "FE04_SETTLEMENT".into(),
            outcome: if !done.landed {
                "NOT_LANDED"
            } else if done.landed_success {
                "LANDED_SUCCESS"
            } else {
                "LANDED_FAILED"
            }
            .into(),
            landed_slot: landed_slot.filter(|_| done.landed),
            fee_lamports: Some(done.fee_lamports),
            compute_units_consumed: None,
            paid_tip_lamports: None,
            network_and_tip_lamports: None,
            error: None,
            exposure_released: released,
            pnl_complete: true,
        })
    }

    /// Finalized but not yet booked (e.g. a valuation that was unknown, or a restart between the
    /// two transitions): settled from the journal alone, no read needed.
    pub fn awaits_booking(&self) -> bool {
        self.saved
            .as_ref()
            .is_some_and(|s| s.phase == Phase::HaltFinalized)
    }

    /// FE-04/C3: one settlement step through the reconciliation RPC (worker read, then apply).
    /// F-14: the tests' composition; the owner reads on its worker and applies the evidence.
    #[cfg(test)]
    pub fn settle_step(
        &mut self,
        rpc: &RpcClient,
        sources: &NonLandingSources,
        value: &dyn Fn(&str, i128) -> Option<i128>,
        now_unix_s: u64,
    ) -> Result<SettleStep, String> {
        let evidence = match self.settlement_job() {
            Some((intent, observed)) => fetch_settlement_evidence(
                rpc,
                Ok(sources),
                &SettlementSubject::from(&intent),
                &observed,
            ),
            None => SettlementEvidence::default(),
        };
        self.apply_settlement_evidence(evidence, value, now_unix_s)
    }

    /// FE-04/C3: applies the worker's evidence to the attempt. New observations are durable
    /// (only when something changed); every bound signature observed finalized without unknown
    /// reasons ⇒ reconcile and book (`value`); nothing observed and a non-landing read that
    /// proves it (DEC-03: B+C or C alone) ⇒ finalized and booked with nothing paid; anything
    /// partial, vetoed or unproven stays UNKNOWN with its reasons (holds retained). Idempotent
    /// across steps and restarts. A settlement made now is booked at `now_unix_s`.
    pub fn apply_settlement_evidence(
        &mut self,
        evidence: SettlementEvidence,
        value: &dyn Fn(&str, i128) -> Option<i128>,
        now_unix_s: u64,
    ) -> Result<SettleStep, String> {
        let mut saved = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        match &saved.phase {
            Phase::Settled(done) => return Ok(SettleStep::Settled(done.clone())),
            Phase::HaltFinalized => return self.settle(value, now_unix_s).map(SettleStep::Settled),
            Phase::Reserved => return Ok(SettleStep::Unknown(vec![err("NOT_HANDED_OFF")])),
            Phase::HandoffUnknown => {}
        }
        let before = saved.observations.clone();
        let mut reasons = Vec::new();
        for (signature, read) in evidence.observations {
            if !saved.intent.signatures.contains(&signature) {
                reasons.push(format!("{signature}:SETTLEMENT_SIGNATURE_FOREIGN"));
                continue;
            }
            match read {
                Ok(o) => keep_strongest(&mut saved.observations, signature, o),
                Err(reason) => reasons.push(format!("{signature}:{reason}")),
            }
        }
        for (signature, o) in &saved.observations {
            reasons.extend(o.unknown_reasons.iter().map(|r| format!("{signature}:{r}")));
        }
        let complete = saved.observations.len() == saved.intent.signatures.len();
        let none = saved.observations.is_empty();
        if saved.observations != before {
            self.persist(saved, Transition::Observe)?;
        }
        if complete && reasons.is_empty() {
            self.reconcile_observed()?;
            return self.settle(value, now_unix_s).map(SettleStep::Settled);
        }
        if none {
            match evidence.non_landing {
                Some(Ok(read)) if read.verdict().proves_non_landing() => {
                    self.reconcile_not_landed(&read)?;
                    return self.settle(value, now_unix_s).map(SettleStep::Settled);
                }
                Some(Ok(read)) => reasons.extend(non_landing_reasons(&read)),
                Some(Err(reason)) => reasons.push(reason),
                None => reasons.push(err("NON_LANDING_NOT_ATTEMPTED")),
            }
        }
        Ok(SettleStep::Unknown(reasons))
    }

    /// Books the outcome of a finalized attempt exactly once, at `booked_unix_s` (persisted: its
    /// UTC booking day). `value(asset, delta)` is the conservative USD-micro value of a signed
    /// delta (`None`: not valued); a booked settlement is returned unchanged, never booked twice.
    pub fn settle(
        &mut self,
        value: &dyn Fn(&str, i128) -> Option<i128>,
        booked_unix_s: u64,
    ) -> Result<Settlement, String> {
        let s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        let receipts = match &s.phase {
            Phase::Settled(done) => return Ok(done.clone()),
            Phase::HaltFinalized => s.finalized.clone().ok_or_else(|| err("NOT_FINALIZED"))?,
            _ => return Err(err("NOT_FINALIZED_HOLD_RETAINED")),
        };
        let settlement = compute(&receipts, value, booked_unix_s)?;
        let mut next = s;
        next.phase = Phase::Settled(settlement.clone());
        self.persist(next, Transition::Settle)?;
        Ok(settlement)
    }
}

fn compute(
    receipts: &[Receipt],
    value: &dyn Fn(&str, i128) -> Option<i128>,
    booked_unix_s: u64,
) -> Result<Settlement, String> {
    let overflow = || err("SETTLEMENT_OVERFLOW");
    let mut deltas: BTreeMap<String, i128> = BTreeMap::new();
    let mut fee = 0u64;
    for r in receipts {
        fee = fee
            .checked_add(
                r.fee_lamports
                    .ok_or_else(|| err("SETTLEMENT_FEE_UNKNOWN"))?,
            )
            .ok_or_else(overflow)?;
        for (asset, d) in r
            .wallet_deltas
            .as_ref()
            .ok_or_else(|| err("SETTLEMENT_DELTAS_UNKNOWN"))?
        {
            let total = deltas.entry(asset.clone()).or_default();
            *total = total.checked_add(*d).ok_or_else(overflow)?;
        }
    }
    let mut valuation = BTreeMap::new();
    for (asset, d) in deltas.iter().filter(|(_, d)| **d != 0) {
        let v = value(asset, *d).ok_or_else(|| format!("SETTLEMENT_VALUE_UNKNOWN:{asset}"))?;
        if v.signum() * d.signum() < 0 {
            return Err(format!("SETTLEMENT_VALUE_SIGN:{asset}"));
        }
        valuation.insert(asset.clone(), v);
    }
    let (realized, open_inventory, unrealized) = classify(&deltas, &valuation)?;
    Ok(Settlement {
        landed: !receipts.is_empty(),
        landed_success: !receipts.is_empty() && receipts.iter().all(|r| r.success),
        fee_lamports: fee,
        wallet_deltas: deltas,
        valuation,
        realized_net_usd_micro: realized,
        open_inventory,
        unrealized_inventory_usd_micro: unrealized,
        booked_unix_s: Some(booked_unix_s),
    })
}

#[cfg(test)]
mod tests {
    use super::super::non_landing::{loopback, NonLandingSources, Verdict};
    use super::super::receipts::FinalizedObservation;
    use super::super::tests::{path_of, setup};
    use super::super::*;
    use super::{SettleStep, Settlement, SettlementEvidence, SettlementSubject};
    use serde_json::{json, Value};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    /// The non-landing sources of the tests' intent (state slot 50, expiry height 60): C and two
    /// B operators over one loopback chain of `n` blocks from slot 50, heights from 58 (four
    /// blocks reach beyond the expiry); the methods in call order.
    fn sources_of(
        n: u64,
        sighting: Option<(u64, String)>,
    ) -> (NonLandingSources, Arc<Mutex<Vec<String>>>) {
        let (url, calls) = loopback::endpoint(loopback::chain(50, n, 58, 0, sighting));
        (
            loopback::sources(url.clone(), vec![("op-a", url.clone()), ("op-b", url)]),
            calls,
        )
    }

    /// A finalized successful transaction of `i`'s first signature: 5 000 lamports fee.
    fn landed_tx(i: &Intent) -> Value {
        json!({"slot": 55, "blockTime": 123, "transaction": {
            "signatures": [i.signatures[0]],
            "message": {"header": {"numRequiredSignatures": 1, "numReadonlySignedAccounts": 0,
                "numReadonlyUnsignedAccounts": 0},
                "accountKeys": [i.wallet, solana_sdk::pubkey::Pubkey::new_unique().to_string()],
                "recentBlockhash": solana_sdk::hash::Hash::new_unique().to_string(),
                "instructions": []}},
            "meta": {"err": null, "status": {"Ok": null}, "fee": 5000,
                "preBalances": [100000, 0], "postBalances": [95000, 0],
                "preTokenBalances": [], "postTokenBalances": [],
                "loadedAddresses": {"writable": [], "readonly": []}}})
    }

    fn frames(f: &std::path::Path) -> usize {
        let mut r = crate::analytics::journal::Reader::open(f).unwrap();
        std::iter::from_fn(|| r.next_frame().unwrap()).count()
    }

    /// The journal after reserve and handoff of `n` bound signatures.
    fn handed_off(n: usize) -> (PathBuf, Journal, Intent) {
        let (path, a, mut i, mut c) = setup();
        i.signatures = (0..n)
            .map(|_| solana_sdk::signature::Signature::new_unique().to_string())
            .collect();
        c.bundle_simulated = n > 1;
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a, i.clone(), &c).unwrap();
        j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
        (path, j, i)
    }

    fn observe(
        j: &mut Journal,
        signature: &str,
        success: bool,
        sol: i128,
        mint: Option<(&str, i128)>,
    ) {
        let mut s = j.saved.clone().unwrap();
        s.observations.insert(
            signature.into(),
            FinalizedObservation {
                signature: signature.into(),
                slot: 500,
                success,
                fee_lamports: 5_000,
                wallet_sol_delta_lamports: sol,
                wallet_token_deltas: Some(
                    mint.map(|(m, d)| (m.to_string(), d)).into_iter().collect(),
                ),
                unknown_reasons: vec![],
            },
        );
        j.persist(s, Transition::Observe).unwrap();
    }

    /// The booking second of the tests' settlements.
    const BOOKED: u64 = 1_000;

    /// USD micro: 1 lamport = 0.1, 1 token base unit = 1 (positive and negative alike here).
    fn value(asset: &str, d: i128) -> Option<i128> {
        match asset {
            "SOL" => Some(d / 10),
            "MINT" => Some(d),
            _ => None,
        }
    }

    /// V2-01 (P7-01/P8-01): an incomplete finalized observation (unknown token metadata, an
    /// archive 429) is read again by the worker until it is complete, then the attempt settles;
    /// an identical answer adds nothing to the journal.
    #[test]
    fn v2_01_incomplete_observation_is_read_again_until_complete() {
        let (path, mut j, i) = handed_off(1);
        let s1 = i.signatures[0].clone();
        let complete = FinalizedObservation {
            signature: s1.clone(),
            slot: 500,
            success: true,
            fee_lamports: 5_000,
            wallet_sol_delta_lamports: -5_000,
            wallet_token_deltas: Some([("MINT".to_string(), 10)].into()),
            unknown_reasons: vec![],
        };
        let mut incomplete = complete.clone();
        incomplete.wallet_token_deltas = None;
        incomplete.unknown_reasons = vec!["ARCHIVE_429".into()];
        let ev = |o: &FinalizedObservation| SettlementEvidence {
            observations: vec![(s1.clone(), Ok(o.clone()))],
            non_landing: None,
        };
        let journal = path.join(format!("{}.p0j", i.wallet));
        let step = j.apply_settlement_evidence(ev(&incomplete), &value, BOOKED);
        assert!(matches!(step, Ok(SettleStep::Unknown(_))), "{step:?}");
        assert!(
            j.settlement_job().unwrap().1.complete.is_empty(),
            "an incomplete observation is read again"
        );
        let size = fs::metadata(&journal).unwrap().len();
        let _ = j.apply_settlement_evidence(ev(&incomplete), &value, BOOKED);
        assert_eq!(fs::metadata(&journal).unwrap().len(), size, "nothing new");
        let step = j.apply_settlement_evidence(ev(&complete), &value, BOOKED);
        assert!(matches!(step, Ok(SettleStep::Settled(_))), "{step:?}");
        fs::remove_dir_all(path).unwrap();
    }

    /// Review E3 (V2-01): with an observation stored (here incomplete) a failed re-read reads no
    /// non-landing evidence — it could never be accepted (an observation exists), so its B and
    /// C calls would only spend the Reconcile budget.
    #[test]
    fn v2_01_a_stored_observation_reads_no_non_landing_proof() {
        let (path, mut j, i) = handed_off(1);
        let s1 = i.signatures[0].clone();
        let incomplete = FinalizedObservation {
            signature: s1.clone(),
            slot: 500,
            success: true,
            fee_lamports: 5_000,
            wallet_sol_delta_lamports: -5_000,
            wallet_token_deltas: None,
            unknown_reasons: vec!["ARCHIVE_429".into()],
        };
        let evidence = SettlementEvidence {
            observations: vec![(s1.clone(), Ok(incomplete))],
            non_landing: None,
        };
        let step = j.apply_settlement_evidence(evidence, &value, BOOKED);
        assert!(matches!(step, Ok(SettleStep::Unknown(_))), "{step:?}");
        let (client, calls) = rpc(vec![]);
        let (sources, non_landing) = sources_of(4, None);
        let step = j.settle_step(&client, &sources, &value, BOOKED);
        assert!(matches!(step, Ok(SettleStep::Unknown(_))), "{step:?}");
        let calls = calls.lock().unwrap().clone();
        assert!(
            calls.iter().all(|m| m == "getTransaction"),
            "only the re-read: {calls:?}"
        );
        assert!(non_landing.lock().unwrap().is_empty(), "no B or C read");
        assert!(j.active_hold().is_some());
        fs::remove_dir_all(path).unwrap();
    }

    /// V2-01: complete evidence is never replaced by a weaker (incomplete) read of the same
    /// signature; in a bundle only the incomplete signature is read again.
    #[test]
    fn v2_01_complete_evidence_is_never_replaced_by_a_weaker_read() {
        let (path, mut j, i) = handed_off(2);
        let (s1, s2) = (i.signatures[0].clone(), i.signatures[1].clone());
        let obs = |sig: &str, complete: bool| FinalizedObservation {
            signature: sig.into(),
            slot: 500,
            success: true,
            fee_lamports: 5_000,
            wallet_sol_delta_lamports: -5_000,
            wallet_token_deltas: complete.then(|| [("MINT".to_string(), 5)].into()),
            unknown_reasons: if complete {
                vec![]
            } else {
                vec!["TOKEN_METADATA_UNKNOWN".into()]
            },
        };
        let ev = |sig: &str, o: FinalizedObservation| SettlementEvidence {
            observations: vec![(sig.to_string(), Ok(o))],
            non_landing: None,
        };
        let _ = j.apply_settlement_evidence(ev(&s1, obs(&s1, true)), &value, BOOKED);
        let _ = j.apply_settlement_evidence(ev(&s1, obs(&s1, false)), &value, BOOKED);
        assert!(j.saved.as_ref().unwrap().observations[&s1]
            .unknown_reasons
            .is_empty());
        assert_eq!(j.settlement_job().unwrap().1.complete, [s1.clone()].into());
        let step = j.apply_settlement_evidence(ev(&s2, obs(&s2, true)), &value, BOOKED);
        assert!(matches!(step, Ok(SettleStep::Settled(_))), "{step:?}");
        fs::remove_dir_all(path).unwrap();
    }

    /// C3: a landed success books its realized net from the finalized deltas of every own
    /// transaction once — durable across a restart, never twice; holds are released only then.
    #[test]
    fn landed_success_books_the_realized_net_once() {
        let (path, mut j, i) = handed_off(2);
        let (s1, s2) = (&i.signatures[0], &i.signatures[1]);
        observe(&mut j, s1, true, -5_000, Some(("MINT", -1_000)));
        observe(&mut j, s2, true, -15_000, Some(("MINT", 1_600)));
        assert_eq!(
            j.settle(&value, BOOKED).unwrap_err(),
            "NOT_FINALIZED_HOLD_RETAINED"
        );
        j.reconcile_observed().unwrap();
        let s = j.settle(&value, BOOKED).unwrap();
        assert!(s.landed && s.landed_success);
        assert_eq!(s.fee_lamports, 10_000);
        assert_eq!(s.wallet_deltas["SOL"], -20_000);
        assert_eq!(s.wallet_deltas["MINT"], 600);
        // The net token inflow is open inventory, not realized.
        assert_eq!(s.realized_net_usd_micro, -2_000);
        assert_eq!(
            (s.open_inventory["MINT"], s.unrealized_inventory_usd_micro),
            (600, 600)
        );
        assert!(j.active_hold().is_none() && j.holds().is_empty());
        // Idempotent: the same settlement, no second booking; after a restart as well.
        assert_eq!(j.settle(&|_, _| None, BOOKED).unwrap(), s);
        drop(j);
        let mut reopened = Journal::open(&path, &i.wallet).unwrap();
        assert_eq!(reopened.settle(&|_, _| None, BOOKED).unwrap(), s);
        assert!(
            reopened.reconcile_observed().is_err(),
            "no second finalization"
        );
        fs::remove_dir_all(path).unwrap();
    }

    /// C3: a landed failure books the paid fees as a realized loss.
    #[test]
    fn landed_failure_books_the_paid_fee_as_loss() {
        let (path, mut j, i) = handed_off(1);
        observe(&mut j, &i.signatures[0], false, -5_000, Some(("MINT", 0)));
        j.reconcile_observed().unwrap();
        let s = j.settle(&value, BOOKED).unwrap();
        assert!(s.landed && !s.landed_success);
        assert_eq!(s.realized_net_usd_micro, -500);
        fs::remove_dir_all(path).unwrap();
    }

    /// C3: UNKNOWN stays UNKNOWN — a partially observed bundle, an observation with unknown
    /// reasons or an unvalued delta (inventory without a proven exit) never settles; the hold
    /// and the attempt stay.
    #[test]
    fn incomplete_evidence_never_settles_and_keeps_the_hold() {
        let (path, mut j, i) = handed_off(2);
        let (s1, s2) = (&i.signatures[0], &i.signatures[1]);
        observe(&mut j, s1, true, -5_000, Some(("MINT", 10)));
        assert_eq!(
            j.reconcile_observed().unwrap_err(),
            "SETTLEMENT_SIGNATURE_UNOBSERVED_HOLD_RETAINED",
            "partial bundle"
        );
        let mut s = j.saved.clone().unwrap();
        s.observations.get_mut(s1).unwrap().unknown_reasons = vec!["ARCHIVE_429".into()];
        j.persist(s, Transition::Observe).unwrap();
        observe(&mut j, s2, true, -5_000, Some(("MINT", 10)));
        assert_eq!(
            j.reconcile_observed().unwrap_err(),
            "SETTLEMENT_OBSERVATION_UNKNOWN_HOLD_RETAINED"
        );
        assert!(j.active_hold().is_some(), "UNKNOWN keeps the hold");
        observe(&mut j, s1, true, -5_000, Some(("OTHER", 10)));
        j.reconcile_observed().unwrap();
        assert_eq!(
            j.settle(&value, BOOKED).unwrap_err(),
            "SETTLEMENT_VALUE_UNKNOWN:OTHER",
            "inventory without a proven exit"
        );
        assert_eq!(j.saved.as_ref().unwrap().phase, Phase::HaltFinalized);
        let wrong_sign = |_: &str, d: i128| Some(-d);
        assert!(j
            .settle(&wrong_sign, BOOKED)
            .unwrap_err()
            .starts_with("SETTLEMENT_VALUE_SIGN"));
        fs::remove_dir_all(path).unwrap();
    }

    /// C3: a receipt that is not finalized (a reorg can still remove it) or a duplicated receipt
    /// event never finalizes; the attempt stays UNKNOWN with its hold.
    #[test]
    fn unfinalized_or_duplicated_receipts_never_finalize() {
        let (path, mut j, i) = handed_off(1);
        let receipt = |finalized| Receipt {
            signature: i.signatures[0].clone(),
            finalized,
            success: true,
            fee_lamports: Some(5_000),
            wallet_deltas: Some([("SOL".to_string(), -5_000)].into_iter().collect()),
            exit_complete: true,
        };
        let held = "RECONCILIATION_INCOMPLETE_HOLD_RETAINED";
        assert_eq!(
            j.reconcile(vec![receipt(false)]).unwrap_err(),
            held,
            "confirmed, not finalized"
        );
        assert_eq!(
            j.reconcile(vec![receipt(true), receipt(true)]).unwrap_err(),
            held,
            "duplicated receipt event"
        );
        assert!(j.active_hold().is_some_and(|h| h.unknown_outcome));
        assert_eq!(
            j.settle(&value, BOOKED).unwrap_err(),
            "NOT_FINALIZED_HOLD_RETAINED"
        );
        fs::remove_dir_all(path).unwrap();
    }

    /// C3/DEC-03: a non-landing releases only through a read of exactly this intent that proves
    /// it (B+C or C alone, nothing observed); then it settles at zero (nothing paid) with its
    /// class persisted. A read of another subject, a read that does not prove it, an intent
    /// without its expiry or a stored observation keeps the hold.
    #[test]
    fn not_landed_settles_only_with_a_proving_read() {
        let (path, mut j, i) = handed_off(2);
        let subject = SettlementSubject::from(&i).non_landing().unwrap();
        let reject = "NON_LANDING_NOT_PROVEN_HOLD_RETAINED";
        let other = |f: fn(&mut super::Subject)| {
            let mut s = subject.clone();
            f(&mut s);
            loopback::proving(&s, true)
        };
        let mut incomplete = loopback::proving(&subject, true);
        incomplete.c = super::super::non_landing::CResult::Incomplete(
            super::super::non_landing::CIncomplete::Gap,
        );
        for bad in [
            other(|s| s.last_valid_block_height -= 1),
            other(|s| s.state_slot += 1),
            other(|s| {
                s.signatures.pop();
            }),
            incomplete,
        ] {
            assert_eq!(j.reconcile_not_landed(&bad).unwrap_err(), reject);
        }
        assert!(j.active_hold().is_some());
        // A schema-2 intent (no recorded expiry) is never proven absent.
        let mut old = j.saved.clone().unwrap();
        old.intent.last_valid_block_height = None;
        let saved = j.saved.replace(old);
        assert_eq!(
            j.reconcile_not_landed(&loopback::proving(&subject, true))
                .unwrap_err(),
            reject
        );
        j.saved = saved;
        j.reconcile_not_landed(&loopback::proving(&subject, false))
            .unwrap();
        let s = j.settle(&value, BOOKED).unwrap();
        assert_eq!(
            (s.landed, s.realized_net_usd_micro, s.fee_lamports),
            (false, 0, 0)
        );
        assert!(j.active_hold().is_none());
        assert_eq!(
            j.saved
                .as_ref()
                .unwrap()
                .non_landing
                .as_ref()
                .unwrap()
                .class,
            Verdict::CAloneQualified
        );
        // A landed signature can never be proven absent.
        let (path2, mut k, i2) = handed_off(1);
        observe(&mut k, &i2.signatures[0], true, -5_000, Some(("MINT", 1)));
        let subject2 = SettlementSubject::from(&i2).non_landing().unwrap();
        assert_eq!(
            k.reconcile_not_landed(&loopback::proving(&subject2, true))
                .unwrap_err(),
            reject
        );
        fs::remove_dir_all(path).unwrap();
        fs::remove_dir_all(path2).unwrap();
    }

    /// DEC-03 (N-2 §7) through the settlement step (loopback reconciliation RPC and sources):
    /// two B and C negative ⇒ `B+C`, settled with nothing paid, the class persisted (a restart
    /// reads it), the repeated read changes nothing; a B source rate limited and one
    /// unreachable, C complete ⇒ C alone; C incomplete ⇒ UNKNOWN, hold kept; a B sighting
    /// without a landing ⇒ open contradiction, hold kept; a C sighting is a veto until the
    /// receipts confirm the landing, which settles through the regular reconciliation; a landing
    /// with incomplete deltas stays UNKNOWN with the hold.
    #[test]
    fn dec03_n2_matrix_through_the_settlement_step() {
        use loopback::{chain, closed, endpoint, Reply};
        let class = |j: &Journal| j.non_landing_status(None).unwrap().class;
        let unknown = |step: Result<SettleStep, String>| match step {
            Ok(SettleStep::Unknown(reasons)) => reasons,
            other => panic!("UNKNOWN expected: {other:?}"),
        };
        let (client, _) = rpc(vec![]);
        // Two B negative, C negative.
        let (path, mut j, i) = handed_off(1);
        let (sources, _) = sources_of(4, None);
        let SettleStep::Settled(s) = j.settle_step(&client, &sources, &value, BOOKED).unwrap()
        else {
            panic!("settled");
        };
        assert!(!s.landed && j.active_hold().is_none());
        assert_eq!(class(&j), Some(Verdict::BcConfirmed));
        let file = path_of(&j);
        let before = frames(&file);
        assert_eq!(
            j.settle_step(&client, &sources, &value, BOOKED),
            Ok(SettleStep::Settled(s))
        );
        assert_eq!(frames(&file), before, "no second settlement");
        drop(j);
        let reopened = Journal::open(&path, &i.wallet).unwrap();
        assert_eq!(class(&reopened), Some(Verdict::BcConfirmed));
        // One B rate limited, one unreachable, C complete and negative.
        let (path2, mut k, _) = handed_off(1);
        let (c_url, _) = endpoint(chain(50, 4, 58, 0, None));
        let (limited, _) = endpoint(Arc::new(|_, _| Reply::Status(429)));
        let alone = loopback::sources(c_url, vec![("op-a", limited), ("op-b", closed())]);
        assert!(matches!(
            k.settle_step(&client, &alone, &value, BOOKED),
            Ok(SettleStep::Settled(_))
        ));
        assert_eq!(class(&k), Some(Verdict::CAloneQualified));
        // C incomplete (the chain not beyond the expiry).
        let (path3, mut l, _) = handed_off(1);
        let (waiting, _) = sources_of(3, None);
        let reasons = unknown(l.settle_step(&client, &waiting, &value, BOOKED));
        assert!(
            reasons.contains(&"NON_LANDING_UNKNOWN_INCOMPLETE".to_string())
                && reasons.contains(&"NON_LANDING_C:C_NOT_EXPIRED".to_string()),
            "{reasons:?}"
        );
        assert!(l.active_hold().is_some_and(|h| h.unknown_outcome));
        assert_eq!(class(&l), Some(Verdict::UnknownIncomplete));
        // A B source reports a bound signature finalized, C negative, no landing observed.
        let (path4, mut m, _) = handed_off(1);
        let base = chain(50, 4, 58, 0, None);
        let (seeing, _) = endpoint(Arc::new(move |method, params| {
            if method == "getSignatureStatuses" {
                Reply::Result(json!({"context": {"slot": 1}, "value": [{"slot": 52,
                    "confirmations": null, "err": null, "confirmationStatus": "finalized"}]}))
            } else {
                base(method, params)
            }
        }));
        let (c_url, _) = endpoint(chain(50, 4, 58, 0, None));
        let vetoed = loopback::sources(c_url.clone(), vec![("op-a", seeing), ("op-b", c_url)]);
        let reasons = unknown(m.settle_step(&client, &vetoed, &value, BOOKED));
        assert_eq!(
            reasons[1], "NON_LANDING_VETO_CONTRADICTION_OPEN",
            "{reasons:?}"
        );
        assert!(m.active_hold().is_some());
        // C finds the signature: a veto while the receipts see nothing, then the landing is
        // observed complete and settles through the regular reconciliation.
        let (path5, mut n, i5) = handed_off(1);
        let (c_seen, _) = sources_of(4, Some((52, i5.signatures[0].clone())));
        let reasons = unknown(n.settle_step(&client, &c_seen, &value, BOOKED));
        assert_eq!(
            reasons[1], "NON_LANDING_VETO_CONTRADICTION_OPEN",
            "{reasons:?}"
        );
        assert!(reasons.contains(&"NON_LANDING_C:SIGHTING".to_string()));
        let (receipts, _) = rpc(vec![("getTransaction", landed_tx(&i5))]);
        let SettleStep::Settled(s) = n.settle_step(&receipts, &c_seen, &value, BOOKED).unwrap()
        else {
            panic!("landed");
        };
        assert!(s.landed && n.active_hold().is_none());
        assert_eq!(class(&n), Some(Verdict::VetoLandingConfirmed));
        // The landing observed with incomplete deltas: UNKNOWN, the hold stays.
        let (path6, mut o, i6) = handed_off(1);
        let partial = FinalizedObservation {
            signature: i6.signatures[0].clone(),
            slot: 52,
            success: true,
            fee_lamports: 5_000,
            wallet_sol_delta_lamports: -5_000,
            wallet_token_deltas: None,
            unknown_reasons: vec!["TOKEN_BALANCE_METADATA_MISSING".into()],
        };
        let evidence = SettlementEvidence {
            observations: vec![(i6.signatures[0].clone(), Ok(partial))],
            non_landing: None,
        };
        unknown(o.apply_settlement_evidence(evidence, &value, BOOKED));
        assert!(o.active_hold().is_some());
        assert_eq!(class(&o), Some(Verdict::VetoContradiction));
        drop((reopened, k, l, m, n, o));
        for p in [path, path2, path3, path4, path5, path6] {
            fs::remove_dir_all(p).unwrap();
        }
    }

    /// A loopback JSON-RPC endpoint answering each method with its scripted `result` (an
    /// unscripted method gets an error); the methods in call order.
    fn rpc(
        answers: Vec<(&'static str, Value)>,
    ) -> (
        solana_client::rpc_client::RpcClient,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = calls.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let (answers, log) = (answers.clone(), log.clone());
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut stream = stream;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let mut length = 0usize;
                        loop {
                            let mut h = String::new();
                            if reader.read_line(&mut h).unwrap_or(0) == 0 {
                                return;
                            }
                            if h == "\r\n" {
                                break;
                            }
                            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:")
                            {
                                length = v.trim().parse().unwrap_or(0);
                            }
                        }
                        let mut body = vec![0u8; length];
                        if reader.read_exact(&mut body).is_err() {
                            return;
                        }
                        let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                        let method = request["method"].as_str().unwrap_or("").to_string();
                        log.lock().unwrap().push(method.clone());
                        let answer = match answers.iter().find(|(m, _)| *m == method) {
                            Some((_, r)) => {
                                json!({"jsonrpc": "2.0", "id": request["id"], "result": r})
                            }
                            None => json!({"jsonrpc": "2.0", "id": request["id"],
                                "error": {"code": -32601, "message": "unscripted"}}),
                        }
                        .to_string();
                        let _ = stream.write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                                answer.len(),
                                answer
                            )
                            .as_bytes(),
                        );
                    }
                });
            }
        });
        (solana_client::rpc_client::RpcClient::new(url), calls)
    }

    /// C3: the settlement step through the reconciliation RPC and the non-landing sources
    /// (loopback). Never landed (no finalized transaction; two B operators and C negative beyond
    /// the expiry) books nothing paid and releases the hold; still waiting (the chain not beyond
    /// the expiry) stays UNKNOWN with its reasons, keeps the hold and writes no frame; a
    /// finalized landing books its deltas once.
    #[test]
    fn c3_settle_step_books_landed_and_not_landed_and_keeps_unknown() {
        let (path, mut j, _) = handed_off(1);
        let (client, _) = rpc(vec![]);
        let (sources, _) = sources_of(4, None);
        let none = Settlement {
            landed: false,
            landed_success: false,
            fee_lamports: 0,
            wallet_deltas: Default::default(),
            valuation: Default::default(),
            realized_net_usd_micro: 0,
            open_inventory: Default::default(),
            unrealized_inventory_usd_micro: 0,
            booked_unix_s: Some(BOOKED),
        };
        assert_eq!(
            j.settle_step(&client, &sources, &value, BOOKED),
            Ok(SettleStep::Settled(none.clone()))
        );
        assert!(j.active_hold().is_none());
        assert_eq!(
            j.settle_step(&client, &sources, &value, BOOKED),
            Ok(SettleStep::Settled(none))
        );

        let (path2, mut k, _) = handed_off(1);
        let file = path_of(&k);
        let before = frames(&file);
        let (waiting, _) = sources_of(3, None);
        let SettleStep::Unknown(reasons) =
            k.settle_step(&client, &waiting, &value, BOOKED).unwrap()
        else {
            panic!("still waiting");
        };
        assert!(
            reasons
                .iter()
                .any(|r| r.ends_with("FINALIZED_TRANSACTION_UNAVAILABLE"))
                && reasons.iter().any(|r| r == "NON_LANDING_C:C_NOT_EXPIRED")
                && reasons
                    .iter()
                    .any(|r| r == "NON_LANDING_B:op-a:UNUSABLE_NOT_EXPIRED"),
            "{reasons:?}"
        );
        assert!(k.active_hold().is_some_and(|h| h.unknown_outcome));
        assert_eq!(frames(&file), before, "a waiting step writes no frame");

        let (path3, mut l, i3) = handed_off(1);
        let (client, _) = rpc(vec![("getTransaction", landed_tx(&i3))]);
        let SettleStep::Settled(s) = l.settle_step(&client, &sources, &value, BOOKED).unwrap()
        else {
            panic!("landed");
        };
        assert!(s.landed && s.landed_success);
        assert_eq!((s.fee_lamports, s.wallet_deltas["SOL"]), (5_000, -5_000));
        assert_eq!(s.realized_net_usd_micro, -500);
        assert!(l.active_hold().is_none());
        for p in [path, path2, path3] {
            fs::remove_dir_all(p).unwrap();
        }
    }

    /// C3: realized is the SOL delta and every token outflow; a token inflow (collateral or
    /// surplus left in the wallet) is open inventory at its conservative mark — never realized,
    /// whatever its value; the booking second is persisted with the settlement.
    #[test]
    fn c3_token_inflow_is_open_inventory_never_realized() {
        for (mint, realized, open) in [(1_000, -500, Some(1_000)), (-1_000, -1_500, None)] {
            let (path, mut j, i) = handed_off(1);
            observe(&mut j, &i.signatures[0], true, -5_000, Some(("MINT", mint)));
            j.reconcile_observed().unwrap();
            let s = j.settle(&value, BOOKED).unwrap();
            assert_eq!(s.realized_net_usd_micro, realized, "{mint}");
            assert_eq!(s.open_inventory.get("MINT").copied(), open);
            assert_eq!(s.unrealized_inventory_usd_micro, open.unwrap_or(0));
            assert_eq!(s.booked_unix_s, Some(BOOKED));
            drop(j);
            let reopened = Journal::open(&path, &i.wallet).unwrap();
            let replay = reopened.safety_replay().unwrap().settled.unwrap();
            assert_eq!(
                (
                    replay.unix_s,
                    replay.realized_net_usd_micro,
                    replay.open_inventory_usd_micro
                ),
                (BOOKED, realized, open.unwrap_or(0))
            );
            drop(reopened);
            fs::remove_dir_all(path).unwrap();
        }
    }

    /// FE-04/C3: a gain is valued at its lower bound, a loss at its upper bound; an unknown
    /// price or an amount beyond `u64` stays UNKNOWN.
    #[test]
    fn settlement_values_gains_low_and_losses_high() {
        let low = |a: &str, n: u64| (a == "MINT").then_some(n * 9 / 10);
        let high = |a: &str, n: u64| (a == "MINT").then_some(n * 11 / 10);
        let v = |d| super::conservative_value(&low, &high, "MINT", d);
        assert_eq!(
            (v(1_000), v(-1_000), v(0)),
            (Some(900), Some(-1_100), Some(0))
        );
        assert_eq!(super::conservative_value(&low, &high, "OTHER", 5), None);
        assert_eq!(v(i128::from(u64::MAX) + 1), None);
    }
}
