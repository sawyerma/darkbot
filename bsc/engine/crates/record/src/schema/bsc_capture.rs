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

    /// v2 (Phase F contract §4): the host the run's latencies are of, the
    /// build that wrote it and the SHA-256 of its runtime configuration.
    pub const HOST: u16 = 25;
    pub const BUILD: u16 = 26;
    pub const CONFIG_SHA256: u16 = 27;

    pub const V2: Schema = Schema {
        name: "darkbot.bsc.capture-run.v1",
        record_type: 55,
        schema_version: 2,
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
            Field::opt(HOST, "host", Text),
            Field::opt(BUILD, "build", Text),
            Field::opt(CONFIG_SHA256, "config_sha256", B256),
        ],
    };

    /// v3 (Phase F gate H1/M5): pending events the sources delivered that
    /// the bounded ingress queue dropped before the journal (a loss like a
    /// gap span), and the build's own SHA-256 and whether its source tree
    /// was dirty (`BUILD` names the git commit).
    pub const INGRESS_DROPPED: u16 = 28;
    pub const BUILD_SHA256: u16 = 29;
    pub const BUILD_DIRTY: u16 = 30;
    /// The pending capture scope (`LAUNCHES`, `VENUES`, `ALL`).
    pub const CAPTURE_PENDING: u16 = 31;

    pub const V3: Schema = Schema {
        name: "darkbot.bsc.capture-run.v1",
        record_type: 55,
        schema_version: 3,
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
            Field::opt(HOST, "host", Text),
            Field::opt(BUILD, "build", Text),
            Field::opt(CONFIG_SHA256, "config_sha256", B256),
            Field::req(INGRESS_DROPPED, "ingress_dropped", U64),
            Field::opt(BUILD_SHA256, "build_sha256", B256),
            Field::opt(BUILD_DIRTY, "build_dirty", Bool),
            Field::opt(CAPTURE_PENDING, "capture_pending", Text),
        ],
    };

    /// A run manifest of either version (v1 of the first segmented builds).
    pub fn decode_run(bytes: &[u8]) -> Result<crate::Record, crate::RecordError> {
        crate::decode(&V3, bytes)
            .or_else(|_| crate::decode(&V2, bytes))
            .or_else(|_| crate::decode(&SCHEMA, bytes))
    }
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
    /// Other pending first sightings the capture scope kept
    /// (`TxpoolAccepted`), and those addressed to a venue contract.
    pub const PENDING_SIGHTINGS: u16 = 23;
    pub const PENDING_TO_VENUES: u16 = 24;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.replay-proof.v1",
        record_type: 56,
        schema_version: 2,
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
            Field::req(PENDING_SIGHTINGS, "pending_sightings", U64),
            Field::req(PENDING_TO_VENUES, "pending_to_venues", U64),
        ],
    };

    /// v3 (Phase F gate H2): the runtime's canonical counters of the run
    /// equal the replayed census of that run alone; launches seen full
    /// before their block without a journaled sighting; pending sightings
    /// journaled as other that the decoders take for a launch; whether the
    /// replay ran with the run's configuration and build.
    pub const STATUS_COUNTERS_MATCH: u16 = 25;
    pub const SIGHTINGS_MISSING: u16 = 26;
    pub const PENDING_AS_LAUNCH: u16 = 27;
    pub const CONFIG_MATCH: u16 = 28;
    pub const BUILD_MATCH: u16 = 29;

    pub const V3: Schema = Schema {
        name: "darkbot.bsc.replay-proof.v1",
        record_type: 56,
        schema_version: 3,
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
            Field::req(PENDING_SIGHTINGS, "pending_sightings", U64),
            Field::req(PENDING_TO_VENUES, "pending_to_venues", U64),
            Field::opt(STATUS_COUNTERS_MATCH, "status_counters_match", Bool),
            Field::req(SIGHTINGS_MISSING, "sightings_missing", U64),
            Field::req(PENDING_AS_LAUNCH, "pending_as_launch", U64),
            Field::opt(CONFIG_MATCH, "config_match", Bool),
            Field::opt(BUILD_MATCH, "build_match", Bool),
        ],
    };

    /// A replay proof of either version.
    pub fn decode_proof(bytes: &[u8]) -> Result<crate::Record, crate::RecordError> {
        crate::decode(&V3, bytes).or_else(|_| crate::decode(&SCHEMA, bytes))
    }
}

/// The capacity preflight of a planned scoped capture (master plan §11):
/// the measured inputs (rates of the newest sealed run, the writer
/// throughput just measured on the capture volume, its free bytes) and
/// the verdict. A run that is not qualified is not started.
pub mod capture_preflight {
    use super::*;

    pub const DURATION_S: u16 = 1;
    pub const CHAIN_ID: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// The sealed run the rates come from (absent: none, unmeasured).
    pub const RATES_RUN_ID: u16 = 5;
    pub const AVERAGE_BYTES_PER_S: u16 = 6;
    /// The densest segment's rate, held for `burst_s`.
    pub const PEAK_BYTES_PER_S: u16 = 7;
    pub const BURST_S: u16 = 8;
    pub const OVERHEAD_PPM: u16 = 9;
    /// Bytes written, synced and sealed by the probe, and their rate.
    pub const PROBE_BYTES: u16 = 10;
    pub const WRITER_BYTES_PER_S: u16 = 11;
    pub const FREE_DISK_BYTES: u16 = 12;
    pub const DISK_RESERVE_BYTES: u16 = 13;
    pub const MEMORY_BUDGET_BYTES: u16 = 14;
    /// `QUALIFIED` or `BLOCKED`.
    pub const VERDICT: u16 = 15;
    pub const EXPECTED_BYTES: u16 = 16;
    pub const BURST_BACKLOG_BYTES: u16 = 17;
    /// `UNMEASURED`, `WRITER_THROUGHPUT`, `MEMORY` or `DISK`.
    pub const BLOCKING_RESOURCE: u16 = 18;
    /// The longest duration that would be qualified.
    pub const ACHIEVABLE_S: u16 = 19;
    pub const NON_CLAIMS: u16 = 20;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.capture-preflight.v1",
        record_type: 57,
        schema_version: 1,
        fields: &[
            Field::req(DURATION_S, "duration_s", U64),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(RATES_RUN_ID, "rates_run_id", Text),
            Field::req(AVERAGE_BYTES_PER_S, "average_bytes_per_s", U64),
            Field::req(PEAK_BYTES_PER_S, "peak_bytes_per_s", U64),
            Field::req(BURST_S, "burst_s", U64),
            Field::req(OVERHEAD_PPM, "overhead_ppm", U64),
            Field::req(PROBE_BYTES, "probe_bytes", U64),
            Field::req(WRITER_BYTES_PER_S, "writer_bytes_per_s", U64),
            Field::req(FREE_DISK_BYTES, "free_disk_bytes", U64),
            Field::req(DISK_RESERVE_BYTES, "disk_reserve_bytes", U64),
            Field::req(MEMORY_BUDGET_BYTES, "memory_budget_bytes", U64),
            Field::req(VERDICT, "verdict", Text),
            Field::opt(EXPECTED_BYTES, "expected_bytes", U64),
            Field::opt(BURST_BACKLOG_BYTES, "burst_backlog_bytes", U64),
            Field::opt(BLOCKING_RESOURCE, "blocking_resource", Text),
            Field::opt(ACHIEVABLE_S, "achievable_s", U64),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

/// The native local ingest latency of the production code on one host
/// (Phase F contract §8, the local part): per stage, measured per event on
/// real bytes, the quantiles and the counts at or above 0.5 / 1 / 3 ms.
/// The geth hook stages (peer decode → native event → ring → Rust) need a
/// synced node and are not in it.
pub mod ingest_latency {
    use super::*;

    pub mod stage {
        use super::*;

        pub const NAME: u16 = 1;
        pub const SAMPLES: u16 = 2;
        pub const MIN_NS: u16 = 3;
        pub const MEAN_NS: u16 = 4;
        pub const P50_NS: u16 = 5;
        pub const P95_NS: u16 = 6;
        pub const P99_NS: u16 = 7;
        pub const P99_9_NS: u16 = 8;
        pub const P99_99_NS: u16 = 9;
        pub const MAX_NS: u16 = 10;
        pub const GE_500US: u16 = 11;
        pub const GE_1MS: u16 = 12;
        pub const GE_3MS: u16 = 13;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.ingest-latency.stage",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(NAME, "name", Text),
                Field::req(SAMPLES, "samples", U64),
                Field::req(MIN_NS, "min_ns", U64),
                Field::req(MEAN_NS, "mean_ns", U64),
                Field::req(P50_NS, "p50_ns", U64),
                Field::req(P95_NS, "p95_ns", U64),
                Field::req(P99_NS, "p99_ns", U64),
                Field::opt(P99_9_NS, "p99_9_ns", U64),
                Field::opt(P99_99_NS, "p99_99_ns", U64),
                Field::req(MAX_NS, "max_ns", U64),
                Field::req(GE_500US, "ge_500us", U64),
                Field::req(GE_1MS, "ge_1ms", U64),
                Field::req(GE_3MS, "ge_3ms", U64),
            ],
        };
    }

    pub const HOST: u16 = 1;
    pub const BUILD: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const GIT_HEAD: u16 = 5;
    pub const WARMUP: u16 = 6;
    pub const STAGES: u16 = 7;
    /// Two clock reads back to back: the floor of every sample.
    pub const CLOCK_OVERHEAD_P50_NS: u16 = 8;
    /// Records the recorder refused during the measurement (counted).
    pub const RECORDER_LOST: u16 = 9;
    pub const NON_CLAIMS: u16 = 10;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.ingest-latency.v1",
        record_type: 58,
        schema_version: 1,
        fields: &[
            Field::req(HOST, "host", Text),
            Field::req(BUILD, "build", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(GIT_HEAD, "git_head", Text),
            Field::req(WARMUP, "warmup", U64),
            Field::nested(STAGES, "stages", Presence::Repeated, &stage::SCHEMA),
            Field::req(CLOCK_OVERHEAD_P50_NS, "clock_overhead_p50_ns", U64),
            Field::req(RECORDER_LOST, "recorder_lost", U64),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[
    &capture_run::SCHEMA,
    &capture_run::V2,
    &capture_run::V3,
    &replay_proof::SCHEMA,
    &replay_proof::V3,
    &capture_preflight::SCHEMA,
    &ingest_latency::SCHEMA,
];

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
        // A v1 manifest (the first segmented builds) still decodes.
        assert_eq!(c::decode_run(&bytes).unwrap().schema().schema_version, 1);
    }
}
