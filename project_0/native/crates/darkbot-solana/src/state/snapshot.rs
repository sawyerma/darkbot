//! S2-03: RPC-Snapshot (getProgramAccounts / getMultipleAccounts) mit Context-Slot.
//!
//! Der Snapshot liefert (slot, accounts). Der Aufrufer installiert ihn in `StateCore`,
//! nachdem der Stream läuft und puffert (Reihenfolge ist Pflicht, siehe state/mod.rs).
//!
//! Status: UNTESTED / VERIFY — solana-client API-Namen (RpcProgramAccountsConfig,
//! get_program_accounts_with_config, get_multiple_accounts_with_commitment).

use crate::ingress::{AccountUpdate, SourceKind};
use darkbot_common::timing::Clock;
use solana_account_decoder::UiAccountEncoding;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_request::RpcRequest;
use solana_client::rpc_response::{OptionalContext, RpcKeyedAccount};
use solana_account_decoder::UiAccount;
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

/// Alle Accounts eines Programms. `withContext` liefert den Slot; ohne Slot
/// ist ein Snapshot wertlos (kein Bezug für "Updates > S").
pub fn program_accounts(rpc: &RpcClient, program: &Pubkey, clock: &Clock) -> Result<Snapshot, SnapshotError> {
    let cfg = RpcProgramAccountsConfig {
        filters: None,
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            data_slice: None,
            commitment: Some(CommitmentConfig::confirmed()),
            min_context_slot: None,
        },
        with_context: Some(true),
        sort_results: None, // VERIFY: Feld existiert ab solana-client 1.18/2.x
    };
    // Context-Slot aus **derselben** Antwort (withContext=true). `get_program_accounts_with_config`
    // verwirft den Context, daher roher Request. VERIFY: OptionalContext/RpcKeyedAccount-Pfade in solana-client 3.x.
    let params = serde_json::json!([program.to_string(), cfg]);
    let resp: OptionalContext<Vec<RpcKeyedAccount>> = rpc
        .send(RpcRequest::GetProgramAccounts, params)
        .map_err(|e| SnapshotError(e.to_string()))?;
    let (slot, keyed) = match resp {
        OptionalContext::Context(r) => (r.context.slot, r.value),
        OptionalContext::NoContext(_) => return Err(SnapshotError("getProgramAccounts returned no context slot".into())),
    };
    let t = clock.now_ns();
    let mut accounts = Vec::with_capacity(keyed.len());
    for k in keyed {
        let pubkey = k.pubkey.parse::<Pubkey>().map_err(|e| SnapshotError(e.to_string()))?;
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
    Ok(Snapshot { slot, accounts, missing: Vec::new() })
}

/// Einzelne Accounts (Oracles, FeeState, LUTs) in Batches ≤ 100. Jeder Account trägt
/// den Context-Slot seines Batches; `Snapshot.slot` ist die untere Schranke (min).
pub fn multiple_accounts(rpc: &RpcClient, keys: &[Pubkey], clock: &Clock) -> Result<Snapshot, SnapshotError> {
    let mut accounts = Vec::with_capacity(keys.len());
    let mut missing: Vec<(Pubkey, u64)> = Vec::new();
    let mut slot_min = u64::MAX;
    for chunk in keys.chunks(100) {
        let resp = rpc
            .get_multiple_accounts_with_commitment(chunk, CommitmentConfig::confirmed())
            .map_err(|e| SnapshotError(e.to_string()))?;
        slot_min = slot_min.min(resp.context.slot);
        let t = clock.now_ns();
        for (k, acc) in chunk.iter().zip(resp.value.into_iter()) {
            let Some(acc) = acc else { missing.push((*k, resp.context.slot)); continue; };
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
    Ok(Snapshot { slot: if slot_min == u64::MAX { 0 } else { slot_min }, accounts, missing })
}
