//! The C1 qualification evidence chain: the C1.6 host profile, the C1.9
//! native latency run (Rust consumer summary and report), the C1.9b
//! hook/path run (capture, controlled launches and report) and the C1.10
//! freeze (inputs, bindings and record).
//!
//! Raw per-sample evidence is too large for one record, so it is a
//! fixed-width table ([`table`]); every report binds its tables by SHA-256.
//! Historical JSON evidence in `bsc/native/evidence/c1/*.json` stays as it
//! was written and is never read by these schemas.

use crate::{Field, Kind::*, Presence, Schema};

/// Statistics of one directly measured per-sample quantity, in nanoseconds
/// of one clock. Every value but `samples` is absent when there are no
/// samples; p99.9 and p99.99 are absent (with a note) when the sample count
/// does not support them. The maximum is always the maximum.
pub mod latency_stats {
    use super::*;

    pub const SEGMENT: u16 = 1;
    pub const SAMPLES: u16 = 2;
    pub const MIN_NS: u16 = 3;
    /// The exact mean rounded towards negative infinity.
    pub const MEAN_NS_FLOOR: u16 = 4;
    /// Nearest-rank percentiles.
    pub const P50_NS: u16 = 5;
    pub const P95_NS: u16 = 6;
    pub const P99_NS: u16 = 7;
    pub const P99_9_NS: u16 = 8;
    pub const P99_9_NOTE: u16 = 9;
    pub const P99_99_NS: u16 = 10;
    pub const P99_99_NOTE: u16 = 11;
    pub const MAX_NS: u16 = 12;
    /// Values of at least one millisecond.
    pub const MISSES_GE_1MS: u16 = 13;
    pub const BAND_LT_200US: u16 = 14;
    pub const BAND_200US_TO_LT_500US: u16 = 15;
    pub const BAND_500US_TO_LT_1MS: u16 = 16;
    pub const BAND_GE_1MS: u16 = 17;
    /// `no samples` when there are none.
    pub const NOTE: u16 = 18;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.latency-stats",
        record_type: 0,
        schema_version: 1,
        fields: &[
            Field::req(SEGMENT, "segment", Text),
            Field::req(SAMPLES, "samples", U64),
            Field::opt(MIN_NS, "min_ns", I64),
            Field::opt(MEAN_NS_FLOOR, "mean_ns_floor", I64),
            Field::opt(P50_NS, "p50_ns", I64),
            Field::opt(P95_NS, "p95_ns", I64),
            Field::opt(P99_NS, "p99_ns", I64),
            Field::opt(P99_9_NS, "p99_9_ns", I64),
            Field::opt(P99_9_NOTE, "p99_9_note", Text),
            Field::opt(P99_99_NS, "p99_99_ns", I64),
            Field::opt(P99_99_NOTE, "p99_99_note", Text),
            Field::opt(MAX_NS, "max_ns", I64),
            Field::opt(MISSES_GE_1MS, "misses_ge_1ms", U64),
            Field::opt(BAND_LT_200US, "bands.lt_200us", U64),
            Field::opt(BAND_200US_TO_LT_500US, "bands.200us_to_lt_500us", U64),
            Field::opt(BAND_500US_TO_LT_1MS, "bands.500us_to_lt_1ms", U64),
            Field::opt(BAND_GE_1MS, "bands.ge_1ms", U64),
            Field::opt(NOTE, "note", Text),
        ],
    };
}

/// C1.6: what the host reports about itself, the profile hint and the
/// profile selection. No timestamp: the same host yields the same bytes.
/// Every probe is either its value or the reason it is unavailable.
pub mod host_profile {
    use super::*;

    /// One Apple performance level (`hw.perflevelN.*`).
    pub mod perf_level {
        use super::*;

        pub const INDEX: u16 = 1;
        pub const NAME: u16 = 2;
        pub const NAME_UNAVAILABLE: u16 = 3;
        pub const PHYSICAL_CPUS: u16 = 4;
        pub const PHYSICAL_CPUS_UNAVAILABLE: u16 = 5;
        pub const LOGICAL_CPUS: u16 = 6;
        pub const LOGICAL_CPUS_UNAVAILABLE: u16 = 7;
        pub const L1D_CACHE_BYTES: u16 = 8;
        pub const L1D_CACHE_BYTES_UNAVAILABLE: u16 = 9;
        pub const L1I_CACHE_BYTES: u16 = 10;
        pub const L1I_CACHE_BYTES_UNAVAILABLE: u16 = 11;
        pub const L2_CACHE_BYTES: u16 = 12;
        pub const L2_CACHE_BYTES_UNAVAILABLE: u16 = 13;
        pub const CPUS_PER_L2: u16 = 14;
        pub const CPUS_PER_L2_UNAVAILABLE: u16 = 15;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.host-perf-level",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(INDEX, "index", U64),
                Field::opt(NAME, "name", Text),
                Field::opt(NAME_UNAVAILABLE, "name.unavailable", Text),
                Field::opt(PHYSICAL_CPUS, "physical_cpus", U64),
                Field::opt(PHYSICAL_CPUS_UNAVAILABLE, "physical_cpus.unavailable", Text),
                Field::opt(LOGICAL_CPUS, "logical_cpus", U64),
                Field::opt(LOGICAL_CPUS_UNAVAILABLE, "logical_cpus.unavailable", Text),
                Field::opt(L1D_CACHE_BYTES, "l1d_cache_bytes", U64),
                Field::opt(
                    L1D_CACHE_BYTES_UNAVAILABLE,
                    "l1d_cache_bytes.unavailable",
                    Text,
                ),
                Field::opt(L1I_CACHE_BYTES, "l1i_cache_bytes", U64),
                Field::opt(
                    L1I_CACHE_BYTES_UNAVAILABLE,
                    "l1i_cache_bytes.unavailable",
                    Text,
                ),
                Field::opt(L2_CACHE_BYTES, "l2_cache_bytes", U64),
                Field::opt(
                    L2_CACHE_BYTES_UNAVAILABLE,
                    "l2_cache_bytes.unavailable",
                    Text,
                ),
                Field::opt(CPUS_PER_L2, "cpus_per_l2", U64),
                Field::opt(CPUS_PER_L2_UNAVAILABLE, "cpus_per_l2.unavailable", Text),
            ],
        };
    }

    /// CPUs, caches and memory of the host.
    pub mod topology {
        use super::*;

        pub const OS: u16 = 1;
        pub const ARCH: u16 = 2;
        pub const MODEL: u16 = 3;
        pub const MODEL_UNAVAILABLE: u16 = 4;
        pub const CPU_BRAND: u16 = 5;
        pub const CPU_BRAND_UNAVAILABLE: u16 = 6;
        pub const OS_RELEASE: u16 = 7;
        pub const OS_RELEASE_UNAVAILABLE: u16 = 8;
        pub const PHYSICAL_CPUS: u16 = 9;
        pub const PHYSICAL_CPUS_UNAVAILABLE: u16 = 10;
        pub const LOGICAL_CPUS: u16 = 11;
        pub const LOGICAL_CPUS_UNAVAILABLE: u16 = 12;
        pub const CACHE_LINE_BYTES: u16 = 13;
        pub const CACHE_LINE_BYTES_UNAVAILABLE: u16 = 14;
        pub const PAGE_BYTES: u16 = 15;
        pub const PAGE_BYTES_UNAVAILABLE: u16 = 16;
        pub const MEMORY_BYTES: u16 = 17;
        pub const MEMORY_BYTES_UNAVAILABLE: u16 = 18;
        /// The levels; none and no reason means the host reports zero levels.
        pub const PERF_LEVELS: u16 = 19;
        pub const PERF_LEVELS_UNAVAILABLE: u16 = 20;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.host-topology",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(OS, "os", Text),
                Field::req(ARCH, "arch", Text),
                Field::opt(MODEL, "model", Text),
                Field::opt(MODEL_UNAVAILABLE, "model.unavailable", Text),
                Field::opt(CPU_BRAND, "cpu_brand", Text),
                Field::opt(CPU_BRAND_UNAVAILABLE, "cpu_brand.unavailable", Text),
                Field::opt(OS_RELEASE, "os_release", Text),
                Field::opt(OS_RELEASE_UNAVAILABLE, "os_release.unavailable", Text),
                Field::opt(PHYSICAL_CPUS, "physical_cpus", U64),
                Field::opt(PHYSICAL_CPUS_UNAVAILABLE, "physical_cpus.unavailable", Text),
                Field::opt(LOGICAL_CPUS, "logical_cpus", U64),
                Field::opt(LOGICAL_CPUS_UNAVAILABLE, "logical_cpus.unavailable", Text),
                Field::opt(CACHE_LINE_BYTES, "cache_line_bytes", U64),
                Field::opt(
                    CACHE_LINE_BYTES_UNAVAILABLE,
                    "cache_line_bytes.unavailable",
                    Text,
                ),
                Field::opt(PAGE_BYTES, "page_bytes", U64),
                Field::opt(PAGE_BYTES_UNAVAILABLE, "page_bytes.unavailable", Text),
                Field::opt(MEMORY_BYTES, "memory_bytes", U64),
                Field::opt(MEMORY_BYTES_UNAVAILABLE, "memory_bytes.unavailable", Text),
                Field::nested(
                    PERF_LEVELS,
                    "perf_levels",
                    Presence::Repeated,
                    &perf_level::SCHEMA,
                ),
                Field::opt(PERF_LEVELS_UNAVAILABLE, "perf_levels.unavailable", Text),
            ],
        };
    }

    pub const TOPOLOGY: u16 = 1;
    /// CRC32C of the topology's canonical preimage (see `geth_ipc::host`).
    pub const TOPOLOGY_FINGERPRINT_CRC32C: u16 = 2;
    /// `S` or `P`: the first experiment the topology suggests.
    pub const HINT_PROFILE: u16 = 3;
    pub const HINT_BASIS: u16 = 4;
    /// Always true: a hint qualifies nothing.
    pub const HINT_ONLY: u16 = 5;
    /// The profile that runs and why; absent when the request was refused.
    pub const SELECTION_PROFILE: u16 = 6;
    pub const SELECTION_SOURCE: u16 = 7;
    /// Why the configured profile was refused (never downgraded).
    pub const SELECTION_ERROR: u16 = 8;
    /// `not-requested` or the outcome of the QoS request.
    pub const QOS: u16 = 9;
    pub const QOS_IS_ADVISORY_NOT_PINNING: u16 = 10;
    pub const HOST_ROLE: u16 = 11;
    pub const NON_CLAIMS: u16 = 12;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.host-profile.v1",
        record_type: 42,
        schema_version: 1,
        fields: &[
            Field::nested(TOPOLOGY, "topology", Presence::Required, &topology::SCHEMA),
            Field::req(
                TOPOLOGY_FINGERPRINT_CRC32C,
                "topology_fingerprint_crc32c",
                U64,
            ),
            Field::req(HINT_PROFILE, "profile_hint.profile", Text),
            Field::req(HINT_BASIS, "profile_hint.basis", Text),
            Field::req(HINT_ONLY, "profile_hint.hint_only", Bool),
            Field::opt(SELECTION_PROFILE, "profile_selection.profile", Text),
            Field::opt(SELECTION_SOURCE, "profile_selection.source", Text),
            Field::opt(SELECTION_ERROR, "profile_selection.error", Text),
            Field::req(QOS, "qos", Text),
            Field::req(
                QOS_IS_ADVISORY_NOT_PINNING,
                "qos_is_advisory_not_pinning",
                Bool,
            ),
            Field::req(HOST_ROLE, "host_role", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

/// C1.9: the Rust consumer's summary of one run, binding its raw tables.
pub mod c19_rust_summary {
    use super::*;

    pub const INGRESS_SAMPLES: u16 = 1;
    pub const OUTBOUND_EVIDENCE_ROWS: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const CHAIN_RECORDS_SERVICED: u16 = 5;
    /// Heap allocations of the HOT0 thread inside the qualification window.
    pub const HOT0_ALLOCATIONS_IN_WINDOW: u16 = 6;
    pub const HOT0_POLLS: u16 = 7;
    pub const HOT0_IDLE_POLLS: u16 = 8;
    pub const HOT0_PEER_RECORDS: u16 = 9;
    pub const HOT0_ADMITTED: u16 = 10;
    pub const HOT0_REJECTED: u16 = 11;
    pub const HOT0_DUPLICATES: u16 = 12;
    pub const HOT0_INVALID_RECORDS: u16 = 13;
    pub const HOT0_LANE_FAULTS: u16 = 14;
    pub const HOT0_CHAIN_GAPS: u16 = 15;
    /// Pool exhausted + launch too large + unstable slot.
    pub const HOT0_DROPPED: u16 = 16;
    pub const HOT0_DEDUP_EVICTIONS: u16 = 17;
    /// `CLOCK_MONOTONIC`.
    pub const CLOCK: u16 = 18;
    pub const RAW_INGRESS_SHA256: u16 = 19;
    pub const RAW_OUTBOUND_SHA256: u16 = 20;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.c19-rust-summary.v1",
        record_type: 43,
        schema_version: 1,
        fields: &[
            Field::req(INGRESS_SAMPLES, "ingress_samples", U64),
            Field::req(OUTBOUND_EVIDENCE_ROWS, "outbound_evidence_rows", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(CHAIN_RECORDS_SERVICED, "chain_records_serviced", U64),
            Field::req(
                HOT0_ALLOCATIONS_IN_WINDOW,
                "hot0_allocations_in_qualification_window",
                U64,
            ),
            Field::req(HOT0_POLLS, "hot0_polls", U64),
            Field::req(HOT0_IDLE_POLLS, "hot0_idle_polls", U64),
            Field::req(HOT0_PEER_RECORDS, "hot0_peer_records", U64),
            Field::req(HOT0_ADMITTED, "hot0_admitted", U64),
            Field::req(HOT0_REJECTED, "hot0_rejected", U64),
            Field::req(HOT0_DUPLICATES, "hot0_duplicates", U64),
            Field::req(HOT0_INVALID_RECORDS, "hot0_invalid_records", U64),
            Field::req(HOT0_LANE_FAULTS, "hot0_lane_faults", U64),
            Field::req(HOT0_CHAIN_GAPS, "hot0_chain_gaps", U64),
            Field::req(HOT0_DROPPED, "hot0_dropped", U64),
            Field::req(HOT0_DEDUP_EVICTIONS, "hot0_dedup_evictions", U64),
            Field::req(CLOCK, "clock", Text),
            Field::req(RAW_INGRESS_SHA256, "raw_ingress_sha256", B256),
            Field::req(RAW_OUTBOUND_SHA256, "raw_outbound_sha256", B256),
        ],
    };
}

/// C1.9: the native latency report derived from the raw run evidence. It
/// states measurements and integrity; it decides no gate.
pub mod native_latency {
    use super::*;

    /// One integrity counter; absent value: the producer did not report it.
    pub mod counter {
        use super::*;

        pub const NAME: u16 = 1;
        pub const VALUE: u16 = 2;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.counter",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(NAME, "name", Text),
                Field::opt(VALUE, "value", U64),
            ],
        };
    }

    /// How many worker slots of the window ended in one status.
    pub mod slot_outcome {
        use super::*;

        pub const STATUS: u16 = 1;
        pub const COUNT: u16 = 2;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.slot-outcome",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(STATUS, "status", Text),
                Field::req(COUNT, "count", U64),
            ],
        };
    }

    pub const OUTBOUND_MODE: u16 = 1;
    pub const PRODUCTION_PATH_PRESENT: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const PURPOSE: u16 = 5;
    pub const QUALIFICATION_ELIGIBLE: u16 = 6;
    /// `MEASURED`.
    pub const RESULT: u16 = 7;
    /// `PASS` or `FAIL`.
    pub const INTEGRITY: u16 = 8;
    pub const INTEGRITY_FAILURES: u16 = 9;
    /// `NOT_DECIDED_BY_THIS_TOOL`.
    pub const GATE: u16 = 10;
    /// The manifest's clock id, as written.
    pub const CLOCK_ID: u16 = 11;
    pub const CLOCK_DOMAIN: u16 = 12;
    pub const WINDOW_FIXED_BEFORE_RUN: u16 = 13;
    pub const WINDOW_PEERS: u16 = 14;
    pub const WINDOW_WARMUP_PER_PEER: u16 = 15;
    pub const WINDOW_SAMPLES_PER_PEER: u16 = 16;
    pub const WINDOW_WARMUP_RETAINED: u16 = 17;
    pub const WINDOW_QUALIFICATION_SAMPLES: u16 = 18;
    pub const WINDOW_LOAD_MODEL: u16 = 19;
    pub const L1_SEMANTICS: u16 = 20;
    pub const INGRESS_METRICS: u16 = 21;
    pub const INGRESS_QUEUE_DEPTH_MAX: u16 = 22;
    pub const OUTBOUND_ENABLED: u16 = 23;
    pub const OUTBOUND_WORKERS: u16 = 24;
    pub const OUTBOUND_BUYS_IN_WINDOW: u16 = 25;
    pub const OUTBOUND_SLOT_OUTCOMES: u16 = 26;
    pub const OUTBOUND_BUYS_WITHOUT_ALL_SELECTED: u16 = 27;
    pub const OUTBOUND_METRICS: u16 = 28;
    pub const PREPARED_BUY_PATH_SCOPE: u16 = 29;
    pub const PREPARED_BUY_PATH_METRICS: u16 = 30;
    pub const OUTBOUND_HANDOFF_BOUNDARY: u16 = 31;
    pub const COUNTERS: u16 = 32;
    pub const COUNTERS_HOT0_LOCKS: u16 = 33;
    pub const ID_DARKBOT_COMMIT: u16 = 34;
    pub const ID_DARKBOT_TREE: u16 = 35;
    pub const ID_PINNED_GETH_COMMIT: u16 = 36;
    pub const ID_GO_TEST_BINARY_SHA256: u16 = 37;
    pub const ID_RUST_CONSUMER_BINARY_SHA256: u16 = 38;
    pub const ID_GO_OVERLAY_SOURCES_SHA256: u16 = 39;
    pub const ID_RUST_FASTLANE_SOURCES_SHA256: u16 = 40;
    pub const ID_OVERLAY_TOOL_SHA256: u16 = 41;
    pub const ID_HARNESS_SCRIPT_SHA256: u16 = 42;
    /// The report tool's own executable.
    pub const ID_ANALYZER_SHA256: u16 = 43;
    pub const ID_PRODUCTION_BRIDGE_SHA256: u16 = 44;
    pub const ID_PRODUCTION_PEER_HARNESS_SHA256: u16 = 45;
    pub const ID_CONFIG_MANIFEST_SHA256: u16 = 46;
    pub const ID_HOST_PROFILE_SHA256: u16 = 47;
    pub const ID_TOPOLOGY_FINGERPRINT_CRC32C: u16 = 48;
    pub const ID_PROFILE_SELECTION: u16 = 49;
    pub const ID_RAW_INGRESS_SHA256: u16 = 50;
    pub const ID_RAW_OUTBOUND_SHA256: u16 = 51;
    pub const ID_GO_SNAPSHOT_SHA256: u16 = 52;
    pub const ID_RUST_SUMMARY_SHA256: u16 = 53;
    pub const RULES: u16 = 54;
    pub const NON_CLAIMS: u16 = 55;

    const STATS: &Schema = &latency_stats::SCHEMA;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.native-latency.v1",
        record_type: 44,
        schema_version: 1,
        fields: &[
            Field::req(OUTBOUND_MODE, "outbound_mode", Text),
            Field::req(
                PRODUCTION_PATH_PRESENT,
                "c1_9_production_path_present",
                Bool,
            ),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(PURPOSE, "purpose", Text),
            Field::req(QUALIFICATION_ELIGIBLE, "c1_9_qualification_eligible", Bool),
            Field::req(RESULT, "result", Text),
            Field::req(INTEGRITY, "integrity", Text),
            Field::rep(INTEGRITY_FAILURES, "integrity_failures", Text),
            Field::req(GATE, "gate", Text),
            Field::opt(CLOCK_ID, "clock.clock_id", Text),
            Field::req(CLOCK_DOMAIN, "clock.domain", Text),
            Field::req(WINDOW_FIXED_BEFORE_RUN, "window.fixed_before_run", Bool),
            Field::req(WINDOW_PEERS, "window.peers", U64),
            Field::req(WINDOW_WARMUP_PER_PEER, "window.warmup_per_peer", U64),
            Field::req(WINDOW_SAMPLES_PER_PEER, "window.samples_per_peer", U64),
            Field::req(
                WINDOW_WARMUP_RETAINED,
                "window.warmup_samples_retained_in_raw",
                U64,
            ),
            Field::req(
                WINDOW_QUALIFICATION_SAMPLES,
                "window.qualification_samples",
                U64,
            ),
            Field::opt(WINDOW_LOAD_MODEL, "window.load_model", Text),
            Field::rep(L1_SEMANTICS, "l1_semantics", Text),
            Field::nested(
                INGRESS_METRICS,
                "ingress_metrics",
                Presence::Repeated,
                STATS,
            ),
            Field::req(
                INGRESS_QUEUE_DEPTH_MAX,
                "ingress_metrics.queue_depth_max",
                U64,
            ),
            Field::req(OUTBOUND_ENABLED, "outbound.enabled", Bool),
            Field::opt(OUTBOUND_WORKERS, "outbound.workers", U64),
            Field::opt(OUTBOUND_BUYS_IN_WINDOW, "outbound.buys_in_window", U64),
            Field::nested(
                OUTBOUND_SLOT_OUTCOMES,
                "outbound.worker_slot_outcomes",
                Presence::Repeated,
                &slot_outcome::SCHEMA,
            ),
            Field::opt(
                OUTBOUND_BUYS_WITHOUT_ALL_SELECTED,
                "outbound.buys_without_all_selected_handoffs",
                U64,
            ),
            Field::nested(
                OUTBOUND_METRICS,
                "outbound_metrics",
                Presence::Repeated,
                STATS,
            ),
            Field::opt(PREPARED_BUY_PATH_SCOPE, "prepared_buy_path.scope", Text),
            Field::nested(
                PREPARED_BUY_PATH_METRICS,
                "prepared_buy_path",
                Presence::Repeated,
                STATS,
            ),
            Field::opt(OUTBOUND_HANDOFF_BOUNDARY, "outbound.handoff_boundary", Text),
            Field::nested(COUNTERS, "counters", Presence::Repeated, &counter::SCHEMA),
            Field::req(COUNTERS_HOT0_LOCKS, "counters.hot0_locks", Text),
            Field::req(ID_DARKBOT_COMMIT, "identity.darkbot_commit", Text),
            Field::req(ID_DARKBOT_TREE, "identity.darkbot_tree", Text),
            Field::req(ID_PINNED_GETH_COMMIT, "identity.pinned_geth_commit", Text),
            Field::req(
                ID_GO_TEST_BINARY_SHA256,
                "identity.go_test_binary_sha256",
                B256,
            ),
            Field::req(
                ID_RUST_CONSUMER_BINARY_SHA256,
                "identity.rust_consumer_binary_sha256",
                B256,
            ),
            Field::req(
                ID_GO_OVERLAY_SOURCES_SHA256,
                "identity.go_overlay_sources_sha256",
                B256,
            ),
            Field::req(
                ID_RUST_FASTLANE_SOURCES_SHA256,
                "identity.rust_fastlane_sources_sha256",
                B256,
            ),
            Field::req(ID_OVERLAY_TOOL_SHA256, "identity.overlay_tool_sha256", B256),
            Field::req(
                ID_HARNESS_SCRIPT_SHA256,
                "identity.harness_script_sha256",
                B256,
            ),
            Field::req(ID_ANALYZER_SHA256, "identity.analyzer_sha256", B256),
            Field::req(
                ID_PRODUCTION_BRIDGE_SHA256,
                "identity.production_bridge_sha256",
                B256,
            ),
            Field::req(
                ID_PRODUCTION_PEER_HARNESS_SHA256,
                "identity.production_peer_harness_sha256",
                B256,
            ),
            Field::req(
                ID_CONFIG_MANIFEST_SHA256,
                "identity.config_manifest_sha256",
                B256,
            ),
            Field::req(ID_HOST_PROFILE_SHA256, "identity.host_profile_sha256", B256),
            Field::req(
                ID_TOPOLOGY_FINGERPRINT_CRC32C,
                "identity.topology_fingerprint_crc32c",
                U64,
            ),
            Field::req(ID_PROFILE_SELECTION, "identity.profile_selection", Text),
            Field::req(ID_RAW_INGRESS_SHA256, "identity.raw_ingress_sha256", B256),
            Field::req(ID_RAW_OUTBOUND_SHA256, "identity.raw_outbound_sha256", B256),
            Field::req(ID_GO_SNAPSHOT_SHA256, "identity.go_snapshot_sha256", B256),
            Field::req(ID_RUST_SUMMARY_SHA256, "identity.rust_summary_sha256", B256),
            Field::rep(RULES, "rules", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

/// C1.9b: what the capture saw, binding its observation table. Rows whose
/// clock is not `CLOCK_MONOTONIC` or that do not decode are counted as
/// invalid and not written; a capture with invalid records or lane faults
/// is incomplete.
pub mod c19b_capture {
    use super::*;

    pub const HOST_BOOT_ID: u16 = 1;
    pub const GENERATION: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const PEER_LANES: u16 = 5;
    pub const ROWS: u16 = 6;
    pub const INVALID_RECORDS: u16 = 7;
    pub const LANE_FAULTS: u16 = 8;
    pub const OBSERVATIONS_SHA256: u16 = 9;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.c19b-capture.v1",
        record_type: 45,
        schema_version: 1,
        fields: &[
            Field::req(HOST_BOOT_ID, "host_boot_id", Text),
            Field::req(GENERATION, "generation", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(PEER_LANES, "peer_lanes", U64),
            Field::req(ROWS, "rows", U64),
            Field::req(INVALID_RECORDS, "invalid_records", U64),
            Field::req(LANE_FAULTS, "lane_faults", U64),
            Field::req(OBSERVATIONS_SHA256, "observations_sha256", B256),
        ],
    };
}

/// C1.9b input: the controlled Testnet launches and their handoff times,
/// built with `c19b_report launches init|add`.
pub mod c19b_launches {
    use super::*;

    pub mod launch {
        use super::*;

        pub const TX_HASH: u16 = 1;
        /// `CLOCK_MONOTONIC` of the capture host at the stated boundary.
        pub const HANDOFF_NS: u16 = 2;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.c19b-launch",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(TX_HASH, "tx_hash", B256),
                Field::req(HANDOFF_NS, "handoff_ns", U64),
            ],
        };
    }

    pub const CHAIN_ID: u16 = 1;
    pub const CLOCK_ID: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const HOST_BOOT_ID: u16 = 5;
    /// What `handoff_ns` marks.
    pub const HANDOFF_BOUNDARY: u16 = 6;
    pub const LAUNCHES: u16 = 7;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.c19b-launches.v1",
        record_type: 46,
        schema_version: 1,
        fields: &[
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(CLOCK_ID, "clock_id", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(HOST_BOOT_ID, "host_boot_id", Text),
            Field::req(HANDOFF_BOUNDARY, "handoff_boundary", Text),
            Field::nested(LAUNCHES, "launches", Presence::Repeated, &launch::SCHEMA),
        ],
    };
}

/// C1.9b: the hook/path report (Testnet only). It decides no gate.
pub mod hook_path {
    use super::*;

    const STATS: &Schema = &latency_stats::SCHEMA;

    /// Broadcast or pooled: which kind of source saw a launch first.
    pub mod source_kind {
        use super::*;

        pub const SOURCE: u16 = 1;
        pub const HANDOFF_TO_T_INGRESS: u16 = 2;
        pub const FIRST_FOR_LAUNCHES: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.hook-path-source-kind",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SOURCE, "source", Text),
                Field::nested(
                    HANDOFF_TO_T_INGRESS,
                    "handoff_to_T_ingress",
                    Presence::Required,
                    STATS,
                ),
                Field::req(FIRST_FOR_LAUNCHES, "first_for_launches", U64),
            ],
        };
    }

    /// One source and peer.
    pub mod source_peer {
        use super::*;

        pub const SOURCE: u16 = 1;
        pub const PEER: u16 = 2;
        pub const LAUNCHES_OBSERVED: u16 = 3;
        /// Usable launches minus observations (negative if a peer repeated one).
        pub const LAUNCHES_NOT_OBSERVED: u16 = 4;
        pub const HANDOFF_TO_PACKET_DECODE: u16 = 5;
        pub const HANDOFF_TO_T_INGRESS: u16 = 6;
        pub const PACKET_DECODE_TO_T_INGRESS: u16 = 7;
        pub const T_INGRESS_BEHIND_FIRST_SOURCE: u16 = 8;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.hook-path-source-peer",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SOURCE, "source", Text),
                Field::req(PEER, "peer", B256),
                Field::req(LAUNCHES_OBSERVED, "launches_observed", U64),
                Field::req(LAUNCHES_NOT_OBSERVED, "launches_not_observed", I64),
                Field::nested(
                    HANDOFF_TO_PACKET_DECODE,
                    "handoff_to_packet_decode",
                    Presence::Required,
                    STATS,
                ),
                Field::nested(
                    HANDOFF_TO_T_INGRESS,
                    "handoff_to_T_ingress",
                    Presence::Required,
                    STATS,
                ),
                Field::nested(
                    PACKET_DECODE_TO_T_INGRESS,
                    "packet_decode_to_T_ingress",
                    Presence::Required,
                    STATS,
                ),
                Field::nested(
                    T_INGRESS_BEHIND_FIRST_SOURCE,
                    "T_ingress_behind_first_source",
                    Presence::Required,
                    STATS,
                ),
            ],
        };
    }

    /// One observation of a launch.
    pub mod observation {
        use super::*;

        pub const PEER: u16 = 1;
        pub const SOURCE: u16 = 2;
        pub const T_PACKET_DECODED_NS: u16 = 3;
        pub const T_INGRESS_NS: u16 = 4;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.hook-path-observation",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(PEER, "peer", B256),
                Field::req(SOURCE, "source", Text),
                Field::req(T_PACKET_DECODED_NS, "t_packet_decoded_ns", U64),
                Field::req(T_INGRESS_NS, "t_ingress_ns", U64),
            ],
        };
    }

    /// One controlled launch and what was seen of it.
    pub mod launch {
        use super::*;

        pub const TX_HASH: u16 = 1;
        pub const HANDOFF_NS: u16 = 2;
        pub const OBSERVED: u16 = 3;
        /// Why an observed launch is not measured.
        pub const EXCLUDED: u16 = 4;
        pub const FIRST_SOURCE: u16 = 5;
        pub const FIRST_PEER: u16 = 6;
        pub const HANDOFF_TO_FIRST_INGRESS_NS: u16 = 7;
        /// Ordered by `T_ingress`.
        pub const OBSERVATIONS: u16 = 8;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.hook-path-launch",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(TX_HASH, "tx_hash", B256),
                Field::req(HANDOFF_NS, "handoff_ns", U64),
                Field::req(OBSERVED, "observed", Bool),
                Field::opt(EXCLUDED, "excluded", Text),
                Field::opt(FIRST_SOURCE, "first_source", Text),
                Field::opt(FIRST_PEER, "first_peer", B256),
                Field::opt(
                    HANDOFF_TO_FIRST_INGRESS_NS,
                    "handoff_to_first_ingress_ns",
                    I64,
                ),
                Field::nested(
                    OBSERVATIONS,
                    "observations",
                    Presence::Repeated,
                    &observation::SCHEMA,
                ),
            ],
        };
    }

    /// `MEASURED`.
    pub const RESULT: u16 = 1;
    pub const NETWORK: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const CHAIN_ID: u16 = 5;
    pub const CLOCK_ID: u16 = 6;
    pub const HOST_BOOT_ID: u16 = 7;
    pub const CLOCK_DOMAIN: u16 = 8;
    pub const HANDOFF_BOUNDARY: u16 = 9;
    pub const LAUNCHES: u16 = 10;
    pub const LAUNCHES_OBSERVED: u16 = 11;
    pub const LAUNCHES_MISSED: u16 = 12;
    pub const MISSED_TX_HASHES: u16 = 13;
    pub const EXCLUDED_OBSERVED_BEFORE_HANDOFF: u16 = 14;
    pub const HANDOFF_TO_EARLIEST_PACKET_DECODE: u16 = 15;
    pub const HANDOFF_TO_EARLIEST_T_INGRESS: u16 = 16;
    pub const BROADCAST_VS_POOLED: u16 = 17;
    pub const PER_SOURCE_AND_PEER: u16 = 18;
    pub const PER_LAUNCH: u16 = 19;
    /// `NOT_DECIDED_BY_THIS_TOOL`.
    pub const GATE: u16 = 20;
    pub const NON_CLAIMS: u16 = 21;
    pub const LAUNCHES_SHA256: u16 = 22;
    pub const OBSERVATIONS_SHA256: u16 = 23;
    pub const CAPTURE_SHA256: u16 = 24;
    /// The report tool's own executable.
    pub const ANALYZER_SHA256: u16 = 25;
    pub const CAPTURE_ROWS: u16 = 26;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.hook-path.v1",
        record_type: 47,
        schema_version: 1,
        fields: &[
            Field::req(RESULT, "result", Text),
            Field::req(NETWORK, "network", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(CLOCK_ID, "clock.clock_id", U64),
            Field::req(HOST_BOOT_ID, "clock.host_boot_id", Text),
            Field::req(CLOCK_DOMAIN, "clock.domain", Text),
            Field::req(HANDOFF_BOUNDARY, "handoff_boundary", Text),
            Field::req(LAUNCHES, "launches", U64),
            Field::req(LAUNCHES_OBSERVED, "launches_observed", U64),
            Field::req(LAUNCHES_MISSED, "launches_missed", U64),
            Field::rep(MISSED_TX_HASHES, "missed_tx_hashes", B256),
            Field::rep(
                EXCLUDED_OBSERVED_BEFORE_HANDOFF,
                "launches_excluded_observed_before_handoff",
                B256,
            ),
            Field::nested(
                HANDOFF_TO_EARLIEST_PACKET_DECODE,
                "handoff_to_earliest_packet_decode",
                Presence::Required,
                STATS,
            ),
            Field::nested(
                HANDOFF_TO_EARLIEST_T_INGRESS,
                "handoff_to_earliest_T_ingress",
                Presence::Required,
                STATS,
            ),
            Field::nested(
                BROADCAST_VS_POOLED,
                "broadcast_vs_pooled",
                Presence::Repeated,
                &source_kind::SCHEMA,
            ),
            Field::nested(
                PER_SOURCE_AND_PEER,
                "per_source_and_peer",
                Presence::Repeated,
                &source_peer::SCHEMA,
            ),
            Field::nested(
                PER_LAUNCH,
                "per_launch",
                Presence::Repeated,
                &launch::SCHEMA,
            ),
            Field::req(GATE, "gate", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
            Field::req(LAUNCHES_SHA256, "inputs.launches_sha256", B256),
            Field::req(OBSERVATIONS_SHA256, "inputs.observations_sha256", B256),
            Field::req(CAPTURE_SHA256, "inputs.capture_sha256", B256),
            Field::req(ANALYZER_SHA256, "inputs.analyzer_sha256", B256),
            Field::req(CAPTURE_ROWS, "capture.rows", U64),
        ],
    };
}

/// C1.10 input: what the freeze binds and the local gate acceptances. Built
/// with `c1_freeze inputs ...`, never edited by hand.
pub mod freeze_inputs {
    use super::*;

    /// One artifact path; single roles occur once, list roles repeat.
    pub mod artifact {
        use super::*;

        pub const ROLE: u16 = 1;
        /// Relative to the repository root.
        pub const PATH: u16 = 2;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.freeze-artifact",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(ROLE, "role", Text),
                Field::req(PATH, "path", Text),
            ],
        };
    }

    /// One gate's statement of local acceptance.
    pub mod gate {
        use super::*;

        pub const GATE: u16 = 1;
        /// `PASS` only after local acceptance.
        pub const VERDICT: u16 = 2;
        pub const ACCEPTED_BY: u16 = 3;
        pub const ACCEPTED_AT_UNIX_MS: u16 = 4;
        pub const STATEMENT: u16 = 5;
        pub const EVIDENCE: u16 = 6;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.freeze-gate",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(GATE, "gate", Text),
                Field::req(VERDICT, "verdict", Text),
                Field::opt(ACCEPTED_BY, "accepted_by", Text),
                Field::opt(ACCEPTED_AT_UNIX_MS, "accepted_at_unix_ms", U64),
                Field::opt(STATEMENT, "statement", Text).max(16 * 1024),
                Field::rep(EVIDENCE, "evidence", Text),
            ],
        };
    }

    pub const PINNED_GETH_COMMIT: u16 = 1;
    pub const GETH_CHECKOUT: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// The exact final committed DarkBot HEAD.
    pub const DARKBOT_COMMIT: u16 = 5;
    /// The source binding of that commit.
    pub const FINAL_C1_SOURCE_SHA256: u16 = 6;
    pub const ARTIFACTS: u16 = 7;
    pub const RAW_MEASUREMENT_EVIDENCE: u16 = 8;
    pub const GATES: u16 = 9;
    pub const NON_CLAIMS: u16 = 10;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.freeze-inputs.v1",
        record_type: 48,
        schema_version: 1,
        fields: &[
            Field::req(PINNED_GETH_COMMIT, "pinned_geth_commit", Text),
            Field::req(GETH_CHECKOUT, "geth_checkout", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(DARKBOT_COMMIT, "darkbot_commit", Text),
            Field::opt(FINAL_C1_SOURCE_SHA256, "final_c1_source_sha256", B256),
            Field::nested(
                ARTIFACTS,
                "artifacts",
                Presence::Repeated,
                &artifact::SCHEMA,
            ),
            Field::rep(RAW_MEASUREMENT_EVIDENCE, "raw_measurement_evidence", Text),
            Field::nested(GATES, "gates", Presence::Repeated, &gate::SCHEMA),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

/// C1.10: everything a freeze binds. Its canonical encoding as a record of
/// [`freeze_bindings::SCHEMA`] is what `bindings_sha256` hashes; the freeze
/// record carries the same fields nested ([`freeze_bindings::NESTED`]).
pub mod freeze_bindings {
    use super::*;

    /// The tracked native source tree, compared byte for byte with HEAD.
    pub mod source {
        use super::*;

        pub const ALGORITHM: u16 = 1;
        pub const SCOPE: u16 = 2;
        pub const FILES: u16 = 3;
        pub const SHA256: u16 = 4;
        pub const HEAD_SHA256: u16 = 5;
        /// `clean` or `dirty`.
        pub const WORKING_TREE: u16 = 6;
        /// The start of `git status --porcelain=v1 --untracked-files=all`.
        pub const GIT_STATUS: u16 = 7;
        pub const GIT_STATUS_SHA256: u16 = 8;
        pub const GIT_STATUS_ENTRIES: u16 = 9;
        pub const GIT_STATUS_TRUNCATED: u16 = 10;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.freeze-source",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(ALGORITHM, "algorithm", Text),
                Field::req(SCOPE, "scope", Text),
                Field::req(FILES, "files", U64),
                Field::req(SHA256, "sha256", B256),
                Field::req(HEAD_SHA256, "head_sha256", B256),
                Field::req(WORKING_TREE, "working_tree", Text),
                Field::req(GIT_STATUS, "git_status", Text).max(64 * 1024),
                Field::req(GIT_STATUS_SHA256, "git_status_sha256", B256),
                Field::req(GIT_STATUS_ENTRIES, "git_status_entries", U64),
                Field::req(GIT_STATUS_TRUNCATED, "git_status_truncated", Bool),
            ],
        };
    }

    /// One bound file or directory; no hash: it could not be bound.
    pub mod file {
        use super::*;

        pub const ROLE: u16 = 1;
        pub const PATH: u16 = 2;
        pub const SHA256: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.freeze-file",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(ROLE, "role", Text),
                Field::opt(PATH, "path", Text),
                Field::opt(SHA256, "sha256", B256),
            ],
        };
    }

    /// One gate as accepted, with its bound evidence.
    pub mod gate {
        use super::*;

        pub const GATE: u16 = 1;
        pub const VERDICT: u16 = 2;
        pub const ACCEPTED_BY: u16 = 3;
        pub const ACCEPTED_AT_UNIX_MS: u16 = 4;
        pub const STATEMENT: u16 = 5;
        pub const EVIDENCE: u16 = 6;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.c1.freeze-gate-binding",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(GATE, "gate", Text),
                Field::opt(VERDICT, "verdict", Text),
                Field::opt(ACCEPTED_BY, "accepted_by", Text),
                Field::opt(ACCEPTED_AT_UNIX_MS, "accepted_at_unix_ms", U64),
                Field::opt(STATEMENT, "statement", Text).max(16 * 1024),
                Field::nested(EVIDENCE, "evidence", Presence::Repeated, &file::SCHEMA),
            ],
        };
    }

    pub const PINNED_GETH_COMMIT: u16 = 1;
    pub const DARKBOT_COMMIT: u16 = 2;
    pub const FINAL_C1_SOURCE: u16 = 3;
    pub const ARTIFACTS: u16 = 4;
    pub const PRIOR_GATE_EVIDENCE: u16 = 5;
    pub const RAW_MEASUREMENT_EVIDENCE: u16 = 6;
    pub const GATES: u16 = 7;
    pub const NON_CLAIMS: u16 = 8;

    const FIELDS: &[Field] = &[
        Field::req(PINNED_GETH_COMMIT, "pinned_geth_commit", Text),
        Field::opt(DARKBOT_COMMIT, "darkbot_commit", Text),
        Field::nested(
            FINAL_C1_SOURCE,
            "final_c1_source",
            Presence::Optional,
            &source::SCHEMA,
        ),
        Field::nested(ARTIFACTS, "artifacts", Presence::Repeated, &file::SCHEMA),
        Field::nested(
            PRIOR_GATE_EVIDENCE,
            "prior_gate_evidence",
            Presence::Repeated,
            &file::SCHEMA,
        ),
        Field::nested(
            RAW_MEASUREMENT_EVIDENCE,
            "raw_measurement_evidence",
            Presence::Repeated,
            &file::SCHEMA,
        ),
        Field::nested(GATES, "gates", Presence::Repeated, &gate::SCHEMA),
        Field::rep(NON_CLAIMS, "non_claims", Text),
    ];

    /// The bindings as a record of their own: the preimage of `bindings_sha256`.
    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.freeze-bindings.v1",
        record_type: 49,
        schema_version: 1,
        fields: FIELDS,
    };

    /// The same fields inside a freeze record.
    pub const NESTED: Schema = Schema {
        name: "darkbot.c1.freeze-bindings",
        record_type: 0,
        schema_version: 1,
        fields: FIELDS,
    };
}

/// C1.10: the freeze evaluation. `IMPLEMENTED_TESTED/FROZEN` only when the
/// person performing the freeze asked for it and nothing is missing.
pub mod freeze_record {
    use super::*;

    /// `INCOMPLETE`, `READY_TO_FREEZE` or `IMPLEMENTED_TESTED/FROZEN`.
    pub const STATUS: u16 = 1;
    /// Every reason the freeze is incomplete.
    pub const MISSING: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const BINDINGS: u16 = 5;
    /// SHA-256 of the bindings encoded as a `freeze_bindings` record.
    pub const BINDINGS_SHA256: u16 = 6;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c1.freeze-record.v1",
        record_type: 50,
        schema_version: 1,
        fields: &[
            Field::req(STATUS, "status", Text),
            Field::rep(MISSING, "missing", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::nested(
                BINDINGS,
                "bindings",
                Presence::Required,
                &freeze_bindings::NESTED,
            )
            .max(8 * 1024 * 1024),
            Field::req(BINDINGS_SHA256, "bindings_sha256", B256),
        ],
    };
}

/// Fixed-width raw sample tables (`DBTAB001`): the raw per-sample evidence
/// of a run, too large for one record. Little-endian throughout:
///
/// ```text
/// "DBTAB001" | u16 format version | u16 table id | u32 row width | u64 row count
/// row count x row width bytes
/// SHA-256 over every preceding byte
/// ```
///
/// Reserved bytes are zero and checked; anything else fails closed.
pub mod table {
    use crate::RecordError;

    pub const MAGIC: [u8; 8] = *b"DBTAB001";
    pub const FORMAT_VERSION: u16 = 1;
    const HEADER_LEN: usize = 24;
    const TRAILER_LEN: usize = 32;

    /// One table layout.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Spec {
        pub id: u16,
        pub name: &'static str,
        pub row_width: usize,
    }

    pub const C19_INGRESS: Spec = Spec {
        id: 1,
        name: "darkbot.c1.c19-ingress-sample",
        row_width: IngressSample::WIDTH,
    };
    pub const C19_OUTBOUND: Spec = Spec {
        id: 2,
        name: "darkbot.c1.c19-outbound-evidence",
        row_width: OutboundEvidence::WIDTH,
    };
    pub const C19B_OBSERVATION: Spec = Spec {
        id: 3,
        name: "darkbot.c1.c19b-observation",
        row_width: Observation::WIDTH,
    };

    fn fail<T>(spec: &Spec, message: impl std::fmt::Display) -> Result<T, RecordError> {
        Err(RecordError(format!("{}: {message}", spec.name)))
    }

    /// The table file of `rows` (each exactly `spec.row_width` bytes, concatenated).
    pub fn encode(spec: &Spec, rows: &[u8]) -> Result<Vec<u8>, RecordError> {
        if spec.row_width == 0 || !rows.len().is_multiple_of(spec.row_width) {
            return fail(spec, "rows are not whole rows");
        }
        let width = u32::try_from(spec.row_width).or_else(|_| fail(spec, "row too wide"))?;
        let count = (rows.len() / spec.row_width) as u64;
        let mut out = Vec::with_capacity(HEADER_LEN + rows.len() + TRAILER_LEN);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&spec.id.to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(rows);
        let digest = crate::sha256(&out);
        out.extend_from_slice(&digest);
        Ok(out)
    }

    /// The rows of a table file of `spec`, after checking magic, version,
    /// table id, row width, length and checksum.
    pub fn rows<'a>(
        spec: &Spec,
        bytes: &'a [u8],
    ) -> Result<std::slice::ChunksExact<'a, u8>, RecordError> {
        if bytes.len() < HEADER_LEN + TRAILER_LEN {
            return fail(spec, "table truncated");
        }
        if bytes[..8] != MAGIC {
            return fail(spec, "not a DBTAB001 table");
        }
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let version = u16_at(8);
        if version != FORMAT_VERSION {
            return fail(spec, format!("unsupported table format version {version}"));
        }
        let id = u16_at(10);
        if id != spec.id {
            return fail(spec, format!("table id {id}, expected {}", spec.id));
        }
        let width = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        if width != spec.row_width {
            return fail(
                spec,
                format!("row width {width}, expected {}", spec.row_width),
            );
        }
        let mut count = [0u8; 8];
        count.copy_from_slice(&bytes[16..24]);
        let count = u64::from_le_bytes(count);
        let body = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(width))
            .and_then(|body| body.checked_add(HEADER_LEN + TRAILER_LEN));
        if body != Some(bytes.len()) {
            return fail(
                spec,
                format!("{} bytes do not hold {count} rows", bytes.len()),
            );
        }
        let end = bytes.len() - TRAILER_LEN;
        if crate::sha256(&bytes[..end]) != bytes[end..] {
            return fail(spec, "checksum mismatch");
        }
        Ok(bytes[HEADER_LEN..end].chunks_exact(width))
    }

    fn u64_at(row: &[u8], at: usize) -> u64 {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(&row[at..at + 8]);
        u64::from_le_bytes(raw)
    }

    fn array<const N: usize>(row: &[u8], at: usize) -> [u8; N] {
        let mut out = [0u8; N];
        out.copy_from_slice(&row[at..at + N]);
        out
    }

    fn zero(spec: &Spec, row: &[u8], range: std::ops::Range<usize>) -> Result<(), RecordError> {
        if row[range].iter().any(|byte| *byte != 0) {
            return fail(spec, "reserved bytes are not zero");
        }
        Ok(())
    }

    fn flag(spec: &Spec, byte: u8, what: &str) -> Result<bool, RecordError> {
        match byte {
            0 => Ok(false),
            1 => Ok(true),
            other => fail(spec, format!("{what} byte {other}")),
        }
    }

    /// C1.9 ingress: one launch record as HOT0 consumed it (96 bytes).
    ///
    /// ```text
    /// 0 u8 qualification (0 = warmup) | 1 reserved | 2 u16 lane id | 4 reserved u32
    /// 8 u64 lane sequence | 16 tx hash[32] | 48 u64 T_packet_decoded | 56 u64 T_ingress
    /// 64 u64 L1 pre-commit | 72 u64 L1 post-commit | 80 u64 L2 acquire | 88 u64 queue depth
    /// ```
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct IngressSample {
        /// False for a warmup sample.
        pub qualification: bool,
        pub lane_id: u16,
        pub lane_sequence: u64,
        pub tx_hash: [u8; 32],
        pub t_packet_decoded_ns: u64,
        pub t_ingress_ns: u64,
        pub l1_pre_commit_ns: u64,
        pub l1_post_commit_ns: u64,
        pub l2_acquire_ns: u64,
        pub queue_depth: u64,
    }

    impl IngressSample {
        pub const WIDTH: usize = 96;

        pub fn write(&self, out: &mut Vec<u8>) {
            out.push(u8::from(self.qualification));
            out.push(0);
            out.extend_from_slice(&self.lane_id.to_le_bytes());
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&self.lane_sequence.to_le_bytes());
            out.extend_from_slice(&self.tx_hash);
            for value in [
                self.t_packet_decoded_ns,
                self.t_ingress_ns,
                self.l1_pre_commit_ns,
                self.l1_post_commit_ns,
                self.l2_acquire_ns,
                self.queue_depth,
            ] {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }

        pub fn read(row: &[u8]) -> Result<Self, RecordError> {
            let spec = &C19_INGRESS;
            if row.len() != Self::WIDTH {
                return fail(spec, "row width");
            }
            zero(spec, row, 1..2)?;
            zero(spec, row, 4..8)?;
            Ok(Self {
                qualification: flag(spec, row[0], "phase")?,
                lane_id: u16::from_le_bytes([row[2], row[3]]),
                lane_sequence: u64_at(row, 8),
                tx_hash: array(row, 16),
                t_packet_decoded_ns: u64_at(row, 48),
                t_ingress_ns: u64_at(row, 56),
                l1_pre_commit_ns: u64_at(row, 64),
                l1_post_commit_ns: u64_at(row, 72),
                l2_acquire_ns: u64_at(row, 80),
                queue_depth: u64_at(row, 88),
            })
        }
    }

    /// C1.9 outbound: one decoded handoff evidence record and the egress
    /// commit time of its BUY (120 bytes).
    ///
    /// ```text
    /// 0 tx hash[32] | 32 peer[32] | 64 u64 egress sequence | 72 u64 egress commit
    /// 80 u64 coordinator observed | 88 u64 third | 96 u64 event | 104 u64 evidence lane sequence
    /// 112 u16 evidence lane id | 114 u8 origin (0 coordinator, 1 worker) | 115 u8 status (wire code)
    /// 116 u8 worker present | 117 u8 worker | 118 u8 selected | 119 u8 queued
    /// ```
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct OutboundEvidence {
        pub tx_hash: [u8; 32],
        pub peer: [u8; 32],
        pub egress_sequence: u64,
        pub egress_commit_ns: u64,
        pub coordinator_observed_ns: u64,
        pub third_ns: u64,
        pub event_ns: u64,
        pub evidence_lane_sequence: u64,
        pub evidence_lane_id: u16,
        /// False: the coordinator's record.
        pub from_worker: bool,
        /// The Fast-Lane outbound status wire code.
        pub status: u8,
        pub worker: Option<u8>,
        pub selected: u8,
        pub queued: u8,
    }

    impl OutboundEvidence {
        pub const WIDTH: usize = 120;

        pub fn write(&self, out: &mut Vec<u8>) {
            out.extend_from_slice(&self.tx_hash);
            out.extend_from_slice(&self.peer);
            for value in [
                self.egress_sequence,
                self.egress_commit_ns,
                self.coordinator_observed_ns,
                self.third_ns,
                self.event_ns,
                self.evidence_lane_sequence,
            ] {
                out.extend_from_slice(&value.to_le_bytes());
            }
            out.extend_from_slice(&self.evidence_lane_id.to_le_bytes());
            out.push(u8::from(self.from_worker));
            out.push(self.status);
            out.push(u8::from(self.worker.is_some()));
            out.push(self.worker.unwrap_or(0));
            out.push(self.selected);
            out.push(self.queued);
        }

        pub fn read(row: &[u8]) -> Result<Self, RecordError> {
            let spec = &C19_OUTBOUND;
            if row.len() != Self::WIDTH {
                return fail(spec, "row width");
            }
            let worker = match flag(spec, row[116], "worker present")? {
                true => Some(row[117]),
                false => {
                    zero(spec, row, 117..118)?;
                    None
                }
            };
            Ok(Self {
                tx_hash: array(row, 0),
                peer: array(row, 32),
                egress_sequence: u64_at(row, 64),
                egress_commit_ns: u64_at(row, 72),
                coordinator_observed_ns: u64_at(row, 80),
                third_ns: u64_at(row, 88),
                event_ns: u64_at(row, 96),
                evidence_lane_sequence: u64_at(row, 104),
                evidence_lane_id: u16::from_le_bytes([row[112], row[113]]),
                from_worker: flag(spec, row[114], "origin")?,
                status: row[115],
                worker,
                selected: row[118],
                queued: row[119],
            })
        }
    }

    /// C1.9b: one EARLY_TX_SEEN record of a peer lane (120 bytes).
    ///
    /// ```text
    /// 0 tx hash[32] | 32 peer[32] | 64 source[16] (NUL-padded) | 80 u64 T_packet_decoded
    /// 88 u64 T_ingress | 96 u64 lane epoch | 104 u64 lane sequence | 112 u32 clock id
    /// 116 u16 lane id | 118 reserved u16
    /// ```
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Observation {
        pub tx_hash: [u8; 32],
        pub peer: [u8; 32],
        pub source: [u8; 16],
        pub t_packet_decoded_ns: u64,
        pub t_ingress_ns: u64,
        pub lane_epoch: u64,
        pub lane_sequence: u64,
        pub clock_id: u32,
        pub lane_id: u16,
    }

    impl Observation {
        pub const WIDTH: usize = 120;

        pub fn write(&self, out: &mut Vec<u8>) {
            out.extend_from_slice(&self.tx_hash);
            out.extend_from_slice(&self.peer);
            out.extend_from_slice(&self.source);
            for value in [
                self.t_packet_decoded_ns,
                self.t_ingress_ns,
                self.lane_epoch,
                self.lane_sequence,
            ] {
                out.extend_from_slice(&value.to_le_bytes());
            }
            out.extend_from_slice(&self.clock_id.to_le_bytes());
            out.extend_from_slice(&self.lane_id.to_le_bytes());
            out.extend_from_slice(&[0; 2]);
        }

        pub fn read(row: &[u8]) -> Result<Self, RecordError> {
            let spec = &C19B_OBSERVATION;
            if row.len() != Self::WIDTH {
                return fail(spec, "row width");
            }
            zero(spec, row, 118..120)?;
            Ok(Self {
                tx_hash: array(row, 0),
                peer: array(row, 32),
                source: array(row, 64),
                t_packet_decoded_ns: u64_at(row, 80),
                t_ingress_ns: u64_at(row, 88),
                lane_epoch: u64_at(row, 96),
                lane_sequence: u64_at(row, 104),
                clock_id: u32::from_le_bytes([row[112], row[113], row[114], row[115]]),
                lane_id: u16::from_le_bytes([row[116], row[117]]),
            })
        }

        /// The source name: the bytes before the first NUL, which must be
        /// UTF-8 and followed only by NULs.
        pub fn source_name(&self) -> Result<&str, RecordError> {
            let end = self.source.iter().position(|b| *b == 0).unwrap_or(16);
            if self.source[end..].iter().any(|b| *b != 0) {
                return fail(&C19B_OBSERVATION, "source is not NUL-padded");
            }
            std::str::from_utf8(&self.source[..end])
                .or_else(|_| fail(&C19B_OBSERVATION, "source is not UTF-8"))
        }
    }

    /// Reads every row of a table with `read`.
    pub fn read_all<T>(
        spec: &Spec,
        bytes: &[u8],
        read: impl Fn(&[u8]) -> Result<T, RecordError>,
    ) -> Result<Vec<T>, RecordError> {
        rows(spec, bytes)?.map(read).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn sample(lane_sequence: u64) -> IngressSample {
            IngressSample {
                qualification: lane_sequence > 0,
                lane_id: 17,
                lane_sequence,
                tx_hash: [0xab; 32],
                t_packet_decoded_ns: 1,
                t_ingress_ns: 2,
                l1_pre_commit_ns: 3,
                l1_post_commit_ns: 4,
                l2_acquire_ns: 5,
                queue_depth: 6,
            }
        }

        #[test]
        fn tables_round_trip_and_fail_closed() {
            let mut rows = Vec::new();
            sample(0).write(&mut rows);
            sample(1).write(&mut rows);
            let file = encode(&C19_INGRESS, &rows).unwrap();
            assert_eq!(file.len(), 24 + 2 * 96 + 32);
            let back = read_all(&C19_INGRESS, &file, IngressSample::read).unwrap();
            assert_eq!(back, vec![sample(0), sample(1)]);
            // Another table, a flipped byte, a truncation, a bad flag.
            assert!(rows_err(&C19_OUTBOUND, &file).contains("table id 1"));
            let mut flipped = file.clone();
            flipped[40] ^= 1;
            assert!(rows_err(&C19_INGRESS, &flipped).contains("checksum"));
            assert!(rows_err(&C19_INGRESS, &file[..file.len() - 1]).contains("do not hold"));
            let mut phase = rows.clone();
            phase[0] = 2;
            let bad = encode(&C19_INGRESS, &phase).unwrap();
            let e = read_all(&C19_INGRESS, &bad, IngressSample::read).unwrap_err();
            assert!(e.0.contains("phase byte 2"), "{e}");
            assert!(encode(&C19_INGRESS, &rows[..95]).is_err());

            let outbound = OutboundEvidence {
                tx_hash: [1; 32],
                peer: [2; 32],
                egress_sequence: 3,
                egress_commit_ns: 4,
                coordinator_observed_ns: 5,
                third_ns: 6,
                event_ns: 7,
                evidence_lane_sequence: 8,
                evidence_lane_id: 9,
                from_worker: true,
                status: 1,
                worker: Some(0),
                selected: 2,
                queued: 2,
            };
            let mut row = Vec::new();
            outbound.write(&mut row);
            assert_eq!(OutboundEvidence::read(&row).unwrap(), outbound);
            let coordinator = OutboundEvidence {
                worker: None,
                from_worker: false,
                ..outbound
            };
            let mut row = Vec::new();
            coordinator.write(&mut row);
            assert_eq!(OutboundEvidence::read(&row).unwrap(), coordinator);

            let mut source = [0u8; 16];
            source[..10].copy_from_slice(b"eth-pooled");
            let seen = Observation {
                tx_hash: [3; 32],
                peer: [4; 32],
                source,
                t_packet_decoded_ns: 10,
                t_ingress_ns: 20,
                lane_epoch: 1,
                lane_sequence: 2,
                clock_id: 1,
                lane_id: 18,
            };
            let mut row = Vec::new();
            seen.write(&mut row);
            let back = Observation::read(&row).unwrap();
            assert_eq!(back, seen);
            assert_eq!(back.source_name().unwrap(), "eth-pooled");
        }

        fn rows_err(spec: &Spec, bytes: &[u8]) -> String {
            match rows(spec, bytes) {
                Ok(_) => String::from("accepted"),
                Err(e) => e.0,
            }
        }
    }
}

pub const ALL: &[&Schema] = &[
    &host_profile::SCHEMA,
    &c19_rust_summary::SCHEMA,
    &native_latency::SCHEMA,
    &c19b_capture::SCHEMA,
    &c19b_launches::SCHEMA,
    &hook_path::SCHEMA,
    &freeze_inputs::SCHEMA,
    &freeze_bindings::SCHEMA,
    &freeze_record::SCHEMA,
];
