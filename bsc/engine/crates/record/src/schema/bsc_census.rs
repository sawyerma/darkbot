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
        /// BSC3-064: the capture window's time with a source connected over
        /// an unbroken canonical chain (0: continuity not shown); absent in
        /// windows written before.
        pub const OBSERVABLE_MS: u16 = 15;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.g04-census.window",
            record_type: 0,
            schema_version: 5,
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
                Field::opt(OBSERVABLE_MS, "observable_ms", U64),
            ],
        };

        /// v4, before the observable time: the window of census v8 to v10.
        pub const V4: Schema = Schema {
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
    /// Phase F §10 per venue (gate M3/M4): Four Classic and OpenFour
    /// launches; actionable before execution (seen in full before the block
    /// with the token derived from the launch bytes) in all and per family;
    /// Four launches during a source outage; the lead of Four launches seen
    /// in full before the block; launch calls included and reverted per
    /// venue; journaled launch sightings never included by their window's
    /// end.
    pub const CLASSIC_LAUNCHES: u16 = 51;
    pub const OPENFOUR_LAUNCHES: u16 = 52;
    pub const FOUR_ACTIONABLE: u16 = 53;
    pub const CLASSIC_ACTIONABLE: u16 = 54;
    pub const OPENFOUR_ACTIONABLE: u16 = 55;
    pub const FOUR_OUTAGE_LAUNCHES: u16 = 56;
    pub const FOUR_LEAD_P50_US: u16 = 57;
    pub const FOUR_LEAD_MAX_US: u16 = 58;
    pub const REVERTED_FLAP: u16 = 59;
    pub const REVERTED_CLASSIC: u16 = 60;
    pub const REVERTED_OPENFOUR: u16 = 61;
    pub const LAUNCHES_NOT_INCLUDED: u16 = 62;
    /// v9 (Paket 8 review M1): Four Classic launches whose token is one of
    /// the frozen pairs' candidates (not derivable before execution).
    pub const FOUR_CANDIDATE: u16 = 63;

    /// v10 (Gate-0 G0.4 placement): Flap launches whose first external BUY
    /// in the launch block sits at the launch index + 1 (the earliest valid
    /// position after the launch, taken by a foreign BUY) or later in the
    /// block; of the first, those seen in full before the block; launches
    /// whose placement is unknown (token unknown, or the block's trades
    /// incomplete).
    pub const ADJACENT_BUY_LAUNCHES: u16 = 64;
    pub const LATER_BUY_LAUNCHES: u16 = 65;
    pub const ADJACENT_BUY_FULL_BEFORE: u16 = 66;
    pub const PLACEMENT_UNKNOWN: u16 = 67;
    /// v11 (BSC3-064): the windows' observable time, summed (`covered_ms`
    /// stays the journals' run time).
    pub const OBSERVABLE_MS: u16 = 68;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.g04-census.v1",
        record_type: 41,
        schema_version: 11,
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
            Field::req(CLASSIC_LAUNCHES, "classic_launches", U64),
            Field::req(OPENFOUR_LAUNCHES, "openfour_launches", U64),
            Field::req(FOUR_ACTIONABLE, "four_actionable", U64),
            Field::req(CLASSIC_ACTIONABLE, "classic_actionable", U64),
            Field::req(OPENFOUR_ACTIONABLE, "openfour_actionable", U64),
            Field::req(FOUR_OUTAGE_LAUNCHES, "four_outage_launches", U64),
            Field::opt(FOUR_LEAD_P50_US, "four_lead_p50_us", U64),
            Field::opt(FOUR_LEAD_MAX_US, "four_lead_max_us", U64),
            Field::req(REVERTED_FLAP, "reverted_flap", U64),
            Field::req(REVERTED_CLASSIC, "reverted_classic", U64),
            Field::req(REVERTED_OPENFOUR, "reverted_openfour", U64),
            Field::req(LAUNCHES_NOT_INCLUDED, "launches_not_included", U64),
            Field::req(FOUR_CANDIDATE, "four_candidate", U64),
            Field::req(ADJACENT_BUY_LAUNCHES, "adjacent_buy_launches", U64),
            Field::req(LATER_BUY_LAUNCHES, "later_buy_launches", U64),
            Field::req(ADJACENT_BUY_FULL_BEFORE, "adjacent_buy_full_before", U64),
            Field::req(PLACEMENT_UNKNOWN, "placement_unknown", U64),
            Field::req(OBSERVABLE_MS, "observable_ms", U64),
        ],
    };

    /// v10 (Gate-0 G0.4 placement), kept decodable: before the observable
    /// time.
    pub const V10: Schema = Schema {
        name: "darkbot.bsc.g04-census.v1",
        record_type: 41,
        schema_version: 10,
        fields: &[
            Field::nested(WINDOWS, "windows", Presence::Repeated, &window::V4),
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
            Field::req(CLASSIC_LAUNCHES, "classic_launches", U64),
            Field::req(OPENFOUR_LAUNCHES, "openfour_launches", U64),
            Field::req(FOUR_ACTIONABLE, "four_actionable", U64),
            Field::req(CLASSIC_ACTIONABLE, "classic_actionable", U64),
            Field::req(OPENFOUR_ACTIONABLE, "openfour_actionable", U64),
            Field::req(FOUR_OUTAGE_LAUNCHES, "four_outage_launches", U64),
            Field::opt(FOUR_LEAD_P50_US, "four_lead_p50_us", U64),
            Field::opt(FOUR_LEAD_MAX_US, "four_lead_max_us", U64),
            Field::req(REVERTED_FLAP, "reverted_flap", U64),
            Field::req(REVERTED_CLASSIC, "reverted_classic", U64),
            Field::req(REVERTED_OPENFOUR, "reverted_openfour", U64),
            Field::req(LAUNCHES_NOT_INCLUDED, "launches_not_included", U64),
            Field::req(FOUR_CANDIDATE, "four_candidate", U64),
            Field::req(ADJACENT_BUY_LAUNCHES, "adjacent_buy_launches", U64),
            Field::req(LATER_BUY_LAUNCHES, "later_buy_launches", U64),
            Field::req(ADJACENT_BUY_FULL_BEFORE, "adjacent_buy_full_before", U64),
            Field::req(PLACEMENT_UNKNOWN, "placement_unknown", U64),
        ],
    };

    /// v9 (Paket 8 review M1), kept decodable: what the G0.4 campaign of
    /// 2026-10-27 writes.
    pub const V9: Schema = Schema {
        name: "darkbot.bsc.g04-census.v1",
        record_type: 41,
        schema_version: 9,
        fields: &[
            Field::nested(WINDOWS, "windows", Presence::Repeated, &window::V4),
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
            Field::req(CLASSIC_LAUNCHES, "classic_launches", U64),
            Field::req(OPENFOUR_LAUNCHES, "openfour_launches", U64),
            Field::req(FOUR_ACTIONABLE, "four_actionable", U64),
            Field::req(CLASSIC_ACTIONABLE, "classic_actionable", U64),
            Field::req(OPENFOUR_ACTIONABLE, "openfour_actionable", U64),
            Field::req(FOUR_OUTAGE_LAUNCHES, "four_outage_launches", U64),
            Field::opt(FOUR_LEAD_P50_US, "four_lead_p50_us", U64),
            Field::opt(FOUR_LEAD_MAX_US, "four_lead_max_us", U64),
            Field::req(REVERTED_FLAP, "reverted_flap", U64),
            Field::req(REVERTED_CLASSIC, "reverted_classic", U64),
            Field::req(REVERTED_OPENFOUR, "reverted_openfour", U64),
            Field::req(LAUNCHES_NOT_INCLUDED, "launches_not_included", U64),
            Field::req(FOUR_CANDIDATE, "four_candidate", U64),
        ],
    };

    /// v8, before Four Classic candidates were counted apart.
    pub const V8: Schema = Schema {
        name: "darkbot.bsc.g04-census.v1",
        record_type: 41,
        schema_version: 8,
        fields: &[
            Field::nested(WINDOWS, "windows", Presence::Repeated, &window::V4),
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
            Field::req(CLASSIC_LAUNCHES, "classic_launches", U64),
            Field::req(OPENFOUR_LAUNCHES, "openfour_launches", U64),
            Field::req(FOUR_ACTIONABLE, "four_actionable", U64),
            Field::req(CLASSIC_ACTIONABLE, "classic_actionable", U64),
            Field::req(OPENFOUR_ACTIONABLE, "openfour_actionable", U64),
            Field::req(FOUR_OUTAGE_LAUNCHES, "four_outage_launches", U64),
            Field::opt(FOUR_LEAD_P50_US, "four_lead_p50_us", U64),
            Field::opt(FOUR_LEAD_MAX_US, "four_lead_max_us", U64),
            Field::req(REVERTED_FLAP, "reverted_flap", U64),
            Field::req(REVERTED_CLASSIC, "reverted_classic", U64),
            Field::req(REVERTED_OPENFOUR, "reverted_openfour", U64),
            Field::req(LAUNCHES_NOT_INCLUDED, "launches_not_included", U64),
        ],
    };

    /// A census of any kept version (v10, v9 of the 2026-10-27 campaign, v8).
    pub fn decode_census(bytes: &[u8]) -> Result<crate::Record, crate::RecordError> {
        crate::decode(&SCHEMA, bytes)
            .or_else(|_| crate::decode(&V10, bytes))
            .or_else(|_| crate::decode(&V9, bytes))
            .or_else(|_| crate::decode(&V8, bytes))
    }
}

/// The G0.4 pre-commitment (BSC3-033): the SHA-256 of the confirmed
/// criteria file, the decision it binds and who decided, written once by
/// `bsc_runtime g04-precommit` into the campaign's run directory; its
/// observed time is the commitment time. A verdict rates only criteria
/// whose hash was committed before the campaign's first journal started.
/// Who may decide (the identity's authenticity) is BSC3-001.
pub mod precommit {
    use super::*;

    pub const CRITERIA_SHA256: u16 = 1;
    /// The decision bound (`G0.4/V10-8`).
    pub const SCOPE: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const DECIDED_BY: u16 = 5;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.g04-precommit.v1",
        record_type: 69,
        schema_version: 1,
        fields: &[
            Field::req(CRITERIA_SHA256, "criteria_sha256", B256),
            Field::req(SCOPE, "scope", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(DECIDED_BY, "decided_by", Text),
        ],
    };
}

/// The G0.4 verdict (order V2 §8): the census replayed by `bsc_runtime
/// g04-verdict` against the criteria file (`bsc/config/g04_criteria.env`).
/// Criteria the user has not confirmed give `CRITERIA_UNCONFIRMED` and no
/// venue rows: no evaluation before the confirmation. Since v2 it names the
/// pre-commitment it was rated against (record hash, decider, time).
pub mod verdict {
    use super::*;

    /// One venue's denominator, actionable share, lead and verdict.
    pub mod venue {
        use super::*;

        pub const VENUE: u16 = 1;
        pub const CONNECTED: u16 = 2;
        pub const SCOPE: u16 = 3;
        pub const ACTIONABLE: u16 = 4;
        pub const SHARE_PPM: u16 = 5;
        pub const LEAD_P50_US: u16 = 6;
        pub const MIN_LAUNCHES: u16 = 7;
        pub const VERDICT: u16 = 8;
        pub const REASONS: u16 = 9;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.g04-verdict.venue",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(VENUE, "venue", Text),
                Field::req(CONNECTED, "connected", U64),
                Field::req(SCOPE, "scope", U64),
                Field::req(ACTIONABLE, "actionable", U64),
                Field::opt(SHARE_PPM, "share_ppm", U64),
                Field::opt(LEAD_P50_US, "lead_p50_us", U64),
                Field::req(MIN_LAUNCHES, "min_launches", U64),
                Field::req(VERDICT, "verdict", Text),
                Field::rep(REASONS, "reasons", Text),
            ],
        };
    }

    pub const CRITERIA_SHA256: u16 = 1;
    pub const CRITERIA_STATE: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// `CRITERIA_UNCONFIRMED`, `NOT_EVALUABLE`, `CONTINUE`, `ADAPT` or `STOP`.
    pub const VERDICT: u16 = 5;
    pub const REASONS: u16 = 6;
    pub const CAMPAIGN_MS: u16 = 7;
    pub const COVERED_MS: u16 = 8;
    pub const COVERED_PPM: u16 = 9;
    pub const VENUES: u16 = 10;
    /// The pre-commitment read (absent without one): SHA-256 of its record
    /// bytes, its decider and its commitment time.
    pub const PRECOMMIT_SHA256: u16 = 11;
    pub const PRECOMMIT_DECIDED_BY: u16 = 12;
    pub const PRECOMMIT_UNIX_MS: u16 = 13;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.g04-verdict.v1",
        record_type: 63,
        schema_version: 2,
        fields: &[
            Field::req(CRITERIA_SHA256, "criteria_sha256", B256),
            Field::req(CRITERIA_STATE, "criteria_state", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(VERDICT, "verdict", Text),
            Field::rep(REASONS, "reasons", Text),
            Field::req(CAMPAIGN_MS, "campaign_ms", U64),
            Field::req(COVERED_MS, "covered_ms", U64),
            Field::opt(COVERED_PPM, "covered_ppm", U64),
            Field::nested(VENUES, "venues", Presence::Repeated, &venue::SCHEMA),
            Field::opt(PRECOMMIT_SHA256, "precommit_sha256", B256),
            Field::opt(PRECOMMIT_DECIDED_BY, "precommit_decided_by", Text),
            Field::opt(PRECOMMIT_UNIX_MS, "precommit_unix_ms", U64),
        ],
    };

    /// v1 (no pre-commitment), kept readable.
    pub const V1: Schema = Schema {
        name: "darkbot.bsc.g04-verdict.v1",
        record_type: 63,
        schema_version: 1,
        fields: &[
            Field::req(CRITERIA_SHA256, "criteria_sha256", B256),
            Field::req(CRITERIA_STATE, "criteria_state", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(VERDICT, "verdict", Text),
            Field::rep(REASONS, "reasons", Text),
            Field::req(CAMPAIGN_MS, "campaign_ms", U64),
            Field::req(COVERED_MS, "covered_ms", U64),
            Field::opt(COVERED_PPM, "covered_ppm", U64),
            Field::nested(VENUES, "venues", Presence::Repeated, &venue::SCHEMA),
        ],
    };

    /// A verdict of any kept version (v2, v1).
    pub fn decode_verdict(bytes: &[u8]) -> Result<crate::Record, crate::RecordError> {
        crate::decode(&SCHEMA, bytes).or_else(|_| crate::decode(&V1, bytes))
    }
}

pub const ALL: &[&Schema] = &[
    &census::SCHEMA,
    &census::V10,
    &census::V9,
    &census::V8,
    &precommit::SCHEMA,
    &verdict::SCHEMA,
    &verdict::V1,
];
