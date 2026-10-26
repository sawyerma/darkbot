//! The record schemas of DarkBot BSC, one module per area. Each schema has
//! its own record type; a schema version changes whenever its fields do.
//!
//! Import schemas (`g06a`, parts of `g07`) carry every leaf of one legacy,
//! hash-bound JSON file under its dotted JSON path as the field name, plus
//! the `_import.*` provenance fields. The offline legacy import fills them
//! field by field and proves that nothing was lost or changed.

use crate::{Record, RecordError, Schema, decode, header};

pub mod bsc_census;
pub mod bsc_four_freeze;
pub mod bsc_runtime;
pub mod c1_qualification;
pub mod g06a;
pub mod g07;
pub mod g07_candidate;
pub mod g08;

/// Provenance of an imported legacy file: tags 1 and 2 of every import
/// schema.
pub const IMPORT_LEGACY_SHA256: u16 = 1;
pub const IMPORT_LEGACY_PATH: u16 = 2;
/// Field names with this prefix are provenance, not legacy leaves.
pub const IMPORT_PREFIX: &str = "_import.";
/// Provenance of a native record (no legacy file): tags 3 and 4 of every
/// native DRY schema.
pub const PROVENANCE_TOOL: u16 = 3;
pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = 4;

/// Every schema, for the derived view and the uniqueness check.
pub fn all() -> Vec<&'static Schema> {
    g06a::ALL
        .iter()
        .chain(bsc_census::ALL)
        .chain(bsc_four_freeze::ALL)
        .chain(bsc_runtime::ALL)
        .chain(c1_qualification::ALL)
        .chain(g07::ALL)
        .chain(g07_candidate::ALL)
        .chain(g08::ALL)
        .copied()
        .collect()
}

/// Decodes a record of any known schema.
pub fn decode_any(bytes: &[u8]) -> Result<Record, RecordError> {
    let (record_type, schema_version) = header(bytes)?;
    let schema = all()
        .into_iter()
        .find(|s| s.record_type == record_type && s.schema_version == schema_version)
        .ok_or_else(|| {
            RecordError(format!(
                "no schema for record type {record_type} version {schema_version}"
            ))
        })?;
    decode(schema, bytes)
}
