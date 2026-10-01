//! S3-05: RPC-Submission + S3-06: Landing-Reconciliation über RPC.
//!
//! Status: UNTESTED / VERIFY (solana-client Signaturen; getTransaction-Meta-Felder).

use super::{LandingEvidence, LandingOutcome, SimResult, SubmissionAdapter, SubmissionPath, SubmitAck, SubmitError};
use solana_client::rpc_config::RpcSimulateTransactionConfig;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcSendTransactionConfig, RpcTransactionConfig};
use solana_commitment_config::{CommitmentConfig, CommitmentLevel}; // [FIX-13]
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use solana_transaction_status::UiTransactionEncoding;

pub struct RpcSubmitter {
    rpc: RpcClient,
    /// Preflight kostet Zeit; für Long-Tail kann es an bleiben (Revert-Kosten sparen).
    pub skip_preflight: bool,
    pub max_retries: Option<usize>,
}

impl RpcSubmitter {
    pub fn new(url: String, skip_preflight: bool) -> Self {
        RpcSubmitter {
            rpc: RpcClient::new_with_commitment(url, CommitmentConfig::confirmed()),
            skip_preflight,
            max_retries: Some(0),
        }
    }

    pub fn client(&self) -> &RpcClient {
        &self.rpc
    }
}

impl SubmissionAdapter for RpcSubmitter {
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
        match self.rpc.simulate_transaction_with_config(tx, cfg) {
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
    let statuses = rpc
        .get_signature_statuses(&[*sig])
        .map_err(|e| SubmitError::Transport(e.to_string()))?;
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
    let (fee, cu) = match rpc.get_transaction_with_config(sig, cfg) {
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
