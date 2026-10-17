//! Ticket 3 A4: canonical facts of a decoded transaction or logs notification for the offline
//! differential against the pinned Python verifier. Both sides read the same historical provider
//! bytes; this module only flattens the Rust decode (`chain`, `truth::classify`,
//! `decode_receiver_events`, compute budget) into one comparable line. Not a production path.
use super::*;
use darkbot_solana::chain::{ChainLogs, TxWireVersion};

/// The six market markers with their pinned instruction discriminators (source pin `33c67987`:
/// `marginfi_type_crate::constants::ix_discriminators` for the four start/end markers, the
/// classifier's pinned constants for `lending_account_liquidate` and
/// `lending_pool_handle_bankruptcy`). A test binds every value to the pinned crate and to the
/// Anchor rule `sha256("global:<name>")[..8]` of the pinned program entrypoints.
pub const MARKERS: [([u8; 8], &str); 6] = [
    (
        [214, 169, 151, 213, 251, 167, 86, 219],
        "lending_account_liquidate",
    ),
    ([244, 93, 90, 214, 192, 166, 191, 21], "start_liquidation"),
    ([110, 11, 244, 54, 229, 181, 22, 184], "end_liquidation"),
    ([10, 138, 10, 57, 40, 232, 182, 193], "start_deleverage"),
    ([114, 14, 250, 143, 252, 104, 214, 209], "end_deleverage"),
    (
        [162, 11, 56, 139, 90, 128, 70, 173],
        "lending_pool_handle_bankruptcy",
    ),
];

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or("-".into(), |v| v.to_string())
}

/// Line of a transaction the Rust boundary cannot decode (kept aligned, never dropped).
pub fn decode_error_line(signature: &str, slot: u64, code: &str) -> String {
    format!("TX {signature} {slot} status=UNKNOWN fee=- priority=- version=- payer=- sigs=- markers= events= class=UNKNOWN reasons={code}")
}

/// `TX <sig> <slot> status= fee= priority= version= payer= sigs= markers= events= class= reasons=`.
/// Markers and events keep their execution/log order (no sorting): position, pairing order and
/// duplicates are compared as decided.
pub fn transaction_line(tx: &ChainTx, program: &Pubkey) -> String {
    let mut reasons = Vec::new();
    let status = match tx.status() {
        TxStatus::Success => "SUCCESS",
        TxStatus::Failed(_) => "FAILED",
        TxStatus::Unknown(code) => {
            reasons.push(code);
            "UNKNOWN"
        }
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
                if let Some((_, name)) = MARKERS.iter().find(|(d, _)| ix.data.get(..8) == Some(d)) {
                    marks.push(format!("{name}@{i}.{}", opt(j)));
                }
            }
        }
    }
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
        (_, Err(code)) => {
            if !reasons.contains(&code) {
                reasons.push(code);
            }
            (Vec::new(), "UNKNOWN")
        }
    };
    format!(
        "TX {} {} status={status} fee={} priority={} version={version} payer={} sigs={} markers={} events={} class={class} reasons={}",
        tx.signature().unwrap_or("-"),
        tx.slot,
        opt(tx.meta.as_ref().and_then(|m| m.fee)),
        opt(priority),
        opt(keys.as_ref().and_then(|k| k.first())),
        tx.signatures.len(),
        marks.join(","),
        events.join(";"),
        reasons.join(",")
    )
}

/// `LOGS <sig> <slot> status= truncated= events= reasons=`: events `TYPE:target:actor` in log
/// order (receiver events, then the other classes, each in log order).
pub fn logs_line(l: &ChainLogs, program: &Pubkey) -> String {
    let mut reasons = Vec::new();
    let status = match l.status {
        TxStatus::Success => "SUCCESS",
        TxStatus::Failed(_) => "FAILED",
        TxStatus::Unknown(code) => {
            reasons.push(code.to_string());
            "UNKNOWN"
        }
    };
    let logs = l.logs.as_deref().unwrap_or(&[]);
    let d = decode_receiver_events(logs, program);
    let events: Vec<String> = d
        .events
        .iter()
        .map(|e| format!("RECEIVERSHIP:{}:{}", e.target, e.receiver))
        .chain(
            d.additional_events
                .iter()
                .map(|e| format!("{}:{}:{}", e.kind.as_str(), e.target, e.actor)),
        )
        .collect();
    if d.decoder_errors > 0 {
        reasons.push(format!("DECODER_ERRORS_{}", d.decoder_errors));
    }
    format!(
        "LOGS {} {} status={status} truncated={} events={} reasons={}",
        l.signature,
        l.slot,
        d.truncated,
        events.join(";"),
        reasons.join(",")
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
        assert!(line.contains(" sigs=1 "), "{line}");
        // Execution order is kept: start before end, top-level positions.
        assert!(
            line.contains("markers=start_liquidation@0.-,end_liquidation@4.-"),
            "{line}"
        );
        assert!(
            line.contains("events=RECEIVERSHIP:3GqwCURBtJZfdQxMSyZ1VyWNSB5WXovg7Q8Q46HY7ihx:"),
            "{line}"
        );
        assert!(line.ends_with("class=CLASSIFIED reasons="), "{line}");
    }

    /// Every marker discriminator equals the Anchor rule for the pinned program entrypoint and,
    /// where the pinned crate defines it, the pinned constant (`start_deleverage` included).
    #[test]
    fn marker_discriminators_match_pinned_sources() {
        for (d, name) in MARKERS {
            let derived = solana_sdk::hash::hash(format!("global:{name}").as_bytes()).to_bytes();
            assert_eq!(&derived[..8], &d, "{name}");
        }
        #[cfg(feature = "crate_health")]
        {
            use marginfi_type_crate::constants::ix_discriminators as pinned;
            let by = |n: &str| MARKERS.iter().find(|(_, m)| *m == n).unwrap().0;
            assert_eq!(by("start_liquidation"), pinned::START_LIQUIDATION);
            assert_eq!(by("end_liquidation"), pinned::END_LIQUIDATION);
            assert_eq!(by("start_deleverage"), pinned::START_DELEVERAGE);
            assert_eq!(by("end_deleverage"), pinned::END_DELEVERAGE);
        }
        assert_eq!(
            MARKERS[1].0,
            darkbot_project0::protocol::ix::START_LIQUIDATION
        );
        assert_eq!(
            MARKERS[2].0,
            darkbot_project0::protocol::ix::END_LIQUIDATION
        );
    }
}
