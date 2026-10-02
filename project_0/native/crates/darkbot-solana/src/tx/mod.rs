//! S3-01..03: TX-Builder (legacy/v0) + isolierter v1-Capability-Adapter und Signer.
//!
//! Regeln (PHASENPLAN §S3):
//! - Blockhash kommt aus dem Cache (kein RPC hier).
//! - Account-Lock-Limit 64 gilt für legacy/v0/v1 (Runtime); als Capability geführt.
//! - v0: 1232 Bytes, LUTs; ComputeBudget-Instructions am Anfang.
//! - v1: 4096 Bytes, keine LUTs, Ressourcen im Message-Config, Priority Fee als
//!   Gesamt-Lamports, ComputeBudget-Ix sind No-ops → **nicht** einfügen.
//!   Der v1-Adapter nutzt modulare Solana-4.x-Crates parallel zum marginfi-gepinnten SDK 3.x;
//!   kein zweiter Business-/Execution-Core.
//!
//! Status: UNTESTED / VERIFY (solana-sdk API-Namen).

use solana_message::AddressLookupTableAccount; // [FIX-13]
use solana_compute_budget_interface::ComputeBudgetInstruction; // [FIX-13]
use solana_sdk::hash::Hash;
use solana_sdk::instruction::Instruction;
use solana_sdk::message::{v0, VersionedMessage};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature, Signer};
use solana_sdk::transaction::VersionedTransaction;
use std::collections::HashSet;
use std::fmt;

/// Cluster-Capabilities (aus Manifest/Messung, nicht hart im Code).
#[derive(Clone, Copy, Debug)]
pub struct TxLimits {
    pub max_locked_accounts: usize, // aktuell 64 für alle Versionen
    pub max_bytes_v0: usize,        // 1232
    pub max_bytes_v1: usize,        // 4096
}

impl Default for TxLimits {
    fn default() -> Self {
        TxLimits { max_locked_accounts: 64, max_bytes_v0: 1232, max_bytes_v1: 4096 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildVersion {
    Legacy,
    V0,
    V1,
}

/// Ressourcen-Anforderung — versionsneutral formuliert; der Builder übersetzt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resources {
    pub compute_unit_limit: u32,
    /// v0: µLamports/CU (set_compute_unit_price). v1: wird in Gesamt-Lamports umgerechnet.
    pub priority_micro_lamports_per_cu: u64,
    /// v1-Pflicht (Default 0!). v0: optional (set_loaded_accounts_data_size_limit).
    pub loaded_accounts_data_size: Option<u32>,
}

impl Resources {
    /// v1-Priority-Fee ist ein Gesamtbetrag: rate × CU-Limit / 1e6 (Solana Foundation).
    pub fn priority_total_lamports(&self) -> u64 {
        ((self.priority_micro_lamports_per_cu as u128 * self.compute_unit_limit as u128 + 999_999) / 1_000_000u128) as u64
    }
}

#[derive(Clone, Debug)]
pub struct TxPlan {
    pub version: BuildVersion,
    pub payer: Pubkey,
    pub instructions: Vec<Instruction>,
    pub lookup_tables: Vec<AddressLookupTableAccount>,
    pub resources: Resources,
}

#[derive(Clone, Debug)]
pub struct BuiltTx {
    pub version: BuildVersion,
    pub tx: VersionedTransaction,
    pub serialized_len: usize,
    pub locked_accounts: usize,
    pub blockhash: Hash,
}

#[derive(Debug)]
pub enum BuildError {
    TooManyAccounts { count: usize, max: usize },
    TooLarge { bytes: usize, max: usize },
    Compile(String),
    Serialize(String),
    Sign(String),
    V1Unavailable,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::TooManyAccounts { count, max } => write!(f, "too many locked accounts: {} > {}", count, max),
            BuildError::TooLarge { bytes, max } => write!(f, "transaction too large: {} > {}", bytes, max),
            BuildError::Compile(s) => write!(f, "compile: {}", s),
            BuildError::Serialize(s) => write!(f, "serialize: {}", s),
            BuildError::Sign(s) => write!(f, "sign: {}", s),
            BuildError::V1Unavailable => write!(f, "tx v1 signing/submission adapter not enabled; v1 capability measurement is available with feature txv1"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Zählt die eindeutigen Accounts, die die Runtime sperren würde (Payer +
/// alle Ix-Accounts + Programme). Unabhängig von LUTs (Runtime-Limit).
pub fn locked_account_count(payer: &Pubkey, ixs: &[Instruction]) -> usize {
    let mut set: HashSet<Pubkey> = HashSet::new();
    set.insert(*payer);
    for ix in ixs {
        set.insert(ix.program_id);
        for m in &ix.accounts {
            set.insert(m.pubkey);
        }
    }
    set.len()
}

/// Finale Instruction-Liste der jeweiligen Version. legacy/v0 bekommen ComputeBudget-Ixs;
/// v1 trägt dieselben Ressourcen im Message-Config und bekommt **keine** ComputeBudget-Ixs.
pub fn effective_instructions(plan: &TxPlan) -> Vec<Instruction> {
    let mut out = match plan.version {
        BuildVersion::V1 => Vec::new(),
        _ => resource_ixs(&plan.resources),
    };
    out.extend(plan.instructions.iter().cloned());
    out
}

pub fn effective_locked_account_count(plan: &TxPlan) -> usize {
    locked_account_count(&plan.payer, &effective_instructions(plan))
}

pub fn effective_instruction_count(plan: &TxPlan) -> usize {
    effective_instructions(plan).len()
}

/// Baut die (unsignierte) Transaktion. Signatur separat (`sign`).
pub fn build(plan: &TxPlan, blockhash: Hash, limits: &TxLimits) -> Result<BuiltTx, BuildError> {
    // Lock-Zählung auf der **finalen** Instruction-Liste (inkl. ComputeBudget-Programm bei legacy/v0;
    // v1 trägt Ressourcen im Header und fügt keine Instructions hinzu).
    let final_ixs = effective_instructions(plan);
    let locked = locked_account_count(&plan.payer, &final_ixs);
    if locked > limits.max_locked_accounts {
        return Err(BuildError::TooManyAccounts { count: locked, max: limits.max_locked_accounts });
    }
    match plan.version {
        BuildVersion::Legacy => build_legacy(plan, blockhash, limits, locked),
        BuildVersion::V0 => build_v0(plan, blockhash, limits, locked),
        BuildVersion::V1 => build_v1(plan, blockhash, limits, locked),
    }
}

fn resource_ixs(r: &Resources) -> Vec<Instruction> {
    let mut ixs = Vec::with_capacity(3);
    ixs.push(ComputeBudgetInstruction::set_compute_unit_limit(r.compute_unit_limit));
    if r.priority_micro_lamports_per_cu > 0 {
        ixs.push(ComputeBudgetInstruction::set_compute_unit_price(r.priority_micro_lamports_per_cu));
    }
    if let Some(sz) = r.loaded_accounts_data_size {
        ixs.push(ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(sz));
    }
    ixs
}

/// Legacy: keine LUTs, 1232 Bytes. Für kleine TXs (Tagging, Tip, Cleanup).
fn build_legacy(plan: &TxPlan, blockhash: Hash, limits: &TxLimits, locked: usize) -> Result<BuiltTx, BuildError> {
    let mut ixs = resource_ixs(&plan.resources);
    ixs.extend(plan.instructions.iter().cloned());
    let msg = solana_sdk::message::Message::new_with_blockhash(&ixs, Some(&plan.payer), &blockhash);
    let tx = VersionedTransaction {
        signatures: vec![Signature::default(); msg.header.num_required_signatures as usize],
        message: VersionedMessage::Legacy(msg),
    };
    let bytes = bincode_len(&tx)?;
    if bytes > limits.max_bytes_v0 {
        return Err(BuildError::TooLarge { bytes, max: limits.max_bytes_v0 });
    }
    Ok(BuiltTx { version: BuildVersion::Legacy, tx, serialized_len: bytes, locked_accounts: locked, blockhash })
}

fn build_v0(plan: &TxPlan, blockhash: Hash, limits: &TxLimits, locked: usize) -> Result<BuiltTx, BuildError> {
    let mut ixs = resource_ixs(&plan.resources);
    ixs.extend(plan.instructions.iter().cloned());

    let msg = v0::Message::try_compile(&plan.payer, &ixs, &plan.lookup_tables, blockhash)
        .map_err(|e| BuildError::Compile(e.to_string()))?;
    let tx = VersionedTransaction {
        signatures: vec![Signature::default(); msg.header.num_required_signatures as usize],
        message: VersionedMessage::V0(msg),
    };
    let bytes = bincode_len(&tx)?;
    if bytes > limits.max_bytes_v0 {
        return Err(BuildError::TooLarge { bytes, max: limits.max_bytes_v0 });
    }
    Ok(BuiltTx { version: BuildVersion::V0, tx, serialized_len: bytes, locked_accounts: locked, blockhash })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct V1Measure {
    pub message_bytes: usize,
    pub serialized_tx_bytes: usize,
    pub locked_accounts: usize,
    pub instructions: usize,
    pub signatures: usize,
}

#[cfg(not(feature = "txv1"))]
pub fn measure_v1(_plan: &TxPlan, _blockhash: Hash, _limits: &TxLimits) -> Result<V1Measure, BuildError> {
    Err(BuildError::V1Unavailable)
}

#[cfg(feature = "txv1")]
pub fn measure_v1(plan: &TxPlan, blockhash: Hash, limits: &TxLimits) -> Result<V1Measure, BuildError> {
    use solana_instruction_v1::{AccountMeta as V1AccountMeta, Instruction as V1Instruction};
    use solana_message_v1::v1;
    use solana_pubkey_v1::Pubkey as V1Pubkey;

    let locked = locked_account_count(&plan.payer, &plan.instructions);
    if locked > limits.max_locked_accounts {
        return Err(BuildError::TooManyAccounts { count: locked, max: limits.max_locked_accounts });
    }
    if plan.instructions.len() > 64 {
        return Err(BuildError::Compile(format!("too many v1 instructions: {} > 64", plan.instructions.len())));
    }
    let loaded = plan.resources.loaded_accounts_data_size.ok_or_else(|| {
        BuildError::Compile("v1 requires explicit loaded_accounts_data_size; implicit 0 is unsafe".into())
    })?;

    let payer = V1Pubkey::new_from_array(plan.payer.to_bytes());
    let ixs: Vec<V1Instruction> = plan.instructions.iter().map(|ix| V1Instruction {
        program_id: V1Pubkey::new_from_array(ix.program_id.to_bytes()),
        accounts: ix.accounts.iter().map(|m| V1AccountMeta {
            pubkey: V1Pubkey::new_from_array(m.pubkey.to_bytes()),
            is_signer: m.is_signer,
            is_writable: m.is_writable,
        }).collect(),
        data: ix.data.clone(),
    }).collect();
    let hash = solana_hash_v1::Hash::new_from_array(blockhash.to_bytes());
    let cfg = v1::TransactionConfig::empty()
        .with_compute_unit_limit(plan.resources.compute_unit_limit)
        .with_loaded_accounts_data_size_limit(loaded)
        .with_priority_fee(plan.resources.priority_total_lamports());
    let msg = v1::Message::try_compile_with_config(&payer, &ixs, hash, cfg)
        .map_err(|e| BuildError::Compile(e.to_string()))?;
    let message_bytes = msg.size(); // [FIX-58] canonical v1 serialized message size; no wincode feature needed
    let signatures = msg.header.num_required_signatures as usize;
    // SIMD-0385: v1 has no short-vec signature-count prefix; count comes from header.
    let serialized_tx_bytes = message_bytes.saturating_add(signatures.saturating_mul(64));
    if serialized_tx_bytes > limits.max_bytes_v1 {
        return Err(BuildError::TooLarge { bytes: serialized_tx_bytes, max: limits.max_bytes_v1 });
    }
    Ok(V1Measure {
        message_bytes,
        serialized_tx_bytes,
        locked_accounts: locked,
        instructions: plan.instructions.len(),
        signatures,
    })
}

#[cfg(not(feature = "txv1"))]
fn build_v1(_plan: &TxPlan, _blockhash: Hash, _limits: &TxLimits, _locked: usize) -> Result<BuiltTx, BuildError> {
    Err(BuildError::V1Unavailable)
}

#[cfg(feature = "txv1")]
fn build_v1(plan: &TxPlan, blockhash: Hash, limits: &TxLimits, _locked: usize) -> Result<BuiltTx, BuildError> {
    // Capability/size is compiled with the official modular v1 message crates above.
    // The current `BuiltTx` carries SDK-3.x `VersionedTransaction`, so signing/submission
    // is intentionally not faked through incompatible types. A thin wire adapter can be
    // added after the first compiler round without touching State/Health/Economics/P5.
    let _ = measure_v1(plan, blockhash, limits)?;
    Err(BuildError::V1Unavailable)
}

fn bincode_len(tx: &VersionedTransaction) -> Result<usize, BuildError> {
    // VERIFY: solana-sdk re-exportiert bincode-Serialisierung über
    // `bincode::serialize` (Dependency `bincode = "1"` nötig) oder
    // `solana_sdk::transaction::VersionedTransaction::serialize`-Äquivalent.
    // Bis dahin: Länge über `serialize_tx` in submit/mod.rs.
    crate::submit::serialize_tx(tx).map(|b| b.len())
}

/// Signiert in-place. Signer-Reihenfolge = Reihenfolge der Static-Keys.
pub fn sign(built: &mut BuiltTx, signers: &[&Keypair]) -> Result<(), BuildError> {
    let msg = built.tx.message.clone();
    let tx = VersionedTransaction::try_new(msg, signers).map_err(|e| BuildError::Sign(e.to_string()))?;
    built.tx = tx;
    Ok(())
}

/// Keypair aus Solana-CLI-JSON (Array von 64 Bytes) laden. Kein Key im Code, keine Env-Defaults.
pub fn load_keypair_json(path: &str) -> Result<Keypair, BuildError> {
    let raw = std::fs::read_to_string(path).map_err(|e| BuildError::Sign(format!("read {}: {}", path, e)))?;
    let bytes: Vec<u8> = serde_json::from_str(&raw).map_err(|e| BuildError::Sign(format!("parse {}: {}", path, e)))?;
    // [FIX-13] SDK 3.x: `Keypair::try_from(&[u8])` (from_bytes entfernt).
    Keypair::try_from(&bytes[..]).map_err(|e| BuildError::Sign(e.to_string()))
}

pub fn payer_of(kp: &Keypair) -> Pubkey {
    kp.pubkey()
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::instruction::AccountMeta;

    #[test]
    fn locked_accounts_are_deduplicated() {
        let payer = Pubkey::new_unique();
        let prog = Pubkey::new_unique();
        let a = Pubkey::new_unique();
        let ixs = vec![
            Instruction::new_with_bytes(prog, &[], vec![AccountMeta::new(a, false), AccountMeta::new(payer, true)]),
            Instruction::new_with_bytes(prog, &[], vec![AccountMeta::new_readonly(a, false)]),
        ];
        assert_eq!(locked_account_count(&payer, &ixs), 3);
    }

    #[test]
    fn v1_priority_fee_is_total_lamports() {
        let r = Resources { compute_unit_limit: 200_000, priority_micro_lamports_per_cu: 5_000, loaded_accounts_data_size: None };
        assert_eq!(r.priority_total_lamports(), 1_000);
    }

    #[test]
    fn v1_priority_fee_rounds_up_to_total_lamports() {
        let r = Resources { compute_unit_limit: 1, priority_micro_lamports_per_cu: 1, loaded_accounts_data_size: Some(32 * 1024) };
        assert_eq!(r.priority_total_lamports(), 1);
    }

    #[test]
    fn v1_effective_list_has_no_compute_budget_instructions() {
        let plan = TxPlan {
            version: BuildVersion::V1,
            payer: Pubkey::new_unique(),
            instructions: vec![Instruction::new_with_bytes(Pubkey::new_unique(), &[], vec![])],
            lookup_tables: vec![],
            resources: Resources { compute_unit_limit: 200_000, priority_micro_lamports_per_cu: 5_000, loaded_accounts_data_size: Some(32 * 1024) },
        };
        assert_eq!(effective_instruction_count(&plan), 1);
    }

    #[test]
    fn too_many_accounts_is_rejected_before_compile() {
        let payer = Pubkey::new_unique();
        let prog = Pubkey::new_unique();
        let metas: Vec<AccountMeta> = (0..70).map(|_| AccountMeta::new(Pubkey::new_unique(), false)).collect();
        let plan = TxPlan {
            version: BuildVersion::V0,
            payer,
            instructions: vec![Instruction::new_with_bytes(prog, &[], metas)],
            lookup_tables: vec![],
            resources: Resources { compute_unit_limit: 1, priority_micro_lamports_per_cu: 0, loaded_accounts_data_size: None },
        };
        match build(&plan, Hash::default(), &TxLimits::default()) {
            Err(BuildError::TooManyAccounts { count, max }) => {
                assert_eq!(max, 64);
                assert!(count > 64);
            }
            other => panic!("expected TooManyAccounts, got {:?}", other.map(|b| b.serialized_len)),
        }
    }

    #[test]
    fn v1_is_unavailable_without_feature() {
        #[cfg(not(feature = "txv1"))]
        {
            let plan = TxPlan {
                version: BuildVersion::V1,
                payer: Pubkey::new_unique(),
                instructions: vec![],
                lookup_tables: vec![],
                resources: Resources { compute_unit_limit: 1, priority_micro_lamports_per_cu: 0, loaded_accounts_data_size: Some(0) },
            };
            assert!(matches!(build(&plan, Hash::default(), &TxLimits::default()), Err(BuildError::V1Unavailable)));
        }
    }

    #[cfg(feature = "txv1")]
    #[test]
    fn v1_measure_uses_canonical_message_size() {
        let payer = Pubkey::new_unique();
        let plan = TxPlan {
            version: BuildVersion::V1,
            payer,
            instructions: vec![Instruction::new_with_bytes(
                Pubkey::new_unique(),
                &[1, 2, 3],
                vec![AccountMeta::new(Pubkey::new_unique(), false)],
            )],
            lookup_tables: vec![],
            resources: Resources {
                compute_unit_limit: 200_000,
                priority_micro_lamports_per_cu: 5_000,
                loaded_accounts_data_size: Some(32 * 1024),
            },
        };
        let m = measure_v1(&plan, Hash::default(), &TxLimits::default()).expect("v1 measure");
        assert!(m.message_bytes > 0);
        assert_eq!(m.serialized_tx_bytes, m.message_bytes + m.signatures * 64);
        assert_eq!(m.instructions, 1);
    }
}
