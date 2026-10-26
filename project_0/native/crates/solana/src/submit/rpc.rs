//! S3-05: RPC-Submission + S3-06: finalisierte Receipts für das FE-04-Settlement (`finalized_transaction`).
//!
//! Status (F5): offline-tested — secret-free remote/transport error classes and no release of an
//! ambiguous send (`remote_and_transport_errors_never_render_secrets_or_release_ambiguous_sends`).
//! VERIFY, NOT_RUN: real solana-client responses and the `getTransaction` meta fields.

use super::{SimResult, SubmissionAdapter, SubmissionPath, SubmitAck, SubmitError};
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
    /// V2-05: `sendTransaction` in its own pacing slot (`Workload::Send`) of the same budget;
    /// DEC-12: deadline-bound (one attempt, never retried).
    send: crate::provider::DeadlineRpc,
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

/// E9: why a projection simulation produced no projection.
#[derive(Debug)]
pub enum ProjectionError {
    /// The node is behind the slot the projection was requested from: it simulated at an older
    /// `context` slot, or refused the request because it has not reached `minContextSlot`
    /// (JSON-RPC -32016; the node's slot is then unknown).
    ContextBehind { context: Option<u64>, required: u64 },
    /// Transport, rejection or decode failure.
    Submit(SubmitError),
}

/// E9: the node refused a request because it has not reached its `minContextSlot` — typed by
/// the JSON-RPC error code, never by message text.
fn min_context_slot_not_reached(e: &solana_client::client_error::ClientError) -> bool {
    matches!(
        e.kind(),
        solana_client::client_error::ClientErrorKind::RpcError(
            solana_client::rpc_request::RpcError::RpcResponseError { code, .. }
        ) if *code == solana_client::rpc_custom_error::JSON_RPC_SERVER_ERROR_MIN_CONTEXT_SLOT_NOT_REACHED
    )
}

impl From<SubmitError> for ProjectionError {
    fn from(e: SubmitError) -> Self {
        ProjectionError::Submit(e)
    }
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectionError::ContextBehind {
                context: Some(context),
                required,
            } => write!(
                f,
                "PROJECTION_CONTEXT_BEHIND: context slot {context} below the required {required}"
            ),
            ProjectionError::ContextBehind {
                context: None,
                required,
            } => write!(
                f,
                "PROJECTION_CONTEXT_BEHIND: the node has not reached the required slot {required}"
            ),
            ProjectionError::Submit(e) => e.fmt(f),
        }
    }
}

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
        let budget = candidate
            .as_ref()
            .map_or_else(crate::provider::shared, |c| c.budget());
        let timeout = std::time::Duration::from_millis(budget.limits().rpc_timeout_ms);
        let client = |work| {
            crate::provider::rpc_client_with(
                url.clone(),
                timeout,
                CommitmentConfig::confirmed(),
                work,
                budget.clone(),
            )
        };
        RpcSubmitter {
            rpc: client(crate::provider::Workload::Simulation),
            // V2-05: the send in its own pacing slot of the adapter's budget.
            send: crate::provider::DeadlineRpc::over(
                url.clone(),
                crate::provider::Workload::Send,
                budget.clone(),
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
    /// E9: `min_context_slot` is the newest canonical slot the projection was requested from; the
    /// node must simulate at or after it (`minContextSlot`), and an older context is refused.
    pub fn simulate_with_post_accounts(
        &self,
        tx: &VersionedTransaction,
        addresses: &[Pubkey],
        min_context_slot: u64,
    ) -> Result<PostAccountsSimulation, ProjectionError> {
        if addresses.is_empty() {
            return Err(SubmitError::Rejected(
                "projection simulation requested zero post accounts".into(),
            )
            .into());
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
            min_context_slot: Some(min_context_slot),
            inner_instructions: false,
        };
        let response = self
            .rpc
            .simulate_transaction_with_config(tx, cfg)
            .map_err(|e| {
                if min_context_slot_not_reached(&e) {
                    ProjectionError::ContextBehind {
                        context: None,
                        required: min_context_slot,
                    }
                } else {
                    SubmitError::Transport(safe_rpc_error(&e)).into()
                }
            })?;
        if response.context.slot < min_context_slot {
            return Err(ProjectionError::ContextBehind {
                context: Some(response.context.slot),
                required: min_context_slot,
            });
        }
        if let Some(err) = response.value.err {
            return Err(ProjectionError::Submit(SubmitError::Rejected(format!(
                "projection simulation failed: {:?}",
                err
            ))));
        }
        let returned = response.value.accounts.ok_or_else(|| {
            SubmitError::Rejected("projection simulation returned no accounts".into())
        })?;
        if returned.len() != requested.len() {
            return Err(ProjectionError::Submit(SubmitError::Rejected(format!(
                "projection account count mismatch: requested={} returned={}",
                requested.len(),
                returned.len()
            ))));
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
            return Err(ProjectionError::Submit(SubmitError::Rejected(format!(
                "projection clock/context slot mismatch: clock={} context={}",
                simulation_clock.slot, response.context.slot
            ))));
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
                                                   // V2-05: its own pacing slot — the simulation after it is not spaced by `simulation_rps`.
        let sent = candidate.call_as(
            crate::provider::Workload::FeeQuote,
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
            message_hash: tx.message.hash(),
            context_slot: response.context.slot,
            lamports: response
                .value
                .ok_or(super::FeeQuoteError::Unavailable("FEE_FOR_MESSAGE_NULL"))?,
        })
    }

    fn path(&self) -> SubmissionPath {
        SubmissionPath::Rpc
    }

    /// DEC-12: the handoff is bounded by `deadline` — waiting for the send slot and the request
    /// in flight included: past it nothing is sent (`Deadline { sent: false }`), a request in
    /// flight ends at it (`Deadline { sent: true }`, ambiguous). One attempt, never retried.
    fn submit(
        &self,
        tx: &VersionedTransaction,
        t_handoff_ns: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        use base64::Engine;
        let cfg = RpcSendTransactionConfig {
            skip_preflight: self.skip_preflight,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            encoding: Some(UiTransactionEncoding::Base64),
            max_retries: self.max_retries,
            min_context_slot: None,
        };
        let wire = super::serialize_tx(tx).map_err(|e| SubmitError::Serialize(e.to_string()))?;
        let len = wire.len();
        let params =
            serde_json::json!([base64::engine::general_purpose::STANDARD.encode(&wire), cfg]);
        // [FIX-31] Eindeutige RPC-Ablehnung → Rejected (Kapital frei). Unklar nach dem Senden (Timeout,
        // Verbindungsabbruch, HTTP) → Transport (Ambiguous → Reconciliation). VERIFY Variantennamen 3.1.12.
        // DEC-12: whether the request left is the transport's own count, never guessed.
        let dispatched_before = self.send.request_count();
        let sent = self
            .send
            .call(
                solana_client::rpc_request::RpcRequest::SendTransaction,
                params,
                deadline,
            )
            .map_err(|e| match e {
                crate::provider::CallError::Deadline(_) => SubmitError::Deadline {
                    sent: self.send.request_count() > dispatched_before,
                },
                crate::provider::CallError::Budget(code)
                | crate::provider::CallError::Transport(code) => {
                    SubmitError::Transport(code.into())
                }
                crate::provider::CallError::Remote(e) => submission_error(&e),
            })?;
        let sig = sent
            .as_str()
            .and_then(|s| s.parse::<Signature>().ok())
            .ok_or_else(|| SubmitError::Transport("RPC_SEND_RESULT".into()))?;
        if Some(&sig) != tx.signatures.first() {
            return Err(SubmitError::Transport("RPC_SEND_SIGNATURE_MISMATCH".into()));
        }
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
        sim_result_of(value)
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

/// Metadata request for the FE-04 finalized receipts (one-shot accounting).
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

/// The verdict of a `simulateTransaction` response (external JSON-RPC wire).
fn sim_result_of(value: serde_json::Value) -> SimResult {
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
            // Q3: the explicit error text, without the SDK's `UiTransactionError(…)` wrapper.
            let error = solana_sdk::transaction::TransactionError::from(e);
            SimResult::Failed(super::SimulationFailure {
                code: format!(
                    "RPC_SIMULATION_FAILED {}",
                    super::transaction_error_text(&error)
                ),
                units_consumed: r.value.units_consumed,
                logs,
                log_lines,
                logs_truncated,
            })
        }
    }
}

#[cfg(test)]
mod v2_05_pacing_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};

    /// A loopback JSON-RPC node (one connection per request) that answers until it served a
    /// `sendTransaction` or `within` ends; returns each request's method and arrival.
    fn node(
        signature: String,
        within: Duration,
    ) -> (String, std::thread::JoinHandle<Vec<(String, Instant)>>) {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let end = Instant::now() + within;
            let mut seen = Vec::new();
            while Instant::now() < end {
                let Ok((mut c, _)) = socket.accept() else {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                };
                c.set_nonblocking(false).unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let body = loop {
                    let read = c.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..read]);
                    if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..at]).to_ascii_lowercase();
                        let len = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if buf.len() >= at + 4 + len {
                            break buf[at + 4..at + 4 + len].to_vec();
                        }
                    }
                    assert!(read > 0, "request truncated");
                };
                let arrived = Instant::now();
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let method = request["method"].as_str().unwrap_or_default().to_string();
                let result = match method.as_str() {
                    "getFeeForMessage" => {
                        serde_json::json!({"context": {"slot": 5}, "value": 5000})
                    }
                    "simulateTransaction" => serde_json::json!({"context": {"slot": 5},
                        "value": {"err": null, "logs": [], "unitsConsumed": 3}}),
                    "getVersion" => serde_json::json!({"solana-core": "3.0.0", "feature-set": 1}),
                    _ => serde_json::json!(signature),
                };
                let reply =
                    serde_json::json!({"jsonrpc": "2.0", "id": request["id"], "result": result})
                        .to_string();
                write!(
                    c,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .unwrap();
                let sent = method == "sendTransaction";
                seen.push((method, arrived));
                if sent {
                    break;
                }
            }
            seen
        });
        (url, handle)
    }

    /// DEC-12: the RPC handoff is bounded by its deadline. With the budget's only slot held
    /// past the deadline nothing is sent (`Deadline { sent: false }`, the node sees no request);
    /// a node slower than the deadline ends the request at it (`Deadline { sent: true }`),
    /// not at `rpc_timeout_ms` (before: the deadline was checked once, the send waited for its
    /// slot and the node up to the client timeout).
    #[test]
    fn dec12_the_rpc_handoff_is_bounded_by_its_deadline() {
        let mut config = crate::runtime_config::RuntimeConfig::default();
        config.limits.rpc_concurrency = 1;
        let budget = std::sync::Arc::new(crate::provider::Budget::configured(&config));
        let payer = solana_sdk::signature::Keypair::new();
        let message = solana_sdk::message::Message::new_with_blockhash(
            &[],
            Some(&solana_sdk::signer::Signer::pubkey(&payer)),
            &solana_sdk::hash::Hash::new_unique(),
        );
        let tx = VersionedTransaction::try_new(
            solana_sdk::message::VersionedMessage::Legacy(message),
            &[&payer],
        )
        .unwrap();
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let adapter = RpcSubmitter::with_budget(url, true, budget.clone()).unwrap();
        let held = budget
            .acquire(
                crate::provider::Workload::Simulation,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        let started = Instant::now();
        let waited = adapter.submit(&tx, 0, started + Duration::from_millis(300));
        assert!(
            matches!(waited, Err(SubmitError::Deadline { sent: false })),
            "{waited:?}"
        );
        assert!(started.elapsed() < Duration::from_millis(1_500));
        socket.set_nonblocking(true).unwrap();
        assert!(socket.accept().is_err(), "nothing reached the node");
        drop(held);
        socket.set_nonblocking(false).unwrap();
        let slow = std::thread::spawn(move || {
            let (mut c, _) = socket.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = c.read(&mut request);
            std::thread::sleep(Duration::from_secs(2));
        });
        let started = Instant::now();
        let in_flight = adapter.submit(&tx, 0, started + Duration::from_millis(300));
        assert!(
            matches!(in_flight, Err(SubmitError::Deadline { sent: true })),
            "{in_flight:?}"
        );
        assert!(started.elapsed() < Duration::from_millis(1_500));
        slow.join().unwrap();
    }

    /// V2-05 (P8-02): fee quote, simulation and send each have their own pacing slot
    /// (`Workload::FeeQuote`/`fee_rps`, `Simulation`, `Send`/`send_rps`) inside the one global
    /// budget of the adapter. With the defaults (`rpc_rps` 5, `rpc_burst` 1 — unchanged,
    /// FREE_ONLY) consecutive calls are ≥ 200 ms apart (global token bucket) and below the
    /// 500 ms of a shared `simulation_rps` = 2 slot (before: fee quote and simulation in
    /// `Simulation` ≥ 500 ms; the send outside the adapter's budget).
    #[test]
    fn v2_05_fee_quote_simulation_and_send_are_paced_in_their_own_slots() {
        let config = crate::runtime_config::RuntimeConfig::default();
        let budget = std::sync::Arc::new(crate::provider::Budget::configured(&config));
        let payer = solana_sdk::signature::Keypair::new();
        let ix = solana_sdk::instruction::Instruction::new_with_bytes(
            Pubkey::new_unique(),
            &[1],
            vec![],
        );
        let message = solana_sdk::message::Message::new_with_blockhash(
            &[ix],
            Some(&solana_sdk::signer::Signer::pubkey(&payer)),
            &solana_sdk::hash::Hash::new_unique(),
        );
        let tx = VersionedTransaction::try_new(
            solana_sdk::message::VersionedMessage::Legacy(message),
            &[&payer],
        )
        .unwrap();
        let (url, node) = node(tx.signatures[0].to_string(), Duration::from_secs(10));
        let adapter = RpcSubmitter::with_budget(url, true, budget).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(adapter.fee_for_message(&tx, deadline).is_ok());
        assert!(matches!(
            adapter.simulate(&tx, deadline),
            SimResult::Ok { .. }
        ));
        adapter
            .submit(
                &tx,
                0,
                std::time::Instant::now() + std::time::Duration::from_secs(60),
            )
            .unwrap();
        let seen = node.join().unwrap();
        let at = |m: &str| seen.iter().find(|(s, _)| s == m).map(|(_, t)| *t).unwrap();
        let (fee, sim, send) = (
            at("getFeeForMessage"),
            at("simulateTransaction"),
            at("sendTransaction"),
        );
        let global = Duration::from_secs_f64(1.0 / config.limits.rpc_rps as f64);
        let shared_slot = Duration::from_secs_f64(1.0 / config.limits.simulation_rps as f64);
        for (name, gap) in [("fee→sim", sim - fee), ("sim→send", send - sim)] {
            assert!(
                gap >= global.mul_f64(0.9) && gap < shared_slot,
                "{name} {gap:?} {seen:?}"
            );
        }
    }
}

#[cfg(test)]
mod v2_07_transport_tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// A loopback JSON-RPC node with HTTP/1.1 keep-alive: every connection serves requests until
    /// the client closes it; counts the TCP connections accepted.
    struct Node {
        url: String,
        accepts: Arc<AtomicUsize>,
        stop: Arc<AtomicBool>,
    }
    impl Drop for Node {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
        }
    }
    fn node(signature: String) -> Node {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let (accepts, stop) = (
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicBool::new(false)),
        );
        let (counted, stopped) = (accepts.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let Ok((c, _)) = socket.accept() else {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                };
                counted.fetch_add(1, Ordering::Relaxed);
                let signature = signature.clone();
                std::thread::spawn(move || serve(c, &signature));
            }
        });
        Node { url, accepts, stop }
    }
    fn serve(c: std::net::TcpStream, signature: &str) {
        c.set_nonblocking(false).unwrap();
        let mut writer = c.try_clone().unwrap();
        let mut reader = BufReader::new(c);
        loop {
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                if line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let result = match request["method"].as_str().unwrap_or_default() {
                "getFeeForMessage" => serde_json::json!({"context": {"slot": 5}, "value": 5000}),
                "simulateTransaction" => serde_json::json!({"context": {"slot": 5},
                    "value": {"err": null, "logs": [], "unitsConsumed": 3}}),
                "getSlot" => serde_json::json!(7),
                "getVersion" => serde_json::json!({"solana-core": "3.0.0", "feature-set": 1}),
                _ => serde_json::json!(signature),
            };
            let reply =
                serde_json::json!({"jsonrpc": "2.0", "id": request["id"], "result": result})
                    .to_string();
            write!(
                writer,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
        }
    }

    /// V2-07 (P6-01/P8-04): every budgeted RPC call of the process — two candidate adapters
    /// (fee quote, simulation, send) and a plain budgeted client — runs over one shared HTTP
    /// transport and reuses one kept-alive connection to the host (before: a pool per adapter
    /// transport and per client, 4 connections here).
    #[test]
    fn v2_07_budgeted_rpc_calls_share_one_connection_per_host() {
        let mut config = crate::runtime_config::RuntimeConfig::default();
        for field in [
            "rpc_rps",
            "rpc_burst",
            "simulation_rps",
            "fee_rps",
            "send_rps",
            "hotpath_rps",
        ] {
            assert!(config.limits.set(field, 1_000), "{field}");
        }
        let budget = Arc::new(crate::provider::Budget::configured(&config));
        let payer = solana_sdk::signature::Keypair::new();
        let ix = solana_sdk::instruction::Instruction::new_with_bytes(
            Pubkey::new_unique(),
            &[1],
            vec![],
        );
        let message = solana_sdk::message::Message::new_with_blockhash(
            &[ix],
            Some(&solana_sdk::signer::Signer::pubkey(&payer)),
            &solana_sdk::hash::Hash::new_unique(),
        );
        let tx = VersionedTransaction::try_new(
            solana_sdk::message::VersionedMessage::Legacy(message),
            &[&payer],
        )
        .unwrap();
        let node = node(tx.signatures[0].to_string());
        let deadline = Instant::now() + Duration::from_secs(5);
        let a = RpcSubmitter::with_budget(node.url.clone(), true, budget.clone()).unwrap();
        let b = RpcSubmitter::with_budget(node.url.clone(), true, budget.clone()).unwrap();
        assert!(a.fee_for_message(&tx, deadline).is_ok());
        assert!(matches!(a.simulate(&tx, deadline), SimResult::Ok { .. }));
        a.submit(
            &tx,
            0,
            std::time::Instant::now() + std::time::Duration::from_secs(60),
        )
        .unwrap();
        assert!(b.fee_for_message(&tx, deadline).is_ok());
        let rpc = crate::provider::rpc_client_with(
            node.url.clone(),
            Duration::from_secs(5),
            CommitmentConfig::confirmed(),
            crate::provider::Workload::LiveRead,
            budget,
        );
        assert_eq!(rpc.get_slot().unwrap(), 7);
        assert_eq!(node.accepts.load(Ordering::Relaxed), 1);
    }
}

#[cfg(test)]
mod q3_error_text_tests {
    use super::*;

    /// Q3: a failed simulation, from the real JSON-RPC wire, is coded with the explicit error
    /// text — under the SDK 3.x pin its Debug was `UiTransactionError(InstructionError(…))` —
    /// and every shape of `TransactionError` has its pinned text (the pin's `TransactionError`
    /// Debug; the encoder is exhaustive, an SDK rename fails to compile).
    #[test]
    fn q3_simulation_and_landing_errors_have_an_explicit_text() {
        let wire = serde_json::json!({"context": {"slot": 5}, "value": {
            "err": {"InstructionError": [3, {"Custom": 6009}]},
            "logs": ["Program log: SENTINEL"], "unitsConsumed": 7}});
        let SimResult::Failed(failure) = sim_result_of(wire) else {
            panic!("a failed simulation")
        };
        assert_eq!(
            failure.code,
            "RPC_SIMULATION_FAILED InstructionError(3, Custom(6009))"
        );
        assert_eq!(failure.units_consumed, Some(7));
        let ok =
            serde_json::json!({"context": {"slot": 5}, "value": {"err": null, "unitsConsumed": 3}});
        assert!(matches!(
            sim_result_of(ok),
            SimResult::Ok {
                units_consumed: Some(3)
            }
        ));
        use solana_sdk::instruction::InstructionError as I;
        use solana_sdk::transaction::TransactionError as T;
        for (error, text) in [
            (
                T::InstructionError(0, I::BorshIoError),
                "InstructionError(0, BorshIoError)",
            ),
            (
                T::InstructionError(2, I::Custom(1)),
                "InstructionError(2, Custom(1))",
            ),
            (
                T::InstructionError(1, I::BuiltinProgramsMustConsumeComputeUnits),
                "InstructionError(1, BuiltinProgramsMustConsumeComputeUnits)",
            ),
            (
                T::InsufficientFundsForRent { account_index: 2 },
                "InsufficientFundsForRent { account_index: 2 }",
            ),
            (
                T::ProgramExecutionTemporarilyRestricted { account_index: 1 },
                "ProgramExecutionTemporarilyRestricted { account_index: 1 }",
            ),
            (T::DuplicateInstruction(4), "DuplicateInstruction(4)"),
            (T::BlockhashNotFound, "BlockhashNotFound"),
            (T::AccountInUse, "AccountInUse"),
            (T::CommitCancelled, "CommitCancelled"),
        ] {
            assert_eq!(crate::submit::transaction_error_text(&error), text);
        }
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
