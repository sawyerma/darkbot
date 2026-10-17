//! Ticket 3 A4: canonical facts of a decoded transaction or logs notification for the offline
//! differential against the pinned Python verifier. Both sides read the same historical provider
//! bytes; this module only flattens the Rust decode (`chain`, `truth::classify`,
//! `decode_receiver_events`, compute budget) into one comparable line. Not a production path.
use super::*;
use darkbot_solana::chain::{ChainLogs, TxWireVersion};

/// The six market markers by their pinned instruction discriminators. `start_deleverage` has no
/// pinned constant in this crate and is derived (Anchor `sha256("global:<name>")[..8]`).
fn markers() -> [([u8; 8], &'static str); 6] {
    use darkbot_project0::protocol::ix;
    let derived = |name: &str| -> [u8; 8] {
        solana_sdk::hash::hash(format!("global:{name}").as_bytes()).to_bytes()[..8]
            .try_into()
            .unwrap()
    };
    [
        (
            [214, 169, 151, 213, 251, 167, 86, 219],
            "lending_account_liquidate",
        ),
        (ix::START_LIQUIDATION, "start_liquidation"),
        (ix::END_LIQUIDATION, "end_liquidation"),
        (derived("start_deleverage"), "start_deleverage"),
        ([114, 14, 250, 143, 252, 104, 214, 209], "end_deleverage"),
        (
            [162, 11, 56, 139, 90, 128, 70, 173],
            "lending_pool_handle_bankruptcy",
        ),
    ]
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or("-".into(), |v| v.to_string())
}

/// `TX <sig> <slot> status= fee= priority= version= payer= markers= events= class=`
pub fn transaction_line(tx: &ChainTx, program: &Pubkey) -> String {
    let status = match tx.status() {
        TxStatus::Success => "SUCCESS",
        TxStatus::Failed(_) => "FAILED",
        TxStatus::Unknown(_) => "UNKNOWN",
    };
    let keys = tx.account_keys().ok();
    let (_, _, priority) = keys
        .as_ref()
        .map(|k| super::enrichment::compute_budget(tx, k))
        .unwrap_or((None, None, None));
    let version = match tx.version {
        Some(TxWireVersion::Legacy) | None => "legacy".to_string(),
        Some(TxWireVersion::Number(n)) => n.to_string(),
    };
    let mut marks = Vec::new();
    if let (Some(keys), Some(meta)) = (&keys, &tx.meta) {
        let table = markers();
        let inner = meta.inner_instructions.as_deref().unwrap_or(&[]);
        for (i, ix) in tx.message.instructions.iter().enumerate() {
            let group = inner.iter().find(|g| usize::from(g.index) == i);
            let all = std::iter::once((None, ix)).chain(group.into_iter().flat_map(|g| {
                g.instructions
                    .iter()
                    .enumerate()
                    .map(|(j, ix)| (Some(j), ix))
            }));
            for (j, ix) in all {
                if keys.get(usize::from(ix.program_id_index)) != Some(program) {
                    continue;
                }
                if let Some((_, name)) = table.iter().find(|(d, _)| ix.data.get(..8) == Some(d)) {
                    marks.push(format!("{name}@{i}.{}", opt(j)));
                }
            }
        }
    }
    marks.sort();
    let (events, class) = match (
        &tx.status(),
        truth::classify(tx, program, tx.block_time, None),
    ) {
        (TxStatus::Failed(_), _) => (Vec::new(), "FAILED_TRANSACTION"),
        (_, Ok(events)) => (
            events
                .iter()
                .map(|e| {
                    format!(
                        "{}:{}:{}:{}:{}",
                        e.liquidation_type.as_str(),
                        e.target,
                        e.liquidator,
                        e.event_index,
                        e.seized_asset_native_exact.as_deref().unwrap_or("-")
                    )
                })
                .collect(),
            "CLASSIFIED",
        ),
        (_, Err(_)) => (Vec::new(), "UNKNOWN"),
    };
    format!(
        "TX {} {} status={status} fee={} priority={} version={version} payer={} markers={} events={} class={class}",
        tx.signature().unwrap_or("-"),
        tx.slot,
        opt(tx.meta.as_ref().and_then(|m| m.fee)),
        opt(priority),
        opt(keys.as_ref().and_then(|k| k.first())),
        marks.join(","),
        events.join(";")
    )
}

/// `LOGS <sig> <slot> status= truncated= events=` (events sorted `TYPE:target:actor`).
pub fn logs_line(l: &ChainLogs, program: &Pubkey) -> String {
    let status = match l.status {
        TxStatus::Success => "SUCCESS",
        TxStatus::Failed(_) => "FAILED",
        TxStatus::Unknown(_) => "UNKNOWN",
    };
    let logs = l.logs.as_deref().unwrap_or(&[]);
    let d = decode_receiver_events(logs, program);
    let mut events: Vec<String> = d
        .events
        .iter()
        .map(|e| format!("RECEIVERSHIP:{}:{}", e.target, e.receiver))
        .chain(
            d.additional_events
                .iter()
                .map(|e| format!("{}:{}:{}", e.kind.as_str(), e.target, e.actor)),
        )
        .collect();
    events.sort();
    format!(
        "LOGS {} {} status={status} truncated={} events={}",
        l.signature,
        l.slot,
        d.truncated,
        events.join(";")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical line of a real v1 receivership (5 000 lamport signature fee + 14 000
    /// lamport v1 priority fee = meta.fee 19 000) as the differential compares it.
    #[test]
    fn real_v1_receivership_line() {
        let raw: Value = serde_json::from_str(include_str!(
            "../../../../../docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json"
        ))
        .unwrap();
        let tx = darkbot_solana::chain::decode_transaction(&raw).unwrap();
        let program = Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap();
        let line = transaction_line(&tx, &program);
        assert!(
            line.contains("status=SUCCESS fee=19000 priority=14000 version=1"),
            "{line}"
        );
        assert!(
            line.contains("markers=end_liquidation@4.-,start_liquidation@0.-"),
            "{line}"
        );
        assert!(
            line.contains("events=RECEIVERSHIP:3GqwCURBtJZfdQxMSyZ1VyWNSB5WXovg7Q8Q46HY7ihx:"),
            "{line}"
        );
        assert!(line.ends_with("class=CLASSIFIED"), "{line}");
    }
}
