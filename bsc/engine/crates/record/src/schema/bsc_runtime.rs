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

    /// The canonical chain follower (the G0.4 denominator): what really
    /// landed, joined with the pending sightings by local monotonic time.
    pub mod canonical {
        use super::*;

        /// `CONNECTING`, `FOLLOWING`, `RATE_LIMITED`, `FAILING` or `STOPPED`.
        pub const STATE: u16 = 1;
        /// The last canonical block; absent before the first.
        pub const HEAD: u16 = 2;
        /// Milliseconds since the follower first had that block's header.
        pub const HEAD_AGE_MS: u16 = 3;
        pub const BLOCKS: u16 = 4;
        pub const REORGS: u16 = 5;
        /// Reorgs deeper than the kept window, and head gaps: resynced, never silent.
        pub const RESYNCS: u16 = 6;
        pub const RPC_ERRORS: u16 = 7;
        pub const RATE_LIMITED: u16 = 8;
        /// Canonical Flap launches, orphaned ones removed.
        pub const FLAP_LAUNCHES: u16 = 9;
        pub const FOUR_LAUNCHES: u16 = 10;
        /// Launches of blocks a reorg orphaned (taken out of the counts above).
        pub const ORPHANED_LAUNCHES: u16 = 11;
        /// Flap launches with a verified full pending sighting before the
        /// block time (header ms; local wall-clock estimate of the sighting).
        pub const SEEN_FULL_BEFORE: u16 = 12;
        /// Flap launches only seen as a hash (not actionable) before the block.
        pub const SEEN_HASH_ONLY_BEFORE: u16 = 13;
        /// Flap launches first seen pending only after the block.
        pub const SEEN_AFTER: u16 = 14;
        /// Flap launches no source delivered.
        pub const MISSED: u16 = 15;
        /// External buys after the launch in the launch block, summed over the
        /// Flap launches whose buys are known (see `buys_unknown`).
        pub const EXTERNAL_BUYS: u16 = 16;
        /// Flap launches whose external buys are unknown (never counted as 0).
        pub const BUYS_UNKNOWN: u16 = 17;
        /// Flap launches whose deciding sighting has no wall-clock estimate.
        pub const CLOCK_UNKNOWN: u16 = 18;
        /// Flap launches of the C1 scope (direct Portal `newTokenV6`), and
        /// those seen as a verified full transaction before the block time.
        pub const C1_SCOPE_LAUNCHES: u16 = 19;
        pub const C1_SCOPE_FULL_BEFORE: u16 = 20;
        /// Canonical blocks not followed (gaps, resyncs).
        pub const BLOCKS_SKIPPED: u16 = 21;
        /// Launches received and waiting for their join.
        pub const WAITING: u16 = 22;
        /// The head's fetch lag: local wall ms at the header fetch minus the
        /// block's ms (negative: local clock behind).
        pub const HEAD_FETCH_LAG_MS: u16 = 23;
        /// Launches (any venue) whose transaction is a type/form the
        /// verifier does not rebuild (EIP-4844/7702, pre-EIP-155).
        pub const TX_UNVERIFIABLE: u16 = 24;
        /// Launches (any venue) whose provider answers were refused every
        /// time they were fetched: unknown, not counted as C1 scope.
        pub const TX_ANSWER_REFUSED: u16 = 25;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.canonical",
            record_type: 0,
            schema_version: 4,
            fields: &[
                Field::req(STATE, "state", Text),
                Field::opt(HEAD, "head", U64),
                Field::opt(HEAD_AGE_MS, "head_age_ms", U64),
                Field::req(BLOCKS, "blocks", U64),
                Field::req(REORGS, "reorgs", U64),
                Field::req(RESYNCS, "resyncs", U64),
                Field::req(RPC_ERRORS, "rpc_errors", U64),
                Field::req(RATE_LIMITED, "rate_limited", U64),
                Field::req(FLAP_LAUNCHES, "flap_launches", U64),
                Field::req(FOUR_LAUNCHES, "four_launches", U64),
                Field::req(ORPHANED_LAUNCHES, "orphaned_launches", U64),
                Field::req(SEEN_FULL_BEFORE, "seen_full_before", U64),
                Field::req(SEEN_HASH_ONLY_BEFORE, "seen_hash_only_before", U64),
                Field::req(SEEN_AFTER, "seen_after", U64),
                Field::req(MISSED, "missed", U64),
                Field::req(EXTERNAL_BUYS, "external_buys", U64),
                Field::req(BUYS_UNKNOWN, "buys_unknown", U64),
                Field::req(CLOCK_UNKNOWN, "clock_unknown", U64),
                Field::req(C1_SCOPE_LAUNCHES, "c1_scope_launches", U64),
                Field::req(C1_SCOPE_FULL_BEFORE, "c1_scope_full_before", U64),
                Field::req(BLOCKS_SKIPPED, "blocks_skipped", U64),
                Field::req(WAITING, "waiting", U64),
                Field::opt(HEAD_FETCH_LAG_MS, "head_fetch_lag_ms", I64),
                Field::req(TX_UNVERIFIABLE, "tx_unverifiable", U64),
                Field::req(TX_ANSWER_REFUSED, "tx_answer_refused", U64),
            ],
        };
    }

    /// The source-modes generation the runtime applies.
    pub const GENERATION: u16 = 1;
    pub const SOURCES: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const STARTED_UNIX_MS: u16 = 5;
    /// Distinct C1-scope Flap launches (direct Portal `newTokenV6`) seen as
    /// verified full pending transactions.
    pub const LAUNCHES: u16 = 6;
    /// The journal of this process, relative to the run directory.
    pub const JOURNAL_PATH: u16 = 7;
    pub const JOURNAL_RECORDS: u16 = 8;
    /// Records the bounded journal queue could not take (never silent).
    pub const JOURNAL_LOST: u16 = 9;
    /// `READ_ONLY`: observation only, no build, signature or send exists.
    pub const MODE: u16 = 10;
    /// Absent when no canonical endpoint is configured (then no denominator).
    pub const CANONICAL: u16 = 11;
    /// How often this record is rewritten; a reader derives staleness from it.
    pub const STATUS_INTERVAL_MS: u16 = 12;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.runtime-status.v1",
        record_type: 40,
        schema_version: 4,
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
            Field::nested(
                CANONICAL,
                "canonical",
                Presence::Optional,
                &canonical::SCHEMA,
            ),
            Field::req(STATUS_INTERVAL_MS, "status_interval_ms", U64),
        ],
    };
}

pub const ALL: &[&Schema] = &[
    &source_modes::SCHEMA,
    &source_mode_receipt::SCHEMA,
    &runtime_status::SCHEMA,
];
