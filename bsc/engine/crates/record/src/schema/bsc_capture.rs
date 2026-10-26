//! The binary run manifest of one segmented capture (master plan §11,
//! storage contract; Phase F contract §4): run identity, the bounds it ran
//! under, every sealed segment with its sequence range and exact SHA-256,
//! losses, memory and write health, and whether the run is COMPLETE. A
//! control record next to the `DBJNL001` segments, never a second journal.

use crate::{Field, Kind::*, Presence, Schema};

pub mod capture_run {
    use super::*;

    /// One sealed segment.
    pub mod segment {
        use super::*;

        pub const INDEX: u16 = 1;
        /// File name inside the run directory.
        pub const FILE: u16 = 2;
        pub const RECORDS: u16 = 3;
        pub const FIRST_SEQUENCE: u16 = 4;
        pub const LAST_SEQUENCE: u16 = 5;
        pub const LOST_RECORDS: u16 = 6;
        pub const BYTES: u16 = 7;
        pub const SHA256: u16 = 8;
        pub const FIRST_TIMESTAMP_NS: u16 = 9;
        pub const LAST_TIMESTAMP_NS: u16 = 10;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.capture-run.segment",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(INDEX, "index", U64),
                Field::req(FILE, "file", Text),
                Field::req(RECORDS, "records", U64),
                Field::opt(FIRST_SEQUENCE, "first_sequence", U64),
                Field::opt(LAST_SEQUENCE, "last_sequence", U64),
                Field::req(LOST_RECORDS, "lost_records", U64),
                Field::req(BYTES, "bytes", U64),
                Field::req(SHA256, "sha256", B256),
                Field::opt(FIRST_TIMESTAMP_NS, "first_timestamp_ns", U64),
                Field::opt(LAST_TIMESTAMP_NS, "last_timestamp_ns", U64),
            ],
        };
    }

    pub const RUN_ID: u16 = 1;
    pub const CHAIN_ID: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const STARTED_UNIX_MS: u16 = 5;
    pub const ENDED_UNIX_MS: u16 = 6;
    /// The clock of every record timestamp.
    pub const CLOCK_ID: u16 = 7;
    pub const GIT_HEAD: u16 = 8;
    /// The bounds the run ran under.
    pub const BUDGET_BYTES: u16 = 9;
    pub const QUEUE_RECORDS: u16 = 10;
    pub const SEGMENT_BYTES: u16 = 11;
    pub const SEGMENT_NS: u16 = 12;
    pub const SYNC_RECORDS: u16 = 13;
    pub const SYNC_MS: u16 = 14;
    pub const SEGMENTS: u16 = 15;
    /// Totals over the segments.
    pub const RECORDS: u16 = 16;
    pub const LOST_RECORDS: u16 = 17;
    pub const LAST_SEQUENCE: u16 = 18;
    /// The capture memory: its peak and the records the budget refused.
    pub const PEAK_BYTES: u16 = 19;
    pub const BUDGET_REFUSED: u16 = 20;
    pub const WRITE_ERRORS: u16 = 21;
    /// `COMPLETE` (clean end, every segment sealed and continuous, nothing
    /// lost) or `INCOMPLETE`, with the reasons.
    pub const STATE: u16 = 22;
    pub const REASONS: u16 = 23;
    pub const NON_CLAIMS: u16 = 24;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.capture-run.v1",
        record_type: 55,
        schema_version: 1,
        fields: &[
            Field::req(RUN_ID, "run_id", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(STARTED_UNIX_MS, "started_unix_ms", U64),
            Field::opt(ENDED_UNIX_MS, "ended_unix_ms", U64),
            Field::req(CLOCK_ID, "clock_id", U64),
            Field::opt(GIT_HEAD, "git_head", Text),
            Field::req(BUDGET_BYTES, "budget_bytes", U64),
            Field::req(QUEUE_RECORDS, "queue_records", U64),
            Field::req(SEGMENT_BYTES, "segment_bytes", U64),
            Field::req(SEGMENT_NS, "segment_ns", U64),
            Field::req(SYNC_RECORDS, "sync_records", U64),
            Field::req(SYNC_MS, "sync_ms", U64),
            Field::nested(SEGMENTS, "segments", Presence::Repeated, &segment::SCHEMA),
            Field::req(RECORDS, "records", U64),
            Field::req(LOST_RECORDS, "lost_records", U64),
            Field::opt(LAST_SEQUENCE, "last_sequence", U64),
            Field::req(PEAK_BYTES, "peak_bytes", U64),
            Field::req(BUDGET_REFUSED, "budget_refused", U64),
            Field::req(WRITE_ERRORS, "write_errors", U64),
            Field::req(STATE, "state", Text),
            Field::rep(REASONS, "reasons", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

/// The capture→replay identity proof of one finished run (Phase F
/// contract §9): the run read again through the same Rust API against its
/// manifest, every journaled launch decoded again by the same promoted
/// decoders, the census replayed twice, and the runtime's own launch
/// counter compared with the replay's.
pub mod replay_proof {
    use super::*;

    pub const RUN_ID: u16 = 1;
    pub const CHAIN_ID: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// The run manifest: its file hash and state.
    pub const MANIFEST_SHA256: u16 = 5;
    pub const MANIFEST_STATE: u16 = 6;
    /// What the replay read: segments, frames, the last sequence, whether
    /// every sealed segment equals the manifest, and why reading stopped.
    pub const SEGMENTS: u16 = 7;
    pub const RECORDS: u16 = 8;
    pub const LAST_SEQUENCE: u16 = 9;
    pub const MANIFEST_MATCH: u16 = 10;
    pub const READ_ERROR: u16 = 11;
    /// Journaled first sightings decoded again, by venue, and those the
    /// decoders no longer recognize as a launch.
    pub const FLAP_SIGHTINGS: u16 = 12;
    pub const FOUR_SIGHTINGS: u16 = 13;
    pub const OPENFOUR_SIGHTINGS: u16 = 14;
    pub const UNDECODABLE_SIGHTINGS: u16 = 15;
    /// The run's census window state; two replays gave the same census.
    pub const CENSUS_STATE: u16 = 16;
    pub const CENSUS_DETERMINISTIC: u16 = 17;
    /// The runtime's own Flap launch counter of this run (its last status),
    /// when the status is of this run, and whether it equals the replay's.
    pub const STATUS_LAUNCHES: u16 = 18;
    pub const STATUS_MATCH: u16 = 19;
    /// `PASS`, `PARTIAL` (no status of this run to compare) or `FAIL`.
    pub const VERDICT: u16 = 20;
    pub const REASONS: u16 = 21;
    pub const NON_CLAIMS: u16 = 22;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.replay-proof.v1",
        record_type: 56,
        schema_version: 1,
        fields: &[
            Field::req(RUN_ID, "run_id", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(MANIFEST_SHA256, "manifest_sha256", B256),
            Field::opt(MANIFEST_STATE, "manifest_state", Text),
            Field::req(SEGMENTS, "segments", U64),
            Field::req(RECORDS, "records", U64),
            Field::opt(LAST_SEQUENCE, "last_sequence", U64),
            Field::req(MANIFEST_MATCH, "manifest_match", Bool),
            Field::opt(READ_ERROR, "read_error", Text),
            Field::req(FLAP_SIGHTINGS, "flap_sightings", U64),
            Field::req(FOUR_SIGHTINGS, "four_sightings", U64),
            Field::req(OPENFOUR_SIGHTINGS, "openfour_sightings", U64),
            Field::req(UNDECODABLE_SIGHTINGS, "undecodable_sightings", U64),
            Field::req(CENSUS_STATE, "census_state", Text),
            Field::req(CENSUS_DETERMINISTIC, "census_deterministic", Bool),
            Field::opt(STATUS_LAUNCHES, "status_launches", U64),
            Field::opt(STATUS_MATCH, "status_match", Bool),
            Field::req(VERDICT, "verdict", Text),
            Field::rep(REASONS, "reasons", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[&capture_run::SCHEMA, &replay_proof::SCHEMA];

#[cfg(test)]
mod tests {
    use super::capture_run as c;
    use crate::{Draft, Value, decode};

    #[test]
    fn a_run_manifest_round_trips() {
        let mut seg = Draft::new(&c::segment::SCHEMA);
        seg.set(c::segment::INDEX, 0u64)
            .set(c::segment::FILE, "segment_000000.dbjnl")
            .set(c::segment::RECORDS, 2u64)
            .set(c::segment::LOST_RECORDS, 0u64)
            .set(c::segment::BYTES, 80u64)
            .set(c::segment::SHA256, Value::B256([1; 32]));
        let mut d = Draft::new(&c::SCHEMA);
        d.set(c::RUN_ID, "1793000000000")
            .set(c::CHAIN_ID, 56u64)
            .set(c::PROVENANCE_TOOL, "bsc_runtime")
            .set(c::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(c::STARTED_UNIX_MS, 1u64)
            .set(c::CLOCK_ID, 1u64)
            .set(c::BUDGET_BYTES, 1u64)
            .set(c::QUEUE_RECORDS, 1u64)
            .set(c::SEGMENT_BYTES, 1u64)
            .set(c::SEGMENT_NS, 1u64)
            .set(c::SYNC_RECORDS, 1u64)
            .set(c::SYNC_MS, 1u64)
            .push(c::SEGMENTS, seg.into_nested())
            .set(c::RECORDS, 2u64)
            .set(c::LOST_RECORDS, 0u64)
            .set(c::PEAK_BYTES, 0u64)
            .set(c::BUDGET_REFUSED, 0u64)
            .set(c::WRITE_ERRORS, 0u64)
            .set(c::STATE, "COMPLETE")
            .push(c::NON_CLAIMS, "LOCAL");
        let bytes = d.encode().unwrap();
        assert_eq!(
            decode(&c::SCHEMA, &bytes).unwrap().text(c::STATE).unwrap(),
            "COMPLETE"
        );
        assert!(crate::schema::decode_any(&bytes).is_ok());
    }
}
