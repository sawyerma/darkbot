//! [M2] Winner-Matcher (P6-LT): Wer hat den Account tatsächlich liquidiert, in welchem Slot,
//! und lag unser Signatur-Slot davor? Reine Evidence, kein Einfluss auf Entscheidungen.
//!
//! [FIX-44] Deterministisch: nur **erfolgreiche** TXs; unter allen `end_liquidation` mit
//! `slot ≥ since_slot` die **früheste** Landing; Pagination bis unter `since_slot`, gedeckelt
//! (fail closed bei Überschreitung). `liquidation_receiver` = Account-Index 3 (EndLiquidation-Layout).
//! [FIX-45] Gleicher Slot ist kein belegter Win: `SameSlotUnknown`.
//!
//! Status: UNTESTED / VERIFY (JSON-Feldnamen). Klassifikation offline getestet.

use crate::record::LossReason;
use darkbot_project0::protocol::ix::END_LIQUIDATION;
use serde::Serialize;
use solana_client::rpc_client::{GetConfirmedSignaturesForAddress2Config, RpcClient};
use solana_client::rpc_config::RpcTransactionConfig;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use std::str::FromStr;

#[derive(Clone, Debug, Serialize)]
pub struct Winner {
    pub signature: String,
    pub slot: u64,
    pub receiver: Option<String>,
    pub tx_version: String,
}

const MAX_PAGES: usize = 10;
const PAGE: usize = 100;

/// Früheste **erfolgreiche** `end_liquidation` auf `account` mit Slot ≥ `since_slot`.
/// `Err` bei abgeschnittener Pagination (fail closed), `Ok(None)` wenn niemand liquidiert hat.
pub fn find_winner(rpc: &RpcClient, program: &Pubkey, account: &Pubkey, since_slot: u64) -> Result<Option<Winner>, String> {
    let mut before: Option<Signature> = None;
    let mut candidates: Vec<(u64, String)> = Vec::new();
    let mut reached_since = false;
    for _ in 0..MAX_PAGES {
        let cfg = GetConfirmedSignaturesForAddress2Config { before, until: None, limit: Some(PAGE), commitment: Some(CommitmentConfig::confirmed()) };
        let sigs = rpc.get_signatures_for_address_with_config(account, cfg).map_err(|e| e.to_string())?;
        if sigs.is_empty() { reached_since = true; break; }
        for s in &sigs {
            if s.slot < since_slot { reached_since = true; continue; }
            if s.err.is_some() { continue; } // [FIX-44] nur erfolgreiche TXs
            candidates.push((s.slot, s.signature.clone()));
        }
        if reached_since || sigs.len() < PAGE { reached_since = true; break; }
        before = sigs.last().and_then(|s| Signature::from_str(&s.signature).ok());
    }
    if !reached_since {
        return Err(format!("winner search truncated after {} pages", MAX_PAGES));
    }
    // früheste zuerst
    candidates.sort_by_key(|(slot, _)| *slot);
    for (slot, sig_s) in candidates {
        let sig = Signature::from_str(&sig_s).map_err(|e| format!("{:?}", e))?;
        let tcfg = RpcTransactionConfig { encoding: Some(UiTransactionEncoding::Json), commitment: Some(CommitmentConfig::confirmed()), max_supported_transaction_version: Some(1) };
        let Ok(t) = rpc.get_transaction_with_config(&sig, tcfg) else { continue };
        let v = serde_json::to_value(&t).map_err(|e| e.to_string())?;
        if v.pointer("/transaction/meta/err").map(|e| !e.is_null()).unwrap_or(false) { continue; } // [FIX-44]
        let keys: Vec<String> = v.pointer("/transaction/transaction/message/accountKeys").and_then(|k| k.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().or_else(|| x.get("pubkey").and_then(|p| p.as_str())).map(|s| s.to_string())).collect()).unwrap_or_default();
        let ixs = v.pointer("/transaction/transaction/message/instructions").and_then(|i| i.as_array()).cloned().unwrap_or_default();
        for ix in ixs {
            let pidx = ix.get("programIdIndex").and_then(|p| p.as_u64()).unwrap_or(u64::MAX) as usize;
            if keys.get(pidx).map(|k| k != &program.to_string()).unwrap_or(true) { continue; }
            let data = ix.get("data").and_then(|d| d.as_str()).and_then(|d| bs58::decode(d).into_vec().ok()).unwrap_or_default();
            if data.len() >= 8 && data[..8] == END_LIQUIDATION {
                let acc_idx: Vec<usize> = ix.get("accounts").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64()).map(|x| x as usize).collect()).unwrap_or_default();
                let receiver = acc_idx.get(3).and_then(|i| keys.get(*i)).cloned();
                let version = darkbot_solana::ingress::detect_version_json(v.get("transaction").unwrap_or(&v));
                return Ok(Some(Winner { signature: sig_s, slot, receiver, tx_version: format!("{:?}", version) }));
            }
        }
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum RaceOutcome {
    /// Konservativ belegt: unser Signatur-Slot lag strikt vor dem Gewinner-Slot.
    ReadyBeforeWinner,
    /// [FIX-45] gleicher Slot: Reihenfolge unbekannt, kein Win.
    SameSlotUnknown,
    Loss(LossReason),
}

/// Rennen auswerten. `our_loss` = strukturierter Grund aus dem Record, wenn wir nicht bis ShadowReady kamen.
pub fn race_outcome(winner_slot: u64, our_rx_slot: u64, our_detect_slot: u64, our_sign_slot: Option<u64>, shadow_ready: bool, our_loss: Option<LossReason>) -> RaceOutcome {
    if !shadow_ready {
        return RaceOutcome::Loss(our_loss.unwrap_or(LossReason::Build));
    }
    match our_sign_slot {
        Some(s) if s < winner_slot => RaceOutcome::ReadyBeforeWinner,
        Some(s) if s == winner_slot => RaceOutcome::SameSlotUnknown,
        Some(_) | None => {
            if winner_slot < our_rx_slot { RaceOutcome::Loss(LossReason::Ingress) }
            else if winner_slot < our_detect_slot { RaceOutcome::Loss(LossReason::StateRace) }
            else { RaceOutcome::Loss(LossReason::Build) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes() {
        assert_eq!(race_outcome(105, 100, 101, Some(104), true, None), RaceOutcome::ReadyBeforeWinner);
        assert_eq!(race_outcome(105, 100, 101, Some(105), true, None), RaceOutcome::SameSlotUnknown);
        assert_eq!(race_outcome(100, 101, 102, Some(107), true, None), RaceOutcome::Loss(LossReason::Ingress));
        assert_eq!(race_outcome(101, 100, 102, Some(107), true, None), RaceOutcome::Loss(LossReason::StateRace));
        assert_eq!(race_outcome(105, 100, 101, Some(107), true, None), RaceOutcome::Loss(LossReason::Build));
        // ShadowSimFailed / Skip: nie ein Win, Grund aus dem Record
        assert_eq!(race_outcome(105, 100, 101, Some(104), false, Some(LossReason::ThresholdTooHigh)), RaceOutcome::Loss(LossReason::ThresholdTooHigh));
        assert_eq!(race_outcome(105, 100, 101, Some(104), false, None), RaceOutcome::Loss(LossReason::Build));
    }
}
