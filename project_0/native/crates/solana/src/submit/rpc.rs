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
    /// N2: the deadline-bound transport of candidate fee quotes and simulations; only adapters of
    /// the candidate path have one (`for_candidates`), others answer those calls `Unavailable`
    /// without a network call.
    candidate: Option<crate::provider::DeadlineRpc>,
    /// Preflight kostet Zeit; für Long-Tail kann es an bleiben (Revert-Kosten sparen).
    pub skip_preflight: bool,
    pub max_retries: Option<usize>,
}

/// `(Clock slot, Clock unix timestamp, selected post accounts, units consumed)` of a projection simulation.
pub type PostAccountsSimulation = (u64, i64, Vec<(Pubkey, Account)>, Option<u64>);

impl RpcSubmitter {
    /// An adapter without a candidate transport (projection workers, tools that only submit or
    /// reconcile): `fee_for_message`/`simulate` answer `Unavailable` without a network call.
    pub fn new(url: String, skip_preflight: bool) -> Self {
        Self::with_candidate(url, skip_preflight, None)
    }

    /// N2: the adapter of the candidate path: fee quotes and simulations through the
    /// deadline-bound transport against the process provider budget (`shared()`).
    pub fn for_candidates(url: String, skip_preflight: bool) -> Result<Self, String> {
        let candidate =
            crate::provider::DeadlineRpc::new(url.clone(), crate::provider::Workload::Simulation)?;
        Ok(Self::with_candidate(url, skip_preflight, Some(candidate)))
    }

    /// N2: the candidate adapter against an explicit provider budget.
    pub fn with_budget(
        url: String,
        skip_preflight: bool,
        budget: std::sync::Arc<crate::provider::Budget>,
    ) -> Result<Self, String> {
        let candidate = crate::provider::DeadlineRpc::with_budget(
            url.clone(),
            crate::provider::Workload::Simulation,
            budget,
        )?;
        Ok(Self::with_candidate(url, skip_preflight, Some(candidate)))
    }

    fn with_candidate(
        url: String,
        skip_preflight: bool,
        candidate: Option<crate::provider::DeadlineRpc>,
    ) -> Self {
        RpcSubmitter {
            rpc: crate::provider::rpc_client(
                url,
                std::time::Duration::from_secs(30),
                CommitmentConfig::confirmed(),
                crate::provider::Workload::Simulation,
            ),
            candidate,
            skip_preflight,
            max_retries: Some(0),
        }
    }

    /// N2: requests the candidate transport has sent (fee quotes and simulations).
    pub fn candidate_requests(&self) -> usize {
        self.candidate
            .as_ref()
            .map_or(0, crate::provider::DeadlineRpc::request_count)
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
    /// N2: bounded by `deadline` (permit, attempts, request, response read); every failure is
    /// typed, a timeout is never "no quote".
    fn fee_for_message(
        &self,
        tx: &VersionedTransaction,
        deadline: std::time::Instant,
    ) -> Result<super::MessageFeeQuote, super::FeeQuoteError> {
        use base64::Engine;
        let Some(candidate) = &self.candidate else {
            return Err(super::FeeQuoteError::Unavailable("NO_CANDIDATE_TRANSPORT"));
        };
        let mt0_start = std::time::Instant::now(); // [MT0]
        let sent = candidate.call(
            solana_client::rpc_request::RpcRequest::GetFeeForMessage,
            serde_json::json!([base64::engine::general_purpose::STANDARD.encode(tx.message.serialize()),
                {"commitment":"confirmed"}]),
            deadline,
        );
        darkbot_common::mt0::MT0.record_remote(
            darkbot_common::mt0::RemoteClass::FeeRpc,
            mt0_start.elapsed(),
            remote_outcome(&sent),
        );
        let response: solana_client::rpc_response::Response<Option<u64>> =
            serde_json::from_value(sent.map_err(fee_error)?)
                .map_err(|_| super::FeeQuoteError::Transport("RPC_FEE_DECODE"))?;
        Ok(super::MessageFeeQuote {
            message_hash: tx.message.hash().to_string(),
            context_slot: response.context.slot,
            lamports: response
                .value
                .ok_or(super::FeeQuoteError::Unavailable("FEE_FOR_MESSAGE_NULL"))?,
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
    /// B9: `submit`/`simulate` send SDK `VersionedTransaction`s (legacy/v0 wire); there is no
    /// V1 wire on this adapter, so it does not claim V1 (the former `true` was a false claim).
    fn supports_tx_version(&self, v: crate::tx::BuildVersion) -> bool {
        !matches!(v, crate::tx::BuildVersion::V1)
    }

    /// [FIX-6] Simulation derselben signierten TX; kein Rebuild zwischen Simulation und Send.
    /// N2: bounded by `deadline` like the fee quote.
    fn simulate(&self, tx: &VersionedTransaction, deadline: std::time::Instant) -> SimResult {
        let cfg = RpcSimulateTransactionConfig {
            sig_verify: true,
            replace_recent_blockhash: false,
            commitment: Some(CommitmentConfig::confirmed()),
            encoding: Some(UiTransactionEncoding::Base64),
            accounts: None,
            min_context_slot: None,
            inner_instructions: false,
        };
        let Some(candidate) = &self.candidate else {
            return SimResult::Unavailable("NO_CANDIDATE_TRANSPORT".into());
        };
        let Ok(encoded) = super::encode_base64(tx) else {
            return SimResult::Unavailable("SIMULATION_ENCODE".into());
        };
        let mt0_start = std::time::Instant::now(); // [MT0]
        let simulated = candidate.call(
            solana_client::rpc_request::RpcRequest::SimulateTransaction,
            serde_json::json!([encoded, cfg]),
            deadline,
        );
        darkbot_common::mt0::MT0.record_remote(
            darkbot_common::mt0::RemoteClass::Simulation,
            mt0_start.elapsed(),
            remote_outcome(&simulated),
        );
        let value = match simulated {
            Ok(v) => v,
            Err(crate::provider::CallError::Deadline(_)) => return SimResult::Deadline,
            Err(crate::provider::CallError::Budget(code)) => {
                return SimResult::BudgetRejected(code)
            }
            Err(crate::provider::CallError::Transport(code)) => {
                return SimResult::Unavailable(code.into())
            }
            Err(crate::provider::CallError::Remote(e)) => {
                return SimResult::Unavailable(safe_rpc_error(&e))
            }
        };
        let Ok(r) = serde_json::from_value::<
            solana_client::rpc_response::Response<
                solana_client::rpc_response::RpcSimulateTransactionResult,
            >,
        >(value) else {
            return SimResult::Unavailable("RPC_SIMULATION_DECODE".into());
        };
        match r.value.err {
            None => SimResult::Ok {
                units_consumed: r.value.units_consumed,
            },
            Some(e) => {
                // F1: bounded, redacted logs for the native journal only.
                let (logs, log_lines, logs_truncated) =
                    super::bounded_simulation_logs(r.value.logs.as_deref());
                SimResult::Failed(super::SimulationFailure {
                    code: format!("RPC_SIMULATION_FAILED {e:?}"),
                    units_consumed: r.value.units_consumed,
                    logs,
                    log_lines,
                    logs_truncated,
                })
            }
        }
    }

    /// The RPC path has no bundle simulation (no network call).
    fn simulate_bundle(&self, _: &[VersionedTransaction], _: std::time::Instant) -> SimResult {
        SimResult::Unavailable("no bundle simulation on the RPC path".into())
    }
}

/// N2: the fee-quote error of a failed candidate call.
fn fee_error(e: crate::provider::CallError) -> super::FeeQuoteError {
    use crate::provider::CallError as C;
    match e {
        C::Deadline(_) => super::FeeQuoteError::Deadline,
        C::Budget(code) => super::FeeQuoteError::BudgetRejected(code),
        C::Transport(code) => super::FeeQuoteError::Transport(code),
        C::Remote(_) => super::FeeQuoteError::Transport("RPC_REMOTE_ERROR"),
    }
}

/// [MT0] outcome class of a candidate call (measurement only).
fn remote_outcome(
    r: &Result<serde_json::Value, crate::provider::CallError>,
) -> darkbot_common::mt0::RemoteOutcome {
    use crate::provider::CallError as C;
    use darkbot_common::mt0::RemoteOutcome as O;
    match r {
        Ok(_) => O::Ok,
        Err(C::Deadline(_)) => O::Timeout,
        Err(C::Budget("RPC_RATE_LIMIT")) => O::RateLimited,
        Err(C::Remote(e)) => O::classify_text(&e.to_string()),
        Err(_) => O::Error,
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
    // C4: settlement facts only from a finalized status and finalized metadata of that slot.
    let finalized = matches!(
        status.confirmation_status,
        Some(solana_transaction_status::TransactionConfirmationStatus::Finalized)
    );
    let meta = if finalized {
        transaction_with_meta(rpc, sig, CommitmentConfig::finalized())
            .ok()
            .and_then(|t| {
                let meta = t.transaction.meta?;
                Some(FinalizedReceipt {
                    slot: t.slot,
                    fee: meta.fee,
                    compute_units: Option::<u64>::from(meta.compute_units_consumed.clone()),
                    failed: meta.err.is_some(),
                })
            })
    } else {
        None
    };
    Ok(landing_evidence(
        path,
        sig,
        status.slot,
        status
            .confirmation_status
            .as_ref()
            .map(|c| format!("{:?}", c)),
        finalized,
        status.err.as_ref().map(|e| format!("{:?}", e)),
        meta,
        now_ns,
    ))
}

/// The finalized receipt's settlement facts (`getTransaction` at finalized commitment).
struct FinalizedReceipt {
    slot: u64,
    fee: u64,
    compute_units: Option<u64>,
    failed: bool,
}

/// C4: landing evidence from a signature status and, for a finalized status, the finalized
/// receipt. A status below finalized is `LandedUnfinalized` without fee or outcome. A finalized
/// status yields a fee only with a receipt of the same slot whose own success/failure (`meta.err`)
/// agrees with the status; a missing, foreign-slot or contradicting receipt keeps the fee unknown
/// (the attempt stays unresolved, never settled). The outcome is the receipt's.
#[allow(clippy::too_many_arguments)]
fn landing_evidence(
    path: SubmissionPath,
    sig: &Signature,
    status_slot: u64,
    confirmation: Option<String>,
    finalized: bool,
    err: Option<String>,
    receipt: Option<FinalizedReceipt>,
    now_ns: u64,
) -> LandingEvidence {
    let receipt = receipt.filter(|r| finalized && r.slot == status_slot);
    let consistent = receipt.as_ref().filter(|r| r.failed == err.is_some());
    let (fee, cu) = consistent.map_or((None, None), |r| (Some(r.fee), r.compute_units));
    let failed = receipt.as_ref().map_or(err.is_some(), |r| r.failed);
    LandingEvidence {
        path,
        signature: *sig,
        outcome: if !finalized {
            LandingOutcome::LandedUnfinalized
        } else if failed {
            LandingOutcome::LandedFailed
        } else {
            LandingOutcome::LandedSuccess
        },
        landed_slot: Some(status_slot),
        tx_index_in_slot: None,
        confirmation,
        fee_lamports: fee,
        compute_units_consumed: cu,
        error: err,
        t_observed_landed_ns: Some(now_ns),
    }
}

#[cfg(test)]
mod landing_tests {
    use super::*;
    use solana_client::rpc_request::RpcRequest;

    /// C4 through `reconcile()`: only a finalized status with a finalized receipt of the same
    /// slot and the same success/failure yields a fee; the outcome is the receipt's. Confirmed,
    /// a foreign-slot receipt and a receipt contradicting the status stay unresolved (no fee).
    #[test]
    fn c4_reconcile_settles_only_on_a_consistent_finalized_receipt() {
        let sig = Signature::new_unique();
        // The client's own default receipt (slot 2, err null), as the RPC wire carries it.
        let receipt = serde_json::to_value(
            RpcClient::new_mock("succeeds")
                .get_transaction(&sig, UiTransactionEncoding::Json)
                .unwrap(),
        )
        .unwrap();
        let status = |slot: u64, level: &str, failed: bool| {
            let err = if failed {
                serde_json::json!({"InstructionError": [0, {"Custom": 1}]})
            } else {
                serde_json::Value::Null
            };
            let result = if failed {
                serde_json::json!({"Err": err.clone()})
            } else {
                serde_json::json!({"Ok": null})
            };
            serde_json::json!({"context": {"slot": 9}, "value": [{
                "slot": slot, "confirmations": null, "err": err, "status": result,
                "confirmationStatus": level}]})
        };
        let with = |slot: u64, fee: u64, failed: bool| {
            let mut r = receipt.clone();
            r["slot"] = slot.into();
            r["meta"]["fee"] = fee.into();
            if failed {
                r["meta"]["err"] = serde_json::json!({"InstructionError": [0, {"Custom": 1}]});
                r["meta"]["status"] = serde_json::json!({"Err": r["meta"]["err"].clone()});
            }
            r
        };
        let run = |st: serde_json::Value, tx: serde_json::Value| {
            let rpc = RpcClient::new_mock_with_mocks(
                "succeeds",
                [
                    (RpcRequest::GetSignatureStatuses, st),
                    (RpcRequest::GetTransaction, tx),
                ]
                .into_iter()
                .collect(),
            );
            reconcile(&rpc, SubmissionPath::Rpc, &sig, 7).unwrap()
        };
        let ok = run(status(2, "finalized", false), with(2, 5_000, false));
        assert_eq!(
            (ok.outcome, ok.fee_lamports),
            (LandingOutcome::LandedSuccess, Some(5_000))
        );
        let reverted = run(status(2, "finalized", true), with(2, 5_000, true));
        assert_eq!(
            (reverted.outcome, reverted.fee_lamports),
            (LandingOutcome::LandedFailed, Some(5_000))
        );
        let confirmed = run(status(2, "confirmed", false), with(2, 5_000, false));
        assert_eq!(
            (confirmed.outcome, confirmed.fee_lamports),
            (LandingOutcome::LandedUnfinalized, None)
        );
        let foreign_slot = run(status(2, "finalized", false), with(3, 5_000, false));
        assert_eq!(foreign_slot.fee_lamports, None);
        // Status success, finalized receipt failure: the receipt decides, nothing is settled.
        let contradiction = run(status(2, "finalized", false), with(2, 5_000, true));
        assert_eq!(
            (contradiction.outcome, contradiction.fee_lamports),
            (LandingOutcome::LandedFailed, None)
        );
    }
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
