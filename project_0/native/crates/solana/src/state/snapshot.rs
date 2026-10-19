//! S2-03: RPC-Snapshot (getProgramAccounts / getMultipleAccounts) mit Context-Slot.
//!
//! Der Snapshot liefert (slot, accounts). Der Aufrufer installiert ihn in `StateCore`,
//! nachdem der Stream läuft und puffert (Reihenfolge ist Pflicht, siehe state/mod.rs).
//!
//! Status: UNTESTED / VERIFY — solana-client API-Namen (RpcProgramAccountsConfig,
//! get_program_accounts_with_config, get_multiple_accounts_with_commitment).

use crate::ingress::{AccountUpdate, SourceKind};
use darkbot_common::timing::Clock;
use solana_account_decoder::UiAccount;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_request::RpcRequest;
use solana_client::rpc_response::{OptionalContext, RpcKeyedAccount};
use solana_commitment_config::CommitmentConfig; // [FIX-13]
use solana_sdk::pubkey::Pubkey;

#[derive(Debug)]
pub struct SnapshotError(pub String);

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "snapshot: {}", self.0)
    }
}
impl std::error::Error for SnapshotError {}

pub struct Snapshot {
    pub slot: u64,
    pub accounts: Vec<AccountUpdate>,
    /// [FIX-37] Angefragte Keys ohne Account (mit Context-Slot des Batches). Für required
    /// Dependencies macht das die Recovery fail-closed.
    pub missing: Vec<(Pubkey, u64)>,
}

/// B17: snapshots read at the state commitment (see `state::STATE_COMMITMENT`).
pub(crate) const STATE_SNAPSHOT_COMMITMENT: &str = super::STATE_COMMITMENT;
pub(crate) fn state_commitment_config() -> CommitmentConfig {
    use crate::ingress::SlotStatus as S;
    match super::STATE_SLOT_STATUS {
        S::Processed => CommitmentConfig::processed(),
        S::Confirmed => CommitmentConfig::confirmed(),
        S::Finalized => CommitmentConfig::finalized(),
    }
}

fn decode_program_accounts(
    rpc: &RpcClient,
    program: &Pubkey,
    filters: Option<Vec<crate::ingress::ProgramFilter>>,
    clock: &Clock,
) -> Result<Snapshot, SnapshotError> {
    let mut cfg = serde_json::json!({
        "encoding": "base64+zstd",
        "commitment": STATE_SNAPSHOT_COMMITMENT,
        "withContext": true
    });
    if let Some(filters) = filters {
        cfg["filters"] = filters
            .iter()
            .map(crate::ingress::ProgramFilter::wire)
            .collect();
    }
    let params = serde_json::json!([program.to_string(), cfg]);
    let resp: OptionalContext<Vec<RpcKeyedAccount>> = rpc
        .send(RpcRequest::GetProgramAccounts, params)
        .map_err(|e| SnapshotError(e.to_string()))?;
    let (slot, keyed) = match resp {
        OptionalContext::Context(r) => (r.context.slot, r.value),
        OptionalContext::NoContext(_) => {
            return Err(SnapshotError(
                "getProgramAccounts returned no context slot".into(),
            ))
        }
    };
    let t = clock.now_ns();
    let mut accounts = Vec::with_capacity(keyed.len());
    for k in keyed {
        let pubkey = k
            .pubkey
            .parse::<Pubkey>()
            .map_err(|e| SnapshotError(e.to_string()))?;
        let acc = UiAccount::decode::<solana_sdk::account::Account>(&k.account)
            .ok_or_else(|| SnapshotError(format!("undecodable account {}", k.pubkey)))?;
        accounts.push(AccountUpdate {
            pubkey,
            owner: acc.owner,
            lamports: acc.lamports,
            data: acc.data,
            slot,
            write_version: None,
            source: SourceKind::Snapshot,
            t_rx_ns: t,
        });
    }
    Ok(Snapshot {
        slot,
        accounts,
        missing: Vec::new(),
    })
}

/// Alle Accounts eines Programms. Nur noch für explizite Census-/Debug-Zwecke.
pub fn program_accounts(
    rpc: &RpcClient,
    program: &Pubkey,
    clock: &Clock,
) -> Result<Snapshot, SnapshotError> {
    decode_program_accounts(rpc, program, None, clock)
}

/// Gefilterte Program-Accounts mit Context-Slot. [FIX-77]
pub fn program_accounts_filtered(
    rpc: &RpcClient,
    program: &Pubkey,
    filters: Vec<crate::ingress::ProgramFilter>,
    clock: &Clock,
) -> Result<Snapshot, SnapshotError> {
    decode_program_accounts(rpc, program, Some(filters), clock)
}

/// Einzelne Accounts (Oracles, FeeState, LUTs) in Batches ≤ 100. Jeder Account trägt
/// den Context-Slot seines Batches; `Snapshot.slot` ist die untere Schranke (min).
pub fn multiple_accounts(
    rpc: &RpcClient,
    keys: &[Pubkey],
    clock: &Clock,
) -> Result<Snapshot, SnapshotError> {
    let mut accounts = Vec::with_capacity(keys.len());
    let mut missing: Vec<(Pubkey, u64)> = Vec::new();
    let mut slot_min = u64::MAX;
    for chunk in keys.chunks(100) {
        let resp = rpc
            .get_multiple_accounts_with_commitment(chunk, state_commitment_config())
            .map_err(|e| SnapshotError(e.to_string()))?;
        slot_min = slot_min.min(resp.context.slot);
        let t = clock.now_ns();
        for (k, acc) in chunk.iter().zip(resp.value.into_iter()) {
            let Some(acc) = acc else {
                missing.push((*k, resp.context.slot));
                continue;
            };
            {
                accounts.push(AccountUpdate {
                    pubkey: *k,
                    owner: acc.owner,
                    lamports: acc.lamports,
                    data: acc.data,
                    slot: resp.context.slot,
                    write_version: None,
                    source: SourceKind::Snapshot,
                    t_rx_ns: t,
                });
            }
        }
    }
    Ok(Snapshot {
        slot: if slot_min == u64::MAX { 0 } else { slot_min },
        accounts,
        missing,
    })
}

#[cfg(test)]
mod compressed_snapshot_tests {
    use super::*;
    use solana_account_decoder::{encode_ui_account, UiAccountEncoding};
    use solana_sdk::account::Account;
    use std::collections::HashMap;

    #[test]
    fn compressed_gpa_preserves_all_bytes_owner_lamports_and_context() {
        let key = Pubkey::new_unique();
        let account = Account {
            lamports: 987,
            data: (0..2304).map(|i| (i % 251) as u8).collect(),
            owner: Pubkey::new_unique(),
            executable: false,
            rent_epoch: 0,
        };
        let ui = encode_ui_account(&key, &account, UiAccountEncoding::Base64Zstd, None, None);
        let plain = encode_ui_account(&key, &account, UiAccountEncoding::Base64, None, None);
        let reply = serde_json::json!({"context":{"slot":123},"value":[{"pubkey":key.to_string(),"account":ui}]});
        let rpc = RpcClient::new_mock_with_mocks(
            "succeeds",
            HashMap::from([(RpcRequest::GetProgramAccounts, reply)]),
        );
        let snapshot =
            program_accounts_filtered(&rpc, &Pubkey::new_unique(), vec![], &Clock::new()).unwrap();
        assert_eq!(snapshot.slot, 123);
        assert_eq!(snapshot.accounts.len(), 1);
        let got = &snapshot.accounts[0];
        assert_eq!(got.pubkey, key);
        assert_eq!(got.slot, 123);
        assert_eq!(got.owner, account.owner);
        assert_eq!(got.lamports, account.lamports);
        assert_eq!(got.data, account.data);
        assert_eq!(got.data, plain.decode::<Account>().unwrap().data);
    }

    #[test]
    fn corrupt_compressed_account_cannot_produce_partial_snapshot() {
        let key = Pubkey::new_unique();
        let account = Account {
            lamports: 1,
            data: vec![7; 2304],
            owner: Pubkey::new_unique(),
            executable: false,
            rent_epoch: 0,
        };
        let mut ui = serde_json::to_value(encode_ui_account(
            &key,
            &account,
            UiAccountEncoding::Base64Zstd,
            None,
            None,
        ))
        .unwrap();
        ui["data"] = serde_json::json!(["AAAA", "base64+zstd"]);
        let reply = serde_json::json!({"context":{"slot":123},"value":[{"pubkey":key.to_string(),"account":ui}]});
        let rpc = RpcClient::new_mock_with_mocks(
            "succeeds",
            HashMap::from([(RpcRequest::GetProgramAccounts, reply)]),
        );
        assert!(
            program_accounts_filtered(&rpc, &Pubkey::new_unique(), vec![], &Clock::new()).is_err()
        );
    }
}
