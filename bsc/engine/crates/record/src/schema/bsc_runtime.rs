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

/// The running runtime's state, rewritten while it runs: per source its
/// effective mode, derived health and counters (plan §5a.2/§5a.3).
pub mod runtime_status {
    use super::*;

    /// One source of the runtime status.
    pub mod source {
        use super::*;

        pub const SOURCE_ID: u16 = 1;
        /// `OFF`, `SHADOW` or `BLOCKED` (the owner's, as applied by the runtime).
        pub const EFFECTIVE: u16 = 2;
        /// `OFF`, `UNQUALIFIED`, `DISCONNECTED`, `RATE_LIMITED`, `DEGRADED`, `HEALTHY`.
        pub const HEALTH: u16 = 3;
        pub const CONNECTS: u16 = 4;
        pub const DISCONNECTS: u16 = 5;
        pub const RATE_LIMITED: u16 = 6;
        pub const FULL: u16 = 7;
        pub const HASH_ONLY: u16 = 8;
        pub const REJECTED: u16 = 9;
        pub const DROPPED: u16 = 10;
        pub const DUPLICATES: u16 = 11;
        pub const FIRST_ANY: u16 = 12;
        pub const FIRST_FULL: u16 = 13;
        pub const LAUNCH_SIGHTINGS: u16 = 14;
        pub const LAUNCH_FIRST: u16 = 15;
        /// Milliseconds since its last observation; absent before the first.
        pub const LAST_EVENT_AGE_MS: u16 = 16;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.source",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SOURCE_ID, "source_id", Text),
                Field::req(EFFECTIVE, "effective", Text),
                Field::req(HEALTH, "health", Text),
                Field::req(CONNECTS, "connects", U64),
                Field::req(DISCONNECTS, "disconnects", U64),
                Field::req(RATE_LIMITED, "rate_limited", U64),
                Field::req(FULL, "full", U64),
                Field::req(HASH_ONLY, "hash_only", U64),
                Field::req(REJECTED, "rejected", U64),
                Field::req(DROPPED, "dropped", U64),
                Field::req(DUPLICATES, "duplicates", U64),
                Field::req(FIRST_ANY, "first_any", U64),
                Field::req(FIRST_FULL, "first_full", U64),
                Field::req(LAUNCH_SIGHTINGS, "launch_sightings", U64),
                Field::req(LAUNCH_FIRST, "launch_first", U64),
                Field::opt(LAST_EVENT_AGE_MS, "last_event_age_ms", U64),
            ],
        };
    }

    /// The source-modes generation the runtime applies.
    pub const GENERATION: u16 = 1;
    pub const SOURCES: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const STARTED_UNIX_MS: u16 = 5;
    /// Distinct Flap launches seen as verified full transactions.
    pub const LAUNCHES: u16 = 6;
    /// The journal of this process, relative to the run directory.
    pub const JOURNAL_PATH: u16 = 7;
    pub const JOURNAL_RECORDS: u16 = 8;
    /// Records the bounded journal queue could not take (never silent).
    pub const JOURNAL_LOST: u16 = 9;
    /// `READ_ONLY`: observation only, no build, signature or send exists.
    pub const MODE: u16 = 10;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.runtime-status.v1",
        record_type: 40,
        schema_version: 1,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::nested(SOURCES, "sources", Presence::Repeated, &source::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(STARTED_UNIX_MS, "started_unix_ms", U64),
            Field::req(LAUNCHES, "launches", U64),
            Field::req(JOURNAL_PATH, "journal_path", Text),
            Field::req(JOURNAL_RECORDS, "journal_records", U64),
            Field::req(JOURNAL_LOST, "journal_lost", U64),
            Field::req(MODE, "mode", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[
    &source_modes::SCHEMA,
    &source_mode_receipt::SCHEMA,
    &runtime_status::SCHEMA,
];
