//! [FIX-5] Reconciliation offener Submissions → `Pipeline::reconcile_outcome` (Safety-Buchhaltung).
//! Jede Submission wird genau einmal abgeschlossen: Landed (Erfolg/Fehler) oder nach Timeout NotLanded.
//! Realisierter PnL ist erst mit Token-Deltas (P8-05) bekannt; bis dahin wird nur die bekannte
//! Gebühr als negativer Anteil gebucht und `pnl_complete=false` im Record geführt — nie 0 erfinden.

use darkbot_solana::submit::jito::JitoAdapter;
use darkbot_solana::submit::rpc::reconcile;
use darkbot_solana::submit::{LandingEvidence, LandingOutcome, SubmissionPath};
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signature;

#[derive(Clone, Debug)]
pub struct PendingSubmission {
    /// Sentinel (Single: die TX; Bundle: Receivership-TX).
    pub signature: Signature,
    /// [FIX-18] alle Signaturen (Bundle: prereqs…, receivership, tip) für vollständige Gebühren.
    pub signatures: Vec<Signature>,
    pub bundle_id: Option<String>,
    pub path: SubmissionPath,
    pub exposure: u64,
    pub submitted_unix_s: u64,
    pub tip: Option<SubmittedTip>,
}

/// Only constructed from our signed, single-transfer tip transaction.
#[derive(Clone, Debug, Serialize)]
pub struct SubmittedTip {
    pub signature: String,
    pub lamports: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReconcileRecord {
    pub signature: String,
    pub path: &'static str,
    pub outcome: String,
    pub landed_slot: Option<u64>,
    pub fee_lamports: Option<u64>,
    pub compute_units_consumed: Option<u64>,
    pub paid_tip_lamports: Option<u64>,
    pub network_and_tip_lamports: Option<u64>,
    pub error: Option<String>,
    pub exposure_released: u64,
    pub pnl_complete: bool,
}

pub struct ReconcileResult {
    pub record: ReconcileRecord,
    pub landed_success: bool,
    /// [FIX-24] None = UNKNOWN (fehlende Fee oder Token-Deltas) — nie 0.
    pub realized_pnl: Option<i128>,
}

/// Aggregate only identified, actually landed transactions. Missing signatures,
/// metadata or unresolved bundle members keep the complete attempt UNKNOWN.
/// An inflight Failed response or wall-clock timeout is never absence proof.
pub fn measured_attempt_costs(p: &PendingSubmission, evidence: &[LandingEvidence]) -> (Option<u64>, Option<u64>) {
    let mut signatures = p.signatures.clone();
    signatures.sort_unstable(); signatures.dedup();
    if signatures.is_empty() || !signatures.contains(&p.signature) { return (None, None); }
    let fee = signatures.iter().try_fold(0u64, |total, sig| {
        let e = evidence.iter().find(|e| e.signature == *sig && e.path == p.path)?;
        if e.outcome == LandingOutcome::NotLanded { return None; }
        total.checked_add(e.fee_lamports?)
    });
    let tip = match (&p.tip, p.path) {
        (None, SubmissionPath::Rpc) => Some(0),
        (Some(tip), SubmissionPath::JitoBundle | SubmissionPath::JitoSingle) => {
            tip.signature.parse::<Signature>().ok().filter(|s| signatures.contains(s))
                .and_then(|s| evidence.iter().find(|e| e.signature == s && e.path == p.path))
                .and_then(|e| match e.outcome {
                    LandingOutcome::LandedSuccess if e.fee_lamports.is_some() => Some(tip.lamports),
                    LandingOutcome::LandedFailed if e.fee_lamports.is_some() => Some(0),
                    _ => None,
                })
        }
        _ => None,
    };
    (fee, tip)
}

/// None = retry. Timeout yields unresolved UNKNOWN, never a fictitious zero PnL.
pub fn try_reconcile(rpc: &RpcClient, _jito: Option<&JitoAdapter>, p: &PendingSubmission, now_unix_s: u64, now_ns: u64, timeout_s: u64) -> Option<ReconcileResult> {
    let mut signatures = p.signatures.clone();
    if !signatures.contains(&p.signature) { signatures.push(p.signature); }
    signatures.sort_unstable(); signatures.dedup();
    let evidence: Vec<_> = signatures.iter().map(|s| reconcile(rpc, p.path, s, now_ns))
        .collect::<Result<_, _>>().ok()?;
    let ev = evidence.iter().find(|e| e.signature == p.signature)?;
    let unresolved = evidence.iter().any(|e| e.outcome == LandingOutcome::NotLanded || e.fee_lamports.is_none());
    if unresolved && now_unix_s.saturating_sub(p.submitted_unix_s) < timeout_s { return None; }
    let landed_success = !unresolved && evidence.iter().all(|e| e.outcome == LandingOutcome::LandedSuccess);
    let outcome = if unresolved { "UNRESOLVED" } else if landed_success { "LANDED_SUCCESS" } else { "LANDED_FAILED" };
    let (fee, tip) = measured_attempt_costs(p, &evidence);
    Some(ReconcileResult {
        record: ReconcileRecord {
            signature: p.signature.to_string(), path: p.path.as_str(), outcome: outcome.into(),
            landed_slot: ev.landed_slot, fee_lamports: fee,
            compute_units_consumed: ev.compute_units_consumed,
            paid_tip_lamports: tip,
            network_and_tip_lamports: fee.and_then(|f| f.checked_add(tip?)),
            error: if unresolved { Some("absence/lifecycle or fee evidence incomplete".into()) } else { ev.error.clone() },
            exposure_released: p.exposure, pnl_complete: false,
        },
        landed_success,
        // Complete execution PnL additionally requires token/protocol/rent deltas.
        realized_pnl: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(path: SubmissionPath, outcome: LandingOutcome, fee: Option<u64>) -> (PendingSubmission, LandingEvidence) {
        let sig = Signature::new_unique();
        (PendingSubmission { signature:sig, signatures:vec![sig], bundle_id:None, path,
            exposure:0, submitted_unix_s:0, tip:None },
         LandingEvidence { path, signature:sig, outcome, landed_slot:Some(10), tx_index_in_slot:None,
            confirmation:Some("finalized".into()), fee_lamports:fee, compute_units_consumed:Some(123),
            error:None, t_observed_landed_ns:Some(1) })
    }
    #[test]
    fn recorded_mainnet_success_and_revert_fees_are_preserved() {
        // The third-party revert's submission path is unknown. This test validates
        // the fee reducer with real meta values, not attribution to our RPC path.
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../docs/evidence/FIX129_fee_sources.json")).unwrap();
        for transaction in [&fixture["landed_ata_transaction"], &fixture["third_party_landed_revert"]["transaction"]] {
            let fee = transaction["meta"]["fee"].as_u64().unwrap();
            let outcome = if transaction["meta"]["err"].is_null() { LandingOutcome::LandedSuccess } else { LandingOutcome::LandedFailed };
            let (p, e) = sample(SubmissionPath::Rpc, outcome, Some(fee));
            assert_eq!(measured_attempt_costs(&p, &[e]).0, Some(fee));
        }
    }

    #[test]
    fn landed_success_and_revert_use_meta_fee() {
        for outcome in [LandingOutcome::LandedSuccess, LandingOutcome::LandedFailed] {
            let (p,e) = sample(SubmissionPath::Rpc, outcome, Some(7123));
            assert_eq!(measured_attempt_costs(&p, &[e]), (Some(7123), Some(0)));
        }
    }
    #[test]
    fn absence_and_missing_metadata_never_invent_fee() {
        for outcome in [LandingOutcome::NotLanded, LandingOutcome::LandedFailed] {
            let (p,e) = sample(SubmissionPath::Rpc, outcome, None);
            assert_eq!(measured_attempt_costs(&p, &[e]).0, None);
        }
        let (p,e) = sample(SubmissionPath::JitoBundle, LandingOutcome::NotLanded, None);
        assert_eq!(measured_attempt_costs(&p, &[e]), (None,None));
    }
    #[test]
    fn tip_requires_our_matching_landed_transfer_and_partial_bundle_is_unknown() {
        let (mut p, e) = sample(SubmissionPath::JitoBundle, LandingOutcome::LandedSuccess, Some(6000));
        let (_,mut t) = sample(SubmissionPath::JitoBundle, LandingOutcome::LandedSuccess, Some(5000));
        p.signatures.push(t.signature);
        p.tip = Some(SubmittedTip { signature:t.signature.to_string(), lamports:1234 });
        assert_eq!(measured_attempt_costs(&p, &[e.clone(),t.clone()]), (Some(11000),Some(1234)));
        t.outcome = LandingOutcome::LandedFailed;
        assert_eq!(measured_attempt_costs(&p, &[e.clone(),t.clone()]), (Some(11000),Some(0)));
        t.outcome = LandingOutcome::NotLanded; t.fee_lamports=None;
        assert_eq!(measured_attempt_costs(&p, &[e.clone(),t]), (None,None));
        assert_eq!(measured_attempt_costs(&p, &[e]), (None,None));
    }
}
