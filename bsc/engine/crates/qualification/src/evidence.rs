//! What a Q5A run records, and (R8, BSC-048) its record; the historical
//! JSON evidence stays as it was written.
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

use darkbot_bsc_record::schema::q5a::{
    connection as qc, evidence as qe, provider as qp, reason as qr, reconnect as qn, sample as qs,
};
use darkbot_bsc_record::{Draft, RecordError, Value};

use crate::identity::Identity;
use crate::verdict::{ProviderVerdict, Reason, Verdict};

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

fn ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn facts_record(facts: &ConnectionFacts) -> Value {
    let mut d = Draft::new(&qc::SCHEMA);
    d.set(qc::GENERATION, facts.generation)
        .set(qc::PEER, facts.peer.to_string())
        .set(qc::TLS_VERSION, format!("{:?}", facts.tls_version))
        .set(qc::CIPHER_SUITE, format!("{:?}", facts.cipher_suite))
        .set(qc::HTTP, format!("{:?}", facts.http))
        .set(qc::ALPN_SELECTED, facts.alpn_selected);
    d.into_nested()
}

/// R8 (BSC-048): one sample as its record.
fn sample_record(sample: &Sample) -> Result<Value, String> {
    let body: [u8; 32] = hex::decode(&sample.body_sha256)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            format!(
                "sample {} {}: its body SHA-256 is not 32 bytes of hex",
                sample.phase.name(),
                sample.index
            )
        })?;
    let mut d = Draft::new(&qs::SCHEMA);
    d.set(qs::PHASE, sample.phase.name())
        .set(qs::INDEX, u64::from(sample.index))
        .set(qs::STATE_BEFORE, state_name(sample.state_before))
        .set(qs::REQUEST_PLAINTEXT_BYTES, sample.request_bytes as u64)
        .set(qs::BODY_BYTES, sample.body_bytes as u64)
        .set(qs::BODY_SHA256, Value::B256(body))
        .set(qs::STATE_AFTER, state_name(sample.state_after));
    if let Some(peer) = sample.peer {
        d.set(qs::PEER, peer.to_string());
    }
    let times = [
        (qs::GENERATION, sample.generation),
        (qs::PREPARE_NS, sample.prepare_ns),
        (qs::T_WRITE_CHECK_NS, sample.t_write_check_ns),
        (qs::T_BOUNDARY_NS, sample.t_boundary_ns),
        (
            qs::WRITE_CHECK_TO_BOUNDARY_NS,
            sample.write_check_to_boundary_ns,
        ),
        (qs::WIRE_BYTES, sample.wire_bytes),
        (qs::RESPONSE_LATENCY_NS, sample.response_latency_ns),
    ];
    for (tag, value) in times {
        if let Some(value) = value {
            d.set(tag, value);
        }
    }
    match sample.outcome {
        SampleOutcome::HandedOff => d.set(qs::OUTCOME, "handed_off"),
        SampleOutcome::Skipped(state) => d
            .set(qs::OUTCOME, "skipped")
            .set(qs::OUTCOME_STATE, state_name(state)),
        SampleOutcome::Unknown { stage, error } => d
            .set(qs::OUTCOME, "unknown")
            .set(qs::OUTCOME_STAGE, stage_name(stage))
            .set(qs::OUTCOME_ERROR, format!("{error:?}")),
    };
    if let Some(class) = sample.response {
        let name = match class {
            ResponseClass::Ack => "ack",
            ResponseClass::Reject { code } => {
                if let Some(code) = code {
                    d.set(qs::RESPONSE_REJECT_CODE, code);
                }
                "reject"
            }
            ResponseClass::RateLimited => "rate_limited",
            ResponseClass::HttpStatus(status) => {
                d.set(qs::RESPONSE_HTTP_STATUS, u64::from(status));
                "http_status"
            }
            ResponseClass::Malformed(failure) => {
                d.set(qs::RESPONSE_MALFORMED, format!("{failure:?}"));
                "malformed"
            }
            ResponseClass::Connection => "connection",
            ResponseClass::Timeout => "timeout",
        };
        d.set(qs::RESPONSE, name)
            .set(qs::RESPONSE_ANSWERED, class.answered());
    }
    Ok(d.into_nested())
}

fn reconnect_record(cycle: &ReconnectCycle) -> Result<Value, String> {
    let mut d = Draft::new(&qn::SCHEMA);
    d.set(qn::CYCLE, u64::from(cycle.cycle))
        .set(qn::PREVIOUS_STILL_CURRENT, cycle.previous_still_current);
    if let Some(previous) = cycle.previous {
        d.set(qn::PREVIOUS_GENERATION, previous.generation);
    }
    match &cycle.result {
        Ok(facts) => d.set(qn::CONNECTION, facts_record(facts)),
        Err(error) => d.set(qn::ERROR, error.as_str()),
    };
    if let Some(probe) = &cycle.probe {
        d.set(qn::PROBE, sample_record(probe)?);
    }
    Ok(d.into_nested())
}

fn reason_record(reason: Reason) -> Value {
    let mut d = Draft::new(&qr::SCHEMA);
    d.set(qr::NAME, reason.name())
        .set(qr::FAILS, reason.fails());
    match reason {
        Reason::IdentityUnknown(field) | Reason::IdentityInvalid(field) => {
            d.set(qr::FIELD, field);
        }
        Reason::InconsistentEvidence {
            samples,
            phase,
            index,
            issue,
        } => {
            d.set(qr::SAMPLES, u64::from(samples))
                .set(qr::FIRST_PHASE, phase.name())
                .set(qr::FIRST_INDEX, u64::from(index))
                .set(qr::FIRST_ISSUE, issue.name());
        }
        Reason::MeasuredSamplesMissing { expected, recorded }
        | Reason::MeasuredTimingMissing { expected, recorded }
        | Reason::ReconnectCyclesMissing { expected, recorded } => {
            d.set(qr::EXPECTED, u64::from(expected))
                .set(qr::RECORDED, u64::from(recorded));
        }
        Reason::Skipped(count)
        | Reason::TransportFailures(count)
        | Reason::DeadlineMisses(count)
        | Reason::ReconnectFailed(count)
        | Reason::GenerationNotIncreased(count)
        | Reason::PreviousFactsStillCurrent(count)
        | Reason::ReconnectProbeFailed(count) => {
            d.set(qr::COUNT, u64::from(count));
        }
        _ => {}
    }
    d.into_nested()
}

fn provider_record(run: &ProviderRun, verdict: &ProviderVerdict) -> Result<Value, String> {
    let mut d = Draft::new(&qp::SCHEMA);
    let summary = &verdict.summary;
    d.set(qp::PROVIDER, run.provider)
        .set(qp::HOST, run.host.as_str())
        .set(qp::PORT, u64::from(run.port))
        .set(qp::AUTH_SCOPE, run.auth.name())
        .set(qp::CONNECT_TIMEOUT_MS, ms(run.policy.connect_timeout))
        .set(qp::HANDSHAKE_TIMEOUT_MS, ms(run.policy.handshake_timeout))
        .set(qp::MAX_IDLE_MS, ms(run.policy.max_idle))
        .set(qp::RESPONSE_TIMEOUT_MS, ms(run.policy.response_timeout))
        .set(
            qp::MAX_PLAINTEXT_REQUEST_LEN,
            run.policy.max_plaintext_request_len as u64,
        )
        .set(qp::WARMUP, u64::from(run.workload.warmup))
        .set(qp::MEASURED, u64::from(run.workload.measured))
        .set(qp::PACING_MS, ms(run.workload.pacing))
        .set(
            qp::RECONNECT_CYCLES,
            u64::from(run.workload.reconnect_cycles),
        )
        .set(qp::STARTED_AT, run.started_at.as_str())
        .set(qp::ENDED_AT, run.ended_at.as_str())
        .set(qp::SUMMARY_COUNT, summary.count as u64)
        .set(qp::LIMIT_NS, summary.limit)
        .set(qp::AT_OR_ABOVE_LIMIT, summary.at_or_above_limit as u64)
        .set(qp::HANDED_OFF, verdict.handed_off as u64)
        .set(qp::SKIPPED, verdict.skipped as u64)
        .set(qp::UNKNOWN, verdict.unknown as u64)
        .set(qp::STALE_AFTER, verdict.stale_after as u64)
        .set(qp::DISCONNECTED_AFTER, verdict.disconnected_after as u64)
        .set(qp::API_SCOPE, verdict.api.name())
        .set(qp::TRANSPORT_VERDICT, verdict.transport.name());
    for (tag, value) in [
        (qp::P50_NS, summary.p50),
        (qp::P95_NS, summary.p95),
        (qp::P99_NS, summary.p99),
        (qp::MAX_NS, summary.max),
    ] {
        if let Some(value) = value {
            d.set(tag, value);
        }
    }
    match &run.connect {
        Ok(facts) => d.set(qp::CONNECTION, facts_record(facts)),
        Err(error) => d.set(qp::CONNECT_ERROR, error.as_str()),
    };
    for sample in &run.samples {
        d.push(qp::SAMPLES, sample_record(sample)?);
    }
    for cycle in &run.reconnects {
        d.push(qp::RECONNECTS, reconnect_record(cycle)?);
    }
    for reason in &verdict.reasons {
        d.push(qp::REASONS, reason_record(*reason));
    }
    Ok(d.into_nested())
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

/// R8 (BSC-048): the complete evidence as its record, made at
/// `now_unix_ms`.
pub fn evidence_record(
    identity: &Identity,
    runs: &[ProviderRun],
    verdicts: &[ProviderVerdict],
    overall: Verdict,
    now_unix_ms: u64,
) -> Result<Vec<u8>, String> {
    let mut d = Draft::new(&qe::SCHEMA);
    d.set(qe::PROVENANCE_TOOL, "u5_tls_qualify")
        .set(qe::PROVENANCE_OBSERVED_UNIX_MS, now_unix_ms)
        .set(qe::BASELINE, identity.baseline.as_str())
        .set(qe::GIT_HEAD, identity.git_head.as_str())
        .set(qe::GIT_DIRTY, identity.git_dirty.as_str())
        .set(qe::BASELINE_ANCESTOR, identity.baseline_ancestor.as_str())
        .set(
            qe::FROZEN_SOURCES_UNCHANGED,
            identity.frozen_sources_unchanged.as_str(),
        )
        .set(qe::RUSTC, identity.rustc.as_str())
        .set(qe::CARGO, identity.cargo.as_str())
        .set(qe::RUSTLS, identity.rustls.as_str())
        .set(qe::RING, identity.ring.as_str())
        .set(qe::PROFILE, identity.profile.as_str())
        .set(qe::OS, identity.os.as_str())
        .set(qe::ARCH, identity.arch.as_str())
        .set(
            qe::TRANSPORT_IMPLEMENTATION,
            identity.transport_implementation.as_str(),
        )
        .set(qe::HARNESS, identity.harness.as_str())
        .set(
            qe::REQUIRED_WARMUP,
            u64::from(QUALIFICATION_WORKLOAD.warmup),
        )
        .set(
            qe::REQUIRED_MEASURED,
            u64::from(QUALIFICATION_WORKLOAD.measured),
        )
        .set(qe::REQUIRED_PACING_MS, ms(QUALIFICATION_WORKLOAD.pacing))
        .set(
            qe::REQUIRED_RECONNECT_CYCLES,
            u64::from(QUALIFICATION_WORKLOAD.reconnect_cycles),
        )
        .set(qe::MISS_LIMIT_NS, MISS_NS)
        .set(qe::VERDICT, overall.name());
    for provider in ["club48", "blockrazor"] {
        d.push(qe::REQUIRED_PROVIDERS, provider);
    }
    for (run, verdict) in runs.iter().zip(verdicts) {
        d.push(qe::PROVIDERS, provider_record(run, verdict)?);
    }
    for item in NOT_QUALIFIED {
        d.push(qe::NOT_QUALIFIED, item);
    }
    d.encode().map_err(|e| format!("Q5A evidence: {e:?}"))
}

/// R8 (BSC-048): an evidence record checked against itself, from its raw
/// samples: a handed-off sample with all its timings has the duration of
/// its boundary less its write check (one without them is the verdict's
/// INCONSISTENT_EVIDENCE); per provider, the summary of the measured
/// durations (nearest-rank, misses at [`MISS_NS`]) and the counts of all
/// samples, reconnect probes included, are the ones it states; review M3:
/// the identity's reasons (from its fields) and the count reasons are its
/// reasons, its transport verdict theirs, the overall verdict the
/// providers'.
pub fn check_record(bytes: &[u8]) -> Result<(), String> {
    let e = |x: RecordError| x.0;
    let r = darkbot_bsc_record::decode(&qe::SCHEMA, bytes).map_err(e)?;
    if r.u64(qe::MISS_LIMIT_NS).map_err(e)? != MISS_NS {
        return Err(format!("the miss limit is not {MISS_NS} ns"));
    }
    let text = |tag| r.text(tag).map(str::to_string).map_err(e);
    let identity = Identity {
        baseline: text(qe::BASELINE)?,
        git_head: text(qe::GIT_HEAD)?,
        git_dirty: text(qe::GIT_DIRTY)?,
        baseline_ancestor: text(qe::BASELINE_ANCESTOR)?,
        frozen_sources_unchanged: text(qe::FROZEN_SOURCES_UNCHANGED)?,
        rustc: text(qe::RUSTC)?,
        cargo: text(qe::CARGO)?,
        rustls: text(qe::RUSTLS)?,
        ring: text(qe::RING)?,
        profile: text(qe::PROFILE)?,
        os: text(qe::OS)?,
        arch: text(qe::ARCH)?,
        transport_implementation: text(qe::TRANSPORT_IMPLEMENTATION)?,
        harness: text(qe::HARNESS)?,
    };
    let from_identity: Vec<&str> = crate::verdict::identity_reasons(&identity)
        .into_iter()
        .map(Reason::name)
        .collect();
    let mut verdicts = Vec::new();
    for p in r.nested_all(qe::PROVIDERS).map_err(e)? {
        let name = p.text(qp::PROVIDER).map_err(e)?.to_string();
        let mut samples = p.nested_all(qp::SAMPLES).map_err(e)?;
        for cycle in p.nested_all(qp::RECONNECTS).map_err(e)? {
            samples.extend(cycle.opt_nested(qn::PROBE).map_err(e)?);
        }
        let mut durations = Vec::new();
        let mut counts = [0u64; 5];
        for s in &samples {
            let at = || {
                format!(
                    "{name} {} {}",
                    s.text(qs::PHASE).unwrap_or("?"),
                    s.u64(qs::INDEX).unwrap_or(0)
                )
            };
            let (write, boundary, duration) = (
                s.opt_u64(qs::T_WRITE_CHECK_NS).map_err(e)?,
                s.opt_u64(qs::T_BOUNDARY_NS).map_err(e)?,
                s.opt_u64(qs::WRITE_CHECK_TO_BOUNDARY_NS).map_err(e)?,
            );
            match s.text(qs::OUTCOME).map_err(e)? {
                "handed_off" => {
                    counts[0] += 1;
                    if let (Some(w), Some(b), Some(d)) = (write, boundary, duration)
                        && b.checked_sub(w) != Some(d)
                    {
                        return Err(format!(
                            "{}: its duration is not its boundary less its write check",
                            at()
                        ));
                    }
                }
                "skipped" => counts[1] += 1,
                "unknown" => counts[2] += 1,
                other => return Err(format!("{}: outcome {other}", at())),
            }
            match s.text(qs::STATE_AFTER).map_err(e)? {
                "stale" => counts[3] += 1,
                "disconnected" => counts[4] += 1,
                _ => {}
            }
            if s.text(qs::PHASE).map_err(e)? == "measured"
                && let Some(d) = duration
            {
                durations.push(d);
            }
        }
        let summary = crate::stats::summarize(&durations, MISS_NS);
        let stated = [
            (qp::HANDED_OFF, counts[0]),
            (qp::SKIPPED, counts[1]),
            (qp::UNKNOWN, counts[2]),
            (qp::STALE_AFTER, counts[3]),
            (qp::DISCONNECTED_AFTER, counts[4]),
            (qp::SUMMARY_COUNT, summary.count as u64),
            (qp::AT_OR_ABOVE_LIMIT, summary.at_or_above_limit as u64),
            (qp::LIMIT_NS, MISS_NS),
        ];
        for (tag, value) in stated {
            if p.u64(tag).map_err(e)? != value {
                return Err(format!(
                    "{name}: field {tag} states {}, its samples {value}",
                    p.u64(tag).map_err(e)?
                ));
            }
        }
        let percentiles = [
            (qp::P50_NS, summary.p50),
            (qp::P95_NS, summary.p95),
            (qp::P99_NS, summary.p99),
            (qp::MAX_NS, summary.max),
        ];
        for (tag, value) in percentiles {
            if p.opt_u64(tag).map_err(e)? != value {
                return Err(format!("{name}: field {tag} is not its samples' {value:?}"));
            }
        }
        // Review M3: its reasons and its verdict.
        let mut reasons = Vec::new();
        for x in p.nested_all(qp::REASONS).map_err(e)? {
            let reason = (
                x.text(qr::NAME).map_err(e)?.to_string(),
                x.opt_u64(qr::COUNT).map_err(e)?,
            );
            reasons.push((reason, x.bool(qr::FAILS).map_err(e)?));
        }
        let named = |reason: &str| reasons.iter().find(|((n, _), _)| n == reason);
        if let Some(missing) = from_identity.iter().find(|r| named(r).is_none()) {
            return Err(format!(
                "{name}: the identity's reason {missing} is not named"
            ));
        }
        let derived = [
            ("skipped", counts[1]),
            ("transport_failures", counts[2]),
            ("deadline_misses", summary.at_or_above_limit as u64),
        ];
        for (reason, count) in derived {
            let stated = named(reason).map(|((_, c), _)| *c);
            if stated != (count > 0).then_some(Some(count)) {
                return Err(format!(
                    "{name}: reason {reason} is not its samples' {count}"
                ));
            }
        }
        let transport = if reasons.iter().any(|(_, fails)| *fails) {
            Verdict::Fail
        } else if reasons.is_empty() {
            Verdict::Pass
        } else {
            Verdict::Incomplete
        };
        if p.text(qp::TRANSPORT_VERDICT).map_err(e)? != transport.name() {
            return Err(format!(
                "{name}: its verdict is not its reasons' {}",
                transport.name()
            ));
        }
        verdicts.push((name, transport));
    }
    let present = |provider: &str| verdicts.iter().any(|(n, _)| n == provider);
    let overall = if verdicts.iter().any(|(_, v)| *v == Verdict::Fail) {
        Verdict::Fail
    } else if verdicts.iter().all(|(_, v)| *v == Verdict::Pass)
        && present("club48")
        && present("blockrazor")
    {
        Verdict::Pass
    } else {
        Verdict::Incomplete
    };
    if r.text(qe::VERDICT).map_err(e)? != overall.name() {
        return Err(format!(
            "the overall verdict is not its providers' {}",
            overall.name()
        ));
    }
    Ok(())
}

/// Whether `bytes` contain `secret`. An empty secret is never contained.
pub fn contains_secret(bytes: &[u8], secret: &str) -> bool {
    !secret.is_empty()
        && bytes
            .windows(secret.len())
            .any(|window| window == secret.as_bytes())
}
