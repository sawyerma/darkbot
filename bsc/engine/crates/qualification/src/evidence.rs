//! What a Q5A run records, and its JSON form.
//!
//! Evidence never contains a request, a header or a credential: per sample
//! it holds the plaintext length of the request and the length and SHA-256
//! of its body, which carries no secret.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use darkbot_bsc_execution::submission::response::ProtocolFailure;
use darkbot_bsc_execution::submission::tls::{ConnectionFacts, TlsPolicy};
use darkbot_bsc_execution::submission::transport::ConnectionState;

use crate::identity::Identity;
use crate::json::Json;
use crate::stats::Summary;
use crate::verdict::{ProviderVerdict, Verdict};

/// Schema of the evidence document.
pub const SCHEMA: &str = "darkbot.bsc.q5a.tls-transport-evidence.v1";

/// A handoff of this duration or longer, from the write check to the
/// boundary, is a miss.
pub const MISS_NS: u64 = 1_000_000;

/// What the provider requires of the run and what it got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthScope {
    /// The provider takes no authorization (48Club).
    NotRequired,
    /// An authorization was configured and sent (its value is not
    /// recorded).
    Present,
    /// The provider needs an authorization and none was configured: only
    /// the TLS handshake is measured.
    Missing,
    /// The configured authorization is not usable: only the TLS handshake
    /// is measured.
    Invalid,
}

impl AuthScope {
    /// The name in evidence.
    pub fn name(self) -> &'static str {
        match self {
            Self::NotRequired => "NOT_REQUIRED",
            Self::Present => "PRESENT",
            Self::Missing => "UNQUALIFIED_AUTH_MISSING",
            Self::Invalid => "UNQUALIFIED_AUTH_INVALID",
        }
    }
}

/// The sequence of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Workload {
    /// Samples before the measured ones, not aggregated.
    pub warmup: u32,
    /// Samples that are aggregated and judged.
    pub measured: u32,
    /// Pause before every sample but the first.
    pub pacing: Duration,
    /// Explicit reconnects after the series, each followed by a probe.
    pub reconnect_cycles: u32,
}

/// The workload of a Q5A qualification. Anything else is not one.
pub const QUALIFICATION_WORKLOAD: Workload = Workload {
    warmup: 10,
    measured: 100,
    pacing: Duration::from_millis(200),
    reconnect_cycles: 3,
};

/// Which part of a run a sample belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Before the measured series.
    Warmup,
    /// The measured series.
    Measured,
    /// The probe after an explicit reconnect.
    Reconnect,
}

impl Phase {
    /// Short tag, used in request ids.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Warmup => "w",
            Self::Measured => "m",
            Self::Reconnect => "r",
        }
    }

    /// The name in evidence.
    pub fn name(self) -> &'static str {
        match self {
            Self::Warmup => "warmup",
            Self::Measured => "measured",
            Self::Reconnect => "reconnect",
        }
    }
}

/// Where a handoff failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// `Transport::prepare`.
    Prepare,
    /// `Transport::write`.
    Write,
    /// `Transport::flush`.
    Flush,
}

/// What became of one sample's request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleOutcome {
    /// Not started: the connection was not ready. Nothing was repaired.
    Skipped(ConnectionState),
    /// Started and failed: whether bytes left is unknown.
    Unknown {
        /// The failing step.
        stage: Stage,
        /// Its error kind.
        error: io::ErrorKind,
    },
    /// Write and flush returned: every ciphertext byte was taken by the
    /// kernel.
    HandedOff,
}

/// What the response to one sample was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseClass {
    /// HTTP 200, the request's id, a bundle hash. Not an inclusion.
    Ack,
    /// HTTP 200, the request's id, a JSON-RPC error: the expected answer
    /// to the probe.
    Reject {
        /// The error's integer code, if any.
        code: Option<i64>,
    },
    /// HTTP 429.
    RateLimited,
    /// A complete HTTP response with another status.
    HttpStatus(u16),
    /// A response that could not be understood, or answered another
    /// request.
    Malformed(ProtocolFailure),
    /// The connection ended or failed while the response was read.
    Connection,
    /// No complete response within the policy's response timeout.
    Timeout,
}

impl ResponseClass {
    /// Whether this is a complete HTTP response, bound to the request: the
    /// connection can carry the next request.
    pub fn answered(self) -> bool {
        matches!(
            self,
            Self::Ack | Self::Reject { .. } | Self::RateLimited | Self::HttpStatus(_)
        )
    }
}

/// One request of a run.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    /// The part of the run.
    pub phase: Phase,
    /// Index within the phase.
    pub index: u32,
    /// Generation of the connection, if there was one.
    pub generation: Option<u64>,
    /// Address connected to, if there was a connection.
    pub peer: Option<SocketAddr>,
    /// State of the transport when the sample began.
    pub state_before: ConnectionState,
    /// Plaintext length of the HTTP request.
    pub request_bytes: usize,
    /// Length of the JSON-RPC body.
    pub body_bytes: usize,
    /// SHA-256 of the body, hex.
    pub body_sha256: String,
    /// What became of the request.
    pub outcome: SampleOutcome,
    /// From before `prepare` to the write check.
    pub prepare_ns: Option<u64>,
    /// The write check: the clock read immediately before the first
    /// `Transport::write`.
    pub t_write_check_ns: Option<u64>,
    /// The clock read after write and flush returned.
    pub t_boundary_ns: Option<u64>,
    /// From the write check to the boundary: the measured handoff.
    pub write_check_to_boundary_ns: Option<u64>,
    /// Bytes the kernel took for this request, TLS records and all.
    pub wire_bytes: Option<u64>,
    /// The response, if one was awaited.
    pub response: Option<ResponseClass>,
    /// From the boundary to the classified response.
    pub response_latency_ns: Option<u64>,
    /// State of the transport when the sample ended.
    pub state_after: ConnectionState,
}

/// One explicit reconnect after the measured series.
#[derive(Clone, Debug, PartialEq)]
pub struct ReconnectCycle {
    /// Index of the cycle.
    pub cycle: u32,
    /// The connection before the reconnect.
    pub previous: Option<ConnectionFacts>,
    /// The new connection, or why there is none.
    pub result: Result<ConnectionFacts, String>,
    /// Whether the owner still took the previous facts as current.
    pub previous_still_current: bool,
    /// The probe over the new connection.
    pub probe: Option<Sample>,
}

/// Everything one provider's run recorded.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderRun {
    /// Provider name: club48 or blockrazor.
    pub provider: &'static str,
    /// DNS name connected to and sent as `Host`.
    pub host: String,
    /// Port.
    pub port: u16,
    /// The authorization scope.
    pub auth: AuthScope,
    /// The transport policy.
    pub policy: TlsPolicy,
    /// The workload run.
    pub workload: Workload,
    /// The initial connection, or why there is none.
    pub connect: Result<ConnectionFacts, String>,
    /// Wall-clock start, RFC 3339.
    pub started_at: String,
    /// Wall-clock end, RFC 3339.
    pub ended_at: String,
    /// Warmup and measured samples, in order.
    pub samples: Vec<Sample>,
    /// The reconnect cycles.
    pub reconnects: Vec<ReconnectCycle>,
}

fn state_name(state: ConnectionState) -> &'static str {
    match state {
        ConnectionState::Ready => "ready",
        ConnectionState::AwaitingResponse => "awaiting_response",
        ConnectionState::NotReady => "not_ready",
        ConnectionState::Stale => "stale",
        ConnectionState::Disconnected => "disconnected",
    }
}

fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Prepare => "prepare",
        Stage::Write => "write",
        Stage::Flush => "flush",
    }
}

fn duration_ms(duration: Duration) -> Json {
    Json::U64(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
}

fn opt_u64(value: Option<u64>) -> Json {
    Json::opt(value, Json::U64)
}

/// JSON of connection facts.
pub fn facts_json(facts: &ConnectionFacts) -> Json {
    Json::object([
        ("generation", Json::U64(facts.generation)),
        ("peer", Json::str(facts.peer.to_string())),
        ("tls_version", Json::str(format!("{:?}", facts.tls_version))),
        (
            "cipher_suite",
            Json::str(format!("{:?}", facts.cipher_suite)),
        ),
        ("http", Json::str(format!("{:?}", facts.http))),
        ("alpn_selected", Json::Bool(facts.alpn_selected)),
    ])
}

fn connect_json(result: &Result<ConnectionFacts, String>) -> Json {
    match result {
        Ok(facts) => Json::object([("ok", Json::Bool(true)), ("facts", facts_json(facts))]),
        Err(error) => Json::object([("ok", Json::Bool(false)), ("error", Json::str(error))]),
    }
}

fn outcome_json(outcome: SampleOutcome) -> Json {
    match outcome {
        SampleOutcome::HandedOff => Json::object([("kind", Json::str("handed_off"))]),
        SampleOutcome::Skipped(state) => Json::object([
            ("kind", Json::str("skipped")),
            ("state", Json::str(state_name(state))),
        ]),
        SampleOutcome::Unknown { stage, error } => Json::object([
            ("kind", Json::str("unknown")),
            ("stage", Json::str(stage_name(stage))),
            ("error", Json::str(format!("{error:?}"))),
        ]),
    }
}

fn response_json(class: ResponseClass) -> Json {
    let (name, detail) = match class {
        ResponseClass::Ack => ("ack", Json::Null),
        ResponseClass::Reject { code } => ("reject", Json::opt(code, Json::I64)),
        ResponseClass::RateLimited => ("rate_limited", Json::Null),
        ResponseClass::HttpStatus(status) => ("http_status", Json::U64(u64::from(status))),
        ResponseClass::Malformed(failure) => ("malformed", Json::str(format!("{failure:?}"))),
        ResponseClass::Connection => ("connection", Json::Null),
        ResponseClass::Timeout => ("timeout", Json::Null),
    };
    Json::object([
        ("class", Json::str(name)),
        ("detail", detail),
        ("answered", Json::Bool(class.answered())),
    ])
}

/// JSON of one sample.
pub fn sample_json(sample: &Sample) -> Json {
    Json::object([
        ("phase", Json::str(sample.phase.name())),
        ("index", Json::U64(u64::from(sample.index))),
        ("generation", opt_u64(sample.generation)),
        (
            "peer",
            Json::opt(sample.peer, |peer| Json::str(peer.to_string())),
        ),
        ("state_before", Json::str(state_name(sample.state_before))),
        (
            "request_plaintext_bytes",
            Json::U64(sample.request_bytes as u64),
        ),
        ("body_bytes", Json::U64(sample.body_bytes as u64)),
        ("body_sha256", Json::str(&sample.body_sha256)),
        ("outcome", outcome_json(sample.outcome)),
        ("prepare_ns", opt_u64(sample.prepare_ns)),
        ("t_write_check_ns", opt_u64(sample.t_write_check_ns)),
        ("t_boundary_ns", opt_u64(sample.t_boundary_ns)),
        (
            "write_check_to_boundary_ns",
            opt_u64(sample.write_check_to_boundary_ns),
        ),
        ("wire_bytes", opt_u64(sample.wire_bytes)),
        ("response", Json::opt(sample.response, response_json)),
        ("response_latency_ns", opt_u64(sample.response_latency_ns)),
        ("state_after", Json::str(state_name(sample.state_after))),
    ])
}

fn reconnect_json(cycle: &ReconnectCycle) -> Json {
    Json::object([
        ("cycle", Json::U64(u64::from(cycle.cycle))),
        (
            "previous_generation",
            Json::opt(cycle.previous, |facts| Json::U64(facts.generation)),
        ),
        ("result", connect_json(&cycle.result)),
        (
            "previous_still_current",
            Json::Bool(cycle.previous_still_current),
        ),
        ("probe", Json::opt(cycle.probe.as_ref(), sample_json)),
    ])
}

/// JSON of a workload.
pub fn workload_json(workload: &Workload) -> Json {
    Json::object([
        ("warmup", Json::U64(u64::from(workload.warmup))),
        ("measured", Json::U64(u64::from(workload.measured))),
        ("pacing_ms", duration_ms(workload.pacing)),
        (
            "reconnect_cycles",
            Json::U64(u64::from(workload.reconnect_cycles)),
        ),
    ])
}

fn policy_json(policy: &TlsPolicy) -> Json {
    Json::object([
        ("connect_timeout_ms", duration_ms(policy.connect_timeout)),
        (
            "handshake_timeout_ms",
            duration_ms(policy.handshake_timeout),
        ),
        ("max_idle_ms", duration_ms(policy.max_idle)),
        ("response_timeout_ms", duration_ms(policy.response_timeout)),
        (
            "max_plaintext_request_len",
            Json::U64(policy.max_plaintext_request_len as u64),
        ),
    ])
}

fn summary_json(summary: &Summary) -> Json {
    Json::object([
        ("count", Json::U64(summary.count as u64)),
        ("p50_ns", opt_u64(summary.p50)),
        ("p95_ns", opt_u64(summary.p95)),
        ("p99_ns", opt_u64(summary.p99)),
        ("max_ns", opt_u64(summary.max)),
        ("limit_ns", Json::U64(summary.limit)),
        (
            "at_or_above_limit",
            Json::U64(summary.at_or_above_limit as u64),
        ),
        (
            "fraction_at_or_above_limit",
            Json::F64(summary.fraction_at_or_above_limit),
        ),
    ])
}

fn provider_json(run: &ProviderRun, verdict: &ProviderVerdict) -> Json {
    Json::object([
        ("provider", Json::str(run.provider)),
        ("host", Json::str(&run.host)),
        ("port", Json::U64(u64::from(run.port))),
        ("auth_scope", Json::str(run.auth.name())),
        ("policy", policy_json(&run.policy)),
        ("workload", workload_json(&run.workload)),
        ("started_at", Json::str(&run.started_at)),
        ("ended_at", Json::str(&run.ended_at)),
        ("connect", connect_json(&run.connect)),
        (
            "samples",
            Json::Array(run.samples.iter().map(sample_json).collect()),
        ),
        (
            "reconnects",
            Json::Array(run.reconnects.iter().map(reconnect_json).collect()),
        ),
        ("summary", summary_json(&verdict.summary)),
        (
            "counts",
            Json::object([
                ("handed_off", Json::U64(verdict.handed_off as u64)),
                ("skipped", Json::U64(verdict.skipped as u64)),
                ("unknown", Json::U64(verdict.unknown as u64)),
                ("stale_after", Json::U64(verdict.stale_after as u64)),
                (
                    "disconnected_after",
                    Json::U64(verdict.disconnected_after as u64),
                ),
                ("reconnects", Json::U64(run.reconnects.len() as u64)),
            ]),
        ),
        ("api_scope", Json::str(verdict.api.name())),
        ("transport_verdict", Json::str(verdict.transport.name())),
        (
            "reasons",
            Json::Array(
                verdict
                    .reasons
                    .iter()
                    .map(|reason| reason.to_json())
                    .collect(),
            ),
        ),
    ])
}

/// What this evidence never qualifies, whatever its verdict.
pub const NOT_QUALIFIED: [&str; 8] = [
    "48Club positionFirst effect on block placement",
    "noMerge effect",
    "ordered, atomic and same-block behaviour of either builder",
    "pre-submission behaviour",
    "placement N/N+1 behind a launch",
    "builder inclusion",
    "any provider feature or lifecycle property",
    "T0 to T_buy",
];

/// The complete evidence document.
pub fn evidence_json(
    identity: &Identity,
    runs: &[ProviderRun],
    verdicts: &[ProviderVerdict],
    overall: Verdict,
) -> Json {
    Json::object([
        ("schema", Json::str(SCHEMA)),
        (
            "kind",
            Json::str(
                "Q5A real TLS transport qualification. Transport only; not a provider \
                 qualification. The probe is an eth_sendBundle without transactions.",
            ),
        ),
        ("identity", identity.to_json()),
        (
            "required",
            Json::object([
                ("workload", workload_json(&QUALIFICATION_WORKLOAD)),
                ("miss_limit_ns", Json::U64(MISS_NS)),
                (
                    "providers",
                    Json::Array(vec![Json::str("club48"), Json::str("blockrazor")]),
                ),
            ]),
        ),
        (
            "providers",
            Json::Array(
                runs.iter()
                    .zip(verdicts)
                    .map(|(run, verdict)| provider_json(run, verdict))
                    .collect(),
            ),
        ),
        (
            "verdict",
            Json::object([("tls_transport_qualification", Json::str(overall.name()))]),
        ),
        (
            "not_qualified_by_this_evidence",
            Json::Array(NOT_QUALIFIED.iter().map(|item| Json::str(*item)).collect()),
        ),
    ])
}

/// Whether `text` contains `secret`. An empty secret is never contained.
pub fn contains_secret(text: &str, secret: &str) -> bool {
    !secret.is_empty() && text.contains(secret)
}
