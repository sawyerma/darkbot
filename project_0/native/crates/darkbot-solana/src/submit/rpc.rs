//! S3-05: RPC-Submission + S3-06: Landing-Reconciliation über RPC.
//!
//! Status: UNTESTED / VERIFY (solana-client Signaturen; getTransaction-Meta-Felder).

use super::{LandingEvidence, LandingOutcome, SimResult, SubmissionAdapter, SubmissionPath, SubmitAck, SubmitError};
use solana_client::rpc_config::{RpcSimulateTransactionAccountsConfig, RpcSimulateTransactionConfig};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcSendTransactionConfig, RpcTransactionConfig};
use solana_commitment_config::{CommitmentConfig, CommitmentLevel}; // [FIX-13]
use solana_account_decoder::UiAccountEncoding;
use solana_sdk::account::Account;
use solana_sdk::clock::Clock as SolanaClock;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use solana_transaction_status::UiTransactionEncoding;
use std::str::FromStr;

fn safe_rpc_error(e: &solana_client::client_error::ClientError) -> String {
    use solana_client::client_error::ClientErrorKind as K;
    match e.kind() {
        K::RpcError(rpc) => format!("RPC response error: {rpc}"),
        K::SerdeJson(err) => format!("RPC decode error: {err}"),
        K::SigningError(err) => format!("RPC signing error: {err}"),
        K::TransactionError(err) => format!("RPC transaction error: {err:?}"),
        _ => "RPC transport error".into(),
    }
}

pub struct RpcSubmitter {
    rpc: RpcClient,
    /// Preflight kostet Zeit; für Long-Tail kann es an bleiben (Revert-Kosten sparen).
    pub skip_preflight: bool,
    pub max_retries: Option<usize>,
}

impl RpcSubmitter {
    pub fn new(url: String, skip_preflight: bool) -> Self {
        RpcSubmitter {
            rpc: crate::provider::rpc_client(
                url,
                std::time::Duration::from_secs(30),
                CommitmentConfig::confirmed(),
                crate::provider::Workload::Simulation,
            ),
            skip_preflight,
            max_retries: Some(0),
        }
    }

    pub fn client(&self) -> &RpcClient {
        &self.rpc
    }


    /// [FIX-111][R6] Simulate the exact signed transaction and return selected post-execution
    /// accounts. This is the authoritative projection primitive for integration refreshes:
    /// callers can overlay the returned accounts onto reconciled RAM state without mutating the
    /// canonical state or faking Clock/freshness fields.
    pub fn simulate_with_post_accounts(
        &self,
        tx: &VersionedTransaction,
        addresses: &[Pubkey],
    ) -> Result<(u64, i64, Vec<(Pubkey, Account)>, Option<u64>), SubmitError> {
        if addresses.is_empty() {
            return Err(SubmitError::Rejected(
                "projection simulation requested zero post accounts".into(),
            ));
        }

        // [FIX-118a][R6] JupLend freshness is exact to the simulation Clock unix timestamp.
        // Request the Clock sysvar alongside mutated accounts so downstream Health/Sizing uses
        // the same protocol time as the refresh instructions, not wall-clock time a second later.
        let clock_key = Pubkey::from_str("SysvarC1ock11111111111111111111111111111111")
            .map_err(|e| SubmitError::Rejected(format!("clock sysvar pubkey: {e}")))?;
        let mut requested = addresses.to_vec();
        if !requested.contains(&clock_key) {
            requested.push(clock_key);
        }

        let cfg = RpcSimulateTransactionConfig {
            sig_verify: true,
            replace_recent_blockhash: false,
            commitment: Some(CommitmentConfig::confirmed()),
            encoding: Some(UiTransactionEncoding::Base64),
            accounts: Some(RpcSimulateTransactionAccountsConfig {
                encoding: Some(UiAccountEncoding::Base64),
                addresses: requested.iter().map(ToString::to_string).collect(),
            }),
            min_context_slot: None,
            inner_instructions: false,
        };
        let response = self
            .rpc
            .simulate_transaction_with_config(tx, cfg)
            .map_err(|e| SubmitError::Transport(safe_rpc_error(&e)))?;
        if let Some(err) = response.value.err {
            return Err(SubmitError::Rejected(format!(
                "projection simulation failed: {:?} logs={:?}",
                err, response.value.logs
            )));
        }
        let returned = response
            .value
            .accounts
            .ok_or_else(|| SubmitError::Rejected("projection simulation returned no accounts".into()))?;
        if returned.len() != requested.len() {
            return Err(SubmitError::Rejected(format!(
                "projection account count mismatch: requested={} returned={}",
                requested.len(),
                returned.len()
            )));
        }

        let mut out = Vec::with_capacity(addresses.len());
        let mut simulation_clock: Option<SolanaClock> = None;
        for (key, ui) in requested.iter().copied().zip(returned.into_iter()) {
            let ui = ui.ok_or_else(|| {
                SubmitError::Rejected(format!("projection account missing after simulation: {key}"))
            })?;
            let account = ui.decode::<Account>().ok_or_else(|| {
                SubmitError::Rejected(format!("projection account decode failed: {key}"))
            })?;
            if key == clock_key {
                let clock: SolanaClock = bincode::deserialize(&account.data).map_err(|e| {
                    SubmitError::Rejected(format!("projection clock decode failed: {e}"))
                })?;
                simulation_clock = Some(clock);
            } else {
                out.push((key, account));
            }
        }
        let simulation_clock = simulation_clock.ok_or_else(|| {
            SubmitError::Rejected("projection simulation returned no Clock sysvar".into())
        })?;
        if simulation_clock.slot != response.context.slot {
            return Err(SubmitError::Rejected(format!(
                "projection clock/context slot mismatch: clock={} context={}",
                simulation_clock.slot, response.context.slot
            )));
        }
        Ok((
            simulation_clock.slot,
            simulation_clock.unix_timestamp,
            out,
            response.value.units_consumed,
        ))
    }
}

impl SubmissionAdapter for RpcSubmitter {
    fn fee_for_message(&self, tx: &VersionedTransaction) -> Option<super::MessageFeeQuote> {
        use base64::Engine;
        let mt0_start = std::time::Instant::now(); // [MT0]
        let sent = self.rpc.send::<solana_client::rpc_response::Response<Option<u64>>>(
            solana_client::rpc_request::RpcRequest::GetFeeForMessage,
            serde_json::json!([base64::engine::general_purpose::STANDARD.encode(tx.message.serialize()),
                {"commitment":"confirmed"}]),
        );
        let mt0_outcome = match &sent {
            Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
            Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(&e.to_string()),
        };
        darkbot_common::mt0::MT0.record_remote(darkbot_common::mt0::RemoteClass::FeeRpc, mt0_start.elapsed(), mt0_outcome);
        let response = sent.ok()?;
        Some(super::MessageFeeQuote { message_hash:tx.message.hash().to_string(),
            context_slot:response.context.slot, lamports:response.value? })
    }

    fn path(&self) -> SubmissionPath {
        SubmissionPath::Rpc
    }

    fn submit(&self, tx: &VersionedTransaction, t_handoff_ns: u64) -> Result<SubmitAck, SubmitError> {
        let cfg = RpcSendTransactionConfig {
            skip_preflight: self.skip_preflight,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            encoding: Some(UiTransactionEncoding::Base64),
            max_retries: self.max_retries,
            min_context_slot: None,
        };
        let len = super::serialize_tx(tx).map(|b| b.len()).map_err(|e| SubmitError::Serialize(e.to_string()))?;
        // [FIX-31] Eindeutige RPC-Ablehnung → Rejected (Kapital frei). Unklar nach dem Senden (Timeout,
        // Verbindungsabbruch, HTTP) → Transport (Ambiguous → Reconciliation). VERIFY Variantennamen 3.1.12.
        let sig = self.rpc.send_transaction_with_config(tx, cfg).map_err(|e| {
            use solana_client::client_error::ClientErrorKind as K;
            match e.kind() {
                K::RpcError(_) | K::SerdeJson(_) | K::SigningError(_) | K::TransactionError(_) => SubmitError::Rejected(e.to_string()),
                _ => SubmitError::Transport(e.to_string()),
            }
        })?;
        Ok(SubmitAck { path: SubmissionPath::Rpc, signature: sig, signatures: vec![sig], submission_id: None, t_handoff_ns, serialized_len: len })
    }

    /// [FIX-38] RPC akzeptiert v1 (Solana Foundation, base64 >1232 B) — VERIFY per Capability-Gate S3-02.
    fn supports_tx_version(&self, _v: crate::tx::BuildVersion) -> bool { true }

    /// [FIX-6] Simulation derselben signierten TX; kein Rebuild zwischen Simulation und Send.
    fn simulate(&self, tx: &VersionedTransaction) -> SimResult {
        let cfg = RpcSimulateTransactionConfig {
            sig_verify: true,
            replace_recent_blockhash: false,
            commitment: Some(CommitmentConfig::confirmed()),
            encoding: Some(UiTransactionEncoding::Base64),
            accounts: None,
            min_context_slot: None,
            inner_instructions: false,
        };
        let mt0_start = std::time::Instant::now(); // [MT0]
        let simulated = self.rpc.simulate_transaction_with_config(tx, cfg);
        let mt0_outcome = match &simulated {
            Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
            Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(&e.to_string()),
        };
        darkbot_common::mt0::MT0.record_remote(darkbot_common::mt0::RemoteClass::Simulation, mt0_start.elapsed(), mt0_outcome);
        match simulated {
            Ok(r) => match r.value.err {
                None => SimResult::Ok { units_consumed: r.value.units_consumed },
                Some(e) => SimResult::Failed(format!("{:?} logs={:?}", e, r.value.logs)),
            },
            Err(e) => SimResult::Unavailable(e.to_string()),
        }
    }
}

/// Landing-Reconciliation: getSignatureStatuses → getTransaction (json, maxSupportedTransactionVersion=1).
/// `tx_index_in_slot` bleibt None (nur über getBlock ermittelbar; P6/H0).
pub fn reconcile(rpc: &RpcClient, path: SubmissionPath, sig: &Signature, now_ns: u64) -> Result<LandingEvidence, SubmitError> {
    // [MT0] every network request of the reconcile path is counted individually.
    let mt0_start = std::time::Instant::now();
    let statuses = rpc.get_signature_statuses_with_history(&[*sig]);
    let mt0_outcome = match &statuses {
        Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
        Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(&e.to_string()),
    };
    darkbot_common::mt0::MT0.record_remote(darkbot_common::mt0::RemoteClass::Reconcile, mt0_start.elapsed(), mt0_outcome);
    let statuses = statuses.map_err(|e| SubmitError::Transport(e.to_string()))?;
    let status = match statuses.value.into_iter().next().flatten() {
        None => {
            return Ok(LandingEvidence {
                path,
                signature: *sig,
                outcome: LandingOutcome::NotLanded,
                landed_slot: None,
                tx_index_in_slot: None,
                confirmation: None,
                fee_lamports: None,
                compute_units_consumed: None,
                error: None,
                t_observed_landed_ns: None,
            })
        }
        Some(s) => s,
    };
    let cfg = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(1), // Pflicht seit v1 (sonst -32015)
    };
    let mt0_start = std::time::Instant::now(); // [MT0]
    let fetched = rpc.get_transaction_with_config(sig, cfg);
    let mt0_outcome = match &fetched {
        Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
        Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(&e.to_string()),
    };
    darkbot_common::mt0::MT0.record_remote(darkbot_common::mt0::RemoteClass::Reconcile, mt0_start.elapsed(), mt0_outcome);
    let (fee, cu) = match fetched {
        Ok(t) => {
            let meta = t.transaction.meta;
            // VERIFY: Feldnamen `fee` und `compute_units_consumed` (OptionSerializer) in solana-transaction-status 3.x
            let fee = meta.as_ref().map(|m| m.fee);
            let cu = meta.as_ref().and_then(|m| Option::<u64>::from(m.compute_units_consumed.clone()));
            (fee, cu)
        }
        Err(_) => (None, None),
    };
    Ok(LandingEvidence {
        path,
        signature: *sig,
        outcome: if status.err.is_some() { LandingOutcome::LandedFailed } else { LandingOutcome::LandedSuccess },
        landed_slot: Some(status.slot),
        tx_index_in_slot: None,
        confirmation: status.confirmation_status.as_ref().map(|c| format!("{:?}", c)),
        fee_lamports: fee,
        compute_units_consumed: cu,
        error: status.err.as_ref().map(|e| format!("{:?}", e)),
        t_observed_landed_ns: Some(now_ns),
    })
}
