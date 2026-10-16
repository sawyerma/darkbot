//! Historical transaction enrichment and identity checks.
use super::*;

// ------------------------------------------------------------------------------------------
// On-chain enrichment from a full transaction object ({transaction, meta, version})
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenDelta {
    pub mint: String,
    pub native_delta: i128,
    pub decimals: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TxEnrichment {
    pub meta_available: bool,
    pub tx_version: Option<String>,
    pub fee_payer: Option<String>,
    pub tx_fee_lamports: Option<u64>,
    pub receiver_lamport_delta: Option<i128>,
    pub receiver_token_deltas: Vec<TokenDelta>,
    pub token_deltas_complete: bool,
    pub cu_limit: Option<u32>,
    pub cu_price_micro_lamports: Option<u64>,
}

pub(super) fn version_label(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => format!("v{n}"),
        _ => "unknown".to_string(),
    }
}

pub(super) fn key_str(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_string)
        .or_else(|| v.get("pubkey").and_then(Value::as_str).map(str::to_string))
}

/// static accountKeys + meta.loadedAddresses.writable + meta.loadedAddresses.readonly.
/// `None` if lookups exist but loaded addresses are missing (indices would be ambiguous).
pub(super) fn full_account_keys(message: &Value, meta: Option<&Value>) -> Option<Vec<String>> {
    let statics = message.get("accountKeys")?.as_array()?;
    let mut keys = Vec::with_capacity(statics.len());
    for k in statics {
        keys.push(key_str(k)?);
    }
    let has_lookups = message
        .get("addressTableLookups")
        .and_then(Value::as_array)
        .is_some_and(|l| !l.is_empty());
    match meta
        .and_then(|m| m.get("loadedAddresses"))
        .filter(|l| l.is_object())
    {
        Some(loaded) => {
            for part in ["writable", "readonly"] {
                if let Some(arr) = loaded.get(part).and_then(Value::as_array) {
                    for k in arr {
                        keys.push(key_str(k)?);
                    }
                }
            }
        }
        None if has_lookups => return None,
        None => {}
    }
    Some(keys)
}

pub(super) fn lamport_delta(keys: &[String], meta: &Value, receiver: &str) -> Option<i128> {
    let mut positions = keys
        .iter()
        .enumerate()
        .filter(|(_, k)| k.as_str() == receiver)
        .map(|(i, _)| i);
    let i = positions.next()?;
    if positions.next().is_some() {
        return None; // not unique -> do not guess
    }
    let pre = meta.get("preBalances")?.as_array()?.get(i)?.as_u64()?;
    let post = meta.get("postBalances")?.as_array()?.get(i)?.as_u64()?;
    Some(i128::from(post) - i128::from(pre))
}

pub(super) fn token_deltas(meta: &Value, receiver: &str) -> (Vec<TokenDelta>, bool) {
    let mut acc: BTreeMap<String, (i128, Option<u64>)> = BTreeMap::new();
    let mut complete = true;
    for (field, sign) in [("preTokenBalances", -1i128), ("postTokenBalances", 1i128)] {
        let Some(arr) = meta.get(field).and_then(Value::as_array) else {
            complete = false;
            continue;
        };
        for b in arr {
            if b.get("owner").and_then(Value::as_str) != Some(receiver) {
                continue;
            }
            let mint = b.get("mint").and_then(Value::as_str);
            let amount = b
                .pointer("/uiTokenAmount/amount")
                .and_then(Value::as_str)
                .and_then(|a| a.parse::<i128>().ok());
            let (Some(mint), Some(amount)) = (mint, amount) else {
                complete = false;
                continue;
            };
            let decimals = b.pointer("/uiTokenAmount/decimals").and_then(Value::as_u64);
            let entry = acc.entry(mint.to_string()).or_insert((0, decimals));
            entry.0 += sign * amount;
            if entry.1.is_none() {
                entry.1 = decimals;
            }
        }
    }
    let deltas = acc
        .into_iter()
        .map(|(mint, (native_delta, decimals))| TokenDelta {
            mint,
            native_delta,
            decimals,
        })
        .collect();
    (deltas, complete)
}

/// Legacy/v0 ComputeBudget instructions: 2 = SetComputeUnitLimit(u32), 3 = SetComputeUnitPrice(u64).
/// v1 transactions carry compute budget in their transaction config, not as instructions -> None.
pub(super) fn compute_budget(message: &Value, keys: &[String]) -> (Option<u32>, Option<u64>) {
    let (mut limit, mut price) = (None, None);
    let Some(ixs) = message.get("instructions").and_then(Value::as_array) else {
        return (None, None);
    };
    for ix in ixs {
        let Some(pi) = ix.get("programIdIndex").and_then(Value::as_u64) else {
            continue;
        };
        let is_cb = usize::try_from(pi)
            .ok()
            .and_then(|i| keys.get(i))
            .map(String::as_str)
            == Some(COMPUTE_BUDGET_PROGRAM);
        if !is_cb {
            continue;
        }
        let Some(data) = ix
            .get("data")
            .and_then(Value::as_str)
            .and_then(|d| bs58::decode(d).into_vec().ok())
        else {
            continue;
        };
        match data.first() {
            Some(2) if data.len() >= 5 => {
                limit = Some(u32::from_le_bytes([data[1], data[2], data[3], data[4]]))
            }
            Some(3) if data.len() >= 9 => {
                let mut a = [0u8; 8];
                a.copy_from_slice(&data[1..9]);
                price = Some(u64::from_le_bytes(a));
            }
            _ => {}
        }
    }
    (limit, price)
}

/// Enrich one receiver from a full transaction object (`{transaction:{signatures,message}, meta, version}`,
/// i.e. `getTransaction` result or Helius `transactionNotification.result.transaction`).
pub fn enrich(tx: &Value, receiver: &Pubkey) -> TxEnrichment {
    let meta = tx.get("meta").filter(|m| m.is_object());
    let message = tx.pointer("/transaction/message").filter(|m| m.is_object());
    let receiver_s = receiver.to_string();
    let keys = message.and_then(|m| full_account_keys(m, meta));
    let mut e = TxEnrichment {
        meta_available: meta.is_some(),
        tx_version: tx.get("version").map(version_label),
        fee_payer: keys.as_ref().and_then(|k| k.first().cloned()),
        ..TxEnrichment::default()
    };
    if let Some(m) = meta {
        e.tx_fee_lamports = m.get("fee").and_then(Value::as_u64);
        if let Some(k) = &keys {
            e.receiver_lamport_delta = lamport_delta(k, m, &receiver_s);
        }
        let (deltas, complete) = token_deltas(m, &receiver_s);
        e.receiver_token_deltas = deltas;
        e.token_deltas_complete = complete;
    }
    if let (Some(msg), Some(k)) = (message, &keys) {
        let (limit, price) = compute_budget(msg, k);
        e.cu_limit = limit;
        e.cu_price_micro_lamports = price;
    }
    e
}

/// The fetched transaction must be exactly the observed one (first signature and slot).
pub fn verify_tx_identity(tx: &Value, signature: &str, slot: u64) -> Result<(), String> {
    match tx
        .pointer("/transaction/signatures/0")
        .and_then(Value::as_str)
    {
        Some(s) if s == signature => {}
        Some(s) => {
            return Err(format!(
                "signature mismatch requested={signature} returned={s}"
            ))
        }
        None => return Err("signature missing".into()),
    }
    match tx.get("slot").and_then(Value::as_u64) {
        Some(s) if s == slot => Ok(()),
        Some(s) => Err(format!("slot mismatch observed={slot} returned={s}")),
        None => Err("slot missing".into()),
    }
}
