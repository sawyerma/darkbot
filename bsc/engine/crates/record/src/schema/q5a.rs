//! REV.1.0 R8 (BSC-048): the Q5A real TLS transport qualification as a
//! record, written by `u5_tls_qualify`. The historical JSON evidence in
//! `bsc/native/evidence/q5a/*.json` (pinned by its SHA-256 as the
//! transport's qualification identity) stays as it was written and is
//! never read by these schemas.
//!
//! Evidence never contains a request, a header or a credential: per sample
//! the plaintext length of the request and the length and SHA-256 of its
//! body, which carries no secret.

use super::{PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL};
use crate::{Field, Kind::*, Presence, Schema};

/// The facts of one TLS connection.
pub mod connection {
    use super::*;

    pub const GENERATION: u16 = 1;
    pub const PEER: u16 = 2;
    pub const TLS_VERSION: u16 = 3;
    pub const CIPHER_SUITE: u16 = 4;
    pub const HTTP: u16 = 5;
    pub const ALPN_SELECTED: u16 = 6;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.q5a.connection",
        record_type: 0,
        schema_version: 1,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::req(PEER, "peer", Text),
            Field::req(TLS_VERSION, "tls_version", Text),
            Field::req(CIPHER_SUITE, "cipher_suite", Text),
            Field::req(HTTP, "http", Text),
            Field::req(ALPN_SELECTED, "alpn_selected", Bool),
        ],
    };
}

/// One request of a run: what became of it and its raw timestamps.
pub mod sample {
    use super::*;

    /// `warmup`, `measured` or `reconnect`.
    pub const PHASE: u16 = 1;
    pub const INDEX: u16 = 2;
    pub const GENERATION: u16 = 3;
    pub const PEER: u16 = 4;
    pub const STATE_BEFORE: u16 = 5;
    pub const REQUEST_PLAINTEXT_BYTES: u16 = 6;
    pub const BODY_BYTES: u16 = 7;
    pub const BODY_SHA256: u16 = 8;
    /// `handed_off`, `skipped` or `unknown`.
    pub const OUTCOME: u16 = 9;
    /// A skipped sample's transport state.
    pub const OUTCOME_STATE: u16 = 10;
    /// An unknown sample's failing step and its error kind.
    pub const OUTCOME_STAGE: u16 = 11;
    pub const OUTCOME_ERROR: u16 = 12;
    pub const PREPARE_NS: u16 = 13;
    pub const T_WRITE_CHECK_NS: u16 = 14;
    pub const T_BOUNDARY_NS: u16 = 15;
    pub const WRITE_CHECK_TO_BOUNDARY_NS: u16 = 16;
    pub const WIRE_BYTES: u16 = 17;
    /// The response class, when one was awaited.
    pub const RESPONSE: u16 = 18;
    pub const RESPONSE_REJECT_CODE: u16 = 19;
    pub const RESPONSE_HTTP_STATUS: u16 = 20;
    pub const RESPONSE_MALFORMED: u16 = 21;
    pub const RESPONSE_ANSWERED: u16 = 22;
    pub const RESPONSE_LATENCY_NS: u16 = 23;
    pub const STATE_AFTER: u16 = 24;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.q5a.sample",
        record_type: 0,
        schema_version: 1,
        fields: &[
            Field::req(PHASE, "phase", Text),
            Field::req(INDEX, "index", U64),
            Field::opt(GENERATION, "generation", U64),
            Field::opt(PEER, "peer", Text),
            Field::req(STATE_BEFORE, "state_before", Text),
            Field::req(REQUEST_PLAINTEXT_BYTES, "request_plaintext_bytes", U64),
            Field::req(BODY_BYTES, "body_bytes", U64),
            Field::req(BODY_SHA256, "body_sha256", B256),
            Field::req(OUTCOME, "outcome", Text),
            Field::opt(OUTCOME_STATE, "outcome.state", Text),
            Field::opt(OUTCOME_STAGE, "outcome.stage", Text),
            Field::opt(OUTCOME_ERROR, "outcome.error", Text),
            Field::opt(PREPARE_NS, "prepare_ns", U64),
            Field::opt(T_WRITE_CHECK_NS, "t_write_check_ns", U64),
            Field::opt(T_BOUNDARY_NS, "t_boundary_ns", U64),
            Field::opt(
                WRITE_CHECK_TO_BOUNDARY_NS,
                "write_check_to_boundary_ns",
                U64,
            ),
            Field::opt(WIRE_BYTES, "wire_bytes", U64),
            Field::opt(RESPONSE, "response", Text),
            Field::opt(RESPONSE_REJECT_CODE, "response.reject_code", I64),
            Field::opt(RESPONSE_HTTP_STATUS, "response.http_status", U64),
            Field::opt(RESPONSE_MALFORMED, "response.malformed", Text),
            Field::opt(RESPONSE_ANSWERED, "response.answered", Bool),
            Field::opt(RESPONSE_LATENCY_NS, "response_latency_ns", U64),
            Field::req(STATE_AFTER, "state_after", Text),
        ],
    };
}

/// One explicit reconnect after the measured series.
pub mod reconnect {
    use super::*;

    pub const CYCLE: u16 = 1;
    pub const PREVIOUS_GENERATION: u16 = 2;
    /// The new connection, or why there is none.
    pub const CONNECTION: u16 = 3;
    pub const ERROR: u16 = 4;
    pub const PREVIOUS_STILL_CURRENT: u16 = 5;
    pub const PROBE: u16 = 6;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.q5a.reconnect",
        record_type: 0,
        schema_version: 1,
        fields: &[
            Field::req(CYCLE, "cycle", U64),
            Field::opt(PREVIOUS_GENERATION, "previous_generation", U64),
            Field::nested(
                CONNECTION,
                "connection",
                Presence::Optional,
                &connection::SCHEMA,
            ),
            Field::opt(ERROR, "error", Text),
            Field::req(PREVIOUS_STILL_CURRENT, "previous_still_current", Bool),
            Field::nested(PROBE, "probe", Presence::Optional, &sample::SCHEMA),
        ],
    };
}

/// One reason a provider's transport is not a pass, with its detail.
pub mod reason {
    use super::*;

    pub const NAME: u16 = 1;
    /// It fails the transport, rather than leaving it incomplete.
    pub const FAILS: u16 = 2;
    /// The identity field it names.
    pub const FIELD: u16 = 3;
    /// Inconsistent evidence: how many samples, the first one and its issue.
    pub const SAMPLES: u16 = 4;
    pub const FIRST_PHASE: u16 = 5;
    pub const FIRST_INDEX: u16 = 6;
    pub const FIRST_ISSUE: u16 = 7;
    pub const EXPECTED: u16 = 8;
    pub const RECORDED: u16 = 9;
    pub const COUNT: u16 = 10;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.q5a.reason",
        record_type: 0,
        schema_version: 1,
        fields: &[
            Field::req(NAME, "reason", Text),
            Field::req(FAILS, "fails", Bool),
            Field::opt(FIELD, "field", Text),
            Field::opt(SAMPLES, "samples", U64),
            Field::opt(FIRST_PHASE, "first_phase", Text),
            Field::opt(FIRST_INDEX, "first_index", U64),
            Field::opt(FIRST_ISSUE, "first_issue", Text),
            Field::opt(EXPECTED, "expected", U64),
            Field::opt(RECORDED, "recorded", U64),
            Field::opt(COUNT, "count", U64),
        ],
    };
}

/// Everything one provider's run recorded, and its verdict.
pub mod provider {
    use super::*;

    pub const PROVIDER: u16 = 1;
    pub const HOST: u16 = 2;
    pub const PORT: u16 = 3;
    pub const AUTH_SCOPE: u16 = 4;
    pub const CONNECT_TIMEOUT_MS: u16 = 5;
    pub const HANDSHAKE_TIMEOUT_MS: u16 = 6;
    pub const MAX_IDLE_MS: u16 = 7;
    pub const RESPONSE_TIMEOUT_MS: u16 = 8;
    pub const MAX_PLAINTEXT_REQUEST_LEN: u16 = 9;
    pub const WARMUP: u16 = 10;
    pub const MEASURED: u16 = 11;
    pub const PACING_MS: u16 = 12;
    pub const RECONNECT_CYCLES: u16 = 13;
    /// Wall-clock start and end, RFC 3339.
    pub const STARTED_AT: u16 = 14;
    pub const ENDED_AT: u16 = 15;
    /// The initial connection, or why there is none.
    pub const CONNECTION: u16 = 16;
    pub const CONNECT_ERROR: u16 = 17;
    pub const SAMPLES: u16 = 18;
    pub const RECONNECTS: u16 = 19;
    /// The measured handoffs' summary (nearest-rank; the fraction of
    /// misses is `at_or_above_limit / count`).
    pub const SUMMARY_COUNT: u16 = 20;
    pub const P50_NS: u16 = 21;
    pub const P95_NS: u16 = 22;
    pub const P99_NS: u16 = 23;
    pub const MAX_NS: u16 = 24;
    pub const LIMIT_NS: u16 = 25;
    pub const AT_OR_ABOVE_LIMIT: u16 = 26;
    pub const HANDED_OFF: u16 = 27;
    pub const SKIPPED: u16 = 28;
    pub const UNKNOWN: u16 = 29;
    pub const STALE_AFTER: u16 = 30;
    pub const DISCONNECTED_AFTER: u16 = 31;
    pub const API_SCOPE: u16 = 32;
    pub const TRANSPORT_VERDICT: u16 = 33;
    pub const REASONS: u16 = 34;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.q5a.provider",
        record_type: 0,
        schema_version: 1,
        fields: &[
            Field::req(PROVIDER, "provider", Text),
            Field::req(HOST, "host", Text),
            Field::req(PORT, "port", U64),
            Field::req(AUTH_SCOPE, "auth_scope", Text),
            Field::req(CONNECT_TIMEOUT_MS, "policy.connect_timeout_ms", U64),
            Field::req(HANDSHAKE_TIMEOUT_MS, "policy.handshake_timeout_ms", U64),
            Field::req(MAX_IDLE_MS, "policy.max_idle_ms", U64),
            Field::req(RESPONSE_TIMEOUT_MS, "policy.response_timeout_ms", U64),
            Field::req(
                MAX_PLAINTEXT_REQUEST_LEN,
                "policy.max_plaintext_request_len",
                U64,
            ),
            Field::req(WARMUP, "workload.warmup", U64),
            Field::req(MEASURED, "workload.measured", U64),
            Field::req(PACING_MS, "workload.pacing_ms", U64),
            Field::req(RECONNECT_CYCLES, "workload.reconnect_cycles", U64),
            Field::req(STARTED_AT, "started_at", Text),
            Field::req(ENDED_AT, "ended_at", Text),
            Field::nested(
                CONNECTION,
                "connection",
                Presence::Optional,
                &connection::SCHEMA,
            ),
            Field::opt(CONNECT_ERROR, "connect_error", Text),
            Field::nested(SAMPLES, "samples", Presence::Repeated, &sample::SCHEMA),
            Field::nested(
                RECONNECTS,
                "reconnects",
                Presence::Repeated,
                &reconnect::SCHEMA,
            ),
            Field::req(SUMMARY_COUNT, "summary.count", U64),
            Field::opt(P50_NS, "summary.p50_ns", U64),
            Field::opt(P95_NS, "summary.p95_ns", U64),
            Field::opt(P99_NS, "summary.p99_ns", U64),
            Field::opt(MAX_NS, "summary.max_ns", U64),
            Field::req(LIMIT_NS, "summary.limit_ns", U64),
            Field::req(AT_OR_ABOVE_LIMIT, "summary.at_or_above_limit", U64),
            Field::req(HANDED_OFF, "counts.handed_off", U64),
            Field::req(SKIPPED, "counts.skipped", U64),
            Field::req(UNKNOWN, "counts.unknown", U64),
            Field::req(STALE_AFTER, "counts.stale_after", U64),
            Field::req(DISCONNECTED_AFTER, "counts.disconnected_after", U64),
            Field::req(API_SCOPE, "api_scope", Text),
            Field::req(TRANSPORT_VERDICT, "transport_verdict", Text),
            Field::nested(REASONS, "reasons", Presence::Repeated, &reason::SCHEMA),
        ],
    };
}

/// The complete Q5A evidence: the build's identity, the required
/// workload, every provider's run and the overall verdict.
pub mod evidence {
    use super::*;

    pub const PROVENANCE_TOOL: u16 = super::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = super::PROVENANCE_OBSERVED_UNIX_MS;
    pub const BASELINE: u16 = 10;
    pub const GIT_HEAD: u16 = 11;
    pub const GIT_DIRTY: u16 = 12;
    pub const BASELINE_ANCESTOR: u16 = 13;
    pub const FROZEN_SOURCES_UNCHANGED: u16 = 14;
    pub const RUSTC: u16 = 15;
    pub const CARGO: u16 = 16;
    pub const RUSTLS: u16 = 17;
    pub const RING: u16 = 18;
    pub const PROFILE: u16 = 19;
    pub const OS: u16 = 20;
    pub const ARCH: u16 = 21;
    pub const TRANSPORT_IMPLEMENTATION: u16 = 22;
    pub const HARNESS: u16 = 23;
    pub const REQUIRED_WARMUP: u16 = 30;
    pub const REQUIRED_MEASURED: u16 = 31;
    pub const REQUIRED_PACING_MS: u16 = 32;
    pub const REQUIRED_RECONNECT_CYCLES: u16 = 33;
    pub const MISS_LIMIT_NS: u16 = 34;
    pub const REQUIRED_PROVIDERS: u16 = 35;
    pub const PROVIDERS: u16 = 40;
    /// `PASS`, `FAIL` or `INCOMPLETE`.
    pub const VERDICT: u16 = 41;
    /// What this evidence never qualifies, whatever its verdict.
    pub const NOT_QUALIFIED: u16 = 42;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.q5a.tls-transport-evidence.v2",
        record_type: 75,
        schema_version: 1,
        fields: &[
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(BASELINE, "identity.baseline", Text),
            Field::req(GIT_HEAD, "identity.git_head", Text),
            Field::req(GIT_DIRTY, "identity.git_dirty", Text),
            Field::req(BASELINE_ANCESTOR, "identity.baseline_ancestor", Text),
            Field::req(
                FROZEN_SOURCES_UNCHANGED,
                "identity.frozen_sources_unchanged",
                Text,
            ),
            Field::req(RUSTC, "identity.rustc", Text),
            Field::req(CARGO, "identity.cargo", Text),
            Field::req(RUSTLS, "identity.rustls", Text),
            Field::req(RING, "identity.ring", Text),
            Field::req(PROFILE, "identity.profile", Text),
            Field::req(OS, "identity.os", Text),
            Field::req(ARCH, "identity.arch", Text),
            Field::req(
                TRANSPORT_IMPLEMENTATION,
                "identity.transport_implementation",
                Text,
            ),
            Field::req(HARNESS, "identity.harness", Text),
            Field::req(REQUIRED_WARMUP, "required.workload.warmup", U64),
            Field::req(REQUIRED_MEASURED, "required.workload.measured", U64),
            Field::req(REQUIRED_PACING_MS, "required.workload.pacing_ms", U64),
            Field::req(
                REQUIRED_RECONNECT_CYCLES,
                "required.workload.reconnect_cycles",
                U64,
            ),
            Field::req(MISS_LIMIT_NS, "required.miss_limit_ns", U64),
            Field::rep(REQUIRED_PROVIDERS, "required.providers", Text),
            Field::nested(
                PROVIDERS,
                "providers",
                Presence::Repeated,
                &provider::SCHEMA,
            ),
            Field::req(VERDICT, "verdict.tls_transport_qualification", Text),
            Field::rep(NOT_QUALIFIED, "not_qualified_by_this_evidence", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[&evidence::SCHEMA];
