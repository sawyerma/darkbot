//! DEC-18 §2 (P0-025, approval C-01 steps 5–6): the typed records of the deployment read — its
//! plan (`C01_READ_PLAN`), what was read (`C01_READS`) and the parity verdict derived from it
//! (`P1A_PARITY`). Plain data, independent of the health engine, so every feature set registers
//! and reads the same kinds (`c01_read` reads).
use crate::analytics::journal;
use crate::analytics::payload::Payload;
use serde::{Deserialize, Serialize};

/// The store of C-01 plans and their reads (under a project; git-ignored, P0-026).
pub const STORE: &str = "data/c01";

/// The read budget a plan binds (the FE-01 scope's: `fe_calls`, `fe_max_mib`, `fe_seconds`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Budget {
    pub calls: u64,
    pub mib: u64,
    pub seconds: u64,
    pub rps: u64,
    pub batch: u64,
}

/// The offline step: what is read, through which provider, under which budget and identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Plan {
    pub plan_id: String,
    pub created_unix_ms: Option<u64>,
    pub registry_version: u64,
    pub rpc: super::provider_measurement::ProviderRef,
    pub config_digest: String,
    pub binary_sha256: String,
    pub program: String,
    pub group: String,
    pub budget: C01Budget,
    /// The reads, in order, as the operator approves them.
    pub reads: Vec<String>,
}
impl Payload for C01Plan {
    const KIND: u16 = journal::kind::C01_READ_PLAN;
}

/// The deployment (P1A-01): the program account and its ProgramData.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Deployment {
    /// `None`: the program account did not decode as a loader-v3 program (its reason).
    pub program_data: Option<String>,
    pub decode_error: Option<String>,
    pub executable: bool,
    pub deployment_slot: Option<u64>,
    pub upgrade_authority: Option<String>,
    /// SHA-256 of the whole ProgramData account (the deployment pin's hash, base58).
    pub account_hash: Option<String>,
    /// SHA-256 of the deployed ELF (hex) and its length.
    pub elf_sha256: Option<String>,
    pub elf_len: Option<u64>,
    /// REFERENCE, NOT_REFERENCE or UNPINNED (`protocol::deployment_pin`).
    pub pin: Option<String>,
}

/// The program's Anchor IDL account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Idl {
    pub address: String,
    pub present: bool,
    pub authority: Option<String>,
    pub compressed_sha256: Option<String>,
    /// SHA-256 of the decompressed IDL bytes (compared with `protocol::SOURCE_IDL_SHA256`).
    pub sha256: Option<String>,
    /// Its instruction names, in order.
    pub instructions: Vec<String>,
    pub error: Option<String>,
}

/// One bank of the group (the resize census).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Bank {
    pub key: String,
    pub len: u64,
    /// The known layout of its length (`0.1.11`, `0.1.12`), if any.
    pub layout: Option<String>,
    /// Whether the pinned crate decodes it exactly.
    pub decodes: bool,
    pub oracle_setup: Option<u8>,
    /// Its observation keys (oracles, integrations); `Err` text when the setup is unsupported.
    pub observation_keys: Vec<String>,
    pub observation_error: Option<String>,
}

/// One oracle or integration account a bank observes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Dependency {
    pub key: String,
    /// `None`: the account does not exist.
    pub owner: Option<String>,
    pub len: u64,
    pub sha256: Option<String>,
}

/// A token program's bytecode (C-10, the EXIT-WSOL proof): its loader and ELF.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Program {
    pub name: String,
    pub key: String,
    pub owner: Option<String>,
    pub executable: bool,
    pub elf_sha256: Option<String>,
    pub elf_len: Option<u64>,
    /// The file the ELF is stored in beside the record (`programs/<name>.so`).
    pub file: Option<String>,
    pub error: Option<String>,
}

/// What a C-01 run read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Reads {
    pub plan_id: String,
    pub plan_sha256: String,
    pub started_unix_ms: Option<u64>,
    pub finished_unix_ms: Option<u64>,
    /// The context slot of every read, in order (never regressing).
    pub context_slots: Vec<u64>,
    pub deployment: C01Deployment,
    pub idl: C01Idl,
    pub group_decodes: bool,
    pub fee_state_decodes: bool,
    pub banks: Vec<C01Bank>,
    pub dependencies: Vec<C01Dependency>,
    pub programs: Vec<C01Program>,
    /// A read that did not complete (the run ends there).
    pub error: Option<String>,
    pub calls: u64,
    pub bytes: u64,
}
impl Payload for C01Reads {
    const KIND: u16 = journal::kind::C01_READS;
}

/// A reproducible build of the pinned source (the BUILD gate, `p0ctl c01 build`): the source
/// checkout's commit as git reports it, the ELF's hash computed from the file (its trailing zero
/// padding trimmed, as deployed), and how it was built.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Build {
    pub plan_id: String,
    /// `HEAD` of the source checkout and whether a tracked file differs from it.
    pub source_commit: String,
    pub source_modified: bool,
    /// The build as the operator ran it: the toolchain image and the command.
    pub image: String,
    pub command: String,
    pub elf_sha256: String,
    pub elf_len: u64,
    pub recorded_unix_ms: Option<u64>,
}
impl Payload for C01Build {
    const KIND: u16 = journal::kind::C01_BUILD;
}

/// The FE-01 observation bound to the read deployment (the FE01 gate, `p0ctl c01 fe01`): what the
/// sealed `fe01.p0j` of `p0-fe-qualify --fe-mode full --execute` configured and observed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct C01Fe01 {
    pub plan_id: String,
    /// SHA-256 of the observation's journal (hex); whether its manifest sealed it complete.
    pub journal_sha256: String,
    pub sealed_complete: bool,
    pub execution: String,
    pub status: String,
    pub reason: Option<String>,
    /// The observation's chain slot, ProgramData, the SHA-256 of that whole account (hex) and
    /// its deployment slot; `None` when no scope closed.
    pub slot: Option<u64>,
    pub program_data: Option<String>,
    pub program_data_sha256: Option<String>,
    pub deployment_slot: Option<u64>,
    pub accounts: u64,
    pub targets: u64,
    pub recorded_unix_ms: Option<u64>,
}
impl Payload for C01Fe01 {
    const KIND: u16 = journal::kind::C01_FE01;
}

/// The FE-01 full observation's read plan (C-01 step 6, `p0-fe-qualify --plan`): the provider,
/// the evidence, the effective configuration and its bounds, the executable — what the operator
/// approves by its token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fe01Plan {
    pub plan_id: String,
    pub created_unix_ms: Option<u64>,
    pub registry_version: u64,
    pub rpc: super::provider_measurement::ProviderRef,
    pub evidence_id: String,
    /// SHA-256 of the evidence record's bytes.
    pub evidence_sha256: String,
    /// SHA-256 of the observation's effective configuration (bindings and bounds included).
    pub config_digest: String,
    pub binary_sha256: String,
    pub funding_class: String,
    pub group: String,
    pub targets: Vec<String>,
    /// The bounds: calls, seconds, wire MiB (the journal bound too), requests per second,
    /// keys per read, scope rounds; the reads of the initial scope alone and of the worst case.
    pub calls: u64,
    pub seconds: u64,
    pub mib: u64,
    pub rps: u64,
    pub batch: u64,
    pub rounds: u64,
    pub required_reads: u64,
    pub worst_case_reads: u64,
}
impl Payload for Fe01Plan {
    const KIND: u16 = journal::kind::C01_FE01_PLAN;
}

/// One gate of the parity (DEC-18 §2.2–2.5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityGate {
    pub name: String,
    /// PASS, FAIL, BLOCKED or NOT_RUN.
    pub verdict: String,
    pub mandatory: bool,
    pub detail: String,
}

/// The parity verdict of a C-01 run: PASS only when every mandatory gate passes (DEC-18 §2.4:
/// only then may step 7 promote).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P1aParity {
    pub plan_id: String,
    /// SHA-256 of the reads record it judges.
    pub reads_sha256: String,
    pub source_tag: String,
    pub gates: Vec<ParityGate>,
    /// PASS, or BLOCKED naming the first mandatory gate that is not.
    pub overall: String,
}
impl Payload for P1aParity {
    const KIND: u16 = journal::kind::P1A_PARITY;
}
