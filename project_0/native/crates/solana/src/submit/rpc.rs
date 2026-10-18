//! S3-05: RPC-Submission + S3-06: Landing-Reconciliation über RPC.
//!
//! Status (F5): offline-tested — secret-free remote/transport error classes and no release of an
//! ambiguous send (`remote_and_transport_errors_never_render_secrets_or_release_ambiguous_sends`).
//! VERIFY, NOT_RUN: real solana-client responses and the `getTransaction` meta fields.

use super::{
    LandingEvidence, LandingOutcome, SimResult, SubmissionAdapter, SubmissionPath, SubmitAck,
    SubmitError,
};
use solana_account_decoder::UiAccountEncoding;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcSendTransactionConfig, RpcTransactionConfig};
use solana_client::rpc_config::{
    RpcSimulateTransactionAccountsConfig, RpcSimulateTransactionConfig,
};
use solana_commitment_config::{CommitmentConfig, CommitmentLevel}; // [FIX-13]
use solana_sdk::account::Account;
use solana_sdk::clock::Clock as SolanaClock;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use solana_transaction_status::UiTransactionEncoding;
use std::str::FromStr;

/// Never render remote text, nested errors, URLs or simulation logs at an error boundary.
pub fn safe_rpc_error(e: &solana_client::client_error::ClientError) -> String {
    use solana_client::{client_error::ClientErrorKind as K, rpc_request::RpcError};
    match e.kind() {
        K::RpcError(RpcError::RpcResponseError { code, .. }) => format!("RPC_RESPONSE code={code}"),
        K::RpcError(_) => "RPC_REQUEST_ERROR".into(),
        K::SerdeJson(_) => "RPC_DECODE_ERROR".into(),
        K::SigningError(_) => "RPC_SIGNING_ERROR".into(),
        K::TransactionError(_) => "RPC_TRANSACTION_ERROR".into(),
        K::Reqwest(e) => safe_http_error(e).into(),
        K::Io(_) => "RPC_IO_ERROR".into(),
        _ => "RPC_TRANSPORT_ERROR".into(),
    }
}
pub(super) fn safe_http_error(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "RPC_TIMEOUT"
    } else if e.is_connect() {
        "RPC_CONNECT"
    } else if e.is_decode() {
        "RPC_DECODE_ERROR"
    } else {
        "RPC_HTTP_OR_TRANSPORT"
    }
}
pub(super) fn safe_remote_error(v: &serde_json::Value) -> String {
    match v.get("code").and_then(|v| v.as_i64()) {
        Some(code) => format!("RPC_RESPONSE code={code}"),
        None => "RPC_REMOTE_ERROR".into(),
    }
}
fn submission_error(e: &solana_client::client_error::ClientError) -> SubmitError {
    use solana_client::{client_error::ClientErrorKind as K, rpc_request::RpcError};
    // Only structured definitive rejections may release admission. Decode failures,
    // local transport/budget strings and unknown remote codes cannot prove non-submission.
    match e.kind() {
        K::RpcError(RpcError::RpcResponseError {
            code: -32002 | -32600 | -32602,
            ..
        })
        | K::SigningError(_)
        | K::TransactionError(_) => SubmitError::Rejected(safe_rpc_error(e)),
        _ => SubmitError::Transport(safe_rpc_error(e)),
    }
}

pub struct RpcSubmitter {
    rpc: RpcClient,
    /// Preflight kostet Zeit; für Long-Tail kann es an bleiben (Revert-Kosten sparen).
    pub skip_preflight: bool,
    pub max_retries: Option<usize>,
}

/// `(Clock slot, Clock unix timestamp, selected post accounts, units consumed)` of a projection simulation.
pub type PostAccountsSimulation = (u64, i64, Vec<(Pubkey, Account)>, Option<u64>);

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
    ) -> Result<PostAccountsSimulation, SubmitError> {
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
                "projection simulation failed: {:?}",
                err
            )));
        }
        let returned = response.value.accounts.ok_or_else(|| {
            SubmitError::Rejected("projection simulation returned no accounts".into())
        })?;
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
                SubmitError::Rejected(format!(
                    "projection account missing after simulation: {key}"
                ))
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
        darkbot_common::mt0::MT0.record_remote(
            darkbot_common::mt0::RemoteClass::FeeRpc,
            mt0_start.elapsed(),
            mt0_outcome,
        );
        let response = sent.ok()?;
        Some(super::MessageFeeQuote {
            message_hash: tx.message.hash().to_string(),
            context_slot: response.context.slot,
            lamports: response.value?,
        })
    }

    fn path(&self) -> SubmissionPath {
        SubmissionPath::Rpc
    }

    fn submit(
        &self,
        tx: &VersionedTransaction,
        t_handoff_ns: u64,
    ) -> Result<SubmitAck, SubmitError> {
        let cfg = RpcSendTransactionConfig {
            skip_preflight: self.skip_preflight,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            encoding: Some(UiTransactionEncoding::Base64),
            max_retries: self.max_retries,
            min_context_slot: None,
        };
        let len = super::serialize_tx(tx)
            .map(|b| b.len())
            .map_err(|e| SubmitError::Serialize(e.to_string()))?;
        // [FIX-31] Eindeutige RPC-Ablehnung → Rejected (Kapital frei). Unklar nach dem Senden (Timeout,
        // Verbindungsabbruch, HTTP) → Transport (Ambiguous → Reconciliation). VERIFY Variantennamen 3.1.12.
        let sig = self
            .rpc
            .send_transaction_with_config(tx, cfg)
            .map_err(|e| submission_error(&e))?;
        Ok(SubmitAck {
            path: SubmissionPath::Rpc,
            signature: sig,
            signatures: vec![sig],
            submission_id: None,
            t_handoff_ns,
            serialized_len: len,
        })
    }

    /// [FIX-38] RPC akzeptiert v1 (Solana Foundation, base64 >1232 B) — VERIFY per Capability-Gate S3-02.
    fn supports_tx_version(&self, _v: crate::tx::BuildVersion) -> bool {
        true
    }

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
        darkbot_common::mt0::MT0.record_remote(
            darkbot_common::mt0::RemoteClass::Simulation,
            mt0_start.elapsed(),
            mt0_outcome,
        );
        match simulated {
            Ok(r) => match r.value.err {
                None => SimResult::Ok {
                    units_consumed: r.value.units_consumed,
                },
                Some(e) => SimResult::Failed(format!("RPC_SIMULATION_FAILED {e:?}")),
            },
            Err(e) => SimResult::Unavailable(safe_rpc_error(&e)),
        }
    }
}

/// Shared metadata request for landing reconciliation and one-shot accounting.
/// The finalized entrypoint does not accept a caller-selected weaker commitment.
fn transaction_with_meta(
    rpc: &RpcClient,
    sig: &Signature,
    commitment: CommitmentConfig,
) -> Result<solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta, SubmitError> {
    let cfg = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(commitment),
        max_supported_transaction_version: Some(1),
    };
    let started = std::time::Instant::now();
    let result = rpc.get_transaction_with_config(sig, cfg);
    let outcome = match &result {
        Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
        Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(&e.to_string()),
    };
    darkbot_common::mt0::MT0.record_remote(
        darkbot_common::mt0::RemoteClass::Reconcile,
        started.elapsed(),
        outcome,
    );
    result.map_err(|e| SubmitError::Transport(safe_rpc_error(&e)))
}

/// Uses the existing RPC client; caller owns timeout/quota policy. No signer or submission.
/// Missing/not-yet-finalized transactions return an error, never zero fees.
pub fn finalized_transaction(
    rpc: &RpcClient,
    sig: &Signature,
) -> Result<solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta, SubmitError> {
    transaction_with_meta(rpc, sig, CommitmentConfig::finalized())
}

/// Landing-Reconciliation: getSignatureStatuses → getTransaction (json, maxSupportedTransactionVersion=1).
/// `tx_index_in_slot` bleibt None (nur über getBlock ermittelbar; P6/H0).
pub fn reconcile(
    rpc: &RpcClient,
    path: SubmissionPath,
    sig: &Signature,
    now_ns: u64,
) -> Result<LandingEvidence, SubmitError> {
    // [MT0] every network request of the reconcile path is counted individually.
    let mt0_start = std::time::Instant::now();
    let statuses = rpc.get_signature_statuses_with_history(&[*sig]);
    let mt0_outcome = match &statuses {
        Ok(_) => darkbot_common::mt0::RemoteOutcome::Ok,
        Err(e) => darkbot_common::mt0::RemoteOutcome::classify_text(&e.to_string()),
    };
    darkbot_common::mt0::MT0.record_remote(
        darkbot_common::mt0::RemoteClass::Reconcile,
        mt0_start.elapsed(),
        mt0_outcome,
    );
    let statuses = statuses.map_err(|e| SubmitError::Transport(safe_rpc_error(&e)))?;
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
    let fetched = transaction_with_meta(rpc, sig, CommitmentConfig::confirmed());
    let (fee, cu) = match fetched {
        Ok(t) => {
            let meta = t.transaction.meta;
            // VERIFY: Feldnamen `fee` und `compute_units_consumed` (OptionSerializer) in solana-transaction-status 3.x
            let fee = meta.as_ref().map(|m| m.fee);
            let cu = meta
                .as_ref()
                .and_then(|m| Option::<u64>::from(m.compute_units_consumed.clone()));
            (fee, cu)
        }
        Err(_) => (None, None),
    };
    Ok(LandingEvidence {
        path,
        signature: *sig,
        outcome: if status.err.is_some() {
            LandingOutcome::LandedFailed
        } else {
            LandingOutcome::LandedSuccess
        },
        landed_slot: Some(status.slot),
        tx_index_in_slot: None,
        confirmation: status
            .confirmation_status
            .as_ref()
            .map(|c| format!("{:?}", c)),
        fee_lamports: fee,
        compute_units_consumed: cu,
        error: status.err.as_ref().map(|e| format!("{:?}", e)),
        t_observed_landed_ns: Some(now_ns),
    })
}

#[cfg(test)]
mod secret_tests {
    use super::*;
    #[test]
    fn remote_and_transport_errors_never_render_secrets_or_release_ambiguous_sends() {
        use solana_client::rpc_request::{RpcError, RpcResponseErrorData};
        let secret = "https://private.invalid/path?api-key=SECRET";
        for code in [-32002, -32602, -32005] {
            let e: solana_client::client_error::ClientError = RpcError::RpcResponseError {
                code,
                message: secret.into(),
                data: RpcResponseErrorData::Empty,
            }
            .into();
            let rendered = format!("{:?}", submission_error(&e));
            assert!(!rendered.contains("SECRET") && !rendered.contains("private.invalid"));
            assert_eq!(
                matches!(submission_error(&e), SubmitError::Rejected(_)),
                code != -32005
            );
        }
        let e: solana_client::client_error::ClientError =
            RpcError::RpcRequestError(secret.into()).into();
        assert!(matches!(submission_error(&e), SubmitError::Transport(_)));
        let error = reqwest::blocking::Client::new()
            .get("http://127.0.0.1:0/private?api-key=SECRET")
            .send()
            .unwrap_err();
        let e: solana_client::client_error::ClientError = error.into();
        assert!(!safe_rpc_error(&e).contains("SECRET"));
        assert!(matches!(submission_error(&e), SubmitError::Transport(_)));
    }
}
