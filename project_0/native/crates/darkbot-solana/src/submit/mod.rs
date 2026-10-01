//! S3-04..06: Submission-Contract, RPC-Adapter, Jito-Adapter, Landing-Reconciliation.
//!
//! - ACK/Bundle-ID ≠ landed. `SubmitAck` ist nur T_handoff-Evidence.
//! - Outcome-Taxonomie = Ledger §4 (Submission-path failure matrix). Unbekannte
//!   Kosten bleiben `None`, nie 0.
//! - Kein Provider-Name in Businesslogik; nur URLs in der Konfiguration.
//!
//! Status: UNTESTED / VERIFY (RpcClient-/Jito-API, JSON-Felder von getTransaction).

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
    Ok { units_consumed: Option<u64> },
    Failed(String),
    Unavailable(String),
}

pub trait SubmissionAdapter {
    fn path(&self) -> SubmissionPath;
    /// [FIX-14] Fähigkeit, nicht Pfad: nur Adapter mit `true` dürfen Prerequisite-/Tip-Bundles senden.
    fn supports_bundle(&self) -> bool { false }
    /// [FIX-38] TxVersion-Fähigkeit des Pfads. v1 über Jito-Bundle/BAM ist **nicht belegt**
    /// (jito-solana Issue zu 1232-Byte-Grenze) → fail closed, bis Evidence vorliegt.
    fn supports_tx_version(&self, v: crate::tx::BuildVersion) -> bool {
        !matches!(v, crate::tx::BuildVersion::V1)
    }
    /// [FIX-15] Bundle-Simulation (sequenziell, atomar). Default: Unavailable → fail closed.
    fn simulate_bundle(&self, _txs: &[VersionedTransaction]) -> SimResult {
        SimResult::Unavailable("no bundle simulation on this adapter".into())
    }
    fn submit(&self, tx: &VersionedTransaction, t_handoff_ns: u64) -> Result<SubmitAck, SubmitError>;
    /// [FIX-4] Atomares Bundle (Prerequisite-TXs, Receivership-TX, Tip-TX). Default: nicht unterstützt.
    fn submit_bundle(&self, txs: &[VersionedTransaction], _t_handoff_ns: u64) -> Result<SubmitAck, SubmitError> {
        Err(SubmitError::Rejected(format!("bundle of {} txs not supported by {}", txs.len(), self.path().as_str())))
    }
    /// [FIX-6] Simulate-first auf **genau** der TX, die gesendet wird. Default: Unavailable.
    fn simulate(&self, _tx: &VersionedTransaction) -> SimResult {
        SimResult::Unavailable("no simulation on this adapter".into())
    }
}

/// [FIX-4] Jito-Tip als eigene legacy-TX: ein System-Transfer an einen Tip-Account.
/// Niemals in der Receivership-TX (System-Programm dort nicht allowlisted).
pub fn build_tip_tx(payer: &solana_sdk::signature::Keypair, tip_account: &solana_sdk::pubkey::Pubkey, lamports: u64,
                    blockhash: solana_sdk::hash::Hash) -> Result<VersionedTransaction, SubmitError> {
    use solana_sdk::signature::Signer;
    let ix = solana_system_interface::instruction::transfer(&payer.pubkey(), tip_account, lamports); // [FIX-13]
    let msg = solana_sdk::message::Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &blockhash);
    VersionedTransaction::try_new(solana_sdk::message::VersionedMessage::Legacy(msg), &[payer])
        .map_err(|e| SubmitError::Serialize(e.to_string()))
}

/// Ledger §4 — Outcome pro Pfad. Kosten werden erst durch Reconciliation gefüllt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandingOutcome {
    NotLanded,
    LandedSuccess,
    LandedFailed,
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
