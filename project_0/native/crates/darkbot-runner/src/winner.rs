//! [M2] Winner-Matcher (P6-LT): Wer hat den Account tatsächlich liquidiert, in welchem Slot,
//! und lag unser Signatur-Slot davor? Reine Evidence, kein Einfluss auf Entscheidungen.
//!
//! [FIX-44] Deterministisch: nur **erfolgreiche** TXs; unter allen `end_liquidation` mit
//! `slot ≥ since_slot` die **früheste** Landing; Pagination bis unter `since_slot`, gedeckelt
//! (fail closed bei Überschreitung).
//! [FIX-45] Gleicher Slot ist kein belegter Win: `SameSlotUnknown`.
//!
//! [F8] Rewrite. The previous implementation read JSON pointers
//! `/transaction/transaction/message/...` from `serde_json::to_value` of the SDK type
//! `EncodedConfirmedTransactionWithStatusMeta`. In solana-transaction-status 3.1.12 that type
//! flattens `transaction`/`meta` (`#[serde(flatten)]`), so those pointers never matched.
//! Additionally this workspace enables serde_json `arbitrary_precision` (runner Cargo.toml);
//! serde's buffered `flatten`/`untagged` deserialization is known to break under that feature,
//! which plausibly explains the audit helper's "SDK JSON deserialization" failure and would make
//! `get_transaction_with_config` fail silently (old code: `else { continue }`).
//! New approach: raw `getTransaction` via `RpcClient::send` into minimal *typed* serde structs
//! without flatten/untagged, explicit v0 account-key resolution
//! (static keys + loadedAddresses.writable + loadedAddresses.readonly), target-account check,
//! and fail-closed typed errors instead of silent skips.
use crate::record::LossReason;
use darkbot_project0::protocol::ix::END_LIQUIDATION;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::{GetConfirmedSignaturesForAddress2Config, RpcClient};
use solana_client::rpc_request::RpcRequest;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::str::FromStr;

/// Earliest successful `end_liquidation` landing for an account.
/// The slot is a proven fact; the identity is only asserted when it is unique in that slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Winner {
    pub slot: u64,
    pub identity: WinnerIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WinnerIdentity {
    /// Exactly one successful end_liquidation for the account in the earliest slot.
    Unique { signature: String, receiver: String, tx_version: String },
    /// [F8] Several successful end_liquidations for the account in the same earliest slot.
    /// Intra-slot order is UNKNOWN: no signature/receiver is asserted as winner. All candidates
    /// are retained as evidence, sorted by signature for determinism only (no ordering meaning).
    /// `transaction_index` is recorded when the provider returns it, but it is NOT used to
    /// resolve the tie until its semantics are separately qualified.
    SameSlotAmbiguous { candidates: Vec<SameSlotCandidate> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SameSlotCandidate {
    pub signature: String,
    pub receiver: String,
    pub tx_version: String,
    pub transaction_index: Option<u64>,
}

const MAX_PAGES: usize = 10;
const PAGE: usize = 100;
/// EndLiquidation account layout (marginfi `EndLiquidation` accounts struct order).
const END_LIQ_IDX_MARGINFI_ACCOUNT: usize = 0;
const END_LIQ_IDX_RECEIVER: usize = 3;

/// Minimal typed view of RPC `getTransaction` with `encoding = "json"`.
/// Deliberately no `flatten` / `untagged` (see module docs).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RpcTransaction {
    pub slot: u64,
    /// Provider-supplied position in the block, if any. Evidence only (see `WinnerIdentity`).
    #[serde(default)]
    pub transaction_index: Option<u64>,
    pub transaction: RpcTransactionBody,
    pub meta: Option<RpcTransactionMeta>,
    #[serde(default)]
    pub version: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RpcTransactionBody {
    pub signatures: Vec<String>,
    pub message: RpcMessage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RpcMessage {
    pub account_keys: Vec<String>,
    pub instructions: Vec<RpcInstruction>,
    #[serde(default)]
    pub address_table_lookups: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RpcInstruction {
    pub program_id_index: u64,
    pub accounts: Vec<u64>,
    pub data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RpcTransactionMeta {
    #[serde(default)]
    pub err: Option<serde_json::Value>,
    #[serde(default)]
    pub loaded_addresses: Option<RpcLoadedAddresses>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RpcLoadedAddresses {
    pub writable: Vec<String>,
    pub readonly: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EndLiquidationHit {
    pub receiver: Pubkey,
    pub tx_version: String,
    pub transaction_index: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WinnerParseError {
    MetaMissing,
    InvalidKey(String),
    LoadedAddressesMissing,
    IndexOutOfRange { index: u64, keys: usize },
    EndLiquidationLayout { accounts: usize },
    InvalidInstructionData,
}

impl std::fmt::Display for WinnerParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WinnerParseError::MetaMissing => write!(f, "WINNER_META_MISSING"),
            WinnerParseError::InvalidKey(k) => write!(f, "WINNER_INVALID_KEY:{k}"),
            WinnerParseError::LoadedAddressesMissing => {
                write!(f, "WINNER_LOADED_ADDRESSES_MISSING: v0 lookups without meta.loadedAddresses")
            }
            WinnerParseError::IndexOutOfRange { index, keys } => {
                write!(f, "WINNER_INDEX_OUT_OF_RANGE: index={index} keys={keys}")
            }
            WinnerParseError::EndLiquidationLayout { accounts } => {
                write!(f, "WINNER_END_LIQUIDATION_LAYOUT: accounts={accounts}")
            }
            WinnerParseError::InvalidInstructionData => write!(f, "WINNER_INVALID_INSTRUCTION_DATA"),
        }
    }
}

fn parse_key(raw: &str) -> Result<Pubkey, WinnerParseError> {
    Pubkey::from_str(raw).map_err(|_| WinnerParseError::InvalidKey(raw.to_string()))
}

fn key_at(keys: &[Pubkey], index: u64) -> Result<Pubkey, WinnerParseError> {
    usize::try_from(index)
        .ok()
        .and_then(|i| keys.get(i).copied())
        .ok_or(WinnerParseError::IndexOutOfRange { index, keys: keys.len() })
}

fn version_label(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => format!("v{n}"),
        _ => "unknown".to_string(),
    }
}

/// Successful top-level marginfi `end_liquidation` for exactly `account`?
/// Account indices resolve against static keys + loaded writable + loaded readonly (v0 order).
pub(crate) fn end_liquidation_hit(
    tx: &RpcTransaction,
    program: &Pubkey,
    account: &Pubkey,
) -> Result<Option<EndLiquidationHit>, WinnerParseError> {
    let meta = tx.meta.as_ref().ok_or(WinnerParseError::MetaMissing)?;
    if meta.err.is_some() {
        return Ok(None); // [FIX-44] failed transactions never win
    }
    let message = &tx.transaction.message;
    let mut keys = Vec::with_capacity(message.account_keys.len());
    for k in &message.account_keys {
        keys.push(parse_key(k)?);
    }
    let has_lookups = message
        .address_table_lookups
        .as_ref()
        .is_some_and(|l| !l.is_empty());
    match &meta.loaded_addresses {
        Some(loaded) => {
            for k in loaded.writable.iter().chain(loaded.readonly.iter()) {
                keys.push(parse_key(k)?);
            }
        }
        None if has_lookups => return Err(WinnerParseError::LoadedAddressesMissing),
        None => {}
    }
    for ix in &message.instructions {
        if key_at(&keys, ix.program_id_index)? != *program {
            continue;
        }
        let data = bs58::decode(&ix.data)
            .into_vec()
            .map_err(|_| WinnerParseError::InvalidInstructionData)?;
        if data.len() < 8 || data[..8] != END_LIQUIDATION {
            continue;
        }
        let (Some(&target_idx), Some(&receiver_idx)) = (
            ix.accounts.get(END_LIQ_IDX_MARGINFI_ACCOUNT),
            ix.accounts.get(END_LIQ_IDX_RECEIVER),
        ) else {
            return Err(WinnerParseError::EndLiquidationLayout { accounts: ix.accounts.len() });
        };
        if key_at(&keys, target_idx)? != *account {
            continue; // end_liquidation of a different MarginfiAccount
        }
        return Ok(Some(EndLiquidationHit {
            receiver: key_at(&keys, receiver_idx)?,
            tx_version: version_label(tx.version.as_ref()),
            transaction_index: tx.transaction_index,
        }));
    }
    Ok(None)
}

/// Earliest landing slot wins. Several hits in that slot are `SameSlotAmbiguous`: the slot is
/// proven, the identity is not. Never resolved by signature order or other guessing.
pub(crate) fn select_earliest(hits: Vec<(u64, String, EndLiquidationHit)>) -> Option<Winner> {
    let min_slot = hits.iter().map(|(slot, _, _)| *slot).min()?;
    let mut same: Vec<(String, EndLiquidationHit)> = hits
        .into_iter()
        .filter(|(slot, _, _)| *slot == min_slot)
        .map(|(_, signature, hit)| (signature, hit))
        .collect();
    let identity = if same.len() == 1 {
        let (signature, hit) = same.pop()?;
        WinnerIdentity::Unique { signature, receiver: hit.receiver.to_string(), tx_version: hit.tx_version }
    } else {
        same.sort_by(|a, b| a.0.cmp(&b.0));
        WinnerIdentity::SameSlotAmbiguous {
            candidates: same
                .into_iter()
                .map(|(signature, hit)| SameSlotCandidate {
                    signature,
                    receiver: hit.receiver.to_string(),
                    tx_version: hit.tx_version,
                    transaction_index: hit.transaction_index,
                })
                .collect(),
        }
    };
    Some(Winner { slot: min_slot, identity })
}

/// The fetched transaction must be exactly the one requested: first signature (transaction id)
/// and slot must match what `getSignaturesForAddress` reported. Otherwise fail closed.
pub(crate) fn check_transaction_identity(tx: &RpcTransaction, signature: &str, slot: u64) -> Result<(), String> {
    match tx.transaction.signatures.first() {
        Some(first) if first == signature => {}
        Some(first) => return Err(format!("signature mismatch requested={signature} returned={first}")),
        None => return Err(format!("signature missing in returned transaction for {signature}")),
    }
    if tx.slot != slot {
        return Err(format!("slot mismatch signatures={slot} transaction={}", tx.slot));
    }
    Ok(())
}

fn fetch_transaction(rpc: &RpcClient, signature: &str) -> Result<RpcTransaction, String> {
    Signature::from_str(signature).map_err(|e| format!("signature {signature}: {e:?}"))?;
    let value: serde_json::Value = rpc
        .send(
            RpcRequest::GetTransaction,
            serde_json::json!([
                signature,
                {"encoding": "json", "commitment": "confirmed", "maxSupportedTransactionVersion": 1}
            ]),
        )
        .map_err(|e| e.to_string())?;
    if value.is_null() {
        return Err(format!("transaction {signature} not found"));
    }
    serde_json::from_value(value).map_err(|e| format!("transaction {signature} decode: {e}"))
}

/// Früheste **erfolgreiche** `end_liquidation` auf `account` mit Slot ≥ `since_slot`.
/// `Err` bei abgeschnittener Pagination oder nicht lesbarer Kandidaten-TX (fail closed),
/// `Ok(None)` wenn niemand liquidiert hat.
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
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut hits: Vec<(u64, String, EndLiquidationHit)> = Vec::new();
    let mut earliest_hit_slot: Option<u64> = None;
    for (slot, signature) in candidates {
        // Ascending slots: once a hit exists, only same-slot candidates can still tie.
        if earliest_hit_slot.is_some_and(|hit_slot| slot > hit_slot) {
            break;
        }
        let tx = fetch_transaction(rpc, &signature)?;
        // Fail closed if getTransaction returns another transaction or another slot.
        check_transaction_identity(&tx, &signature, slot).map_err(|e| format!("winner tx {signature}: {e}"))?;
        match end_liquidation_hit(&tx, program, account) {
            Ok(Some(hit)) => {
                earliest_hit_slot = Some(earliest_hit_slot.map_or(tx.slot, |s| s.min(tx.slot)));
                hits.push((tx.slot, signature, hit));
            }
            Ok(None) => {}
            Err(e) => return Err(format!("winner tx {signature}: {e}")),
        }
    }
    Ok(select_earliest(hits))
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

    // ---------------------------------------------------------------- [F8] fixtures
    use serde_json::json;

    struct Fx {
        program: Pubkey,
        target: Pubkey,
        receiver: Pubkey,
    }

    fn fx() -> Fx {
        Fx { program: Pubkey::new_unique(), target: Pubkey::new_unique(), receiver: Pubkey::new_unique() }
    }

    fn end_liq_data() -> String {
        bs58::encode(END_LIQUIDATION).into_string()
    }

    fn keys(v: &[Pubkey]) -> Vec<String> {
        v.iter().map(|k| k.to_string()).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn tx(
        slot: u64,
        version: serde_json::Value,
        static_keys: &[Pubkey],
        loaded_w: &[Pubkey],
        loaded_r: &[Pubkey],
        lookups: bool,
        ixs: serde_json::Value,
        err: serde_json::Value,
    ) -> RpcTransaction {
        let lookups_v = if lookups {
            json!([{"accountKey": Pubkey::new_unique().to_string(), "writableIndexes": [0], "readonlyIndexes": []}])
        } else {
            json!([])
        };
        let v = json!({
            "slot": slot,
            "blockTime": 1_700_000_000,
            "version": version,
            "transaction": {
                "signatures": ["sig"],
                "message": {
                    "accountKeys": keys(static_keys),
                    "header": {"numRequiredSignatures": 1, "numReadonlySignedAccounts": 0, "numReadonlyUnsignedAccounts": 1},
                    "recentBlockhash": Pubkey::new_unique().to_string(),
                    "instructions": ixs,
                    "addressTableLookups": lookups_v
                }
            },
            "meta": {
                "err": err,
                "status": {"Ok": null},
                "fee": 5000,
                "preBalances": [1, 2],
                "postBalances": [1, 2],
                "loadedAddresses": {"writable": keys(loaded_w), "readonly": keys(loaded_r)}
            }
        });
        serde_json::from_value(v).expect("fixture decodes into typed view")
    }

    /// Static layout: [payer, target, record, group, receiver, fee_state, fee_wallet, system, program]
    fn legacy_keys(f: &Fx) -> Vec<Pubkey> {
        vec![Pubkey::new_unique(), f.target, Pubkey::new_unique(), Pubkey::new_unique(), f.receiver,
             Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique(), f.program]
    }

    fn end_liq_ix(program_idx: u64, accounts: &[u64]) -> serde_json::Value {
        json!([{"programIdIndex": program_idx, "accounts": accounts, "data": end_liq_data(), "stackHeight": null}])
    }

    #[test]
    fn f8_legacy_successful_winner() {
        let f = fx();
        let t = tx(100, json!("legacy"), &legacy_keys(&f), &[], &[], false,
            end_liq_ix(8, &[1, 2, 3, 4, 5, 6, 7]), json!(null));
        let hit = end_liquidation_hit(&t, &f.program, &f.target).unwrap().unwrap();
        assert_eq!(hit.receiver, f.receiver);
        assert_eq!(hit.tx_version, "legacy");
    }

    #[test]
    fn f8_v0_winner_without_relevant_lut_address() {
        let f = fx();
        let unrelated = [Pubkey::new_unique()];
        let t = tx(100, json!(0), &legacy_keys(&f), &unrelated, &[], true,
            end_liq_ix(8, &[1, 2, 3, 4, 5, 6, 7]), json!(null));
        let hit = end_liquidation_hit(&t, &f.program, &f.target).unwrap().unwrap();
        assert_eq!(hit.receiver, f.receiver);
        assert_eq!(hit.tx_version, "v0");
    }

    #[test]
    fn f8_v0_winner_with_receiver_from_lut() {
        let f = fx();
        // static: [payer, target, record, group, fee_state, fee_wallet, system, program] (0..=7)
        let statics = vec![Pubkey::new_unique(), f.target, Pubkey::new_unique(), Pubkey::new_unique(),
            Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique(), f.program];
        let ro = [Pubkey::new_unique()];
        // receiver = loaded writable[0] -> combined index 8; a readonly LUT key follows at 9.
        let t = tx(100, json!(0), &statics, &[f.receiver], &ro, true,
            end_liq_ix(7, &[1, 2, 3, 8, 4, 5, 6, 9]), json!(null));
        let hit = end_liquidation_hit(&t, &f.program, &f.target).unwrap().unwrap();
        assert_eq!(hit.receiver, f.receiver);
    }

    #[test]
    fn f8_failed_liquidation_is_not_a_winner() {
        let f = fx();
        let t = tx(100, json!("legacy"), &legacy_keys(&f), &[], &[], false,
            end_liq_ix(8, &[1, 2, 3, 4, 5, 6, 7]), json!({"InstructionError": [0, {"Custom": 101}]}));
        assert_eq!(end_liquidation_hit(&t, &f.program, &f.target), Ok(None));
    }

    #[test]
    fn f8_unrelated_successful_tx_is_not_a_winner() {
        let f = fx();
        // other program
        let t = tx(100, json!("legacy"), &legacy_keys(&f), &[], &[], false,
            end_liq_ix(0, &[1, 2, 3, 4, 5, 6, 7]), json!(null));
        assert_eq!(end_liquidation_hit(&t, &f.program, &f.target), Ok(None));
        // marginfi program, other discriminator
        let other = json!([{"programIdIndex": 8, "accounts": [1, 2, 3, 4], "data": bs58::encode([1u8; 8]).into_string()}]);
        let t = tx(100, json!("legacy"), &legacy_keys(&f), &[], &[], false, other, json!(null));
        assert_eq!(end_liquidation_hit(&t, &f.program, &f.target), Ok(None));
        // end_liquidation for a different MarginfiAccount
        let t = tx(100, json!("legacy"), &legacy_keys(&f), &[], &[], false,
            end_liq_ix(8, &[1, 2, 3, 4, 5, 6, 7]), json!(null));
        assert_eq!(end_liquidation_hit(&t, &f.program, &Pubkey::new_unique()), Ok(None));
    }

    fn hit(receiver: Pubkey, transaction_index: Option<u64>) -> EndLiquidationHit {
        EndLiquidationHit { receiver, tx_version: "v0".into(), transaction_index }
    }

    #[test]
    fn f8_single_earliest_hit_is_unique_winner() {
        let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
        let w = select_earliest(vec![
            (105, "zzz".into(), hit(a, Some(3))),
            (101, "bbb".into(), hit(b, Some(7))),
        ])
        .unwrap();
        assert_eq!(w.slot, 101);
        assert_eq!(
            w.identity,
            WinnerIdentity::Unique { signature: "bbb".into(), receiver: b.to_string(), tx_version: "v0".into() }
        );
        assert!(select_earliest(vec![]).is_none());
    }

    #[test]
    fn f8_same_slot_hits_are_ambiguous_never_guessed() {
        let (a, b, c) = (Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique());
        let w = select_earliest(vec![
            (105, "zzz".into(), hit(a, None)),
            (101, "bbb".into(), hit(b, Some(9))),
            (101, "aaa".into(), hit(c, Some(2))),
        ])
        .unwrap();
        // The slot is proven; the identity is not asserted, even with a transaction index.
        assert_eq!(w.slot, 101);
        match w.identity {
            WinnerIdentity::SameSlotAmbiguous { candidates } => {
                assert_eq!(candidates.len(), 2);
                assert_eq!(candidates[0].signature, "aaa");
                assert_eq!(candidates[0].receiver, c.to_string());
                assert_eq!(candidates[0].transaction_index, Some(2));
                assert_eq!(candidates[1].signature, "bbb");
            }
            other => panic!("same-slot hits must be ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn f8_winner_serializes_ambiguity_explicitly() {
        let w = select_earliest(vec![
            (7, "x".into(), hit(Pubkey::new_unique(), None)),
            (7, "y".into(), hit(Pubkey::new_unique(), None)),
        ])
        .unwrap();
        let v = serde_json::to_value(&w).unwrap();
        assert_eq!(v["identity"]["kind"], "SAME_SLOT_AMBIGUOUS");
        assert!(v["identity"].get("signature").is_none());
        assert!(v["identity"].get("receiver").is_none());
    }

    #[test]
    fn f8_fail_closed_shapes() {
        let f = fx();
        // meta missing
        let mut t = tx(100, json!("legacy"), &legacy_keys(&f), &[], &[], false,
            end_liq_ix(8, &[1, 2, 3, 4, 5, 6, 7]), json!(null));
        t.meta = None;
        assert_eq!(end_liquidation_hit(&t, &f.program, &f.target), Err(WinnerParseError::MetaMissing));
        // v0 lookups without loadedAddresses
        let mut t = tx(100, json!(0), &legacy_keys(&f), &[], &[], true,
            end_liq_ix(8, &[1, 2, 3, 4, 5, 6, 7]), json!(null));
        if let Some(m) = t.meta.as_mut() { m.loaded_addresses = None; }
        assert_eq!(end_liquidation_hit(&t, &f.program, &f.target), Err(WinnerParseError::LoadedAddressesMissing));
        // program index out of range
        let t = tx(100, json!("legacy"), &legacy_keys(&f), &[], &[], false,
            end_liq_ix(42, &[1, 2, 3, 4]), json!(null));
        assert!(matches!(end_liquidation_hit(&t, &f.program, &f.target), Err(WinnerParseError::IndexOutOfRange { .. })));
        // truncated end_liquidation account list
        let t = tx(100, json!("legacy"), &legacy_keys(&f), &[], &[], false,
            end_liq_ix(8, &[1, 2]), json!(null));
        assert!(matches!(end_liquidation_hit(&t, &f.program, &f.target), Err(WinnerParseError::EndLiquidationLayout { .. })));
    }

    /// [F8] Audit Step 5 gate: the six frozen real Mainnet winners (raw RPC `getTransaction`
    /// JSON, v0 with LUT loaded addresses and v1) must all resolve with the correct signature,
    /// slot, target MarginfiAccount and liquidation receiver.
    #[test]
    fn f8_six_real_audit_winners_resolve_6_of_6() {
        let program = Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap();
        let cases: [(&str, &str, u64, &str, &str); 6] = [
            (include_str!("../../../../docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json"), "2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf", 451703584, "3GqwCURBtJZfdQxMSyZ1VyWNSB5WXovg7Q8Q46HY7ihx", "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn"),
            (include_str!("../../../../docs/evidence/AUDIT_20261005/38ZXvW6P5sGZmdzj4QbUPSdQnCnJEBHoXTh4vvvfitFPNDHRsgPjELCnFxxCfW2wygE8J6k2Yv8RJJXPUVchBm7F.json"), "38ZXvW6P5sGZmdzj4QbUPSdQnCnJEBHoXTh4vvvfitFPNDHRsgPjELCnFxxCfW2wygE8J6k2Yv8RJJXPUVchBm7F", 451704201, "AzQVTAtLxdaQUJzh7xZ7rj956tkeVNk4D4iuK5EVkxDi", "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn"),
            (include_str!("../../../../docs/evidence/AUDIT_20261005/3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB.json"), "3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB", 451657746, "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", "TraceR2t18nnwkXgHoB54iPD5N4x2EJZC1uS55ggsH8"),
            (include_str!("../../../../docs/evidence/AUDIT_20261005/59Qt6h1iPnEgfaDaEi5Y5jVvqHJCs4gzwvHqWW37wVJMHyThYjM6zsS8AmE9bvoxqdUj8AMnNbBuKHQtXysdnwNa.json"), "59Qt6h1iPnEgfaDaEi5Y5jVvqHJCs4gzwvHqWW37wVJMHyThYjM6zsS8AmE9bvoxqdUj8AMnNbBuKHQtXysdnwNa", 453063240, "HMjc1LecbCMUqYG3x36gaCzxG5AJ7gJPkPB85fx1pPKK", "qbvAGHo9JpVRsuRem8rBCzLJJNV6ZDNv63qYHkcmNCV"),
            (include_str!("../../../../docs/evidence/AUDIT_20261005/C6JnpzGrv53y9dEExtpDYkzTbexLEu7oikPRNx8sCqf85Jvobc7UBpsaDSgGHzEgeB88HkVFqMkCkvs1ZqVWRwZ.json"), "C6JnpzGrv53y9dEExtpDYkzTbexLEu7oikPRNx8sCqf85Jvobc7UBpsaDSgGHzEgeB88HkVFqMkCkvs1ZqVWRwZ", 451635082, "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", "TraceR2t18nnwkXgHoB54iPD5N4x2EJZC1uS55ggsH8"),
            (include_str!("../../../../docs/evidence/AUDIT_20261005/gLVXyfVZmizZqpF9752x3uhgJixW5Jqrp3Agjhhr6cNUnU5aQDBAxQ1CF9RWSpy2oB2K9EAroFJaGPTkz5vkUzB.json"), "gLVXyfVZmizZqpF9752x3uhgJixW5Jqrp3Agjhhr6cNUnU5aQDBAxQ1CF9RWSpy2oB2K9EAroFJaGPTkz5vkUzB", 451643526, "6mzmLGPk7CeGCM7dRH8ZZpHsSzfy6aAYf47nbH8HuYy8", "D9i4VJA8XaY49Rzff71B1uAJmjnuNfUuuZ1BLrjsPqYn"),
        ];
        for (raw, signature, slot, target, receiver) in cases {
            let tx: RpcTransaction = serde_json::from_str(raw).expect("audit fixture decodes into typed view");
            assert_eq!(tx.transaction.signatures.first().map(String::as_str), Some(signature));
            assert_eq!(tx.slot, slot);
            assert_eq!(check_transaction_identity(&tx, signature, slot), Ok(()));
            let target = Pubkey::from_str(target).unwrap();
            let hit = end_liquidation_hit(&tx, &program, &target)
                .expect("fixture parses")
                .expect("winner found");
            assert_eq!(hit.receiver, Pubkey::from_str(receiver).unwrap());
            assert_eq!(end_liquidation_hit(&tx, &program, &Pubkey::new_unique()), Ok(None));
        }
    }

    #[test]
    fn f8_transaction_identity_is_fail_closed() {
        let f = fx();
        let mut t = tx(100, json!("legacy"), &legacy_keys(&f), &[], &[], false,
            end_liq_ix(8, &[1, 2, 3, 4, 5, 6, 7]), json!(null));
        assert_eq!(check_transaction_identity(&t, "sig", 100), Ok(()));
        assert!(check_transaction_identity(&t, "other", 100).is_err());
        assert!(check_transaction_identity(&t, "sig", 101).is_err());
        t.transaction.signatures.clear();
        assert!(check_transaction_identity(&t, "sig", 100).is_err());
    }

    /// Diagnostic only (run with `--ignored`): does the pinned SDK type decode the RPC shape in
    /// this workspace (serde_json `arbitrary_precision` enabled)? Prints the result; never fails.
    #[test]
    #[ignore]
    fn f8_probe_sdk_encoded_transaction_decoding() {
        let f = fx();
        let v = json!({
            "slot": 100, "blockTime": 1_700_000_000, "version": 0,
            "transaction": {"signatures": ["1111111111111111111111111111111111111111111111111111111111111111"],
                "message": {"accountKeys": keys(&legacy_keys(&f)),
                    "header": {"numRequiredSignatures": 1, "numReadonlySignedAccounts": 0, "numReadonlyUnsignedAccounts": 1},
                    "recentBlockhash": Pubkey::new_unique().to_string(),
                    "instructions": end_liq_ix(8, &[1, 2, 3, 4, 5, 6, 7]),
                    "addressTableLookups": []}},
            "meta": {"err": null, "status": {"Ok": null}, "fee": 5000, "preBalances": [1], "postBalances": [1],
                "innerInstructions": [], "logMessages": [], "preTokenBalances": [], "postTokenBalances": [],
                "rewards": [], "loadedAddresses": {"writable": [], "readonly": []}, "computeUnitsConsumed": 1}
        });
        let r = serde_json::from_value::<solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta>(v);
        eprintln!("F8_SDK_DECODE_PROBE ok={} err={:?}", r.is_ok(), r.err().map(|e| e.to_string()));
    }
}
