//! The native configuration of the BSC runtime (C0/C1, plan §5a.2, order V2
//! §5): the five signal sources, each with the mode the user requested
//! (`OFF | SHADOW | LIVE`) and the mode the native owner made effective
//! (`OFF | SHADOW | BLOCKED`, with its reason). One record per generation;
//! every request leaves one receipt.

use crate::{Field, Kind::*, Presence, Schema};

/// One generation of the source modes.
pub mod source_modes {
    use super::*;

    /// One source of a generation.
    pub mod entry {
        use super::*;

        /// `BSC_INGRESS_GETH`, `_PUBLICNODE`, `_DRPC`, `_BLOCKREQ`, `_48CLUB`.
        pub const SOURCE_ID: u16 = 1;
        /// What the user requested: `OFF`, `SHADOW` or `LIVE`.
        pub const REQUESTED: u16 = 2;
        /// What the native owner made of it: `OFF`, `SHADOW` or `BLOCKED`.
        pub const EFFECTIVE: u16 = 3;
        /// Why a request is not effective as requested.
        pub const BLOCK_REASON: u16 = 4;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.source-mode",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SOURCE_ID, "source_id", Text),
                Field::req(REQUESTED, "requested", Text),
                Field::req(EFFECTIVE, "effective", Text),
                Field::opt(BLOCK_REASON, "block_reason", Text),
            ],
        };
    }

    /// Strictly increasing; the CAS every change states.
    pub const GENERATION: u16 = 1;
    pub const SOURCES: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// The request this generation applied (none for the initial one).
    pub const REQUEST_ID: u16 = 5;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.source-modes.v1",
        record_type: 38,
        schema_version: 1,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::nested(SOURCES, "sources", Presence::Repeated, &entry::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(REQUEST_ID, "request_id", Text),
        ],
    };
}

/// The receipt of one source-mode request.
pub mod source_mode_receipt {
    use super::*;

    pub const REQUEST_ID: u16 = 1;
    pub const SOURCE_ID: u16 = 2;
    pub const REQUESTED: u16 = 3;
    pub const EXPECTED_GENERATION: u16 = 4;
    /// `APPLIED` or `REJECTED_STALE`.
    pub const OUTCOME: u16 = 5;
    /// The generation now current (the new one when applied).
    pub const GENERATION: u16 = 6;
    pub const EFFECTIVE: u16 = 7;
    pub const BLOCK_REASON: u16 = 8;
    pub const OBSERVED_UNIX_MS: u16 = 9;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.source-mode-receipt.v1",
        record_type: 39,
        schema_version: 1,
        fields: &[
            Field::req(REQUEST_ID, "request_id", Text),
            Field::req(SOURCE_ID, "source_id", Text),
            Field::req(REQUESTED, "requested", Text),
            Field::req(EXPECTED_GENERATION, "expected_generation", U64),
            Field::req(OUTCOME, "outcome", Text),
            Field::req(GENERATION, "generation", U64),
            Field::req(EFFECTIVE, "effective", Text),
            Field::opt(BLOCK_REASON, "block_reason", Text),
            Field::req(OBSERVED_UNIX_MS, "observed_unix_ms", U64),
        ],
    };
}

pub const ALL: &[&Schema] = &[&source_modes::SCHEMA, &source_mode_receipt::SCHEMA];
