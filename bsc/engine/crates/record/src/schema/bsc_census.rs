//! The G0.4/V10-8 market census (plan rule 2, order V2 §8): what the
//! read-only runtime journals prove about foreign Flap launches — canonical
//! count, pending visibility before the block time per class, lead times,
//! competition in the launch block, outages, coverage gaps and the covered
//! window. "Before the block" compares the runtime's wall-clock estimate of
//! the first sighting with the header's millisecond block time (two clocks;
//! the header fetch lag bounds the local offset: a negative minimum proves
//! the local clock behind, `clock_suspect`). It states measurements; a
//! verdict exists only against criteria the user confirmed (`criteria`).

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
        /// `COMPLETE` (clean end record), `RUNNING` (the journal the running
        /// runtime still writes), `UNTERMINATED` (killed: no end record),
        /// `TAIL_INCOMPLETE` (cut: the verified frames count) or `INVALID`
        /// (a corrupt frame: nothing counts).
        pub const STATE: u16 = 9;
        /// Canonical blocks not followed (gaps, resyncs).
        pub const BLOCKS_SKIPPED: u16 = 10;
        /// Launches received but never joined (lost at a crash).
        pub const UNJOINED: u16 = 11;
        /// A RUNNING window's launches still inside their join delay.
        pub const PENDING_JOIN: u16 = 12;
        /// Canonical/reorg records that do not parse (older layout, damage).
        pub const UNPARSED_RECORDS: u16 = 13;
        /// Pending events the ingress queue dropped before the journal
        /// (`D` records: the sources' cumulative totals).
        pub const INGRESS_DROPPED: u16 = 14;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.g04-census.window",
            record_type: 0,
            schema_version: 4,
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
                Field::req(BLOCKS_SKIPPED, "blocks_skipped", U64),
                Field::req(UNJOINED, "unjoined", U64),
                Field::req(PENDING_JOIN, "pending_join", U64),
                Field::req(UNPARSED_RECORDS, "unparsed_records", U64),
                Field::req(INGRESS_DROPPED, "ingress_dropped", U64),
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
    /// Lead of the first verified full sighting before the block time (µs), over
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
    /// Flap launches whose deciding sighting has no wall-clock estimate.
    pub const CLOCK_UNKNOWN: u16 = 23;
    /// Flap launches of the C1 scope (direct Portal `newTokenV6`), and those
    /// seen as a verified full transaction before the block time.
    pub const C1_SCOPE_LAUNCHES: u16 = 24;
    pub const C1_SCOPE_FULL_BEFORE: u16 = 25;
    /// Launches received but never joined (lost at a crash).
    pub const UNJOINED: u16 = 26;
    /// Canonical blocks not followed (gaps, resyncs) in all windows.
    pub const BLOCKS_SKIPPED: u16 = 27;
    /// Header fetch lag of the launch blocks (local wall ms minus block ms).
    pub const FETCH_LAG_MS_MIN: u16 = 28;
    pub const FETCH_LAG_MS_P50: u16 = 29;
    pub const FETCH_LAG_MS_MAX: u16 = 30;
    /// A negative fetch lag: the local clock is behind the block time.
    pub const CLOCK_SUSPECT: u16 = 31;
    /// Launches (any venue) whose transaction is a type/form the verifier
    /// does not rebuild (EIP-4844/7702, pre-EIP-155): never actionable.
    pub const TX_UNVERIFIABLE: u16 = 32;
    /// Canonical heights no process window followed (between two windows).
    pub const BLOCKS_BETWEEN_WINDOWS: u16 = 33;
    /// Canonical/reorg records that do not parse, in all windows.
    pub const UNPARSED_RECORDS: u16 = 34;
    /// Launches (any venue) whose provider answers were refused every time
    /// they were fetched (malformed, a hash or signer mismatch): unknown.
    pub const TX_ANSWER_REFUSED: u16 = 35;
    /// Four.meme launches by the decoder's pre-execution verdict (Phase E,
    /// Phase F contract §10 "actionable pre-execution address"): the token
    /// derived before execution is the canonical one; no proven
    /// derivation; derived but another token; not a decoded launch call;
    /// a record of an older build without the verdict.
    pub const FOUR_DERIVED: u16 = 36;
    pub const FOUR_NO_DERIVATION: u16 = 37;
    pub const FOUR_MISMATCH: u16 = 38;
    pub const FOUR_NOT_RECOGNIZED: u16 = 39;
    pub const FOUR_VERDICT_UNRECORDED: u16 = 40;
    /// Four.meme launches by pending coverage (Phase F §10, per venue).
    pub const FOUR_FULL_BEFORE: u16 = 41;
    pub const FOUR_HASH_ONLY_BEFORE: u16 = 42;
    pub const FOUR_AFTER: u16 = 43;
    pub const FOUR_MISSED: u16 = 44;
    pub const FOUR_CLOCK_UNKNOWN: u16 = 45;
    /// Later sightings of a launch by another source and their lag behind
    /// the first (Phase F §7: the first-seen spread across sources).
    pub const LATER_SIGHTINGS: u16 = 46;
    pub const LATER_LAG_P50_US: u16 = 47;
    pub const LATER_LAG_P95_US: u16 = 48;
    pub const LATER_LAG_MAX_US: u16 = 49;
    /// Pending events the ingress queue dropped before the journal, in all
    /// windows (Phase F gate H1: never a clean capture).
    pub const INGRESS_DROPPED: u16 = 50;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.g04-census.v1",
        record_type: 41,
        schema_version: 7,
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
            Field::req(CLOCK_UNKNOWN, "clock_unknown", U64),
            Field::req(C1_SCOPE_LAUNCHES, "c1_scope_launches", U64),
            Field::req(C1_SCOPE_FULL_BEFORE, "c1_scope_full_before", U64),
            Field::req(UNJOINED, "unjoined", U64),
            Field::req(BLOCKS_SKIPPED, "blocks_skipped", U64),
            Field::opt(FETCH_LAG_MS_MIN, "fetch_lag_ms_min", I64),
            Field::opt(FETCH_LAG_MS_P50, "fetch_lag_ms_p50", I64),
            Field::opt(FETCH_LAG_MS_MAX, "fetch_lag_ms_max", I64),
            Field::req(CLOCK_SUSPECT, "clock_suspect", Bool),
            Field::req(TX_UNVERIFIABLE, "tx_unverifiable", U64),
            Field::req(BLOCKS_BETWEEN_WINDOWS, "blocks_between_windows", U64),
            Field::req(UNPARSED_RECORDS, "unparsed_records", U64),
            Field::req(TX_ANSWER_REFUSED, "tx_answer_refused", U64),
            Field::req(FOUR_DERIVED, "four_derived", U64),
            Field::req(FOUR_NO_DERIVATION, "four_no_derivation", U64),
            Field::req(FOUR_MISMATCH, "four_mismatch", U64),
            Field::req(FOUR_NOT_RECOGNIZED, "four_not_recognized", U64),
            Field::req(FOUR_VERDICT_UNRECORDED, "four_verdict_unrecorded", U64),
            Field::req(FOUR_FULL_BEFORE, "four_full_before", U64),
            Field::req(FOUR_HASH_ONLY_BEFORE, "four_hash_only_before", U64),
            Field::req(FOUR_AFTER, "four_after", U64),
            Field::req(FOUR_MISSED, "four_missed", U64),
            Field::req(FOUR_CLOCK_UNKNOWN, "four_clock_unknown", U64),
            Field::req(LATER_SIGHTINGS, "later_sightings", U64),
            Field::opt(LATER_LAG_P50_US, "later_lag_p50_us", U64),
            Field::opt(LATER_LAG_P95_US, "later_lag_p95_us", U64),
            Field::opt(LATER_LAG_MAX_US, "later_lag_max_us", U64),
            Field::req(INGRESS_DROPPED, "ingress_dropped", U64),
        ],
    };
}

pub const ALL: &[&Schema] = &[&census::SCHEMA];
