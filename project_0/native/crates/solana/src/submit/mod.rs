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

#[derive(Clone, Debug)]
pub enum SubmitError {
    Transport(String),
    Rejected(String),
    Serialize(String),
    /// DEC-12: the handoff deadline ended the call — before any request (`sent = false`) or
    /// with the request in flight (`sent = true`, outcome ambiguous).
    Deadline {
        sent: bool,
    },
}

impl fmt::Display for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubmitError::Transport(s) => write!(f, "submit transport: {}", s),
            SubmitError::Rejected(s) => write!(f, "submit rejected: {}", s),
            SubmitError::Serialize(s) => write!(f, "submit serialize: {}", s),
            SubmitError::Deadline { sent: false } => write!(f, "submit deadline: not sent"),
            SubmitError::Deadline { sent: true } => write!(f, "submit deadline: in flight"),
        }
    }
}
impl std::error::Error for SubmitError {}

/// Simulationsergebnis (Simulate-first). `Unavailable` ist **kein** Freifahrtschein.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimResult {
    Ok {
        units_consumed: Option<u64>,
        /// DEC-02: the slot the node simulated at (`context.slot`); `None` unknown.
        context_slot: Option<u64>,
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

/// Q3: the persisted text of a transaction error (simulation failure code, landing `err`) —
/// explicit and exhaustive over the pinned SDK enums, never their derived Debug: an SDK rename
/// fails to compile here instead of changing stored text. Equal to the Debug of
/// `TransactionError` at the pin; the SDK 3.x simulation wrapper `UiTransactionError(…)` is not
/// part of the text.
pub fn transaction_error_text(e: &solana_sdk::transaction::TransactionError) -> String {
    use solana_sdk::transaction::TransactionError as T;
    let name = match e {
        T::InstructionError(index, error) => {
            return format!(
                "InstructionError({index}, {})",
                instruction_error_text(error)
            )
        }
        T::DuplicateInstruction(index) => return format!("DuplicateInstruction({index})"),
        T::InsufficientFundsForRent { account_index } => {
            return format!("InsufficientFundsForRent {{ account_index: {account_index} }}")
        }
        T::ProgramExecutionTemporarilyRestricted { account_index } => {
            return format!(
                "ProgramExecutionTemporarilyRestricted {{ account_index: {account_index} }}"
            )
        }
        T::AccountInUse => "AccountInUse",
        T::AccountLoadedTwice => "AccountLoadedTwice",
        T::AccountNotFound => "AccountNotFound",
        T::ProgramAccountNotFound => "ProgramAccountNotFound",
        T::InsufficientFundsForFee => "InsufficientFundsForFee",
        T::InvalidAccountForFee => "InvalidAccountForFee",
        T::AlreadyProcessed => "AlreadyProcessed",
        T::BlockhashNotFound => "BlockhashNotFound",
        T::CallChainTooDeep => "CallChainTooDeep",
        T::MissingSignatureForFee => "MissingSignatureForFee",
        T::InvalidAccountIndex => "InvalidAccountIndex",
        T::SignatureFailure => "SignatureFailure",
        T::InvalidProgramForExecution => "InvalidProgramForExecution",
        T::SanitizeFailure => "SanitizeFailure",
        T::ClusterMaintenance => "ClusterMaintenance",
        T::AccountBorrowOutstanding => "AccountBorrowOutstanding",
        T::WouldExceedMaxBlockCostLimit => "WouldExceedMaxBlockCostLimit",
        T::UnsupportedVersion => "UnsupportedVersion",
        T::InvalidWritableAccount => "InvalidWritableAccount",
        T::WouldExceedMaxAccountCostLimit => "WouldExceedMaxAccountCostLimit",
        T::WouldExceedAccountDataBlockLimit => "WouldExceedAccountDataBlockLimit",
        T::TooManyAccountLocks => "TooManyAccountLocks",
        T::AddressLookupTableNotFound => "AddressLookupTableNotFound",
        T::InvalidAddressLookupTableOwner => "InvalidAddressLookupTableOwner",
        T::InvalidAddressLookupTableData => "InvalidAddressLookupTableData",
        T::InvalidAddressLookupTableIndex => "InvalidAddressLookupTableIndex",
        T::InvalidRentPayingAccount => "InvalidRentPayingAccount",
        T::WouldExceedMaxVoteCostLimit => "WouldExceedMaxVoteCostLimit",
        T::WouldExceedAccountDataTotalLimit => "WouldExceedAccountDataTotalLimit",
        T::MaxLoadedAccountsDataSizeExceeded => "MaxLoadedAccountsDataSizeExceeded",
        T::InvalidLoadedAccountsDataSizeLimit => "InvalidLoadedAccountsDataSizeLimit",
        T::ResanitizationNeeded => "ResanitizationNeeded",
        T::UnbalancedTransaction => "UnbalancedTransaction",
        T::ProgramCacheHitMaxLimit => "ProgramCacheHitMaxLimit",
        T::CommitCancelled => "CommitCancelled",
    };
    name.into()
}

/// Q3: the text of an instruction error inside `transaction_error_text` (exhaustive).
fn instruction_error_text(e: &solana_sdk::instruction::InstructionError) -> String {
    use solana_sdk::instruction::InstructionError as I;
    let name = match e {
        I::Custom(code) => return format!("Custom({code})"),
        I::GenericError => "GenericError",
        I::InvalidArgument => "InvalidArgument",
        I::InvalidInstructionData => "InvalidInstructionData",
        I::InvalidAccountData => "InvalidAccountData",
        I::AccountDataTooSmall => "AccountDataTooSmall",
        I::InsufficientFunds => "InsufficientFunds",
        I::IncorrectProgramId => "IncorrectProgramId",
        I::MissingRequiredSignature => "MissingRequiredSignature",
        I::AccountAlreadyInitialized => "AccountAlreadyInitialized",
        I::UninitializedAccount => "UninitializedAccount",
        I::UnbalancedInstruction => "UnbalancedInstruction",
        I::ModifiedProgramId => "ModifiedProgramId",
        I::ExternalAccountLamportSpend => "ExternalAccountLamportSpend",
        I::ExternalAccountDataModified => "ExternalAccountDataModified",
        I::ReadonlyLamportChange => "ReadonlyLamportChange",
        I::ReadonlyDataModified => "ReadonlyDataModified",
        I::DuplicateAccountIndex => "DuplicateAccountIndex",
        I::ExecutableModified => "ExecutableModified",
        I::RentEpochModified => "RentEpochModified",
        // Deprecated in the pinned SDK, still a variant (and still written by older validators).
        #[allow(deprecated)]
        I::NotEnoughAccountKeys => "NotEnoughAccountKeys",
        I::AccountDataSizeChanged => "AccountDataSizeChanged",
        I::AccountNotExecutable => "AccountNotExecutable",
        I::AccountBorrowFailed => "AccountBorrowFailed",
        I::AccountBorrowOutstanding => "AccountBorrowOutstanding",
        I::DuplicateAccountOutOfSync => "DuplicateAccountOutOfSync",
        I::InvalidError => "InvalidError",
        I::ExecutableDataModified => "ExecutableDataModified",
        I::ExecutableLamportChange => "ExecutableLamportChange",
        I::ExecutableAccountNotRentExempt => "ExecutableAccountNotRentExempt",
        I::UnsupportedProgramId => "UnsupportedProgramId",
        I::CallDepth => "CallDepth",
        I::MissingAccount => "MissingAccount",
        I::ReentrancyNotAllowed => "ReentrancyNotAllowed",
        I::MaxSeedLengthExceeded => "MaxSeedLengthExceeded",
        I::InvalidSeeds => "InvalidSeeds",
        I::InvalidRealloc => "InvalidRealloc",
        I::ComputationalBudgetExceeded => "ComputationalBudgetExceeded",
        I::PrivilegeEscalation => "PrivilegeEscalation",
        I::ProgramEnvironmentSetupFailure => "ProgramEnvironmentSetupFailure",
        I::ProgramFailedToComplete => "ProgramFailedToComplete",
        I::ProgramFailedToCompile => "ProgramFailedToCompile",
        I::Immutable => "Immutable",
        I::IncorrectAuthority => "IncorrectAuthority",
        I::BorshIoError => "BorshIoError",
        I::AccountNotRentExempt => "AccountNotRentExempt",
        I::InvalidAccountOwner => "InvalidAccountOwner",
        I::ArithmeticOverflow => "ArithmeticOverflow",
        I::UnsupportedSysvar => "UnsupportedSysvar",
        I::IllegalOwner => "IllegalOwner",
        I::MaxAccountsDataAllocationsExceeded => "MaxAccountsDataAllocationsExceeded",
        I::MaxAccountsExceeded => "MaxAccountsExceeded",
        I::MaxInstructionTraceLengthExceeded => "MaxInstructionTraceLengthExceeded",
        I::BuiltinProgramsMustConsumeComputeUnits => "BuiltinProgramsMustConsumeComputeUnits",
    };
    name.into()
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
    /// DEC-12 (V2-14/P1-02): the native hash of the quoted message (before: its base58 text).
    #[serde(with = "message_hash_serde")]
    pub message_hash: solana_sdk::hash::Hash,
    pub context_slot: u64,
    pub lamports: u64,
}

/// DEC-12: a message hash in a typed payload — its 32 bytes in the binary journal, its base58
/// text in a human-readable view, so historical renders stay identical.
pub mod message_hash_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use solana_sdk::hash::Hash;
    pub fn serialize<S: Serializer>(hash: &Hash, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.serialize_str(&hash.to_string())
        } else {
            hash.to_bytes().serialize(s)
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Hash, D::Error> {
        if d.is_human_readable() {
            String::deserialize(d)?
                .parse()
                .map_err(|_| serde::de::Error::custom("MESSAGE_HASH_TEXT"))
        } else {
            <[u8; 32]>::deserialize(d).map(Hash::new_from_array)
        }
    }
}

/// N2: the network operations a candidate evaluation performs (`fee_for_message`, `simulate`,
/// `simulate_bundle`) take the candidate's monotonic deadline and have no default: every adapter
/// states how it honours it (nothing starts after the deadline; a call in flight ends at it).
/// DEC-12: the handoff (`submit`, `submit_bundle`) takes the same deadline.
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
    /// P8-CTRL.4: the simulation of a synthetically signed transaction (`tx::sign_synthetic`):
    /// without signature verification. An adapter that verifies signatures overrides it.
    fn simulate_unverified(
        &self,
        tx: &VersionedTransaction,
        deadline: std::time::Instant,
    ) -> SimResult {
        self.simulate(tx, deadline)
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
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError>;
    /// [FIX-4] Atomares Bundle (Prerequisite-TXs, Receivership-TX, Tip-TX). Default: nicht unterstützt.
    fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        _t_handoff_ns: u64,
        _deadline: std::time::Instant,
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
/// [FIX-4] The one instruction of a bundle's tip transaction: a System transfer of `lamports`
/// from the payer to the tip account (shared by the builder and its check).
pub fn tip_instruction(
    payer: &solana_sdk::pubkey::Pubkey,
    tip_account: &solana_sdk::pubkey::Pubkey,
    lamports: u64,
) -> solana_sdk::instruction::Instruction {
    solana_system_interface::instruction::transfer(payer, tip_account, lamports)
    // [FIX-13]
}

/// E2: a tip transaction is exactly its plan — the single [`tip_instruction`] to the configured
/// tip account, nothing else (`crate::tx::message_matches`; `Err` is a stable code).
pub fn tip_matches(
    tx: &VersionedTransaction,
    payer: &solana_sdk::pubkey::Pubkey,
    tip_account: &solana_sdk::pubkey::Pubkey,
    lamports: u64,
) -> Result<(), &'static str> {
    crate::tx::message_matches(
        &tx.message,
        payer,
        &[],
        &[tip_instruction(payer, tip_account, lamports)],
    )
}

/// The tip transaction of a bundle, unsigned (P8-CTRL.4: it is signed with the bundle — the
/// wallet's key under a permit, else synthetically).
pub fn build_tip_tx(
    payer: &solana_sdk::pubkey::Pubkey,
    tip_account: &solana_sdk::pubkey::Pubkey,
    lamports: u64,
    blockhash: solana_sdk::hash::Hash,
) -> Result<VersionedTransaction, SubmitError> {
    let ix = tip_instruction(payer, tip_account, lamports);
    let msg = solana_sdk::message::Message::new_with_blockhash(&[ix], Some(payer), &blockhash);
    let message = solana_sdk::message::VersionedMessage::Legacy(msg);
    let signatures = vec![
        solana_sdk::signature::Signature::default();
        usize::from(message.header().num_required_signatures)
    ];
    Ok(VersionedTransaction {
        signatures,
        message,
    })
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

    /// E2: the tip transaction of a bundle is exactly one System transfer of the planned
    /// lamports from the payer to the configured tip account; every deviation of the signed
    /// message has its own stable code (built with the production `build_tip_tx`).
    #[test]
    fn e2_tip_tx_is_exactly_one_transfer_to_the_tip_account() {
        use solana_sdk::message::VersionedMessage;
        use solana_sdk::pubkey::Pubkey;
        use solana_sdk::signature::{Keypair, Signer};
        let payer = Keypair::new();
        let tip = Pubkey::new_unique();
        let tx = build_tip_tx(
            &payer.pubkey(),
            &tip,
            1_000,
            solana_sdk::hash::Hash::default(),
        )
        .unwrap();
        let me = payer.pubkey();
        assert_eq!(tip_matches(&tx, &me, &tip, 1_000), Ok(()));
        assert_eq!(tip_matches(&tx, &me, &tip, 999), Err("MESSAGE_DATA"));
        let other = Pubkey::new_unique();
        assert_eq!(
            tip_matches(&tx, &me, &other, 1_000),
            Err("MESSAGE_ACCOUNTS")
        );
        assert_eq!(tip_matches(&tx, &other, &tip, 1_000), Err("MESSAGE_PAYER"));
        let changed = |f: &dyn Fn(&mut solana_sdk::message::Message)| {
            let mut t = tx.clone();
            match &mut t.message {
                VersionedMessage::Legacy(m) => f(m),
                VersionedMessage::V0(_) => unreachable!("the tip transaction is legacy"),
            }
            tip_matches(&t, &me, &tip, 1_000)
        };
        // A second instruction (the same transfer again).
        assert_eq!(
            changed(&|m| {
                let i = m.instructions[0].clone();
                m.instructions.push(i);
            }),
            Err("MESSAGE_INSTRUCTION_COUNT")
        );
        // The tip account read-only (header privilege).
        assert_eq!(
            changed(&|m| m.header.num_readonly_unsigned_accounts += 1),
            Err("MESSAGE_PRIVILEGES")
        );
        // Another program for the same data and accounts.
        assert_eq!(
            changed(&|m| m.instructions[0].program_id_index = 1),
            Err("MESSAGE_PROGRAM")
        );
        // An unused extra key: appended alone it shifts the read-only region (the System program
        // turns writable); appended read-only it is a key no instruction asks for.
        assert_eq!(
            changed(&|m| m.account_keys.push(Pubkey::new_unique())),
            Err("MESSAGE_PRIVILEGES")
        );
        assert_eq!(
            changed(&|m| {
                m.account_keys.push(Pubkey::new_unique());
                m.header.num_readonly_unsigned_accounts += 1;
            }),
            Err("MESSAGE_KEYS")
        );
    }

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
