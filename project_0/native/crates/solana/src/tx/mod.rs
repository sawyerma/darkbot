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

use solana_compute_budget_interface::ComputeBudgetInstruction; // [FIX-13]
use solana_message::AddressLookupTableAccount; // [FIX-13]
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
        TxLimits {
            max_locked_accounts: 64,
            max_bytes_v0: 1232,
            max_bytes_v1: 4096,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildVersion {
    Legacy,
    V0,
    V1,
}

impl BuildVersion {
    /// Q3: the persisted name (`OpportunityRecord::tx_version`) — explicit, never the derived
    /// Debug layout; the values every journal so far carries.
    pub const fn record_name(self) -> &'static str {
        match self {
            BuildVersion::Legacy => "Legacy",
            BuildVersion::V0 => "V0",
            BuildVersion::V1 => "V1",
        }
    }
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
        (self.priority_micro_lamports_per_cu as u128 * self.compute_unit_limit as u128)
            .div_ceil(1_000_000) as u64
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

/// B9: whether this build of the transaction layer produces a signed V1 transaction, found by
/// running the implementation itself once (`build` + `sign` of a minimal V1 self-transfer with
/// an ephemeral key; no network, no persisted key). A planner offers V1 only when this holds and
/// the submission adapter's own `supports_tx_version(V1)` holds as well.
pub fn v1_build_capability() -> bool {
    static CAPABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CAPABLE.get_or_init(|| {
        let payer = Keypair::new();
        let plan = TxPlan {
            version: BuildVersion::V1,
            payer: payer.pubkey(),
            instructions: vec![solana_system_interface::instruction::transfer(
                &payer.pubkey(),
                &payer.pubkey(),
                0,
            )],
            lookup_tables: vec![],
            resources: Resources {
                compute_unit_limit: 200_000,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: Some(64 * 1024),
            },
        };
        build(&plan, Hash::default(), &TxLimits::default())
            .and_then(|mut built| sign(&mut built, &[&payer]).map(|()| built))
            .is_ok_and(|built| built.version == BuildVersion::V1)
    })
}

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

/// E2: the signed message is exactly its plan — the builder's compute-budget instructions and the
/// validated instructions (`effective_instructions`), in order, with the same program, data and
/// account keys (lookup-table addresses resolved from the plan's tables); every key carries
/// exactly the privileges the instructions ask for (the payer: first key, signer, writable) and
/// the message addresses no other key. `Err` is a stable code.
pub fn message_matches_plan(message: &VersionedMessage, plan: &TxPlan) -> Result<(), &'static str> {
    let (header, statics, compiled, lookups) = match message {
        VersionedMessage::Legacy(m) => (m.header, &m.account_keys, &m.instructions, &[][..]),
        VersionedMessage::V0(m) => (
            m.header,
            &m.account_keys,
            &m.instructions,
            m.address_table_lookups.as_slice(),
        ),
    };
    let required = usize::from(header.num_required_signatures);
    let readonly_signed = usize::from(header.num_readonly_signed_accounts);
    let readonly_unsigned = usize::from(header.num_readonly_unsigned_accounts);
    let static_len = statics.len();
    if required > static_len
        || readonly_signed > required
        || readonly_unsigned > static_len - required
    {
        return Err("MESSAGE_HEADER");
    }
    if statics.first() != Some(&plan.payer) {
        return Err("MESSAGE_PAYER");
    }
    // (key, signer, writable) in message order: static keys, then loaded writable, then loaded
    // readonly addresses.
    let mut keys: Vec<(Pubkey, bool, bool)> = statics
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let writable = if i < required {
                i < required - readonly_signed
            } else {
                i < static_len - readonly_unsigned
            };
            (*k, i < required, writable)
        })
        .collect();
    let mut loaded_readonly = Vec::new();
    for lookup in lookups {
        let table = plan
            .lookup_tables
            .iter()
            .find(|t| t.key == lookup.account_key)
            .ok_or("MESSAGE_LOOKUP_TABLE_UNKNOWN")?;
        let address = |i: &u8| {
            table
                .addresses
                .get(usize::from(*i))
                .copied()
                .ok_or("MESSAGE_LOOKUP_INDEX")
        };
        for i in &lookup.writable_indexes {
            keys.push((address(i)?, false, true));
        }
        for i in &lookup.readonly_indexes {
            loaded_readonly.push((address(i)?, false, false));
        }
    }
    keys.extend(loaded_readonly);
    let expected = effective_instructions(plan);
    if compiled.len() != expected.len() {
        return Err("MESSAGE_INSTRUCTION_COUNT");
    }
    let key = |i: u8| {
        keys.get(usize::from(i))
            .map(|k| k.0)
            .ok_or("MESSAGE_ACCOUNT_INDEX")
    };
    for (c, e) in compiled.iter().zip(&expected) {
        if key(c.program_id_index)? != e.program_id {
            return Err("MESSAGE_PROGRAM");
        }
        if c.data != e.data {
            return Err("MESSAGE_DATA");
        }
        if c.accounts.len() != e.accounts.len() {
            return Err("MESSAGE_ACCOUNTS");
        }
        for (i, meta) in c.accounts.iter().zip(&e.accounts) {
            if key(*i)? != meta.pubkey {
                return Err("MESSAGE_ACCOUNTS");
            }
        }
    }
    // Privileges: exactly the union the instructions ask for; the payer signs and pays.
    let mut wanted: std::collections::BTreeMap<Pubkey, (bool, bool)> =
        std::collections::BTreeMap::from([(plan.payer, (true, true))]);
    for e in &expected {
        wanted.entry(e.program_id).or_insert((false, false));
        for meta in &e.accounts {
            let w = wanted.entry(meta.pubkey).or_insert((false, false));
            w.0 |= meta.is_signer;
            w.1 |= meta.is_writable;
        }
    }
    // Every wanted key is the payer or appears in a compared instruction; a duplicate or any
    // other key is refused.
    let distinct: HashSet<Pubkey> = keys.iter().map(|k| k.0).collect();
    if distinct.len() != keys.len() {
        return Err("MESSAGE_KEYS");
    }
    for (k, signer, writable) in &keys {
        match wanted.get(k) {
            Some(&(s, w)) if s == *signer && w == *writable => {}
            Some(_) => return Err("MESSAGE_PRIVILEGES"),
            None => return Err("MESSAGE_KEYS"),
        }
    }
    Ok(())
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
        return Err(BuildError::TooManyAccounts {
            count: locked,
            max: limits.max_locked_accounts,
        });
    }
    match plan.version {
        BuildVersion::Legacy => build_legacy(plan, blockhash, limits, locked),
        BuildVersion::V0 => build_v0(plan, blockhash, limits, locked),
        BuildVersion::V1 => build_v1(plan, blockhash, limits, locked),
    }
}

fn resource_ixs(r: &Resources) -> Vec<Instruction> {
    let mut ixs = Vec::with_capacity(3);
    ixs.push(ComputeBudgetInstruction::set_compute_unit_limit(
        r.compute_unit_limit,
    ));
    if r.priority_micro_lamports_per_cu > 0 {
        ixs.push(ComputeBudgetInstruction::set_compute_unit_price(
            r.priority_micro_lamports_per_cu,
        ));
    }
    if let Some(sz) = r.loaded_accounts_data_size {
        ixs.push(ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(sz));
    }
    ixs
}

/// Legacy: keine LUTs, 1232 Bytes. Für kleine TXs (Tagging, Tip, Cleanup).
fn build_legacy(
    plan: &TxPlan,
    blockhash: Hash,
    limits: &TxLimits,
    locked: usize,
) -> Result<BuiltTx, BuildError> {
    let mut ixs = resource_ixs(&plan.resources);
    ixs.extend(plan.instructions.iter().cloned());
    let msg = solana_sdk::message::Message::new_with_blockhash(&ixs, Some(&plan.payer), &blockhash);
    let tx = VersionedTransaction {
        signatures: vec![Signature::default(); msg.header.num_required_signatures as usize],
        message: VersionedMessage::Legacy(msg),
    };
    let bytes = bincode_len(&tx)?;
    if bytes > limits.max_bytes_v0 {
        return Err(BuildError::TooLarge {
            bytes,
            max: limits.max_bytes_v0,
        });
    }
    Ok(BuiltTx {
        version: BuildVersion::Legacy,
        tx,
        serialized_len: bytes,
        locked_accounts: locked,
        blockhash,
    })
}

fn build_v0(
    plan: &TxPlan,
    blockhash: Hash,
    limits: &TxLimits,
    locked: usize,
) -> Result<BuiltTx, BuildError> {
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
        return Err(BuildError::TooLarge {
            bytes,
            max: limits.max_bytes_v0,
        });
    }
    Ok(BuiltTx {
        version: BuildVersion::V0,
        tx,
        serialized_len: bytes,
        locked_accounts: locked,
        blockhash,
    })
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
pub fn measure_v1(
    _plan: &TxPlan,
    _blockhash: Hash,
    _limits: &TxLimits,
) -> Result<V1Measure, BuildError> {
    Err(BuildError::V1Unavailable)
}

#[cfg(feature = "txv1")]
fn compile_v1_message(
    plan: &TxPlan,
    blockhash: Hash,
    limits: &TxLimits,
) -> Result<solana_message_v1::v1::Message, BuildError> {
    use solana_instruction_v1::{AccountMeta as V1AccountMeta, Instruction as V1Instruction};
    use solana_message_v1::v1;
    use solana_pubkey_v1::Pubkey as V1Pubkey;

    let locked = locked_account_count(&plan.payer, &plan.instructions);
    if locked > limits.max_locked_accounts {
        return Err(BuildError::TooManyAccounts {
            count: locked,
            max: limits.max_locked_accounts,
        });
    }
    if plan.instructions.len() > 64 {
        return Err(BuildError::Compile(format!(
            "too many v1 instructions: {} > 64",
            plan.instructions.len()
        )));
    }
    let loaded = plan.resources.loaded_accounts_data_size.ok_or_else(|| {
        BuildError::Compile(
            "v1 requires explicit loaded_accounts_data_size; implicit 0 is unsafe".into(),
        )
    })?;

    let payer = V1Pubkey::new_from_array(plan.payer.to_bytes());
    let ixs: Vec<V1Instruction> = plan
        .instructions
        .iter()
        .map(|ix| V1Instruction {
            program_id: V1Pubkey::new_from_array(ix.program_id.to_bytes()),
            accounts: ix
                .accounts
                .iter()
                .map(|m| V1AccountMeta {
                    pubkey: V1Pubkey::new_from_array(m.pubkey.to_bytes()),
                    is_signer: m.is_signer,
                    is_writable: m.is_writable,
                })
                .collect(),
            data: ix.data.clone(),
        })
        .collect();
    let hash = solana_hash_v1::Hash::new_from_array(blockhash.to_bytes());
    let cfg = v1::TransactionConfig::empty()
        .with_compute_unit_limit(plan.resources.compute_unit_limit)
        .with_loaded_accounts_data_size_limit(loaded)
        .with_priority_fee(plan.resources.priority_total_lamports());
    let msg = v1::Message::try_compile_with_config(&payer, &ixs, hash, cfg)
        .map_err(|e| BuildError::Compile(e.to_string()))?;
    // Official serialize includes the 0x81 prefix; Message::size() alone does not.
    let bytes = msg.serialize().len() + usize::from(msg.header.num_required_signatures) * 64;
    if bytes > limits.max_bytes_v1 {
        return Err(BuildError::TooLarge {
            bytes,
            max: limits.max_bytes_v1,
        });
    }
    Ok(msg)
}

#[cfg(feature = "txv1")]
pub fn measure_v1(
    plan: &TxPlan,
    blockhash: Hash,
    limits: &TxLimits,
) -> Result<V1Measure, BuildError> {
    let msg = compile_v1_message(plan, blockhash, limits)?;
    let message_bytes = msg.serialize().len();
    let signatures = usize::from(msg.header.num_required_signatures);
    let serialized_tx_bytes = message_bytes + signatures * 64;
    let locked = msg.account_keys.len();
    Ok(V1Measure {
        message_bytes,
        serialized_tx_bytes,
        locked_accounts: locked,
        instructions: plan.instructions.len(),
        signatures,
    })
}

/// Canonical SIMD-0385 wire, currently used only by the explicit no-send probe.
/// Signatures follow the version-prefixed message, with no count prefix.
#[cfg(feature = "txv1")]
pub fn signed_v1_wire(
    plan: &TxPlan,
    blockhash: Hash,
    limits: &TxLimits,
    signers: &[&Keypair],
) -> Result<Vec<u8>, BuildError> {
    let msg = compile_v1_message(plan, blockhash, limits)?;
    let required = usize::from(msg.header.num_required_signatures);
    if signers.len() != required {
        return Err(BuildError::Sign(format!(
            "v1 expects {} signers, got {}",
            required,
            signers.len()
        )));
    }
    let message = msg.serialize();
    let mut wire = message.clone();
    for key in msg.account_keys.iter().take(required) {
        let signer = signers
            .iter()
            .find(|s| s.pubkey().to_bytes() == key.to_bytes())
            .ok_or_else(|| BuildError::Sign(format!("missing v1 signer {key}")))?;
        let signature = signer.sign_message(&message);
        wire.extend_from_slice(signature.as_ref());
    }
    Ok(wire)
}

#[cfg(not(feature = "txv1"))]
fn build_v1(
    _plan: &TxPlan,
    _blockhash: Hash,
    _limits: &TxLimits,
    _locked: usize,
) -> Result<BuiltTx, BuildError> {
    Err(BuildError::V1Unavailable)
}

#[cfg(feature = "txv1")]
fn build_v1(
    plan: &TxPlan,
    blockhash: Hash,
    limits: &TxLimits,
    _locked: usize,
) -> Result<BuiltTx, BuildError> {
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
    let tx =
        VersionedTransaction::try_new(msg, signers).map_err(|e| BuildError::Sign(e.to_string()))?;
    built.tx = tx;
    Ok(())
}

/// Keypair aus Solana-CLI-JSON (Array von 64 Bytes) laden. Kein Key im Code, keine Env-Defaults.
/// B10: errors are fixed codes (no wallet path, no parser text that could quote a secret byte);
/// the file bytes are read into one buffer sized up front (no reallocation copies) and, like the
/// parsed key bytes (one fixed array), zeroized on drop. Copies inside the OS, the allocator or
/// the SDK are outside this guarantee.
pub fn load_keypair_json(path: &str) -> Result<Keypair, BuildError> {
    use std::io::Read;
    /// A Solana CLI keypair file is a few hundred bytes; anything larger is not a keypair.
    const MAX_KEYPAIR_FILE: u64 = 4096;
    let read_err = |e: std::io::Error| BuildError::Sign(format!("KEYPAIR_READ: {:?}", e.kind()));
    let mut file = std::fs::File::open(path).map_err(read_err)?;
    let len = file.metadata().map_err(read_err)?.len();
    if len > MAX_KEYPAIR_FILE {
        return Err(BuildError::Sign("KEYPAIR_FORMAT".into()));
    }
    let mut raw = zeroize::Zeroizing::new(Vec::with_capacity(MAX_KEYPAIR_FILE as usize + 1));
    file.by_ref()
        .take(MAX_KEYPAIR_FILE + 1)
        .read_to_end(&mut raw)
        .map_err(read_err)?;
    let text = std::str::from_utf8(&raw).map_err(|_| BuildError::Sign("KEYPAIR_FORMAT".into()))?;
    let bytes = keypair_bytes(text).ok_or_else(|| BuildError::Sign("KEYPAIR_FORMAT".into()))?;
    // [FIX-13] SDK 3.x: `Keypair::try_from(&[u8])` (from_bytes entfernt).
    Keypair::try_from(&bytes[..]).map_err(|_| BuildError::Sign("KEYPAIR_INVALID".into()))
}

/// The Solana CLI keypair format: a JSON array of exactly 64 integers 0..=255.
fn keypair_bytes(raw: &str) -> Option<zeroize::Zeroizing<[u8; 64]>> {
    let inner = raw.trim().strip_prefix('[')?.strip_suffix(']')?;
    let mut out = zeroize::Zeroizing::new([0u8; 64]);
    let mut n = 0;
    for part in inner.split(',') {
        let digits = part.trim();
        // JSON integers only: no sign, no exponent, no empty element.
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *out.get_mut(n)? = digits.parse().ok()?;
        n += 1;
    }
    (n == 64).then_some(out)
}

pub fn payer_of(kp: &Keypair) -> Pubkey {
    kp.pubkey()
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::instruction::AccountMeta;

    /// E2: the signed message must be exactly its plan. Real Legacy and V0 builds (V0 with a
    /// lookup-table address) match; each tampering — compute-budget data, a dropped compute-budget
    /// instruction, an account swap, a raised privilege, a foreign or substituted lookup table, an
    /// extra key, another fee payer — is refused with its own code.
    #[test]
    fn e2_signed_message_is_exactly_its_plan() {
        let payer = Keypair::new();
        let (p, q, a, b, c, d) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let instructions = vec![
            Instruction {
                program_id: p,
                accounts: vec![
                    AccountMeta::new(a, false),
                    AccountMeta::new_readonly(b, false),
                ],
                data: vec![1, 2, 3],
            },
            Instruction {
                program_id: q,
                accounts: vec![
                    AccountMeta::new(b, false),
                    AccountMeta::new_readonly(c, false),
                ],
                data: vec![4],
            },
        ];
        let table = AddressLookupTableAccount {
            key: Pubkey::new_unique(),
            addresses: vec![d, c],
        };
        let plan = |version, tables: Vec<AddressLookupTableAccount>| TxPlan {
            version,
            payer: payer.pubkey(),
            instructions: instructions.clone(),
            lookup_tables: tables,
            resources: Resources {
                compute_unit_limit: 200_000,
                priority_micro_lamports_per_cu: 5,
                loaded_accounts_data_size: None,
            },
        };
        let v0 = plan(BuildVersion::V0, vec![table.clone()]);
        let built = build(&v0, Hash::new_unique(), &TxLimits::default()).unwrap();
        let VersionedMessage::V0(m) = &built.tx.message else {
            panic!("v0 message expected")
        };
        assert_eq!(
            m.address_table_lookups.len(),
            1,
            "c is loaded from the table"
        );
        assert_eq!(message_matches_plan(&built.tx.message, &v0), Ok(()));
        let legacy = plan(BuildVersion::Legacy, vec![]);
        let built_legacy = build(&legacy, Hash::new_unique(), &TxLimits::default()).unwrap();
        assert_eq!(
            message_matches_plan(&built_legacy.tx.message, &legacy),
            Ok(())
        );
        let tampered = |f: &dyn Fn(&mut v0::Message)| {
            let mut m = m.clone();
            f(&mut m);
            message_matches_plan(&VersionedMessage::V0(m), &v0)
        };
        // Compute-unit limit changed / price instruction dropped.
        assert_eq!(
            tampered(&|m| m.instructions[0].data[1] ^= 1),
            Err("MESSAGE_DATA")
        );
        assert_eq!(
            tampered(&|m| {
                m.instructions.remove(1);
            }),
            Err("MESSAGE_INSTRUCTION_COUNT")
        );
        // Two accounts of the first receivership instruction swapped.
        assert_eq!(
            tampered(&|m| m.instructions[2].accounts.swap(0, 1)),
            Err("MESSAGE_ACCOUNTS")
        );
        // One readonly static key made writable.
        assert_eq!(
            tampered(&|m| m.header.num_readonly_unsigned_accounts -= 1),
            Err("MESSAGE_PRIVILEGES")
        );
        // Another fee payer.
        assert_eq!(
            tampered(&|m| m.account_keys.swap(0, 1)),
            Err("MESSAGE_PAYER")
        );
        // An extra, unused static key (Legacy: no loaded addresses behind it; in V0 it would
        // shift the loaded indexes and fail as MESSAGE_ACCOUNTS first).
        let VersionedMessage::Legacy(l) = &built_legacy.tx.message else {
            panic!("legacy message expected")
        };
        let mut extra = l.clone();
        extra.account_keys.push(Pubkey::new_unique());
        extra.header.num_readonly_unsigned_accounts += 1;
        assert_eq!(
            message_matches_plan(&VersionedMessage::Legacy(extra), &legacy),
            Err("MESSAGE_KEYS")
        );
        // A readonly key listed twice.
        let mut twice = l.clone();
        let last = *twice.account_keys.last().unwrap();
        twice.account_keys.push(last);
        twice.header.num_readonly_unsigned_accounts += 1;
        assert_eq!(
            message_matches_plan(&VersionedMessage::Legacy(twice), &legacy),
            Err("MESSAGE_KEYS")
        );
        // The message resolves through a table the plan does not have.
        assert_eq!(
            tampered(&|m| m.address_table_lookups[0].account_key = Pubkey::new_unique()),
            Err("MESSAGE_LOOKUP_TABLE_UNKNOWN")
        );
        // The plan's table at that index holds another address.
        let substituted = plan(
            BuildVersion::V0,
            vec![AddressLookupTableAccount {
                key: table.key,
                addresses: vec![d, Pubkey::new_unique()],
            }],
        );
        assert_eq!(
            message_matches_plan(&built.tx.message, &substituted),
            Err("MESSAGE_ACCOUNTS")
        );
        // Another compute-unit price in the plan than in the message.
        let mut other_price = plan(BuildVersion::V0, vec![table.clone()]);
        other_price.resources.priority_micro_lamports_per_cu = 6;
        assert_eq!(
            message_matches_plan(&built.tx.message, &other_price),
            Err("MESSAGE_DATA")
        );
    }

    /// B9: the V1 capability is what `build`/`sign` actually do, and the RPC adapter (SDK
    /// `VersionedTransaction` wire only) does not claim V1.
    #[test]
    fn b9_v1_capability_flag_matches_the_builder() {
        let payer = Keypair::new();
        let plan = TxPlan {
            version: BuildVersion::V1,
            payer: payer.pubkey(),
            instructions: vec![solana_system_interface::instruction::transfer(
                &payer.pubkey(),
                &payer.pubkey(),
                0,
            )],
            lookup_tables: vec![],
            resources: Resources {
                compute_unit_limit: 200_000,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: Some(64 * 1024),
            },
        };
        let built = build(&plan, Hash::new_unique(), &TxLimits::default());
        // The capability is exactly what the build path does; an unavailable V1 build is
        // reported as such, never as another error.
        assert_eq!(built.is_ok(), v1_build_capability());
        if !v1_build_capability() {
            assert!(matches!(built, Err(BuildError::V1Unavailable)));
        }
        // Never contacted: constructing the adapter opens no connection.
        let rpc = crate::submit::rpc::RpcSubmitter::new("http://127.0.0.1:9".into(), false);
        use crate::submit::SubmissionAdapter;
        assert!(!rpc.supports_tx_version(BuildVersion::V1));
        assert!(rpc.supports_tx_version(BuildVersion::V0));
    }

    /// B10: wrong path, malformed JSON, a byte out of range, a wrong length and a key whose
    /// public half does not match fail with fixed codes that name neither the path nor a value.
    #[test]
    fn b10_keypair_errors_never_carry_path_or_secret_bytes() {
        let dir = std::env::temp_dir().join(format!("b10-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let kp = Keypair::new();
        let good = dir.join("SECRET_WALLET_PATH.json");
        let json = |b: &[u8]| {
            format!(
                "[{}]\n",
                b.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
            )
        };
        std::fs::write(&good, json(&kp.to_bytes())).unwrap();
        let loaded = load_keypair_json(good.to_str().unwrap()).unwrap();
        assert_eq!(loaded.pubkey(), kp.pubkey());
        let mut wrong_pub = kp.to_bytes();
        wrong_pub[63] ^= 1;
        let mut out_of_range = json(&kp.to_bytes());
        out_of_range.replace_range(1..1, "300,");
        let mut signed = json(&kp.to_bytes()[..63]);
        signed.replace_range(1..1, "+1,");
        let cases = [
            ("missing", None, "KEYPAIR_READ: NotFound"),
            ("not_json", Some("{\"k\":1}".to_string()), "KEYPAIR_FORMAT"),
            ("range", Some(out_of_range), "KEYPAIR_FORMAT"),
            ("short", Some(json(&kp.to_bytes()[..63])), "KEYPAIR_FORMAT"),
            (
                "long",
                Some(json(&[kp.to_bytes().as_slice(), &[1]].concat())),
                "KEYPAIR_FORMAT",
            ),
            ("mismatch", Some(json(&wrong_pub)), "KEYPAIR_INVALID"),
            ("signed", Some(signed), "KEYPAIR_FORMAT"),
            ("huge", Some(" ".repeat(5000)), "KEYPAIR_FORMAT"),
        ];
        let secret_byte = kp.to_bytes()[5].to_string();
        for (name, body, code) in cases {
            let path = dir.join(format!("SECRET_WALLET_PATH_{name}.json"));
            if let Some(b) = body {
                std::fs::write(&path, b).unwrap();
            }
            let e = load_keypair_json(path.to_str().unwrap())
                .err()
                .unwrap()
                .to_string();
            assert!(e.ends_with(code), "{name}: {e}");
            assert!(
                !e.contains("SECRET_WALLET_PATH") && !e.contains("300"),
                "{name}: {e}"
            );
            assert!(!e.contains(&format!(" {secret_byte}")), "{name}: {e}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "txv1")]
    #[test]
    fn v1_wire_signature_and_exact_size_boundary() {
        let payer = Keypair::new();
        let plan = TxPlan {
            version: BuildVersion::V1,
            payer: payer.pubkey(),
            instructions: vec![solana_system_interface::instruction::transfer(
                &payer.pubkey(),
                &payer.pubkey(),
                0,
            )],
            lookup_tables: vec![],
            resources: Resources {
                compute_unit_limit: 200_000,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: Some(64 * 1024),
            },
        };
        let hash = Hash::new_unique();
        let wire = signed_v1_wire(&plan, hash, &TxLimits::default(), &[&payer]).unwrap();
        assert_eq!(wire[0], 0x81);
        let measurement = measure_v1(&plan, hash, &TxLimits::default()).unwrap();
        assert_eq!(wire.len(), measurement.serialized_tx_bytes);
        assert_eq!(wire.len(), measurement.message_bytes + 64);
        let signature = Signature::try_from(&wire[measurement.message_bytes..]).unwrap();
        assert!(signature.verify(payer.pubkey().as_ref(), &wire[..measurement.message_bytes]));
        let mut tampered = wire[..measurement.message_bytes].to_vec();
        tampered[0] = 0x80;
        assert!(!signature.verify(payer.pubkey().as_ref(), &tampered));
        let mut limits = TxLimits {
            max_bytes_v1: wire.len(),
            ..Default::default()
        };
        assert!(signed_v1_wire(&plan, hash, &limits, &[&payer]).is_ok());
        limits.max_bytes_v1 -= 1;
        assert!(matches!(
            signed_v1_wire(&plan, hash, &limits, &[&payer]),
            Err(BuildError::TooLarge { .. })
        ));
        assert!(signed_v1_wire(&plan, hash, &TxLimits::default(), &[&Keypair::new()]).is_err());
    }

    #[test]
    fn locked_accounts_are_deduplicated() {
        let payer = Pubkey::new_unique();
        let prog = Pubkey::new_unique();
        let a = Pubkey::new_unique();
        let ixs = vec![
            Instruction::new_with_bytes(
                prog,
                &[],
                vec![AccountMeta::new(a, false), AccountMeta::new(payer, true)],
            ),
            Instruction::new_with_bytes(prog, &[], vec![AccountMeta::new_readonly(a, false)]),
        ];
        assert_eq!(locked_account_count(&payer, &ixs), 3);
    }

    #[test]
    fn priority_fee_uses_requested_limit_not_simulated_consumption() {
        let resources = Resources {
            compute_unit_limit: 1_400_000,
            priority_micro_lamports_per_cu: 2_000,
            loaded_accounts_data_size: None,
        };
        assert_eq!(resources.priority_total_lamports(), 2_800);
        let simulated_consumption = 600u64;
        assert_ne!(
            resources.priority_total_lamports(),
            simulated_consumption * 2_000 / 1_000_000
        );
    }

    #[test]
    fn v1_priority_fee_is_total_lamports() {
        let r = Resources {
            compute_unit_limit: 200_000,
            priority_micro_lamports_per_cu: 5_000,
            loaded_accounts_data_size: None,
        };
        assert_eq!(r.priority_total_lamports(), 1_000);
    }

    #[test]
    fn v1_priority_fee_rounds_up_to_total_lamports() {
        let r = Resources {
            compute_unit_limit: 1,
            priority_micro_lamports_per_cu: 1,
            loaded_accounts_data_size: Some(32 * 1024),
        };
        assert_eq!(r.priority_total_lamports(), 1);
    }

    #[test]
    fn v1_effective_list_has_no_compute_budget_instructions() {
        let plan = TxPlan {
            version: BuildVersion::V1,
            payer: Pubkey::new_unique(),
            instructions: vec![Instruction::new_with_bytes(
                Pubkey::new_unique(),
                &[],
                vec![],
            )],
            lookup_tables: vec![],
            resources: Resources {
                compute_unit_limit: 200_000,
                priority_micro_lamports_per_cu: 5_000,
                loaded_accounts_data_size: Some(32 * 1024),
            },
        };
        assert_eq!(effective_instruction_count(&plan), 1);
    }

    #[test]
    fn too_many_accounts_is_rejected_before_compile() {
        let payer = Pubkey::new_unique();
        let prog = Pubkey::new_unique();
        let metas: Vec<AccountMeta> = (0..70)
            .map(|_| AccountMeta::new(Pubkey::new_unique(), false))
            .collect();
        let plan = TxPlan {
            version: BuildVersion::V0,
            payer,
            instructions: vec![Instruction::new_with_bytes(prog, &[], metas)],
            lookup_tables: vec![],
            resources: Resources {
                compute_unit_limit: 1,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: None,
            },
        };
        match build(&plan, Hash::default(), &TxLimits::default()) {
            Err(BuildError::TooManyAccounts { count, max }) => {
                assert_eq!(max, 64);
                assert!(count > 64);
            }
            other => panic!(
                "expected TooManyAccounts, got {:?}",
                other.map(|b| b.serialized_len)
            ),
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
                resources: Resources {
                    compute_unit_limit: 1,
                    priority_micro_lamports_per_cu: 0,
                    loaded_accounts_data_size: Some(0),
                },
            };
            assert!(matches!(
                build(&plan, Hash::default(), &TxLimits::default()),
                Err(BuildError::V1Unavailable)
            ));
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
