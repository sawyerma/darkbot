//! Historical transaction enrichment and identity checks.
use super::*;
use darkbot_solana::chain::ChainMeta;

// ------------------------------------------------------------------------------------------
// On-chain enrichment from a decoded transaction (`darkbot_solana::chain::ChainTx`)
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TokenDelta {
    pub mint: String,
    pub native_delta: i128,
    pub decimals: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Total priority fee in lamports (v1 config, or legacy/v0 limit × price).
    pub priority_fee_lamports: Option<u64>,
}

/// Unique position of `receiver` among the resolved account keys; ambiguity is never guessed.
fn receiver_position(keys: &[Pubkey], receiver: &Pubkey) -> Option<usize> {
    let mut positions = keys
        .iter()
        .enumerate()
        .filter(|(_, k)| *k == receiver)
        .map(|(i, _)| i);
    let i = positions.next()?;
    positions.next().is_none().then_some(i)
}

pub(super) fn lamport_delta(keys: &[Pubkey], meta: &ChainMeta, receiver: &Pubkey) -> Option<i128> {
    let i = receiver_position(keys, receiver)?;
    let pre = *meta.pre_balances.as_ref()?.get(i)?;
    let post = *meta.post_balances.as_ref()?.get(i)?;
    Some(i128::from(post) - i128::from(pre))
}

/// Net token change per mint for token accounts owned by `receiver` (exact atomic units).
/// Incomplete when either balance list is missing.
pub(super) fn token_deltas(meta: &ChainMeta, receiver: &Pubkey) -> (Vec<TokenDelta>, bool) {
    let mut acc: BTreeMap<String, (i128, Option<u64>)> = BTreeMap::new();
    let mut complete = true;
    for (list, sign) in [
        (&meta.pre_token_balances, -1i128),
        (&meta.post_token_balances, 1i128),
    ] {
        let Some(list) = list else {
            complete = false;
            continue;
        };
        for b in list {
            // Unchanged semantics: balances not owned by the receiver (incl. no owner) are skipped.
            if b.owner != Some(*receiver) {
                continue;
            }
            let entry = acc
                .entry(b.mint.to_string())
                .or_insert((0, Some(u64::from(b.decimals))));
            entry.0 += sign * i128::from(b.amount);
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

/// Compute limit, CU price and total priority fee. Legacy/v0: ComputeBudget instructions
/// 2 = SetComputeUnitLimit(u32), 3 = SetComputeUnitPrice(u64 micro-lamports), priority fee
/// = ceil(limit × price / 1e6). v1: `transactionConfig` (`computeUnitLimit`, `priorityFee` in
/// lamports, solana-message 4.4.1 `v1::TransactionConfig`).
pub(super) fn compute_budget(
    tx: &ChainTx,
    keys: &[Pubkey],
) -> (Option<u32>, Option<u64>, Option<u64>) {
    if let Some(c) = tx.message.transaction_config {
        return (c.compute_unit_limit, None, c.priority_fee);
    }
    let program = Pubkey::from_str(COMPUTE_BUDGET_PROGRAM).expect("static");
    let (mut limit, mut price) = (None, None);
    for ix in &tx.message.instructions {
        if keys.get(usize::from(ix.program_id_index)) != Some(&program) {
            continue;
        }
        let data = &ix.data;
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
    let priority = limit
        .zip(price)
        .and_then(|(l, p)| u64::try_from((u128::from(l) * u128::from(p)).div_ceil(1_000_000)).ok());
    (limit, price, priority)
}

/// Enrich one receiver from a decoded transaction (`getTransaction`, block entry or
/// `transactionNotification`).
pub fn enrich(tx: &ChainTx, receiver: &Pubkey) -> TxEnrichment {
    let keys = tx.account_keys().ok();
    let mut e = TxEnrichment {
        meta_available: tx.meta.is_some(),
        tx_version: tx.version.map(|_| tx.version_label()),
        fee_payer: keys
            .as_ref()
            .and_then(|k| k.first())
            .map(ToString::to_string),
        ..TxEnrichment::default()
    };
    if let Some(m) = &tx.meta {
        e.tx_fee_lamports = m.fee;
        if let Some(k) = &keys {
            e.receiver_lamport_delta = lamport_delta(k, m, receiver);
        }
        let (deltas, complete) = token_deltas(m, receiver);
        e.receiver_token_deltas = deltas;
        e.token_deltas_complete = complete;
    }
    if let Some(k) = &keys {
        (
            e.cu_limit,
            e.cu_price_micro_lamports,
            e.priority_fee_lamports,
        ) = compute_budget(tx, k);
    }
    e
}
