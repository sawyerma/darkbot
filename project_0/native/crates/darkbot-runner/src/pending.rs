//! [FIX-5] Reconciliation offener Submissions → `Pipeline::reconcile_outcome` (Safety-Buchhaltung).
//! Jede Submission wird genau einmal abgeschlossen: Landed (Erfolg/Fehler) oder nach Timeout NotLanded.
//! Realisierter PnL ist erst mit Token-Deltas (P8-05) bekannt; bis dahin wird nur die bekannte
//! Gebühr als negativer Anteil gebucht und `pnl_complete=false` im Record geführt — nie 0 erfinden.

use darkbot_solana::submit::jito::{BundleStatus, JitoAdapter};
use darkbot_solana::submit::rpc::reconcile;
use darkbot_solana::submit::{LandingOutcome, SubmissionPath};
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
}

#[derive(Clone, Debug, Serialize)]
pub struct ReconcileRecord {
    pub signature: String,
    pub path: &'static str,
    pub outcome: String,
    pub landed_slot: Option<u64>,
    pub fee_lamports: Option<u64>,
    pub compute_units_consumed: Option<u64>,
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

/// Prüft eine Submission. `None` = noch offen (weiter warten).
pub fn try_reconcile(rpc: &RpcClient, jito: Option<&JitoAdapter>, p: &PendingSubmission, now_unix_s: u64, now_ns: u64, timeout_s: u64) -> Option<ReconcileResult> {
    // [FIX-33] Bundle-ID nutzen: Inflight `Failed` beendet das Warten sofort; on-chain bleibt authoritative.
    if let (Some(j), Some(id)) = (jito, p.bundle_id.as_deref()) {
        if matches!(j.inflight_status(id), Ok(BundleStatus::Failed { .. })) {
            return Some(ReconcileResult { record: ReconcileRecord { signature: p.signature.to_string(), path: p.path.as_str(), outcome: "NOT_LANDED".into(), landed_slot: None, fee_lamports: None, compute_units_consumed: None, error: Some("bundle failed (inflight)".into()), exposure_released: p.exposure, pnl_complete: true }, landed_success: false, realized_pnl: Some(0) });
        }
    }
    let ev = match reconcile(rpc, p.path, &p.signature, now_ns) {
        Ok(ev) => ev,
        Err(_) => return None, // Transport: später erneut
    };
    let timed_out = now_unix_s.saturating_sub(p.submitted_unix_s) >= timeout_s;
    let (outcome, landed_success) = match ev.outcome {
        LandingOutcome::LandedSuccess => ("LANDED_SUCCESS", true),
        LandingOutcome::LandedFailed => ("LANDED_FAILED", false),
        LandingOutcome::NotLanded if timed_out => ("NOT_LANDED", false),
        LandingOutcome::NotLanded => return None,
    };
    // [FIX-18] Gebühren aller gelandeten TXs des Handoffs summieren; fehlt eine → UNKNOWN.
    let mut fee: Option<u64> = None;
    if outcome != "NOT_LANDED" {
        let mut sum = 0u64;
        let mut complete = true;
        for s in &p.signatures {
            match reconcile(rpc, p.path, s, now_ns) {
                Ok(e) if e.fee_lamports.is_some() => sum += e.fee_lamports.unwrap_or(0),
                _ => { complete = false; break; }
            }
        }
        if complete { fee = Some(sum); }
    }
    Some(ReconcileResult {
        record: ReconcileRecord {
            signature: p.signature.to_string(),
            path: p.path.as_str(),
            outcome: outcome.into(),
            landed_slot: ev.landed_slot,
            fee_lamports: fee,
            compute_units_consumed: ev.compute_units_consumed,
            error: ev.error,
            exposure_released: p.exposure,
            pnl_complete: false,
        },
        landed_success,
        // [FIX-24] Nicht gelandet: nichts bezahlt → Some(0) ist hier **gemessen**, nicht erfunden.
        // Gelandet: PnL erst vollständig mit Token-Deltas (P8-05) → bis dahin None = UNKNOWN.
        realized_pnl: if outcome == "NOT_LANDED" { Some(0) } else { None },
    })
}
