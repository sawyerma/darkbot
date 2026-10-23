//! C3/P8-05: finalized settlement of the one durable FE-04 attempt, in the same wallet journal
//! (no second financial store). Receipts come from the durable finalized observations of every
//! bound signature (`reconcile_observed`) or a strict non-landing proof (`reconcile_not_landed`);
//! the realized i128 net PnL is booked exactly once as a journal transition (`Phase::Settled`).
//! An incomplete bundle, an observation with unknown reasons, a missing proof or any delta without
//! a conservative value (inventory whose exit is not proven) stays UNKNOWN: holds and HALT remain.
use super::{err, Journal, Phase, Receipt, Transition};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The booked outcome of one attempt (USD micro where named so; lamports and base units else).
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
    pub realized_net_usd_micro: i128,
}

/// A strict non-landing proof: from a finalized context past the attempt's validity window
/// (`Intent.valid_until_slot`), every bound signature is absent from the finalized history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonLandingProof {
    pub finalized_slot: u64,
    pub absent_signatures: BTreeSet<String>,
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
                exit_complete: true,
            });
        }
        self.reconcile(receipts)
    }

    /// Finalizes a proven non-landing: nothing landed, nothing was paid.
    pub fn reconcile_not_landed(&mut self, proof: &NonLandingProof) -> Result<(), String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        if s.phase != Phase::HandoffUnknown {
            return Err(err("NOT_UNKNOWN"));
        }
        let bound: BTreeSet<String> = s.intent.signatures.iter().cloned().collect();
        if proof.finalized_slot <= s.intent.valid_until_slot
            || proof.absent_signatures != bound
            || !s.observations.is_empty()
        {
            return Err(err("NON_LANDING_NOT_PROVEN_HOLD_RETAINED"));
        }
        s.phase = Phase::HaltFinalized;
        s.finalized = Some(Vec::new());
        self.persist(s, Transition::NotLanded)
    }

    /// Books the realized net PnL of a finalized attempt exactly once. `value(asset, delta)` is
    /// the conservative USD-micro value of a signed delta (`None`: not valued, e.g. inventory
    /// without a proven exit); a booked settlement is returned unchanged, never booked twice.
    pub fn settle(
        &mut self,
        value: &dyn Fn(&str, i128) -> Option<i128>,
    ) -> Result<Settlement, String> {
        let s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        let receipts = match &s.phase {
            Phase::Settled(done) => return Ok(done.clone()),
            Phase::HaltFinalized => s.finalized.clone().ok_or_else(|| err("NOT_FINALIZED"))?,
            _ => return Err(err("NOT_FINALIZED_HOLD_RETAINED")),
        };
        let settlement = compute(&receipts, value)?;
        let mut next = s;
        next.phase = Phase::Settled(settlement.clone());
        self.persist(next, Transition::Settle)?;
        Ok(settlement)
    }
}

fn compute(
    receipts: &[Receipt],
    value: &dyn Fn(&str, i128) -> Option<i128>,
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
    let mut net: i128 = 0;
    for (asset, d) in deltas.iter().filter(|(_, d)| **d != 0) {
        let v = value(asset, *d).ok_or_else(|| format!("SETTLEMENT_VALUE_UNKNOWN:{asset}"))?;
        if v.signum() * d.signum() < 0 {
            return Err(format!("SETTLEMENT_VALUE_SIGN:{asset}"));
        }
        net = net.checked_add(v).ok_or_else(overflow)?;
        valuation.insert(asset.clone(), v);
    }
    Ok(Settlement {
        landed: !receipts.is_empty(),
        landed_success: !receipts.is_empty() && receipts.iter().all(|r| r.success),
        fee_lamports: fee,
        wallet_deltas: deltas,
        valuation,
        realized_net_usd_micro: net,
    })
}

#[cfg(test)]
mod tests {
    use super::super::receipts::FinalizedObservation;
    use super::super::tests::setup;
    use super::super::*;
    use super::NonLandingProof;
    use std::fs;
    use std::path::PathBuf;

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

    /// USD micro: 1 lamport = 0.1, 1 token base unit = 1 (positive and negative alike here).
    fn value(asset: &str, d: i128) -> Option<i128> {
        match asset {
            "SOL" => Some(d / 10),
            "MINT" => Some(d),
            _ => None,
        }
    }

    /// C3: a landed success books its realized net from the finalized deltas of every own
    /// transaction once — durable across a restart, never twice; holds are released only then.
    #[test]
    fn landed_success_books_the_realized_net_once() {
        let (path, mut j, i) = handed_off(2);
        let (s1, s2) = (&i.signatures[0], &i.signatures[1]);
        observe(&mut j, s1, true, -5_000, Some(("MINT", -1_000)));
        observe(&mut j, s2, true, -15_000, Some(("MINT", 1_600)));
        assert_eq!(j.settle(&value).unwrap_err(), "NOT_FINALIZED_HOLD_RETAINED");
        j.reconcile_observed().unwrap();
        let s = j.settle(&value).unwrap();
        assert!(s.landed && s.landed_success);
        assert_eq!(s.fee_lamports, 10_000);
        assert_eq!(s.wallet_deltas["SOL"], -20_000);
        assert_eq!(s.wallet_deltas["MINT"], 600);
        assert_eq!(s.realized_net_usd_micro, 600 - 2_000);
        assert!(j.active_hold().is_none() && j.holds().is_empty());
        // Idempotent: the same settlement, no second booking; after a restart as well.
        assert_eq!(j.settle(&|_, _| None).unwrap(), s);
        drop(j);
        let mut reopened = Journal::open(&path, &i.wallet).unwrap();
        assert_eq!(reopened.settle(&|_, _| None).unwrap(), s);
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
        let s = j.settle(&value).unwrap();
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
            j.settle(&value).unwrap_err(),
            "SETTLEMENT_VALUE_UNKNOWN:OTHER",
            "inventory without a proven exit"
        );
        assert_eq!(j.saved.as_ref().unwrap().phase, Phase::HaltFinalized);
        let wrong_sign = |_: &str, d: i128| Some(-d);
        assert!(j
            .settle(&wrong_sign)
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
        assert_eq!(j.settle(&value).unwrap_err(), "NOT_FINALIZED_HOLD_RETAINED");
        fs::remove_dir_all(path).unwrap();
    }

    /// C3: a non-landing needs a finalized context past the validity window and every bound
    /// signature absent; then it settles at zero (nothing paid). Anything less keeps the hold.
    #[test]
    fn not_landed_settles_only_with_a_strict_proof() {
        let (path, mut j, i) = handed_off(2);
        let proof = |slot, sigs: &[String]| NonLandingProof {
            finalized_slot: slot,
            absent_signatures: sigs.iter().cloned().collect(),
        };
        let reject = "NON_LANDING_NOT_PROVEN_HOLD_RETAINED";
        let all = &i.signatures[..];
        assert_eq!(
            j.reconcile_not_landed(&proof(i.valid_until_slot, all))
                .unwrap_err(),
            reject
        );
        assert_eq!(
            j.reconcile_not_landed(&proof(i.valid_until_slot + 1, &all[..1]))
                .unwrap_err(),
            reject
        );
        assert!(j.active_hold().is_some());
        j.reconcile_not_landed(&proof(i.valid_until_slot + 1, all))
            .unwrap();
        let s = j.settle(&value).unwrap();
        assert_eq!(
            (s.landed, s.realized_net_usd_micro, s.fee_lamports),
            (false, 0, 0)
        );
        assert!(j.active_hold().is_none());
        // A landed signature can never be proven absent.
        let (path2, mut k, i2) = handed_off(1);
        observe(&mut k, &i2.signatures[0], true, -5_000, Some(("MINT", 1)));
        assert_eq!(
            k.reconcile_not_landed(&proof(i2.valid_until_slot + 1, &i2.signatures))
                .unwrap_err(),
            reject
        );
        fs::remove_dir_all(path).unwrap();
        fs::remove_dir_all(path2).unwrap();
    }
}
