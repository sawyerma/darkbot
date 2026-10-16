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

/// The registry: every typed kind and its payload type, listed once.
macro_rules! typed_kinds {
    ($kind:expr, $schema:expr, $bytes:expr, $f:ident) => {{
        use journal::kind as k;
        match $kind {
            journal::MANIFEST => $f::<Manifest>($kind, $schema, $bytes),
            k::OWNER_DIAGNOSTIC => $f::<OwnerDiagnostic>($kind, $schema, $bytes),
            k::OWNER_RECORD => $f::<super::evidence::OwnerRecord>($kind, $schema, $bytes),
            k::OWNER_OPPORTUNITY => $f::<super::record::OpportunityRecord>($kind, $schema, $bytes),
            k::OWNER_RECONCILE => {
                $f::<crate::execution::pending::ReconcileRecord>($kind, $schema, $bytes)
            }
            k::OWNER_SUMMARY => $f::<super::shadow_ledger::DaySummary>($kind, $schema, $bytes),
            k::OWNER_RECOVERY => $f::<super::evidence::OwnerRecovery>($kind, $schema, $bytes),
            _ => Err(bad("PAYLOAD_KIND_UNTYPED")),
        }
    }};
}

/// Decodes a typed frame with the type registered for its kind. A typed frame of a kind
/// without a registered type is rejected: every typed kind must be readable.
pub fn validate(kind: u16, schema: u16, bytes: &[u8]) -> io::Result<()> {
    fn check<P: Payload>(kind: u16, schema: u16, b: &[u8]) -> io::Result<()> {
        decode::<P>(kind, schema, b).map(drop)
    }
    typed_kinds!(kind, schema, bytes, check)
}

/// Human-readable text of a typed frame (offline inspection only).
pub fn describe(kind: u16, schema: u16, bytes: &[u8]) -> io::Result<String> {
    fn text<P: Payload + std::fmt::Debug>(kind: u16, schema: u16, b: &[u8]) -> io::Result<String> {
        decode::<P>(kind, schema, b).map(|p| format!("{p:?}"))
    }
    typed_kinds!(kind, schema, bytes, text)
}
