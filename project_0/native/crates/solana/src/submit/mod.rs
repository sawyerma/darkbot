//! S3-04..06: Submission-Contract, RPC-Adapter, Jito-Adapter, Landing-Reconciliation.
//!
//! - ACK/Bundle-ID ≠ landed. `SubmitAck` ist nur T_handoff-Evidence.
//! - Outcome-Taxonomie = Ledger §4 (Submission-path failure matrix). Unbekannte
//!   Kosten bleiben `None`, nie 0.
//! - Kein Provider-Name in Businesslogik; nur URLs in der Konfiguration.
//!
//! Status (F5): offline-tested parts only — see `rpc.rs` and `jito.rs`. The real RpcClient/Jito
//! responses and the `getTransaction` JSON fields are VERIFY: never exercised against a Mainnet
//! endpoint (FE-05/06, P8 NOT_RUN).

pub mod jito;
pub mod rpc;

use base64::Engine;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubmissionPath {
    Rpc,
    JitoSingle,
    JitoBundle,
}

impl SubmissionPath {
    pub const fn as_str(self) -> &'static str {
        match self {
            SubmissionPath::Rpc => "RPC",
            SubmissionPath::JitoSingle => "JITO_SINGLE",
            SubmissionPath::JitoBundle => "JITO_BUNDLE",
        }
    }
}

/// Evidence des Handoffs. Bedeutet nichts über Landing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitAck {
    pub path: SubmissionPath,
    /// Sentinel-Signatur (Single: die TX; Bundle: Receivership-TX).
    pub signature: Signature,
    /// [FIX-18] Alle Signaturen des Handoffs (Bundle: prereqs…, receivership, tip) für vollständige Reconciliation.
    pub signatures: Vec<Signature>,
    /// Jito: bundle id; RPC: None.
    pub submission_id: Option<String>,
    pub t_handoff_ns: u64,
    pub serialized_len: usize,
}

#[derive(Debug)]
pub enum SubmitError {
    Transport(String),
    Rejected(String),
    Serialize(String),
}

impl fmt::Display for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubmitError::Transport(s) => write!(f, "submit transport: {}", s),
            SubmitError::Rejected(s) => write!(f, "submit rejected: {}", s),
            SubmitError::Serialize(s) => write!(f, "submit serialize: {}", s),
        }
    }
}
impl std::error::Error for SubmitError {}

/// Simulationsergebnis (Simulate-first). `Unavailable` ist **kein** Freifahrtschein.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimResult {
    Ok {
        units_consumed: Option<u64>,
    },
    Failed(SimulationFailure),
    Unavailable(String),
    /// N2: the caller's deadline passed before or during the simulation (nothing simulated).
    Deadline,
    /// N2: the provider budget refused the simulation call.
    BudgetRejected(&'static str),
}

impl SimResult {
    /// Q3: the persisted `simulation_result` — written explicitly, never through the derived
    /// Debug layout of this enum. The values are those every journal so far carries: `PASS`;
    /// `Failed("<code>")`, `Unavailable("<reason>")`, `BudgetRejected("<code>")` with the text
    /// quoted as a Rust string literal; `Deadline`.
    pub fn record_text(&self) -> String {
        match self {
            SimResult::Ok { .. } => "PASS".into(),
            SimResult::Failed(f) => format!("Failed({:?})", f.code),
            SimResult::Unavailable(reason) => format!("Unavailable({reason:?})"),
            SimResult::Deadline => "Deadline".into(),
            SimResult::BudgetRejected(code) => format!("BudgetRejected({code:?})"),
        }
    }
}

/// Q3: the persisted confirmation status of a landing (`Processed`/`Confirmed`/`Finalized`) —
/// explicit, never the SDK type's Debug layout.
pub fn confirmation_status_name(
    status: &solana_transaction_status::TransactionConfirmationStatus,
) -> &'static str {
    use solana_transaction_status::TransactionConfirmationStatus as S;
    match status {
        S::Processed => "Processed",
        S::Confirmed => "Confirmed",
        S::Finalized => "Finalized",
    }
}

/// N2: why there is no fee quote — a timeout or a refused call never looks like "no quote".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeeQuoteError {
    /// The adapter has no fee quote for this path, or the node returned none for the message.
    Unavailable(&'static str),
    /// The provider budget refused the call.
    BudgetRejected(&'static str),
    /// The caller's deadline passed before or during the call.
    Deadline,
    /// Transport, HTTP, decode or remote failure before the deadline.
    Transport(&'static str),
}

/// F1: a failed simulation. `code` is the categorized, secret-free cause; the program logs are
/// bounded and redacted (`bounded_simulation_logs`) and belong only in the native journal:
/// `Debug` shows the code alone (exactly the former `Failed("<code>")` text), never a log line.
#[derive(Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SimulationFailure {
    pub code: String,
    pub units_consumed: Option<u64>,
    pub logs: Vec<String>,
    /// Lines the provider reported (`None`: no log field in the response).
    pub log_lines: Option<u32>,
    pub logs_truncated: bool,
}
impl std::fmt::Debug for SimulationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.code, f)
    }
}
impl From<&str> for SimulationFailure {
    fn from(code: &str) -> Self {
        Self {
            code: code.to_owned(),
            ..Self::default()
        }
    }
}
impl From<String> for SimulationFailure {
    fn from(code: String) -> Self {
        Self {
            code,
            ..Self::default()
        }
    }
}

/// F1 bounds: at most 64 lines of at most 256 bytes, at most 16 KiB in total.
pub const SIM_LOG_MAX_LINES: usize = 64;
pub const SIM_LOG_MAX_LINE_BYTES: usize = 256;
pub const SIM_LOG_MAX_BYTES: usize = 16 * 1024;

/// F1: bounded, redacted program logs of a failed simulation. A line that names an endpoint or
/// a credential is replaced; cut lines end on a char boundary. Returns (logs, reported lines,
/// truncated).
pub fn bounded_simulation_logs(logs: Option<&[String]>) -> (Vec<String>, Option<u32>, bool) {
    let Some(logs) = logs else {
        return (Vec::new(), None, false);
    };
    let mut out = Vec::new();
    let (mut bytes, mut truncated) = (0usize, false);
    for line in logs {
        if out.len() == SIM_LOG_MAX_LINES {
            truncated = true;
            break;
        }
        let lower = line.to_ascii_lowercase();
        let secret = [
            "http://", "https://", "ws://", "wss://", "api-key", "api_key", "apikey", "token=",
        ]
        .iter()
        .any(|m| lower.contains(m));
        let mut line = if secret {
            "[REDACTED]".to_owned()
        } else {
            line.clone()
        };
        if line.len() > SIM_LOG_MAX_LINE_BYTES {
            let mut cut = SIM_LOG_MAX_LINE_BYTES;
            while !line.is_char_boundary(cut) {
                cut -= 1;
            }
            line.truncate(cut);
            truncated = true;
        }
        if bytes + line.len() > SIM_LOG_MAX_BYTES {
            truncated = true;
            break;
        }
        bytes += line.len();
        out.push(line);
    }
    let reported = u32::try_from(logs.len()).unwrap_or(u32::MAX);
    (out, Some(reported), truncated)
}

/// RPC quote for the exact serialized message, not an observed landed fee.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MessageFeeQuote {
    pub message_hash: String,
    pub context_slot: u64,
    pub lamports: u64,
}

/// N2: the network operations a candidate evaluation performs (`fee_for_message`, `simulate`,
/// `simulate_bundle`) take the candidate's monotonic deadline and have no default: every adapter
/// states how it honours it (nothing starts after the deadline; a call in flight ends at it).
pub trait SubmissionAdapter {
    fn path(&self) -> SubmissionPath;
    fn fee_for_message(
        &self,
        tx: &VersionedTransaction,
        deadline: std::time::Instant,
    ) -> Result<MessageFeeQuote, FeeQuoteError>;
    /// [FIX-14] Fähigkeit, nicht Pfad: nur Adapter mit `true` dürfen Prerequisite-/Tip-Bundles senden.
    fn supports_bundle(&self) -> bool {
        false
    }
    /// [FIX-38] TxVersion-Fähigkeit des Pfads. v1 über Jito-Bundle/BAM ist **nicht belegt**
    /// (jito-solana Issue zu 1232-Byte-Grenze) → fail closed, bis Evidence vorliegt.
    fn supports_tx_version(&self, v: crate::tx::BuildVersion) -> bool {
        !matches!(v, crate::tx::BuildVersion::V1)
    }
    /// [FIX-15] Bundle-Simulation (sequenziell, atomar); without a bundle simulator: Unavailable.
    fn simulate_bundle(
        &self,
        txs: &[VersionedTransaction],
        deadline: std::time::Instant,
    ) -> SimResult;
    fn submit(
        &self,
        tx: &VersionedTransaction,
        t_handoff_ns: u64,
    ) -> Result<SubmitAck, SubmitError>;
    /// [FIX-4] Atomares Bundle (Prerequisite-TXs, Receivership-TX, Tip-TX). Default: nicht unterstützt.
    fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        _t_handoff_ns: u64,
    ) -> Result<SubmitAck, SubmitError> {
        Err(SubmitError::Rejected(format!(
            "bundle of {} txs not supported by {}",
            txs.len(),
            self.path().as_str()
        )))
    }
    /// [FIX-6] Simulate-first auf **genau** der TX, die gesendet wird.
    fn simulate(&self, tx: &VersionedTransaction, deadline: std::time::Instant) -> SimResult;
}

/// [FIX-4] Jito-Tip als eigene legacy-TX: ein System-Transfer an einen Tip-Account.
/// Niemals in der Receivership-TX (System-Programm dort nicht allowlisted).
pub fn build_tip_tx(
    payer: &solana_sdk::signature::Keypair,
    tip_account: &solana_sdk::pubkey::Pubkey,
    lamports: u64,
    blockhash: solana_sdk::hash::Hash,
) -> Result<VersionedTransaction, SubmitError> {
    use solana_sdk::signature::Signer;
    let ix = solana_system_interface::instruction::transfer(&payer.pubkey(), tip_account, lamports); // [FIX-13]
    let msg =
        solana_sdk::message::Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &blockhash);
    VersionedTransaction::try_new(solana_sdk::message::VersionedMessage::Legacy(msg), &[payer])
        .map_err(|e| SubmitError::Serialize(e.to_string()))
}

/// Ledger §4 — Outcome pro Pfad. Kosten werden erst durch Reconciliation gefüllt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandingOutcome {
    NotLanded,
    LandedSuccess,
    LandedFailed,
    /// C4: observed in a processed/confirmed status only — telemetry, never settlement evidence
    /// (no fee, outcome or capital release until a finalized status and receipt exist).
    LandedUnfinalized,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LandingEvidence {
    pub path: SubmissionPath,
    pub signature: Signature,
    pub outcome: LandingOutcome,
    pub landed_slot: Option<u64>,
    /// Index im Block, nur wenn per getBlock ermittelt; sonst None.
    pub tx_index_in_slot: Option<u32>,
    pub confirmation: Option<String>,
    /// Gesamtgebühr laut meta.fee (Base + Priority); None wenn nicht gelandet/unbekannt.
    pub fee_lamports: Option<u64>,
    pub compute_units_consumed: Option<u64>,
    pub error: Option<String>,
    pub t_observed_landed_ns: Option<u64>,
}

/// Serialisierung (bincode-kompatibles Wire-Format). VERIFY: `bincode` als
/// Dependency ergänzen, falls solana-sdk keine Serialisierungsfunktion re-exportiert.
pub fn serialize_tx(tx: &VersionedTransaction) -> Result<Vec<u8>, crate::tx::BuildError> {
    bincode::serialize(tx).map_err(|e| crate::tx::BuildError::Serialize(e.to_string()))
}

pub fn encode_base64(tx: &VersionedTransaction) -> Result<String, SubmitError> {
    let bytes = serialize_tx(tx).map_err(|e| SubmitError::Serialize(e.to_string()))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_names_match_ledger() {
        assert_eq!(SubmissionPath::Rpc.as_str(), "RPC");
        assert_eq!(SubmissionPath::JitoBundle.as_str(), "JITO_BUNDLE");
    }
}

#[cfg(test)]
mod simulation_log_tests {
    use super::*;

    /// F1: 0/65/1000 lines, oversized lines and a secret sentinel stay within the bounds; the
    /// Debug text of a failure is its code only.
    #[test]
    fn f1_simulation_logs_are_bounded_redacted_and_never_in_debug() {
        assert_eq!(bounded_simulation_logs(None), (vec![], None, false));
        assert_eq!(bounded_simulation_logs(Some(&[])), (vec![], Some(0), false));
        let lines = |n: usize, len: usize| -> Vec<String> {
            (0..n)
                .map(|i| format!("{i:04}{}", "x".repeat(len)))
                .collect()
        };
        let (out, n, cut) = bounded_simulation_logs(Some(&lines(65, 10)));
        assert_eq!((out.len(), n, cut), (SIM_LOG_MAX_LINES, Some(65), true));
        let (out, n, cut) = bounded_simulation_logs(Some(&lines(1000, 300)));
        assert!(cut && n == Some(1000));
        assert!(out.iter().all(|l| l.len() <= SIM_LOG_MAX_LINE_BYTES));
        assert!(out.iter().map(String::len).sum::<usize>() <= SIM_LOG_MAX_BYTES);
        let multibyte = vec!["é".repeat(200)];
        let (out, _, cut) = bounded_simulation_logs(Some(&multibyte));
        assert!(cut && out[0].len() <= SIM_LOG_MAX_LINE_BYTES);
        let secret = vec![
            "Program log: ok".to_owned(),
            "fetch https://rpc.invalid/?api-key=SECRET failed".to_owned(),
            "Api_Key=SECRET".to_owned(),
        ];
        let (out, _, _) = bounded_simulation_logs(Some(&secret));
        assert_eq!(out, ["Program log: ok", "[REDACTED]", "[REDACTED]"]);
        let failure = SimResult::Failed(SimulationFailure {
            code: "RPC_SIMULATION_FAILED X".into(),
            units_consumed: Some(7),
            logs: vec!["Program log: SENTINEL".into()],
            log_lines: Some(1),
            logs_truncated: false,
        });
        assert_eq!(
            format!("{failure:?}"),
            "Failed(\"RPC_SIMULATION_FAILED X\")"
        );
    }
}
