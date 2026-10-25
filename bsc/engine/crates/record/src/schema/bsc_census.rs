//! The G0.4/V10-8 market census (plan rule 2, order V2 §8): what the
//! read-only runtime journals prove about foreign Flap launches — canonical
//! count, pending visibility before the block per class, lead times on one
//! monotonic clock, competition in the launch block, outages and the covered
//! window. It states measurements; a verdict exists only against criteria
//! the user confirmed (`criteria`), never before.

use crate::{Field, Kind::*, Presence, Schema};

pub mod census {
    use super::*;

    /// One runtime journal the census read.
    pub mod window {
        use super::*;

        /// The journal, relative to the journal directory.
        pub const JOURNAL: u16 = 1;
        pub const STARTED_UNIX_MS: u16 = 2;
        /// Monotonic span of its records (last record time).
        pub const DURATION_MS: u16 = 3;
        pub const FIRST_BLOCK: u16 = 4;
        pub const LAST_BLOCK: u16 = 5;
        pub const REORGS: u16 = 6;
        pub const RESYNCS: u16 = 7;
        /// Records the bounded queue lost (gap spans): the window is incomplete.
        pub const LOST_RECORDS: u16 = 8;
        /// `COMPLETE` (clean end), `TAIL_INCOMPLETE` (still written or cut: the
        /// verified frames count) or `INVALID` (a corrupt frame: nothing counts).
        pub const STATE: u16 = 9;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.g04-census.window",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(JOURNAL, "journal", Text),
                Field::req(STARTED_UNIX_MS, "started_unix_ms", U64),
                Field::req(DURATION_MS, "duration_ms", U64),
                Field::opt(FIRST_BLOCK, "first_block", U64),
                Field::opt(LAST_BLOCK, "last_block", U64),
                Field::req(REORGS, "reorgs", U64),
                Field::req(RESYNCS, "resyncs", U64),
                Field::req(LOST_RECORDS, "lost_records", U64),
                Field::req(STATE, "state", Text),
            ],
        };
    }

    /// Per source: the actionable launches it delivered first.
    pub mod source {
        use super::*;

        pub const SOURCE_ID: u16 = 1;
        pub const FIRST_FULL_BEFORE: u16 = 2;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.g04-census.source",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SOURCE_ID, "source_id", Text),
                Field::req(FIRST_FULL_BEFORE, "first_full_before", U64),
            ],
        };
    }

    pub const WINDOWS: u16 = 1;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// Sum of the windows' durations.
    pub const COVERED_MS: u16 = 5;
    /// Canonical Flap launches (orphaned ones removed): the denominator.
    pub const FLAP_LAUNCHES: u16 = 6;
    pub const FOUR_LAUNCHES: u16 = 7;
    pub const ORPHANED_LAUNCHES: u16 = 8;
    /// Flap launches whose block arrived while no source was connected.
    pub const OUTAGE_LAUNCHES: u16 = 9;
    pub const FULL_BEFORE: u16 = 10;
    pub const HASH_ONLY_BEFORE: u16 = 11;
    pub const AFTER: u16 = 12;
    pub const MISSED: u16 = 13;
    /// Lead of the first verified full sighting before the block (µs), over
    /// the `FULL_BEFORE` launches; absent without any.
    pub const LEAD_P50_US: u16 = 14;
    pub const LEAD_P95_US: u16 = 15;
    pub const LEAD_P99_US: u16 = 16;
    pub const LEAD_MAX_US: u16 = 17;
    /// Flap launches with at least one external buy after them in their block.
    pub const CONTESTED_LAUNCHES: u16 = 18;
    pub const EXTERNAL_BUYS: u16 = 19;
    pub const SOURCES: u16 = 20;
    /// `CRITERIA_UNCONFIRMED` until the user confirmed versioned criteria.
    pub const CRITERIA: u16 = 21;
    /// Flap launches whose external buys are unknown (malformed buy event):
    /// in neither `contested_launches` nor `external_buys`.
    pub const BUYS_UNKNOWN: u16 = 22;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.g04-census.v1",
        record_type: 41,
        schema_version: 1,
        fields: &[
            Field::nested(WINDOWS, "windows", Presence::Repeated, &window::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(COVERED_MS, "covered_ms", U64),
            Field::req(FLAP_LAUNCHES, "flap_launches", U64),
            Field::req(FOUR_LAUNCHES, "four_launches", U64),
            Field::req(ORPHANED_LAUNCHES, "orphaned_launches", U64),
            Field::req(OUTAGE_LAUNCHES, "outage_launches", U64),
            Field::req(FULL_BEFORE, "full_before", U64),
            Field::req(HASH_ONLY_BEFORE, "hash_only_before", U64),
            Field::req(AFTER, "after", U64),
            Field::req(MISSED, "missed", U64),
            Field::opt(LEAD_P50_US, "lead_p50_us", U64),
            Field::opt(LEAD_P95_US, "lead_p95_us", U64),
            Field::opt(LEAD_P99_US, "lead_p99_us", U64),
            Field::opt(LEAD_MAX_US, "lead_max_us", U64),
            Field::req(CONTESTED_LAUNCHES, "contested_launches", U64),
            Field::req(EXTERNAL_BUYS, "external_buys", U64),
            Field::nested(SOURCES, "sources", Presence::Repeated, &source::SCHEMA),
            Field::req(CRITERIA, "criteria", Text),
            Field::req(BUYS_UNKNOWN, "buys_unknown", U64),
        ],
    };
}

pub const ALL: &[&Schema] = &[&census::SCHEMA];
