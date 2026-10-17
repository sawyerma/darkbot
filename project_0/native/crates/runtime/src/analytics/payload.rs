//! JFI-01: typed binary payloads of the `.p0j` journal.
//!
//! A typed frame carries the native encoding of exactly one Rust payload type per kind: bincode
//! (little-endian, fixed-width integers incl. exact i128/u128, no trailing bytes, bounded by the
//! frame limit). There is no JSON value model in between. The frame `schema` is the payload
//! version of that kind (≥ 2); a reader accepts only the version its type declares and fails
//! closed on any other. Schema 1 (generic value tree) is the historical format: it stays readable
//! through `Frame::legacy_value` for offline readers and not-yet-migrated kinds, and is never
//! written for a kind that has a payload type.
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
}

fn options() -> impl Options {
    bincode::DefaultOptions::new()
        .with_little_endian()
        .with_fixint_encoding()
        .reject_trailing_bytes()
        .with_limit(journal::MAX_FRAME as u64)
}

fn bad(code: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, code)
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
        return Err(bad("PAYLOAD_SCHEMA"));
    }
    options()
        .deserialize(bytes)
        .map_err(|_| bad("PAYLOAD_DECODE"))
}

impl Payload for darkbot_solana::runtime_config::EffectiveConfig {
    const KIND: u16 = journal::kind::OPERATIONAL_CONFIG;
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

/// The registry: every typed kind and its payload type, listed once.
macro_rules! typed_kinds {
    ($kind:expr, $schema:expr, $bytes:expr, $f:ident) => {{
        use journal::kind as k;
        match $kind {
            journal::MANIFEST => $f::<Manifest>($kind, $schema, $bytes),
            k::OWNER_DIAGNOSTIC => $f::<OwnerDiagnostic>($kind, $schema, $bytes),
            k::OWNER_RECORD => $f::<super::evidence::OwnerRecord>($kind, $schema, $bytes),
            k::OWNER_OPPORTUNITY => $f::<super::record::OpportunityRecord>($kind, $schema, $bytes),
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
            k::OWNER_RECONCILE => {
                $f::<crate::execution::pending::ReconcileRecord>($kind, $schema, $bytes)
            }
            k::OWNER_SUMMARY => $f::<super::shadow_ledger::DaySummary>($kind, $schema, $bytes),
            k::OWNER_RECOVERY => $f::<super::evidence::OwnerRecovery>($kind, $schema, $bytes),
            k::C0_CENSUS_ROW => $f::<C0CensusAccount>($kind, $schema, $bytes),
            k::C0_CENSUS_RESULT => $f::<C0CensusResult>($kind, $schema, $bytes),
            k::MARKET_TRANSACTION => $f::<market::MarketTransaction>($kind, $schema, $bytes),
            k::MARKET_LIQUIDATION => $f::<market::MarketLiquidation>($kind, $schema, $bytes),
            k::MARKET_COVERAGE => $f::<market::MarketCoverage>($kind, $schema, $bytes),
            k::MARKET_RELATED => {
                $f::<crate::market::observer::related::MarketRelated>($kind, $schema, $bytes)
            }
            k::STRICT_SCOPE => $f::<strict::StrictScope>($kind, $schema, $bytes),
            k::STRICT_RPC_CACHE => $f::<strict::RpcCacheEntry>($kind, $schema, $bytes),
            k::STRICT_CHECKPOINT => $f::<strict::StrictCheckpoint>($kind, $schema, $bytes),
            k::STRICT_RESULT => $f::<strict::StrictResult>($kind, $schema, $bytes),
            k::CAPACITY_PREFLIGHT => {
                $f::<crate::storage::preflight::CapacityReport>($kind, $schema, $bytes)
            }
            k::SEGMENT_LINK => $f::<crate::storage::segments::SegmentLink>($kind, $schema, $bytes),
            k::SEGMENT_NEXT => $f::<crate::storage::segments::SegmentNext>($kind, $schema, $bytes),
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
/// Each entry is an open JFI item; a kind leaves this list when it gets a payload type, and
/// no kind is ever added to it. Typed kinds and unregistered kinds can never write schema 1.
pub const LEGACY_WRITABLE: &[u16] = {
    use journal::kind as k;
    &[
        // JFI-FIX-06 (FE-04 financial journal)
        k::FE04_TRANSITION,
        k::FE04_LEGACY_IMPORT,
        // JFI-FIX-05 (economics, verifier, reporting)
        k::ECON_SNAPSHOT,
        k::ECON_FINAL,
        // remaining journal kinds (measurement, MT0, FE-01, C0 census import)
        k::MT0_SNAPSHOT,
        k::FE01_CONFIG,
        k::FE01_READ,
        k::FE01_RESULT,
        k::C0_CENSUS_ACCOUNT,
        k::C0_CENSUS_GROUP_ACCOUNT,
    ]
};

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
