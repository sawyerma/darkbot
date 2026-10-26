//! JFI-01: typed binary payloads of the `.p0j` journal.
//!
//! A typed frame carries the native encoding of exactly one Rust payload type per kind: bincode
//! (little-endian, fixed-width integers incl. exact i128/u128, no trailing bytes, bounded by the
//! frame limit). There is no JSON value model in between. The frame `schema` is the payload
//! version of that kind (≥ 2); a reader accepts only the version its type declares and fails
//! closed on any other. Schema 1 (generic value tree) is the historical format: never written for a
//! kind that has a payload type; its JSON view (`legacy_view`) exists only in the `historical`
//! build (DEC-09).
use super::journal;
use crate::market::observer::rows as market;
use crate::market::observer::verification as strict;
use bincode::Options;
use serde::{de::DeserializeOwned, Serialize};
use std::io;

/// First typed payload version of every kind.
pub const TYPED: u16 = 2;

pub trait Payload: Serialize + DeserializeOwned {
    const KIND: u16;
    /// Payload version within the kind; bumped on every incompatible layout change.
    const SCHEMA: u16 = TYPED;
    /// A frame of an earlier version of this kind, so historical evidence stays readable after a
    /// layout change (`None`: that version is not readable). Writers only ever write `SCHEMA`.
    fn decode_earlier(_schema: u16, _bytes: &[u8]) -> Option<io::Result<Self>> {
        None
    }
}

fn options() -> impl Options {
    bincode::DefaultOptions::new()
        .with_little_endian()
        .with_fixint_encoding()
        .reject_trailing_bytes()
        .with_limit(journal::MAX_FRAME as u64)
}

fn bad(code: &'static str) -> io::Error {
    crate::analytics::journal::coded(code)
}

pub fn encode<P: Payload>(payload: &P) -> io::Result<Vec<u8>> {
    options()
        .serialize(payload)
        .map_err(|_| bad("PAYLOAD_ENCODE"))
}

/// Exact encoded size without allocating (reserve before encode).
pub fn encoded_len<P: Payload>(payload: &P) -> io::Result<u64> {
    options()
        .serialized_size(payload)
        .map_err(|_| bad("PAYLOAD_ENCODE"))
}

pub fn decode<P: Payload>(kind: u16, schema: u16, bytes: &[u8]) -> io::Result<P> {
    if kind != P::KIND {
        return Err(bad("PAYLOAD_KIND"));
    }
    if schema != P::SCHEMA {
        return P::decode_earlier(schema, bytes).unwrap_or_else(|| Err(bad("PAYLOAD_SCHEMA")));
    }
    decode_wire(bytes)
}

/// The payload encoding of any wire type, also an earlier layout of a kind.
pub fn decode_wire<T: DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    options()
        .deserialize(bytes)
        .map_err(|_| bad("PAYLOAD_DECODE"))
}

/// Test fixture: the bytes of an earlier layout, as its writer produced them.
#[cfg(test)]
pub(crate) fn encode_wire<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    options()
        .serialize(value)
        .map_err(|_| bad("PAYLOAD_ENCODE"))
}

impl Payload for darkbot_solana::runtime_config::EffectiveConfig {
    const KIND: u16 = journal::kind::OPERATIONAL_CONFIG;
    /// Embeds `Limits`: follows its layout revision (revision 1 = schema 2).
    const SCHEMA: u16 = darkbot_solana::runtime_config::LIMITS_REVISION + 1;
    /// A record of an earlier `Limits` revision (from `LIMITS_READABLE_FROM`) stays readable:
    /// the fields that revision did not have keep their built-in default.
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<io::Result<Self>> {
        let revision = schema.checked_sub(1)?;
        darkbot_solana::runtime_config::fields_at(revision)?;
        let mut rest = bytes;
        Some(effective_of_revision(revision, &mut rest).and_then(|e| {
            if rest.is_empty() {
                Ok(e)
            } else {
                Err(bad("PAYLOAD_DECODE"))
            }
        }))
    }
}

/// One value of the payload encoding read from the front of `bytes` (advanced past it).
pub(crate) fn decode_prefix<T: DeserializeOwned>(bytes: &mut &[u8]) -> io::Result<T> {
    options()
        .deserialize_from(&mut *bytes)
        .map_err(|_| bad("PAYLOAD_DECODE"))
}

/// `EffectiveConfig` around the `Limits` (the fields before and after it, unchanged since
/// `LIMITS_READABLE_FROM`; from `OWNER_SETTINGS_REVISION` on [`EffectiveOwner`] follows).
#[derive(serde::Deserialize)]
struct EffectiveHead {
    schema: String,
    profile: darkbot_solana::runtime_config::Profile,
}
#[derive(serde::Deserialize)]
struct EffectiveTail {
    rpc_endpoint_count: u64,
    wss_endpoint_count: u64,
    fallback: bool,
    adaptive: bool,
    duration: String,
    verification: String,
    diagnostics: bool,
    fe_mode: String,
    fe_targets: Vec<String>,
    fe_group: String,
    sources: std::collections::BTreeMap<String, String>,
    provider_capacity_qualified: bool,
    free_only: bool,
    live_authorized: bool,
    submission_authorized: bool,
}
/// DEC-05: the owner's typed settings at the end of an `EffectiveConfig`.
#[derive(serde::Deserialize)]
struct EffectiveOwner {
    threshold_trigger: bool,
    c0_census: bool,
    source_geyser: String,
    source_geyser_provider: String,
}

/// An `EffectiveConfig` of `Limits` layout `revision` from the front of `bytes`. A record before
/// `OWNER_SETTINGS_REVISION` has no owner settings: they read as the built-in defaults, like the
/// `Limits` fields its layout did not have.
pub(crate) fn effective_of_revision(
    revision: u16,
    bytes: &mut &[u8],
) -> io::Result<darkbot_solana::runtime_config::EffectiveConfig> {
    let head: EffectiveHead = decode_prefix(bytes)?;
    let (limits, rest) = darkbot_solana::runtime_config::Limits::decode_revision(revision, bytes)
        .ok_or_else(|| bad("PAYLOAD_DECODE"))?;
    *bytes = rest;
    let t: EffectiveTail = decode_prefix(bytes)?;
    let owner = if revision >= darkbot_solana::runtime_config::OWNER_SETTINGS_REVISION {
        decode_prefix(bytes)?
    } else {
        let d = darkbot_solana::runtime_config::RuntimeConfig::default();
        EffectiveOwner {
            threshold_trigger: d.threshold_trigger,
            c0_census: d.c0_census,
            source_geyser: d.source_geyser,
            source_geyser_provider: d.source_geyser_provider,
        }
    };
    Ok(darkbot_solana::runtime_config::EffectiveConfig {
        schema: head.schema,
        profile: head.profile,
        limits,
        rpc_endpoint_count: t.rpc_endpoint_count,
        wss_endpoint_count: t.wss_endpoint_count,
        fallback: t.fallback,
        adaptive: t.adaptive,
        duration: t.duration,
        verification: t.verification,
        diagnostics: t.diagnostics,
        fe_mode: t.fe_mode,
        fe_targets: t.fe_targets,
        fe_group: t.fe_group,
        sources: t.sources,
        provider_capacity_qualified: t.provider_capacity_qualified,
        free_only: t.free_only,
        live_authorized: t.live_authorized,
        submission_authorized: t.submission_authorized,
        threshold_trigger: owner.threshold_trigger,
        c0_census: owner.c0_census,
        source_geyser: owner.source_geyser,
        source_geyser_provider: owner.source_geyser_provider,
    })
}

/// Test fixture: `value`'s payload with the `Limits` of `effective` re-encoded in layout
/// `revision` and, before `OWNER_SETTINGS_REVISION`, without its owner settings (what that
/// revision's writer produced); the limits must occur exactly once in the payload.
#[cfg(test)]
pub(crate) fn encode_with_limits_revision<T: Serialize>(
    value: &T,
    effective: &darkbot_solana::runtime_config::EffectiveConfig,
    revision: u16,
) -> Vec<u8> {
    let limits = &effective.limits;
    let mut bytes = encode_wire(value).unwrap();
    if revision < darkbot_solana::runtime_config::OWNER_SETTINGS_REVISION {
        let owner = encode_wire(&(
            effective.threshold_trigger,
            effective.c0_census,
            &effective.source_geyser,
            &effective.source_geyser_provider,
        ))
        .unwrap();
        let at = bytes
            .windows(owner.len())
            .position(|w| w == owner.as_slice())
            .expect("owner settings inside the payload");
        bytes.drain(at..at + owner.len());
    }
    let current = encode_wire(limits).unwrap();
    let at = bytes
        .windows(current.len())
        .position(|w| w == current.as_slice())
        .expect("limits inside the payload");
    assert!(
        bytes[at + 1..]
            .windows(current.len())
            .all(|w| w != current.as_slice()),
        "limits occur once"
    );
    let mut old = bytes[..at].to_vec();
    old.extend(limits.encode_revision(revision).unwrap());
    old.extend(&bytes[at + current.len()..]);
    old
}

/// The manifest: last frame of a sealed journal, binding frame count and chain root.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub frames: u64,
    pub root_sha256: [u8; 32],
    pub complete: bool,
}
impl Payload for Manifest {
    const KIND: u16 = journal::MANIFEST;
}

/// Owner diagnostic line; the writer thread also prints it to stderr afterwards.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OwnerDiagnostic {
    pub line: String,
}
impl Payload for OwnerDiagnostic {
    const KIND: u16 = journal::kind::OWNER_DIAGNOSTIC;
}

/// S1-02: one operator request to change a source unit's control (`p0ctl source set`). The
/// runtime's control thread validates it; only the state owner applies it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceControlRequest {
    pub seq: u64,
    /// `wss` or `geyser:<provider>`.
    pub unit: String,
    /// `off`, `observe` or `active`.
    pub mode: String,
    pub requested_unix_s: u64,
}
impl Payload for SourceControlRequest {
    const KIND: u16 = journal::kind::SOURCE_CONTROL_REQUEST;
}

/// S1-02: the state owner's decision on one source-control request — what `p0ctl source status`
/// reports as applied (the request alone is only what the operator asked for).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceControlOutcome {
    pub seq: u64,
    /// `<unit> <mode>` as requested (`unreadable` when the request file was not readable).
    pub request: String,
    pub applied: bool,
    /// The refusal reason, or the canonical handoff of an applied control.
    pub detail: String,
    /// The runtime owner process that decided it.
    pub owner_pid: u32,
    /// `None`: the wall clock was not readable.
    pub decided_unix_s: Option<u64>,
}
impl Payload for SourceControlOutcome {
    const KIND: u16 = journal::kind::SOURCE_CONTROL_OUTCOME;
}

/// DEC-13: one operator request for a capability (`p0ctl capability set`): what the operator
/// asks for; only the state owner decides what is effective.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CapabilityControlRequest {
    pub seq: u64,
    /// `LIVE`.
    pub capability: String,
    /// `OFF` or `LIVE`.
    pub requested: String,
    pub requested_unix_s: u64,
}
impl Payload for CapabilityControlRequest {
    const KIND: u16 = journal::kind::CAPABILITY_CONTROL_REQUEST;
}

/// DEC-13: the state owner's decision on one capability request — the requested value it took
/// over and the effective one with its block reason (`p0ctl capability status`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CapabilityControlOutcome {
    pub seq: u64,
    /// `<capability> <requested>` as requested (`unreadable` when the file was not readable).
    pub request: String,
    /// The request was valid and its value is now the requested one.
    pub applied: bool,
    pub requested: String,
    pub effective: String,
    /// Why the effective value differs from the requested one (or why the request was refused).
    pub detail: String,
    pub owner_pid: u32,
    /// `None`: the wall clock was not readable.
    pub decided_unix_s: Option<u64>,
}
impl Payload for CapabilityControlOutcome {
    const KIND: u16 = journal::kind::CAPABILITY_CONTROL_OUTCOME;
}

/// C0 census row: one debtor (or undecodable member) of the configured group, evaluated against
/// one canonical state view and chain clock; `class` under the result frame's model.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct C0CensusAccount {
    pub attempt: u32,
    pub account: solana_sdk::pubkey::Pubkey,
    pub group: solana_sdk::pubkey::Pubkey,
    /// Slot of the account's canonical state record.
    pub record_slot: Option<u64>,
    pub row: darkbot_project0::census::CensusRow,
    pub class: darkbot_project0::census::C0Class,
}
impl Payload for C0CensusAccount {
    const KIND: u16 = journal::kind::C0_CENSUS_ROW;
}

/// C0 census result: clock, model, identity coverage and totals of one attempt.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct C0CensusResult {
    pub attempt: u32,
    pub group: solana_sdk::pubkey::Pubkey,
    pub clock_slot: u64,
    pub clock_unix_timestamp: i64,
    pub clock_record_slot: u64,
    pub model: darkbot_project0::census::CensusModel,
    pub expected_accounts: u64,
    pub rows: u64,
    pub missing: u64,
    pub extra: u64,
    pub duplicates: u64,
    pub complete: bool,
    pub reason: Option<String>,
    pub census: darkbot_project0::census::Census,
    pub owner_batch_ms: u64,
    pub worker_ms: u64,
}
impl Payload for C0CensusResult {
    const KIND: u16 = journal::kind::C0_CENSUS_RESULT;
}

/// C0 census import (`c0_census.py` → `p0-journal import 900`): one program account of the
/// scanned context slot — its 8-byte discriminator slice only, never the account data.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct C0CensusImportAccount {
    pub pubkey: solana_sdk::pubkey::Pubkey,
    pub lamports: u64,
    pub owner: solana_sdk::pubkey::Pubkey,
    pub discriminator: Vec<u8>,
    pub space: Option<u64>,
    pub slot: u64,
}
impl Payload for C0CensusImportAccount {
    const KIND: u16 = journal::kind::C0_CENSUS_ACCOUNT;
}

/// C0 census import (`p0-journal import 901`): one group indexer account slice.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct C0CensusImportGroupAccount {
    pub pubkey: solana_sdk::pubkey::Pubkey,
    pub indexer_flags: Vec<u8>,
    pub space: Option<u64>,
    pub slot: u64,
}
impl Payload for C0CensusImportGroupAccount {
    const KIND: u16 = journal::kind::C0_CENSUS_GROUP_ACCOUNT;
}

/// One import row of the census handoff: `key=value` fields separated by single spaces, exactly
/// the given keys, each once (pubkeys base58, byte slices base64, `space=-` when unknown).
fn import_fields<'a>(line: &'a str, keys: &[&str]) -> Result<Vec<&'a str>, String> {
    let malformed = || {
        format!(
            "IMPORT_ROW_FORMAT:{}",
            line.chars().take(120).collect::<String>()
        )
    };
    let mut values = vec![None; keys.len()];
    for field in line.split(' ') {
        let (k, v) = field.split_once('=').ok_or_else(malformed)?;
        let i = keys.iter().position(|x| *x == k).ok_or_else(malformed)?;
        if v.is_empty() || values[i].replace(v).is_some() {
            return Err(malformed());
        }
    }
    values
        .into_iter()
        .map(|v| v.ok_or_else(malformed))
        .collect()
}
fn import_key(v: &str) -> Result<solana_sdk::pubkey::Pubkey, String> {
    v.parse().map_err(|_| format!("IMPORT_ROW_PUBKEY:{v}"))
}
fn import_u64(v: &str) -> Result<u64, String> {
    v.parse().map_err(|_| format!("IMPORT_ROW_NUMBER:{v}"))
}
fn import_bytes(v: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(v)
        .map_err(|_| format!("IMPORT_ROW_BASE64:{v}"))
}
fn import_space(v: &str) -> Result<Option<u64>, String> {
    (v != "-").then(|| import_u64(v)).transpose()
}
impl C0CensusImportAccount {
    pub fn from_import_line(line: &str) -> Result<Self, String> {
        let keys = [
            "pubkey",
            "lamports",
            "owner",
            "discriminator",
            "space",
            "slot",
        ];
        let v = import_fields(line, &keys)?;
        Ok(Self {
            pubkey: import_key(v[0])?,
            lamports: import_u64(v[1])?,
            owner: import_key(v[2])?,
            discriminator: import_bytes(v[3])?,
            space: import_space(v[4])?,
            slot: import_u64(v[5])?,
        })
    }
}
impl C0CensusImportGroupAccount {
    pub fn from_import_line(line: &str) -> Result<Self, String> {
        let keys = ["pubkey", "indexer_flags", "space", "slot"];
        let v = import_fields(line, &keys)?;
        Ok(Self {
            pubkey: import_key(v[0])?,
            indexer_flags: import_bytes(v[1])?,
            space: import_space(v[2])?,
            slot: import_u64(v[3])?,
        })
    }
}

/// FE-01 (PHASENPLAN §13): one bank of a frozen one-class scope — its identity, its oracle setup
/// and observation keys, every dependency (mint, oracles, integration accounts) and a hash of its
/// governance configuration.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FrozenBank {
    pub bank: solana_sdk::pubkey::Pubkey,
    pub mint: solana_sdk::pubkey::Pubkey,
    pub mint_decimals: u8,
    pub liquidity_vault: solana_sdk::pubkey::Pubkey,
    pub oracle_setup: u8,
    pub oracle_keys: Vec<solana_sdk::pubkey::Pubkey>,
    pub dependencies: Vec<solana_sdk::pubkey::Pubkey>,
    pub config_sha256: String,
}

/// FE-01 one-class offline freeze: the pinned identities of one closed observation scope for
/// exactly one funding class (`orchestration::fe_freeze`). Never a qualification by itself.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Freeze {
    pub funding_class: String,
    pub source_commit: String,
    pub program: solana_sdk::pubkey::Pubkey,
    pub program_data: solana_sdk::pubkey::Pubkey,
    pub program_data_sha256: String,
    pub deployment_slot: u64,
    pub group: solana_sdk::pubkey::Pubkey,
    pub fee_state: solana_sdk::pubkey::Pubkey,
    pub transaction_version: String,
    /// The class's own pins beyond the scope (swap programs, published LUTs, token programs).
    pub class_pins: Vec<solana_sdk::pubkey::Pubkey>,
    pub targets: Vec<solana_sdk::pubkey::Pubkey>,
    pub banks: Vec<FrozenBank>,
    pub observed_slot: u64,
    /// From saved observations (replay), not a current read.
    pub historical: bool,
}
impl Payload for Fe01Freeze {
    const KIND: u16 = journal::kind::FE01_FREEZE;
}

/// FE-01 (kind 700, JFI-04 schema 2): the configuration of one observation — requested and
/// effective (budget-clamped) typed config, the provider qualification it was admitted on
/// (`None` offline) and its execution class. Schema-1 value frames stay readable.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Config {
    pub requested: darkbot_solana::runtime_config::EffectiveConfig,
    pub effective: darkbot_solana::runtime_config::EffectiveConfig,
    pub qualification_sha256: Option<String>,
    pub execution: String,
}
impl Payload for Fe01Config {
    const KIND: u16 = journal::kind::FE01_CONFIG;
}

/// FE-01 (kind 701, schema 2): one saved `getMultipleAccounts` read (base64, finalized): the
/// requested keys and context floor and the answer as decoded at the wire edge, fsynced before
/// the read is used, so a replay validates exactly these values again.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Read {
    pub keys: Vec<solana_sdk::pubkey::Pubkey>,
    pub min_context_slot: u64,
    pub outcome: Fe01ReadOutcome,
}
impl Payload for Fe01Read {
    const KIND: u16 = journal::kind::FE01_READ;
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Fe01ReadOutcome {
    /// The provider's answer: its context slot and one entry per requested key (`None`: absent).
    Accounts {
        context_slot: u64,
        accounts: Vec<Option<Fe01Account>>,
    },
    /// No decodable answer: the call failed or the result did not match the wire type.
    Failed { reason: String },
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Account {
    pub owner: solana_sdk::pubkey::Pubkey,
    pub lamports: u64,
    pub executable: bool,
    pub data: Vec<u8>,
}

/// FE-01 (kind 702, schema 2): the observation report. Never a qualification: funding,
/// simulation, exits and source parity stay UNKNOWN/NOT_RUN and LIVE/submission stay off.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Report {
    pub status: String,
    pub reason: Option<String>,
    /// The closed scope; `None` when the observation ended before one closed.
    pub observation: Option<Fe01Observation>,
    pub scope: Option<Fe01Scope>,
    pub freeze: Option<Fe01FreezeSummary>,
    pub logical_reads: Option<u64>,
    pub provider_actual: Option<darkbot_solana::provider::ProviderSnapshot>,
    pub provider_cooldown: Option<darkbot_solana::provider::CooldownStatus>,
    pub replay: Option<Fe01Replay>,
    pub source_parity: String,
    pub fe01: String,
    pub fe02: String,
    pub pilot_ready: bool,
    pub live_enabled: bool,
    pub submission_enabled: bool,
}
impl Payload for Fe01Report {
    const KIND: u16 = journal::kind::FE01_RESULT;
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Observation {
    pub rounds: u64,
    pub slot: u64,
    pub unix_timestamp: i64,
    pub clock: String,
    pub program_data: solana_sdk::pubkey::Pubkey,
    pub program_data_sha256: String,
    pub deployment_slot: u64,
    pub source_pin: String,
    pub source_parity_reason: String,
    pub transaction_version: String,
    pub accounts: Vec<Fe01AccountDigest>,
    pub targets: Vec<Fe01Target>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01AccountDigest {
    pub pubkey: solana_sdk::pubkey::Pubkey,
    pub owner: solana_sdk::pubkey::Pubkey,
    pub lamports: u64,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Target {
    pub account: solana_sdk::pubkey::Pubkey,
    pub health: String,
    pub liquidatable: Option<bool>,
    pub maintenance_health_bits: Option<i128>,
    pub receivership_eligibility: String,
    pub eligibility_reason: Option<String>,
    pub funding: String,
    pub net_edge_usd_micro: Option<i128>,
    pub simulation: String,
    pub collateral_exit: String,
    pub source_parity: String,
    pub protocol_sizes: Option<Vec<Fe01Size>>,
    pub sizing_error: Option<String>,
    pub health_error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Size {
    pub asset_bank: solana_sdk::pubkey::Pubkey,
    pub liability_bank: solana_sdk::pubkey::Pubkey,
    pub withdraw_native: u64,
    pub repay_native: u64,
    pub repay_all: bool,
    pub flat_sol_fee_lamports: u64,
    pub executable_value: String,
}
/// The explicitly selected qualification scope (one funding class), never a trading preference.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Scope {
    pub funding_class: String,
    pub selection_is_execution_qualification: bool,
    pub funding: String,
    pub source_pin: String,
    pub gaps: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01FreezeSummary {
    pub status: String,
    pub reason: Option<String>,
    pub funding_class: Option<String>,
    pub banks: Option<u64>,
    pub class_pins: Option<u64>,
    pub historical: Option<bool>,
    pub drift: Vec<String>,
    pub gaps: Vec<String>,
    pub journal: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Replay {
    pub mode: String,
    pub source: String,
    pub reads: Vec<Fe01ReplayRead>,
    pub total_bytes: u64,
    pub freshness: String,
    pub integrity: String,
}
/// One consumed saved read: its origin (`journal_seq=N` or the legacy file name), hash, size.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01ReplayRead {
    pub origin: String,
    pub sha256: String,
    pub bytes: u64,
}
impl Fe01Report {
    /// The terminal/text report (PERSIST-04: human-readable output is text, never JSON).
    pub fn text(&self) -> String {
        use std::fmt::Write;
        let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
        let mut out = String::new();
        let _ = writeln!(
            out,
            "FE01 status={} reason={} source_parity={} fe01={} fe02={} pilot_ready={} live_enabled={} submission_enabled={}",
            self.status, opt(&self.reason), self.source_parity, self.fe01, self.fe02,
            self.pilot_ready, self.live_enabled, self.submission_enabled
        );
        if let Some(o) = &self.observation {
            let _ = writeln!(
                out,
                "observation scope_closed=true rounds={} slot={} unix_timestamp={} clock={} program_data={} program_data_sha256={} deployment_slot={} accounts={}",
                o.rounds, o.slot, o.unix_timestamp, o.clock, o.program_data, o.program_data_sha256,
                o.deployment_slot, o.accounts.len()
            );
            let _ = writeln!(
                out,
                "source_pin={} source_parity_reason={} transaction_version={}",
                o.source_pin, o.source_parity_reason, o.transaction_version
            );
            for t in &o.targets {
                let _ = writeln!(
                    out,
                    "target account={} health={} liquidatable={} maintenance_health_bits={} receivership_eligibility={} eligibility_reason={} funding={} net_edge_usd_micro={} simulation={} collateral_exit={} source_parity={} sizing_error={} health_error={}",
                    t.account, t.health,
                    t.liquidatable.map_or("-".into(), |v| v.to_string()),
                    t.maintenance_health_bits.map_or("-".into(), |v| v.to_string()),
                    t.receivership_eligibility, opt(&t.eligibility_reason), t.funding,
                    t.net_edge_usd_micro.map_or("-".into(), |v| v.to_string()), t.simulation,
                    t.collateral_exit, t.source_parity, opt(&t.sizing_error), opt(&t.health_error)
                );
                for s in t.protocol_sizes.iter().flatten() {
                    let _ = writeln!(
                        out,
                        "  size asset_bank={} liability_bank={} withdraw_native={} repay_native={} repay_all={} flat_sol_fee_lamports={} executable_value={}",
                        s.asset_bank, s.liability_bank, s.withdraw_native, s.repay_native,
                        s.repay_all, s.flat_sol_fee_lamports, s.executable_value
                    );
                }
            }
        }
        if let Some(s) = &self.scope {
            let _ = writeln!(
                out,
                "scope funding_class={} selection_is_execution_qualification={} funding={} source_pin={}",
                s.funding_class, s.selection_is_execution_qualification, s.funding, s.source_pin
            );
            for gap in &s.gaps {
                let _ = writeln!(out, "gap {gap}");
            }
        }
        if let Some(f) = &self.freeze {
            let _ = writeln!(
                out,
                "freeze status={} reason={} funding_class={} banks={} class_pins={} historical={} drift={} gaps={} journal={}",
                f.status, opt(&f.reason), opt(&f.funding_class),
                f.banks.map_or("-".into(), |v| v.to_string()),
                f.class_pins.map_or("-".into(), |v| v.to_string()),
                f.historical.map_or("-".into(), |v| v.to_string()),
                f.drift.join(","), f.gaps.join(","), opt(&f.journal)
            );
        }
        if let Some(r) = &self.replay {
            let _ = writeln!(
                out,
                "replay mode={} source={} reads={} total_bytes={} freshness={} integrity={}",
                r.mode,
                r.source,
                r.reads.len(),
                r.total_bytes,
                r.freshness,
                r.integrity
            );
        }
        if let Some(n) = self.logical_reads {
            let _ = writeln!(out, "logical_reads={n}");
        }
        if let Some(p) = &self.provider_actual {
            let _ = writeln!(
                out,
                "provider requests={} http_429_count={} bytes={} effective_rps={} quota_qualified={}",
                p.requests, p.http_429_count, p.bytes, p.effective_rps, p.quota_qualified
            );
        }
        if let Some(c) = &self.provider_cooldown {
            let _ = writeln!(
                out,
                "provider_cooldown cause={:?} remaining_ms={} http_429_count={}",
                c.cause,
                c.remaining_ms.map_or("-".into(), |v| v.to_string()),
                c.http_429_count
            );
        }
        out
    }
}

/// FE-01 smoke (kind 704, schema 1): the bounded deployment and target observation of
/// `--fe-mode smoke` (P1A upgrade watch) through the budgeted typed reads — program, then
/// ProgramData and its deployment, then the targets. Never a qualification: no health, sizing,
/// simulation or source parity; LIVE and submission stay off.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fe01Smoke {
    /// `SMOKE_OBSERVED_NOT_QUALIFIED` or `INCOMPLETE` (with `reason`).
    pub status: String,
    pub reason: Option<String>,
    pub program: solana_sdk::pubkey::Pubkey,
    pub program_data: Option<solana_sdk::pubkey::Pubkey>,
    pub program_data_sha256: Option<String>,
    pub deployment_slot: Option<u64>,
    /// `REFERENCE` only for the deployment qualified for the pinned source
    /// (`protocol::deployment_pin`); else `NOT_REFERENCE` or `UNPINNED`.
    pub deployment_pin: Option<String>,
    /// The context slot of every read, in order (never regressing).
    pub context_slots: Vec<u64>,
    pub targets: Vec<Fe01SmokeTarget>,
    pub logical_reads: u64,
    pub provider_actual: Option<darkbot_solana::provider::ProviderSnapshot>,
    pub pilot_ready: bool,
    pub live_enabled: bool,
    pub submission_enabled: bool,
}
impl Payload for Fe01Smoke {
    const KIND: u16 = journal::kind::FE01_SMOKE;
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fe01SmokeTarget {
    pub account: solana_sdk::pubkey::Pubkey,
    /// `OBSERVED_NOT_HEALTH_QUALIFIED`, `MISSING` or `FOREIGN_OWNER`.
    pub status: String,
    pub bytes: Option<u64>,
    pub sha256: Option<String>,
}
impl Fe01Smoke {
    /// The terminal/text report (PERSIST-04: human-readable output is text, never JSON).
    pub fn text(&self) -> String {
        use std::fmt::Write;
        let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
        let mut out = String::new();
        let _ = writeln!(
            out,
            "FE01_SMOKE status={} reason={} program={} program_data={} program_data_sha256={} deployment_slot={} deployment_pin={} context_slots={:?} logical_reads={} pilot_ready={} live_enabled={} submission_enabled={}",
            self.status,
            opt(&self.reason),
            self.program,
            self.program_data.map_or("-".into(), |k| k.to_string()),
            opt(&self.program_data_sha256),
            self.deployment_slot.map_or("-".into(), |s| s.to_string()),
            opt(&self.deployment_pin),
            self.context_slots,
            self.logical_reads,
            self.pilot_ready,
            self.live_enabled,
            self.submission_enabled
        );
        for t in &self.targets {
            let _ = writeln!(
                out,
                "target account={} status={} bytes={} sha256={}",
                t.account,
                t.status,
                t.bytes.map_or("-".into(), |b| b.to_string()),
                opt(&t.sha256)
            );
        }
        out
    }
}

/// The registry: every typed kind and its payload type, listed once.
macro_rules! typed_kinds {
    ($kind:expr, $schema:expr, $bytes:expr, $f:ident) => {{
        use journal::kind as k;
        match $kind {
            k::PROVIDER_EVIDENCE => {
                $f::<crate::orchestration::provider_evidence::ProviderEvidence>(
                    $kind, $schema, $bytes,
                )
            }
            k::BOOT_NETWORK => {
                $f::<crate::orchestration::boot_audit::NetworkAudit>($kind, $schema, $bytes)
            }
            k::BOOT_FLOW => {
                $f::<crate::orchestration::boot_control::BootFlow>($kind, $schema, $bytes)
            }
            k::BOOT_STEP => {
                $f::<crate::orchestration::boot_control::BootStepOutcome>($kind, $schema, $bytes)
            }
            k::FE01_FREEZE => $f::<Fe01Freeze>($kind, $schema, $bytes),
            k::FE01_SMOKE => $f::<Fe01Smoke>($kind, $schema, $bytes),
            k::FE01_CONFIG => $f::<Fe01Config>($kind, $schema, $bytes),
            k::FE01_READ => $f::<Fe01Read>($kind, $schema, $bytes),
            k::FE01_RESULT => $f::<Fe01Report>($kind, $schema, $bytes),
            journal::MANIFEST => $f::<Manifest>($kind, $schema, $bytes),
            k::OWNER_DIAGNOSTIC => $f::<OwnerDiagnostic>($kind, $schema, $bytes),
            k::OWNER_RECORD => $f::<super::evidence::OwnerRecord>($kind, $schema, $bytes),
            k::OWNER_OPPORTUNITY => $f::<super::record::OpportunityRecord>($kind, $schema, $bytes),
            k::OWNER_SIMULATION_FAILURE => {
                $f::<super::record::SimulationFailureRecord>($kind, $schema, $bytes)
            }
            k::ECON_SHADOW_EVALUATION => {
                $f::<super::econ::ShadowEvaluation>($kind, $schema, $bytes)
            }
            k::ECON_CANONICAL => $f::<super::econ::CanonicalJoin>($kind, $schema, $bytes),
            k::MEASUREMENT_PHASE => $f::<super::measurement::PhaseRecord>($kind, $schema, $bytes),
            k::MEASUREMENT_PLAN => {
                $f::<super::measurement::MeasurementPlan>($kind, $schema, $bytes)
            }
            k::OPERATIONAL_CONFIG => {
                $f::<darkbot_solana::runtime_config::EffectiveConfig>($kind, $schema, $bytes)
            }
            k::RUN_MANIFEST => $f::<super::measurement::RunManifest>($kind, $schema, $bytes),
            k::ECON_OPPORTUNITY => $f::<super::econ::OpportunityRow>($kind, $schema, $bytes),
            k::ECON_SNAPSHOT => $f::<super::econ::SnapshotFrame>($kind, $schema, $bytes),
            k::ECON_FINAL => $f::<super::econ::SnapshotReport>($kind, $schema, $bytes),
            k::FE04_TRANSITION => $f::<crate::execution::one_shot::Version>($kind, $schema, $bytes),
            k::FE04_LEGACY_IMPORT => {
                $f::<crate::execution::one_shot::LegacyImport>($kind, $schema, $bytes)
            }
            k::OWNER_RECONCILE => {
                $f::<crate::execution::pending::ReconcileRecord>($kind, $schema, $bytes)
            }
            k::OWNER_SUMMARY => $f::<super::shadow_ledger::DaySummary>($kind, $schema, $bytes),
            k::OWNER_RECOVERY => $f::<super::evidence::OwnerRecovery>($kind, $schema, $bytes),
            k::SOURCE_CONTROL_REQUEST => $f::<SourceControlRequest>($kind, $schema, $bytes),
            k::SOURCE_CONTROL_OUTCOME => $f::<SourceControlOutcome>($kind, $schema, $bytes),
            k::CAPABILITY_CONTROL_REQUEST => $f::<CapabilityControlRequest>($kind, $schema, $bytes),
            k::CAPABILITY_CONTROL_OUTCOME => $f::<CapabilityControlOutcome>($kind, $schema, $bytes),
            k::C0_CENSUS_ROW => $f::<C0CensusAccount>($kind, $schema, $bytes),
            k::C0_CENSUS_ACCOUNT => $f::<C0CensusImportAccount>($kind, $schema, $bytes),
            k::C0_CENSUS_GROUP_ACCOUNT => $f::<C0CensusImportGroupAccount>($kind, $schema, $bytes),
            k::C0_CENSUS_RESULT => $f::<C0CensusResult>($kind, $schema, $bytes),
            k::MARKET_TRANSACTION => $f::<market::MarketTransaction>($kind, $schema, $bytes),
            k::MARKET_LIQUIDATION => $f::<market::MarketLiquidation>($kind, $schema, $bytes),
            k::MARKET_COVERAGE => $f::<market::MarketCoverage>($kind, $schema, $bytes),
            k::MARKET_RELATED => {
                $f::<crate::market::observer::related::MarketRelated>($kind, $schema, $bytes)
            }
            k::MARKET_COMPLETION => $f::<market::MarketCompletion>($kind, $schema, $bytes),
            k::MARKET_SUMMARY => $f::<crate::market::observer::MarketFinal>($kind, $schema, $bytes),
            k::REPORT_DATASET => $f::<super::report::DatasetRecord>($kind, $schema, $bytes),
            k::REPORT_QUERY => $f::<super::report::QueryRecord>($kind, $schema, $bytes),
            k::BACKTEST_HEADER => $f::<super::backtest::BacktestHeader>($kind, $schema, $bytes),
            k::BACKTEST_ROW => $f::<super::backtest::BacktestRow>($kind, $schema, $bytes),
            k::BACKTEST_SUMMARY => $f::<super::backtest::BacktestSummary>($kind, $schema, $bytes),
            k::OPTIMIZE_RESULT => $f::<super::backtest::OptimizeResult>($kind, $schema, $bytes),
            k::BACKTEST_PROFILE => $f::<super::backtest::Profile>($kind, $schema, $bytes),
            k::BACKTEST_ATTEMPT => $f::<super::backtest::Attempt>($kind, $schema, $bytes),
            k::ANALYTICS_JOB_REQUEST => $f::<super::jobs::JobRequest>($kind, $schema, $bytes),
            k::ANALYTICS_JOB_OUTCOME => $f::<super::jobs::JobOutcome>($kind, $schema, $bytes),
            k::C0E1_QUALIFICATION => {
                $f::<super::qualification::QualificationRecord>($kind, $schema, $bytes)
            }
            k::STRICT_SCOPE => $f::<strict::StrictScope>($kind, $schema, $bytes),
            k::STRICT_RPC_CACHE => $f::<strict::RpcCacheEntry>($kind, $schema, $bytes),
            k::STRICT_CHAIN_CACHE => $f::<strict::ChainCacheEntry>($kind, $schema, $bytes),
            k::STRICT_CHECKPOINT => $f::<strict::StrictCheckpoint>($kind, $schema, $bytes),
            k::STRICT_RESULT => $f::<strict::StrictResult>($kind, $schema, $bytes),
            k::STRICT_SPEC => $f::<crate::market::observer::strict_driver::VerificationSpec>(
                $kind, $schema, $bytes,
            ),
            k::CAPTURE_EVIDENCE => $f::<crate::market::observer::strict_driver::CaptureEvidence>(
                $kind, $schema, $bytes,
            ),
            k::STRICT_ATTEMPT => {
                $f::<crate::market::observer::strict_driver::AttemptRecord>($kind, $schema, $bytes)
            }
            k::DEPLOYMENT_PROOF => $f::<crate::market::observer::strict_driver::DeploymentProof>(
                $kind, $schema, $bytes,
            ),
            k::CAPACITY_PREFLIGHT => {
                $f::<crate::storage::preflight::CapacityReport>($kind, $schema, $bytes)
            }
            k::SEGMENT_LINK => $f::<crate::storage::segments::SegmentLink>($kind, $schema, $bytes),
            k::SEGMENT_NEXT => $f::<crate::storage::segments::SegmentNext>($kind, $schema, $bytes),
            k::SEGMENT_RECOVERY => {
                $f::<crate::storage::segments::SegmentRecovery>($kind, $schema, $bytes)
            }
            k::MT0_SNAPSHOT => $f::<super::econ::Mt0Telemetry>($kind, $schema, $bytes),
            _ => Err(bad("PAYLOAD_KIND_UNTYPED")),
        }
    }};
}

/// Payload version registered for a typed kind; `None` for kinds without a payload type.
pub fn registered_schema(kind: u16) -> Option<u16> {
    fn schema<P: Payload>(_: u16, _: u16, _: &[u8]) -> io::Result<u16> {
        Ok(P::SCHEMA)
    }
    typed_kinds!(kind, 0, &[], schema).ok()
}

/// Temporary migration paths: kinds that still write the historical value tree (schema 1).
/// JFI-04 closed the last one (FE-01, C0 census import, 24.10.): no kind writes schema 1, and
/// no kind is ever added. Historical schema-1 frames stay readable.
pub const LEGACY_WRITABLE: &[u16] = &[];

/// Write admission of the one journal contract (Ticket 1): a typed kind only in its registered
/// version, schema 1 only for a registered migration path, everything else fails closed.
pub fn check_write(kind: u16, schema: u16) -> io::Result<()> {
    match (schema, registered_schema(kind)) {
        (1, None) if LEGACY_WRITABLE.contains(&kind) => Ok(()),
        (1, Some(_)) => Err(bad("LEGACY_WRITE_OF_TYPED_KIND")),
        (1, None) => Err(bad("LEGACY_WRITE_UNREGISTERED_KIND")),
        (s, Some(r)) if s == r => Ok(()),
        (_, Some(_)) => Err(bad("PAYLOAD_SCHEMA")),
        (_, None) => Err(bad("PAYLOAD_KIND_UNTYPED")),
    }
}

/// Decodes a typed frame with the type registered for its kind. A typed frame of a kind
/// without a registered type is rejected: every typed kind must be readable.
pub fn validate(kind: u16, schema: u16, bytes: &[u8]) -> io::Result<()> {
    fn check<P: Payload>(kind: u16, schema: u16, b: &[u8]) -> io::Result<()> {
        decode::<P>(kind, schema, b).map(drop)
    }
    typed_kinds!(kind, schema, bytes, check)
}

#[cfg(feature = "historical")]
/// The historical JSON row of a frame for explicit offline views: the value tree of a schema-1
/// frame, or the legacy render of a typed market frame. Other typed kinds have no JSON view.
pub fn legacy_view(frame: &journal::Frame) -> io::Result<serde_json::Value> {
    if frame.schema == 1 {
        return frame.legacy_value();
    }
    if frame.kind == journal::kind::MEASUREMENT_PHASE {
        return decode::<super::measurement::PhaseRecord>(frame.kind, frame.schema, &frame.payload)
            .map(|row| row.legacy_json());
    }
    if frame.kind == journal::kind::ECON_OPPORTUNITY {
        return decode::<super::econ::OpportunityRow>(frame.kind, frame.schema, &frame.payload)
            .map(|row| super::econ::opportunity_view(&row));
    }
    if frame.kind == journal::kind::RUN_MANIFEST {
        return decode::<super::measurement::RunManifest>(frame.kind, frame.schema, &frame.payload)
            .map(|m| m.legacy_json());
    }
    if frame.kind == journal::kind::MARKET_SUMMARY {
        return decode::<crate::market::observer::MarketFinal>(
            frame.kind,
            frame.schema,
            &frame.payload,
        )
        .and_then(|m| m.legacy_json().map_err(io::Error::other));
    }
    if frame.kind == journal::kind::ECON_SNAPSHOT {
        return decode::<super::econ::SnapshotFrame>(frame.kind, frame.schema, &frame.payload)
            .and_then(|f| super::econ::render(&f).map_err(io::Error::other));
    }
    if frame.kind == journal::kind::ECON_FINAL {
        return decode::<super::econ::SnapshotReport>(frame.kind, frame.schema, &frame.payload)
            .and_then(|r| super::econ::render(&r).map_err(io::Error::other));
    }
    if frame.kind == journal::kind::MT0_SNAPSHOT {
        // JFI-04: the historical `mt0.jsonl` row (the econ reports' own MT0 render).
        return decode::<super::econ::Mt0Telemetry>(frame.kind, frame.schema, &frame.payload)
            .and_then(|t| t.legacy_json().map_err(io::Error::other));
    }
    if frame.kind == journal::kind::ECON_CANONICAL {
        return decode::<super::econ::CanonicalJoin>(frame.kind, frame.schema, &frame.payload)
            .map(|row| super::econ::canonical_legacy_view(&row));
    }
    market::legacy_view(frame.kind, frame.schema, &frame.payload)
}

/// Human-readable text of a typed frame (offline inspection only).
pub fn describe(kind: u16, schema: u16, bytes: &[u8]) -> io::Result<String> {
    fn text<P: Payload + std::fmt::Debug>(kind: u16, schema: u16, b: &[u8]) -> io::Result<String> {
        decode::<P>(kind, schema, b).map(|p| format!("{p:?}"))
    }
    typed_kinds!(kind, schema, bytes, text)
}
