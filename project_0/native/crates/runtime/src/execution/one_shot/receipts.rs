//! Finalized chain accounting through the existing RPC reconciliation transport.
//! No prices, assumed zero balances, exit qualification or realized-PnL claims.
use super::err;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
    EncodedTransaction, UiMessage, UiTransactionTokenBalance,
};
use std::collections::{BTreeMap, BTreeSet};

/// Persisted observations are evidence only; they never authorize another send.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FinalizedObservation {
    pub signature: String,
    pub slot: u64,
    pub success: bool,
    pub fee_lamports: u64,
    /// Already includes this wallet's fees, rent and any SOL transfers. Do not
    /// subtract meta.fee from this delta a second time.
    pub wallet_sol_delta_lamports: i128,
    pub wallet_token_deltas: Option<BTreeMap<String, i128>>,
    pub unknown_reasons: Vec<String>,
}

impl FinalizedObservation {
    /// V2-01: every fact known (token deltas present, no unknown reason). Only a complete
    /// observation is final for the settlement worker; an incomplete one is read again.
    pub fn is_complete(&self) -> bool {
        self.unknown_reasons.is_empty() && self.wallet_token_deltas.is_some()
    }
}

/// Uses finalized getTransaction via the shared RPC transport. Caller owns
/// provider quotas/timeouts; this function does not poll, retry or submit.
pub fn fetch_finalized(
    rpc: &RpcClient,
    signature: &Signature,
    wallet: &Pubkey,
    minimum_slot: u64,
    expected_assets: &BTreeSet<String>,
) -> Result<FinalizedObservation, String> {
    let tx = darkbot_solana::submit::rpc::finalized_transaction(rpc, signature)
        .map_err(|_| err("FINALIZED_TRANSACTION_UNAVAILABLE"))?;
    parse_finalized(tx, signature, wallet, minimum_slot, expected_assets)
}

// Private: historical JSON or a caller's `finalized: true` must not enter the
// runtime as a fresh finalized RPC response.
fn parse_finalized(
    tx: EncodedConfirmedTransactionWithStatusMeta,
    signature: &Signature,
    wallet: &Pubkey,
    minimum_slot: u64,
    expected_assets: &BTreeSet<String>,
) -> Result<FinalizedObservation, String> {
    if tx.slot < minimum_slot {
        return Err(err("RECEIPT_SLOT_PRECEDES_INTENT"));
    }
    let EncodedTransaction::Json(transaction) = tx.transaction.transaction else {
        return Err(err("RECEIPT_ENCODING_UNSUPPORTED"));
    };
    if transaction.signatures.first() != Some(&signature.to_string()) {
        return Err(err("RECEIPT_SIGNATURE_MISMATCH"));
    }
    let UiMessage::Raw(message) = transaction.message else {
        return Err(err("RECEIPT_RAW_KEYS_REQUIRED"));
    };
    let meta = tx
        .transaction
        .meta
        .ok_or_else(|| err("RECEIPT_META_MISSING"))?;
    let mut keys = message.account_keys;
    let lookups = message.address_table_lookups.unwrap_or_default();
    match &meta.loaded_addresses {
        OptionSerializer::Some(loaded) => {
            let writable: usize = lookups.iter().map(|a| a.writable_indexes.len()).sum();
            let readonly: usize = lookups.iter().map(|a| a.readonly_indexes.len()).sum();
            if loaded.writable.len() != writable || loaded.readonly.len() != readonly {
                return Err(err("RECEIPT_LOADED_KEYS_MISMATCH"));
            }
            keys.extend(loaded.writable.iter().cloned());
            keys.extend(loaded.readonly.iter().cloned());
        }
        _ if !lookups.is_empty() => return Err(err("RECEIPT_LOADED_KEYS_MISSING")),
        _ => {}
    }
    if keys.first() != Some(&wallet.to_string())
        || keys.iter().any(|k| k.parse::<Pubkey>().is_err())
        || keys.iter().collect::<BTreeSet<_>>().len() != keys.len()
        || meta.pre_balances.len() != keys.len()
        || meta.post_balances.len() != keys.len()
        || !expected_assets.contains("SOL")
        || expected_assets
            .iter()
            .any(|k| k != "SOL" && k.parse::<Pubkey>().is_err())
    {
        return Err(err("RECEIPT_WALLET_OR_BALANCES_INVALID"));
    }
    let tokens = match (&meta.pre_token_balances, &meta.post_token_balances) {
        (OptionSerializer::Some(pre), OptionSerializer::Some(post)) => token_deltas(
            pre,
            post,
            &meta.pre_balances,
            &meta.post_balances,
            wallet,
            expected_assets,
        ),
        _ => Err(err("TOKEN_BALANCE_METADATA_MISSING")),
    };
    let (wallet_token_deltas, unknown_reasons) = match tokens {
        Ok(deltas) => (Some(deltas), Vec::new()),
        Err(reason) => (None, vec![reason]),
    };
    Ok(FinalizedObservation {
        signature: signature.to_string(),
        slot: tx.slot,
        success: meta.err.is_none(),
        fee_lamports: meta.fee,
        wallet_sol_delta_lamports: i128::from(meta.post_balances[0])
            - i128::from(meta.pre_balances[0]),
        wallet_token_deltas,
        unknown_reasons,
    })
}

fn token_deltas(
    pre: &[UiTransactionTokenBalance],
    post: &[UiTransactionTokenBalance],
    pre_sol: &[u64],
    post_sol: &[u64],
    wallet: &Pubkey,
    expected_assets: &BTreeSet<String>,
) -> Result<BTreeMap<String, i128>, String> {
    struct TokenBalance<'a> {
        owner: &'a str,
        mint: &'a str,
        decimals: u8,
        amount: u64,
    }
    fn indexed(
        rows: &[UiTransactionTokenBalance],
        key_count: usize,
    ) -> Result<BTreeMap<usize, TokenBalance<'_>>, String> {
        let mut indexed = BTreeMap::new();
        for r in rows {
            let OptionSerializer::Some(owner) = &r.owner else {
                return Err(err("TOKEN_OWNER_MISSING"));
            };
            if owner.parse::<Pubkey>().is_err()
                || r.mint.parse::<Pubkey>().is_err()
                || usize::from(r.account_index) >= key_count
            {
                return Err(err("TOKEN_IDENTITY_INVALID"));
            }
            let raw = r
                .ui_token_amount
                .amount
                .parse::<u64>()
                .map_err(|_| err("TOKEN_RAW_AMOUNT_INVALID"))?;
            if indexed
                .insert(
                    usize::from(r.account_index),
                    TokenBalance {
                        owner,
                        mint: &r.mint,
                        decimals: r.ui_token_amount.decimals,
                        amount: raw,
                    },
                )
                .is_some()
            {
                return Err(err("TOKEN_ACCOUNT_DUPLICATE"));
            }
        }
        Ok(indexed)
    }
    let pre = indexed(pre, pre_sol.len())?;
    let post = indexed(post, post_sol.len())?;
    let indices: BTreeSet<_> = pre.keys().chain(post.keys()).copied().collect();
    let mut deltas = BTreeMap::<String, i128>::new();
    let mut decimals = BTreeMap::new();
    let wallet = wallet.to_string();
    for index in indices {
        let before = pre.get(&index);
        let after = post.get(&index);
        // A missing side is zero only with evidence of account creation/close.
        let (owner, mint, scale, old, new) = match (before, after) {
            (Some(a), Some(b))
                if (a.owner, a.mint, a.decimals) == (b.owner, b.mint, b.decimals) =>
            {
                (a.owner, a.mint, a.decimals, a.amount, b.amount)
            }
            (None, Some(b)) if pre_sol[index] == 0 => (b.owner, b.mint, b.decimals, 0, b.amount),
            (Some(a), None) if post_sol[index] == 0 => (a.owner, a.mint, a.decimals, a.amount, 0),
            _ => return Err(err("TOKEN_TRANSITION_INCOMPLETE_OR_CHANGED")),
        };
        if decimals
            .insert(mint, scale)
            .is_some_and(|prior| prior != scale)
        {
            return Err(err("TOKEN_DECIMALS_INCONSISTENT"));
        }
        if owner != wallet {
            continue;
        }
        let total = deltas.entry(mint.into()).or_default();
        *total = total
            .checked_add(i128::from(new) - i128::from(old))
            .ok_or_else(|| err("TOKEN_DELTA_OVERFLOW"))?;
    }
    // Absence from a transaction does not prove the wallet holds zero of a mint.
    if expected_assets
        .iter()
        .any(|mint| mint != "SOL" && !deltas.contains_key(mint))
    {
        return Err(err("EXPECTED_ASSET_NOT_OBSERVED"));
    }
    Ok(deltas)
}

#[cfg(test)]
mod tests {
    use super::super::{Journal, Phase};
    use super::*;
    use serde_json::{json, Value};
    fn fixture() -> (Value, Signature, Pubkey, String, BTreeSet<String>) {
        let wallet = Pubkey::new_unique();
        let mint = Pubkey::new_unique().to_string();
        let sig = Signature::new_unique();
        let token = |amount: &str| json!({"accountIndex":1,"mint":mint,"owner":wallet.to_string(),"programId":"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA","uiTokenAmount":{"amount":amount,"decimals":6,"uiAmount":null,"uiAmountString":"DO_NOT_USE"}});
        let v = json!({"slot":55,"blockTime":123,"transaction":{"signatures":[sig.to_string()],"message":{"header":{"numRequiredSignatures":1,"numReadonlySignedAccounts":0,"numReadonlyUnsignedAccounts":0},"accountKeys":[wallet.to_string(),Pubkey::new_unique().to_string()],"recentBlockhash":solana_sdk::hash::Hash::new_unique().to_string(),"instructions":[]}},"meta":{"err":null,"status":{"Ok":null},"fee":7123,"preBalances":[100000,2039280],"postBalances":[92877,2039280],"preTokenBalances":[token("18446744073709551000")],"postTokenBalances":[token("18446744073709551007")],"loadedAddresses":{"writable":[],"readonly":[]}}});
        let expected = ["SOL".into(), mint.clone()].into_iter().collect();
        (v, sig, wallet, mint, expected)
    }
    fn parse(
        v: Value,
        sig: &Signature,
        wallet: &Pubkey,
        expected: &BTreeSet<String>,
    ) -> Result<FinalizedObservation, String> {
        parse_finalized(
            serde_json::from_value(v).unwrap(),
            sig,
            wallet,
            50,
            expected,
        )
    }
    #[test]
    fn finalized_success_and_revert_preserve_actual_fees_and_integer_deltas() {
        for reverted in [false, true] {
            let (mut v, sig, wallet, mint, expected) = fixture();
            if reverted {
                v["meta"]["err"] = json!({"InstructionError":[0,{"Custom":1}]});
                v["meta"]["status"] = json!({"Err":{"InstructionError":[0,{"Custom":1}]}});
                v["meta"]["postTokenBalances"] = v["meta"]["preTokenBalances"].clone();
            }
            let r = parse(v, &sig, &wallet, &expected).unwrap();
            assert_eq!(r.success, !reverted);
            assert_eq!(r.fee_lamports, 7123);
            assert_eq!(r.wallet_sol_delta_lamports, -7123);
            assert_eq!(
                r.wallet_token_deltas.unwrap()[&mint],
                if reverted { 0 } else { 7 }
            );
            assert!(r.unknown_reasons.is_empty());
        }
    }
    #[test]
    fn missing_fee_or_metadata_signature_wallet_slot_and_key_mismatch_are_not_receipts() {
        for case in 0..7 {
            let (mut v, sig, wallet, _, expected) = fixture();
            match case {
                0 => v["meta"] = Value::Null,
                1 => v["transaction"]["signatures"][0] = json!(Signature::new_unique().to_string()),
                2 => v["slot"] = json!(49),
                3 => {
                    v["transaction"]["message"]["accountKeys"][0] =
                        json!(Pubkey::new_unique().to_string())
                }
                4 => v["meta"]["postBalances"] = json!([1]),
                5 => {
                    v["meta"]["loadedAddresses"]["writable"] =
                        json!([Pubkey::new_unique().to_string()])
                }
                _ => {
                    v["meta"].as_object_mut().unwrap().remove("fee");
                }
            }
            let parsed = serde_json::from_value(v);
            assert!(
                parsed.is_err()
                    || parse_finalized(parsed.unwrap(), &sig, &wallet, 50, &expected).is_err(),
                "case {case}"
            );
        }
    }
    #[test]
    fn incomplete_token_metadata_preserves_fee_but_never_invents_zero() {
        for case in 0..8 {
            let (mut v, sig, wallet, _, expected) = fixture();
            match case {
                0 => {
                    v["meta"]
                        .as_object_mut()
                        .unwrap()
                        .remove("preTokenBalances");
                }
                1 => {
                    v["meta"]["preTokenBalances"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("owner");
                }
                2 => {
                    v["meta"]["postTokenBalances"][0]["uiTokenAmount"]["amount"] =
                        json!("18446744073709551616")
                }
                3 => {
                    let dup = v["meta"]["preTokenBalances"][0].clone();
                    v["meta"]["preTokenBalances"]
                        .as_array_mut()
                        .unwrap()
                        .push(dup);
                }
                4 => v["meta"]["preTokenBalances"][0]["accountIndex"] = json!(99),
                5 => {
                    v["meta"]["postTokenBalances"][0]["mint"] =
                        json!(Pubkey::new_unique().to_string())
                }
                6 => {
                    v["meta"]["postTokenBalances"][0]["owner"] =
                        json!(Pubkey::new_unique().to_string())
                }
                _ => {
                    v["meta"]["preTokenBalances"] = json!([]);
                    v["meta"]["postTokenBalances"] = json!([]);
                }
            }
            let r = parse(v, &sig, &wallet, &expected).unwrap();
            assert_eq!(r.fee_lamports, 7123);
            assert!(r.wallet_token_deltas.is_none(), "case {case}");
            assert!(!r.unknown_reasons.is_empty());
        }
    }
    #[test]
    fn account_creation_close_and_loaded_keys_require_balance_evidence() {
        for close in [false, true] {
            let (mut v, sig, wallet, mint, expected) = fixture();
            if close {
                v["meta"]["postTokenBalances"] = json!([]);
            } else {
                v["meta"]["preTokenBalances"] = json!([]);
            }
            assert!(parse(v.clone(), &sig, &wallet, &expected)
                .unwrap()
                .wallet_token_deltas
                .is_none());
            v["meta"][if close { "postBalances" } else { "preBalances" }][1] = json!(0);
            let r = parse(v.clone(), &sig, &wallet, &expected).unwrap();
            assert_eq!(
                r.wallet_token_deltas.unwrap()[&mint],
                if close {
                    -18446744073709551000
                } else {
                    18446744073709551007
                }
            );
            let loaded = v["transaction"]["message"]["accountKeys"]
                .as_array_mut()
                .unwrap()
                .pop()
                .unwrap();
            v["transaction"]["message"]["addressTableLookups"] = json!([{"accountKey":Pubkey::new_unique().to_string(),"writableIndexes":[0],"readonlyIndexes":[]}]);
            v["meta"]["loadedAddresses"]["writable"] = json!([loaded]);
            assert!(parse(v.clone(), &sig, &wallet, &expected)
                .unwrap()
                .wallet_token_deltas
                .is_some());
            v["meta"].as_object_mut().unwrap().remove("loadedAddresses");
            assert!(parse(v, &sig, &wallet, &expected).is_err());
        }
    }
    #[test]
    fn multiple_wallet_accounts_aggregate_by_mint_without_other_owners() {
        let (mut v, sig, wallet, mint, expected) = fixture();
        for index in [2, 3] {
            v["transaction"]["message"]["accountKeys"]
                .as_array_mut()
                .unwrap()
                .push(json!(Pubkey::new_unique().to_string()));
            for side in ["preBalances", "postBalances"] {
                v["meta"][side].as_array_mut().unwrap().push(json!(2039280));
            }
            for (side, amount) in [("preTokenBalances", "100"), ("postTokenBalances", "91")] {
                let mut row = v["meta"][side][0].clone();
                row["accountIndex"] = json!(index);
                row["uiTokenAmount"]["amount"] = json!(amount);
                if index == 3 {
                    row["owner"] = json!("11111111111111111111111111111111");
                }
                v["meta"][side].as_array_mut().unwrap().push(row);
            }
        }
        let r = parse(v, &sig, &wallet, &expected).unwrap();
        assert_eq!(r.wallet_token_deltas.unwrap()[&mint], -2);
    }
    #[test]
    fn finalized_partial_bundle_and_missing_tokens_persist_without_releasing_holds() {
        use solana_client::rpc_request::RpcRequest;
        for missing_tokens in [false, true] {
            let (dir, mut a, mut i, mut current) = super::super::tests::setup();
            let (mut v, sig, wallet, _, expected) = fixture();
            if missing_tokens {
                v["meta"]
                    .as_object_mut()
                    .unwrap()
                    .remove("postTokenBalances");
            }
            a.wallet = wallet.to_string();
            i.wallet = a.wallet.clone();
            i.expected_wallet_assets = expected;
            i.signatures = vec![sig.to_string(), Signature::new_unique().to_string()];
            current.bundle_simulated = true;
            let rpc = RpcClient::new_mock_with_mocks(
                "succeeds",
                [(RpcRequest::GetTransaction, v)].into_iter().collect(),
            );
            let mut j = Journal::open(&dir, &a.wallet).unwrap();
            j.reserve(a.clone(), i.clone(), &current).unwrap();
            j.handoff(&i.signed_wire_sha256, &current, || Ok(()))
                .unwrap();
            // C01: through the productive entry (the settlement read), not a second observer.
            let step = j.settle_step(&rpc, &|_, _| None, 1_000).unwrap();
            assert!(
                matches!(step, super::super::settlement::SettleStep::Unknown(_)),
                "{step:?}"
            );
            if missing_tokens {
                assert!(j.saved.as_ref().unwrap().observations[&sig.to_string()]
                    .unknown_reasons
                    .iter()
                    .any(|r| r.contains("TOKEN_BALANCE_METADATA_MISSING")));
            }
            assert_eq!(j.holds(), i.holds);
            assert_eq!(j.saved.as_ref().unwrap().phase, Phase::HandoffUnknown);
            assert_eq!(
                j.saved.as_ref().unwrap().observations[&sig.to_string()].fee_lamports,
                7123
            );
            assert_eq!(j.saved.as_ref().unwrap().observations.len(), 1);
            drop(j);
            let reopened = Journal::open(&dir, &a.wallet).unwrap();
            assert_eq!(reopened.holds(), i.holds);
            assert_eq!(reopened.saved.as_ref().unwrap().observations.len(), 1);
            drop(reopened);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn finalized_rpc_request_uses_shared_transport_with_no_submission() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            time::Duration,
        };
        let (v, sig, wallet, _, expected) = fixture();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            let request = loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                assert!(bytes.len() < 65536);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                            .unwrap();
                    }
                }
            };
            assert_eq!(request["method"], "getTransaction");
            assert_eq!(request["params"][1]["commitment"], "finalized");
            assert_eq!(request["params"][1]["encoding"], "json");
            let body = json!({"jsonrpc":"2.0","id":request["id"],"result":v}).to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        let rpc = RpcClient::new_with_timeout(url, Duration::from_secs(3));
        let result = fetch_finalized(&rpc, &sig, &wallet, 50, &expected);
        server.join().unwrap();
        assert_eq!(result.unwrap().fee_lamports, 7123);
    }
}
