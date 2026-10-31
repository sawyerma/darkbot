//! S4-02 (PHASENPLAN S4: Decode against the official crates): the core's own JSON-RPC wire
//! decoder (`chain::decode_transaction`) against the official RPC client types
//! (`solana-transaction-status` 3.1.12) on the six committed real `getTransaction` answers of the
//! S4-01 corpus (`docs/evidence/AUDIT_20261005`): every field both read is equal — slot, block
//! time, version, signatures, header, keys, blockhash, instructions, lookups, status, fee,
//! balances, token balances, inner instructions, logs, loaded addresses, compute and cost units.
//! The v1 configuration, which the official types do not model, is proven exact by the winners'
//! signatures over the rebuilt message (`s4_tx_parity`). The official top level
//! (`EncodedConfirmedTransactionWithStatusMeta`, a `flatten`) cannot read a number of this
//! workspace's `serde_json` (`arbitrary_precision`): its slot and block time are compared as JSON.
use darkbot_solana::chain::{ChainInstruction, TxStatus, TxWireVersion};
use serde_json::json;
use solana_sdk::transaction::TransactionVersion;
use solana_transaction_status::option_serializer::OptionSerializer;
use solana_transaction_status::{
    EncodedTransaction, EncodedTransactionWithStatusMeta, UiCompiledInstruction, UiInstruction,
    UiMessage,
};
use std::path::Path;

fn some<T: Clone>(o: &OptionSerializer<T>) -> Option<T> {
    match o {
        OptionSerializer::Some(v) => Some(v.clone()),
        _ => None,
    }
}

fn instruction(ui: &UiCompiledInstruction) -> ChainInstruction {
    ChainInstruction {
        program_id_index: ui.program_id_index,
        accounts: ui.accounts.clone(),
        data: bs58::decode(&ui.data).into_vec().unwrap(),
        stack_height: ui.stack_height,
    }
}

fn keys(v: &[solana_sdk::pubkey::Pubkey]) -> Vec<String> {
    v.iter().map(ToString::to_string).collect()
}

#[test]
fn s4_02_the_wire_decoder_reads_every_winner_as_the_official_types() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/evidence/AUDIT_20261005");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_stem().is_some_and(|s| s.len() >= 80)
        })
        .collect();
    files.sort();
    assert_eq!(files.len(), 6, "the S4-01 corpus");
    for path in files {
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let ours = darkbot_solana::chain::decode_transaction(&raw).unwrap();
        let official: EncodedTransactionWithStatusMeta = serde_json::from_value(json!({
            "transaction": raw["transaction"],
            "meta": raw["meta"],
            "version": raw["version"],
        }))
        .unwrap();
        let name = format!("{path:?}");

        assert_eq!(Some(ours.slot), raw["slot"].as_u64(), "{name}");
        assert_eq!(ours.block_time, raw["blockTime"].as_i64(), "{name}");
        let tx = &official;
        assert_eq!(
            ours.version,
            tx.version.as_ref().map(|v| match v {
                TransactionVersion::Legacy(_) => TxWireVersion::Legacy,
                TransactionVersion::Number(n) => TxWireVersion::Number(*n),
            }),
            "{name}"
        );
        let EncodedTransaction::Json(ui) = &tx.transaction else {
            panic!("{name}: json encoding");
        };
        let UiMessage::Raw(m) = &ui.message else {
            panic!("{name}: raw message");
        };
        assert_eq!(ours.signatures, ui.signatures, "{name}");
        let message = &ours.message;
        assert_eq!(
            message.header,
            Some((
                m.header.num_required_signatures,
                m.header.num_readonly_signed_accounts,
                m.header.num_readonly_unsigned_accounts
            )),
            "{name}"
        );
        assert_eq!(keys(&message.account_keys), m.account_keys, "{name}");
        assert_eq!(
            message.recent_blockhash.as_deref(),
            Some(m.recent_blockhash.as_str()),
            "{name}"
        );
        assert_eq!(
            message.instructions,
            m.instructions.iter().map(instruction).collect::<Vec<_>>(),
            "{name}"
        );
        let lookups = |l: &[darkbot_solana::chain::ChainLookup]| {
            l.iter()
                .map(|l| {
                    (
                        l.account_key.to_string(),
                        l.writable_indexes.clone(),
                        l.readonly_indexes.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            message.address_table_lookups.as_deref().map(lookups),
            m.address_table_lookups.as_ref().map(|v| v
                .iter()
                .map(|l| (
                    l.account_key.clone(),
                    l.writable_indexes.clone(),
                    l.readonly_indexes.clone()
                ))
                .collect::<Vec<_>>()),
            "{name}"
        );

        let (meta, theirs) = (ours.meta.as_ref().unwrap(), tx.meta.as_ref().unwrap());
        assert_eq!(meta.status, TxStatus::Success, "{name}");
        assert!(theirs.err.is_none() && theirs.status.is_ok(), "{name}");
        assert_eq!(meta.fee, Some(theirs.fee), "{name}");
        assert_eq!(
            meta.pre_balances.as_ref(),
            Some(&theirs.pre_balances),
            "{name}"
        );
        assert_eq!(
            meta.post_balances.as_ref(),
            Some(&theirs.post_balances),
            "{name}"
        );
        assert_eq!(meta.log_messages, some(&theirs.log_messages), "{name}");
        assert_eq!(
            meta.compute_units_consumed,
            some(&theirs.compute_units_consumed),
            "{name}"
        );
        assert_eq!(meta.cost_units, some(&theirs.cost_units), "{name}");
        let loaded = some(&theirs.loaded_addresses).unwrap();
        let ours_loaded = meta.loaded_addresses.as_ref().unwrap();
        assert_eq!(
            (keys(&ours_loaded.writable), keys(&ours_loaded.readonly)),
            (loaded.writable, loaded.readonly),
            "{name}"
        );
        let inner: Vec<(u8, Vec<ChainInstruction>)> = some(&theirs.inner_instructions)
            .unwrap()
            .iter()
            .map(|g| {
                (
                    g.index,
                    g.instructions
                        .iter()
                        .map(|ix| match ix {
                            UiInstruction::Compiled(c) => instruction(c),
                            UiInstruction::Parsed(_) => panic!("{name}: raw inner instructions"),
                        })
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            meta.inner_instructions
                .as_ref()
                .unwrap()
                .iter()
                .map(|g| (g.index, g.instructions.clone()))
                .collect::<Vec<_>>(),
            inner,
            "{name}"
        );
        for (ours_balances, theirs_balances) in [
            (&meta.pre_token_balances, &theirs.pre_token_balances),
            (&meta.post_token_balances, &theirs.post_token_balances),
        ] {
            let theirs_balances = some(theirs_balances).unwrap();
            let ours_balances = ours_balances.as_ref().unwrap();
            assert_eq!(ours_balances.len(), theirs_balances.len(), "{name}");
            for (o, t) in ours_balances.iter().zip(&theirs_balances) {
                assert_eq!(
                    (
                        o.account_index,
                        o.mint.to_string(),
                        o.owner.map(|k| k.to_string()),
                        o.program_id.map(|k| k.to_string()),
                        o.amount,
                        o.decimals
                    ),
                    (
                        t.account_index,
                        t.mint.clone(),
                        some(&t.owner),
                        some(&t.program_id),
                        t.ui_token_amount.amount.parse::<u64>().unwrap(),
                        t.ui_token_amount.decimals
                    ),
                    "{name}"
                );
            }
        }
    }
}
