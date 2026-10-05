//! The Q5A acceptance rules.
//!
//! A provider's transport passes only if all of this holds for the
//! qualification workload:
//!
//! - the build is fully identified: release profile, sources unchanged
//!   since the baseline, rustls 0.23.45 and ring 0.17.14 locked;
//! - the authorization the provider needs was configured;
//! - the TLS handshake, certificate, server name and HTTP/1.1 succeeded;
//! - every one of the 100 measured samples, and every warmup and reconnect
//!   probe, was handed off: no skip, no unknown;
//! - all 100 measured samples carry their handoff duration, and every
//!   handoff's evidence is complete and consistent ([`handoff_issue`]);
//! - no measured handoff took 1,000,000 ns or longer, from the write check
//!   to the boundary;
//! - every reconnect produced the next generation, retired the previous
//!   facts, and its probe was handed off and answered.
//!
//! A failed rule fails the transport; a rule that could not be judged
//! leaves it incomplete. A failure outweighs incompleteness. No rule is
//! relaxed for any reason.
//!
//! Whether the builder answered every probe is reported beside the
//! verdict, as the API scope: it shows reachability, and nothing about
//! the builder's semantics.

use darkbot_runner::inclusion::tls::{ConnectionFacts, HttpVersion, TLS_TRANSPORT_IMPLEMENTATION};
use darkbot_runner::inclusion::transport::ConnectionState;

use crate::evidence::{
    AuthScope, MISS_NS, Phase, ProviderRun, QUALIFICATION_WORKLOAD, ResponseClass, Sample,
    SampleOutcome,
};
use crate::identity::{EXPECTED_BASELINE, EXPECTED_RING, EXPECTED_RUSTLS, Identity};
use crate::json::Json;
use crate::stats::{Summary, summarize};

/// A verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Every rule held.
    Pass,
    /// A rule was broken.
    Fail,
    /// A rule could not be judged.
    Incomplete,
}

impl Verdict {
    /// The name in evidence.
    pub fn name(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Incomplete => "INCOMPLETE",
        }
    }
}

/// What is wrong with the evidence of one handed-off sample. Checked in
/// this order; the first one found is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceIssue {
    /// No connection generation.
    MissingGeneration,
    /// No peer address.
    MissingPeer,
    /// No write check time.
    MissingWriteCheck,
    /// No boundary time.
    MissingBoundary,
    /// No handoff duration.
    MissingDuration,
    /// No count of bytes the kernel took.
    MissingWireBytes,
    /// The boundary lies before the write check.
    BoundaryBeforeWriteCheck,
    /// The duration is not the boundary minus the write check.
    DurationMismatch,
    /// The kernel took no byte.
    ZeroWireBytes,
    /// The peer is not the one of the connection the sample belongs to.
    PeerMismatch,
}

impl EvidenceIssue {
    /// The name in evidence.
    pub fn name(self) -> &'static str {
        match self {
            Self::MissingGeneration => "missing_generation",
            Self::MissingPeer => "missing_peer",
            Self::MissingWriteCheck => "missing_write_check",
            Self::MissingBoundary => "missing_boundary",
            Self::MissingDuration => "missing_duration",
            Self::MissingWireBytes => "missing_wire_bytes",
            Self::BoundaryBeforeWriteCheck => "boundary_before_write_check",
            Self::DurationMismatch => "duration_mismatch",
            Self::ZeroWireBytes => "zero_wire_bytes",
            Self::PeerMismatch => "peer_mismatch",
        }
    }
}

/// The first issue with the evidence of `sample`, judged against the
/// connection `scope` it belongs to, or `None`. Only a handed-off sample
/// is judged: it must carry its generation, peer, write check, boundary,
/// duration and wire bytes; the boundary must not lie before the write
/// check; the duration must be exactly the boundary minus the write check;
/// the kernel must have taken bytes; and the peer must be the scope's.
///
/// The generation is judged by the rules on the series and the reconnects,
/// which fail the transport.
pub fn handoff_issue(sample: &Sample, scope: &ConnectionFacts) -> Option<EvidenceIssue> {
    if sample.outcome != SampleOutcome::HandedOff {
        return None;
    }
    if sample.generation.is_none() {
        return Some(EvidenceIssue::MissingGeneration);
    }
    let Some(peer) = sample.peer else {
        return Some(EvidenceIssue::MissingPeer);
    };
    let Some(check) = sample.t_write_check_ns else {
        return Some(EvidenceIssue::MissingWriteCheck);
    };
    let Some(boundary) = sample.t_boundary_ns else {
        return Some(EvidenceIssue::MissingBoundary);
    };
    let Some(duration) = sample.write_check_to_boundary_ns else {
        return Some(EvidenceIssue::MissingDuration);
    };
    let Some(wire) = sample.wire_bytes else {
        return Some(EvidenceIssue::MissingWireBytes);
    };
    if boundary < check {
        return Some(EvidenceIssue::BoundaryBeforeWriteCheck);
    }
    if boundary - check != duration {
        return Some(EvidenceIssue::DurationMismatch);
    }
    if wire == 0 {
        return Some(EvidenceIssue::ZeroWireBytes);
    }
    if peer != scope.peer {
        return Some(EvidenceIssue::PeerMismatch);
    }
    None
}

/// Why a provider's transport did not pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    // ---- incomplete
    /// An identity field is unknown.
    IdentityUnknown(&'static str),
    /// An identity field has a value it can never have.
    IdentityInvalid(&'static str),

    /// The build script's baseline is not the expected one.
    BaselineMismatch,
    /// The baseline is not an ancestor of the built commit.
    BaselineNotAncestor,
    /// Core, wire or runner sources differ from the baseline.
    FrozenSourcesChanged,
    /// Tracked files differed from the built commit.
    WorkingTreeDirty,
    /// Not a release build.
    NotReleaseBuild,
    /// Another rustls version is locked.
    RustlsVersion,
    /// Another ring version is locked.
    RingVersion,
    /// The workload is not the qualification workload.
    WorkloadNotQualification,
    /// The provider needs an authorization and none was configured.
    AuthorizationMissing,
    /// The configured authorization was not usable.
    AuthorizationInvalid,
    /// Fewer measured samples than the workload requires.
    MeasuredSamplesMissing {
        /// Required.
        expected: u32,
        /// Recorded.
        recorded: u32,
    },
    /// Fewer measured handoff durations than the workload requires.
    MeasuredTimingMissing {
        /// Required.
        expected: u32,
        /// Recorded.
        recorded: u32,
    },
    /// Handed-off samples whose evidence is incomplete or contradicts
    /// itself.
    InconsistentEvidence {
        /// How many samples.
        samples: u32,
        /// Phase of the first.
        phase: Phase,
        /// Index of the first.
        index: u32,
        /// Its issue.
        issue: EvidenceIssue,
    },
    /// Fewer reconnect cycles than the workload requires.
    ReconnectCyclesMissing {
        /// Required.
        expected: u32,
        /// Recorded.
        recorded: u32,
    },
    // ---- fail
    /// The connection could not be established.
    ConnectFailed,
    /// The connection does not speak HTTP/1.1.
    NotHttp11,
    /// Samples were skipped because the connection was not ready.
    Skipped(u32),
    /// Handoffs failed after they started.
    TransportFailures(u32),
    /// Measured handoffs took 1,000,000 ns or longer.
    DeadlineMisses(u32),
    /// The measured series ran over more than one connection.
    ConnectionChangedDuringSeries,
    /// A reconnect failed.
    ReconnectFailed(u32),
    /// A reconnect did not produce the next generation.
    GenerationNotIncreased(u32),
    /// After a reconnect, the previous facts were still current.
    PreviousFactsStillCurrent(u32),
    /// The probe after a reconnect was not handed off and answered.
    ReconnectProbeFailed(u32),
}

impl Reason {
    /// Whether the reason fails the transport, rather than leaving it
    /// incomplete.
    pub fn fails(self) -> bool {
        matches!(
            self,
            Self::ConnectFailed
                | Self::NotHttp11
                | Self::Skipped(_)
                | Self::TransportFailures(_)
                | Self::DeadlineMisses(_)
                | Self::ConnectionChangedDuringSeries
                | Self::ReconnectFailed(_)
                | Self::GenerationNotIncreased(_)
                | Self::PreviousFactsStillCurrent(_)
                | Self::ReconnectProbeFailed(_)
        )
    }

    /// The name in evidence.
    pub fn name(self) -> &'static str {
        match self {
            Self::IdentityUnknown(_) => "identity_unknown",
            Self::IdentityInvalid(_) => "identity_invalid",
            Self::BaselineMismatch => "baseline_mismatch",
            Self::BaselineNotAncestor => "baseline_not_ancestor",
            Self::FrozenSourcesChanged => "frozen_sources_changed",
            Self::WorkingTreeDirty => "working_tree_dirty",
            Self::NotReleaseBuild => "not_release_build",
            Self::RustlsVersion => "rustls_version",
            Self::RingVersion => "ring_version",
            Self::WorkloadNotQualification => "workload_not_qualification",
            Self::AuthorizationMissing => "authorization_missing",
            Self::AuthorizationInvalid => "authorization_invalid",
            Self::MeasuredSamplesMissing { .. } => "measured_samples_missing",
            Self::MeasuredTimingMissing { .. } => "measured_timing_missing",
            Self::InconsistentEvidence { .. } => "inconsistent_evidence",
            Self::ReconnectCyclesMissing { .. } => "reconnect_cycles_missing",
            Self::ConnectFailed => "connect_failed",
            Self::NotHttp11 => "not_http11",
            Self::Skipped(_) => "skipped",
            Self::TransportFailures(_) => "transport_failures",
            Self::DeadlineMisses(_) => "deadline_misses",
            Self::ConnectionChangedDuringSeries => "connection_changed_during_series",
            Self::ReconnectFailed(_) => "reconnect_failed",
            Self::GenerationNotIncreased(_) => "generation_not_increased",
            Self::PreviousFactsStillCurrent(_) => "previous_facts_still_current",
            Self::ReconnectProbeFailed(_) => "reconnect_probe_failed",
        }
    }

    /// The JSON form: name, severity and detail.
    pub fn to_json(self) -> Json {
        let detail = match self {
            Self::IdentityUnknown(field) | Self::IdentityInvalid(field) => Json::str(field),
            Self::InconsistentEvidence {
                samples,
                phase,
                index,
                issue,
            } => Json::object([
                ("samples", Json::U64(u64::from(samples))),
                ("first_phase", Json::str(phase.name())),
                ("first_index", Json::U64(u64::from(index))),
                ("first_issue", Json::str(issue.name())),
            ]),
            Self::MeasuredSamplesMissing { expected, recorded }
            | Self::MeasuredTimingMissing { expected, recorded }
            | Self::ReconnectCyclesMissing { expected, recorded } => Json::object([
                ("expected", Json::U64(u64::from(expected))),
                ("recorded", Json::U64(u64::from(recorded))),
            ]),
            Self::Skipped(count)
            | Self::TransportFailures(count)
            | Self::DeadlineMisses(count)
            | Self::ReconnectFailed(count)
            | Self::GenerationNotIncreased(count)
            | Self::PreviousFactsStillCurrent(count)
            | Self::ReconnectProbeFailed(count) => Json::U64(u64::from(count)),
            _ => Json::Null,
        };
        Json::object([
            ("reason", Json::str(self.name())),
            (
                "severity",
                Json::str(if self.fails() { "fail" } else { "incomplete" }),
            ),
            ("detail", detail),
        ])
    }
}

/// Whether the builder answered the probes. Reachability only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiScope {
    /// No probe was measured.
    NotMeasured,
    /// No authorization was configured; no probe was sent.
    AuthMissing,
    /// The configured authorization was not usable; no probe was sent.
    AuthInvalid,
    /// A probe sent with the authorization was refused with 401 or 403.
    AuthRejected,
    /// Every measured probe got a complete answer bound to it.
    AllAnswered,
    /// At least one measured probe got no such answer.
    NotAllAnswered,
}

impl ApiScope {
    /// The name in evidence.
    pub fn name(self) -> &'static str {
        match self {
            Self::NotMeasured => "NOT_MEASURED",
            Self::AuthMissing => "UNQUALIFIED_AUTH_MISSING",
            Self::AuthInvalid => "UNQUALIFIED_AUTH_INVALID",
            Self::AuthRejected => "AUTH_REJECTED",
            Self::AllAnswered => "ALL_ANSWERED",
            Self::NotAllAnswered => "NOT_ALL_ANSWERED",
        }
    }
}

/// The judgement of one provider's run.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderVerdict {
    /// The transport verdict.
    pub transport: Verdict,
    /// Every reason it is not a pass.
    pub reasons: Vec<Reason>,
    /// Whether the builder answered.
    pub api: ApiScope,
    /// The measured handoffs, from the write check to the boundary.
    pub summary: Summary,
    /// Samples handed off, all phases.
    pub handed_off: usize,
    /// Samples skipped, all phases.
    pub skipped: usize,
    /// Samples unknown, all phases.
    pub unknown: usize,
    /// Samples after which the transport was stale.
    pub stale_after: usize,
    /// Samples after which the transport was disconnected.
    pub disconnected_after: usize,
}

fn count(samples: &[&Sample], keep: impl Fn(&Sample) -> bool) -> usize {
    samples.iter().filter(|sample| keep(sample)).count()
}

fn narrow(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Judges a three-valued build flag: `wanted` passes, `unwanted` is the
/// `opposite` reason, "unknown" is unknown, and any other value is invalid.
fn flag(
    reasons: &mut Vec<Reason>,
    field: &'static str,
    value: &str,
    wanted: &str,
    unwanted: &str,
    opposite: Reason,
) {
    if value == wanted {
        return;
    }
    reasons.push(if value == unwanted {
        opposite
    } else if value.is_empty() || value == "unknown" {
        Reason::IdentityUnknown(field)
    } else {
        Reason::IdentityInvalid(field)
    });
}

/// Judges a field that must be exactly `wanted`: "unknown" is unknown, and
/// any other value is the `other` reason.
fn exact(reasons: &mut Vec<Reason>, field: &'static str, value: &str, wanted: &str, other: Reason) {
    if value == wanted {
        return;
    }
    reasons.push(if value.is_empty() || value == "unknown" {
        Reason::IdentityUnknown(field)
    } else {
        other
    });
}

/// The identity reasons of `identity`. Only the exact expected values
/// pass: "unknown" is unknown, and any other value is invalid or the
/// specific mismatch. Nothing unexpected is accepted silently.
pub fn identity_reasons(identity: &Identity) -> Vec<Reason> {
    let mut reasons = Vec::new();
    exact(
        &mut reasons,
        "baseline",
        &identity.baseline,
        EXPECTED_BASELINE,
        Reason::BaselineMismatch,
    );
    let head = identity.git_head.as_str();
    if head.is_empty() || head == "unknown" {
        reasons.push(Reason::IdentityUnknown("git_head"));
    } else if head.len() != 40 || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        reasons.push(Reason::IdentityInvalid("git_head"));
    }
    flag(
        &mut reasons,
        "git_dirty",
        &identity.git_dirty,
        "false",
        "true",
        Reason::WorkingTreeDirty,
    );
    flag(
        &mut reasons,
        "baseline_ancestor",
        &identity.baseline_ancestor,
        "true",
        "false",
        Reason::BaselineNotAncestor,
    );
    flag(
        &mut reasons,
        "frozen_sources_unchanged",
        &identity.frozen_sources_unchanged,
        "true",
        "false",
        Reason::FrozenSourcesChanged,
    );
    for (field, value) in [("rustc", &identity.rustc), ("cargo", &identity.cargo)] {
        if value.is_empty() || value == "unknown" {
            reasons.push(Reason::IdentityUnknown(field));
        }
    }
    exact(
        &mut reasons,
        "profile",
        &identity.profile,
        "release",
        Reason::NotReleaseBuild,
    );
    exact(
        &mut reasons,
        "rustls",
        &identity.rustls,
        EXPECTED_RUSTLS,
        Reason::RustlsVersion,
    );
    exact(
        &mut reasons,
        "ring",
        &identity.ring,
        EXPECTED_RING,
        Reason::RingVersion,
    );
    exact(
        &mut reasons,
        "transport_implementation",
        &identity.transport_implementation,
        TLS_TRANSPORT_IMPLEMENTATION,
        Reason::IdentityInvalid("transport_implementation"),
    );
    reasons
}

/// Judges one provider's run.
pub fn evaluate(run: &ProviderRun, identity: &Identity) -> ProviderVerdict {
    let mut reasons = identity_reasons(identity);
    if run.workload != QUALIFICATION_WORKLOAD {
        reasons.push(Reason::WorkloadNotQualification);
    }
    match run.auth {
        AuthScope::Missing => reasons.push(Reason::AuthorizationMissing),
        AuthScope::Invalid => reasons.push(Reason::AuthorizationInvalid),
        AuthScope::NotRequired | AuthScope::Present => {}
    }
    let connected = match &run.connect {
        Ok(facts) => {
            if facts.http != HttpVersion::Http11 {
                reasons.push(Reason::NotHttp11);
            }
            true
        }
        Err(_) => {
            reasons.push(Reason::ConnectFailed);
            false
        }
    };
    let probing = connected && matches!(run.auth, AuthScope::NotRequired | AuthScope::Present);

    // Every sample of every phase.
    let mut all: Vec<&Sample> = run.samples.iter().collect();
    all.extend(
        run.reconnects
            .iter()
            .filter_map(|cycle| cycle.probe.as_ref()),
    );
    let measured: Vec<&Sample> = run
        .samples
        .iter()
        .filter(|sample| sample.phase == Phase::Measured)
        .collect();

    let skipped = count(&all, |s| matches!(s.outcome, SampleOutcome::Skipped(_)));
    let unknown = count(&all, |s| matches!(s.outcome, SampleOutcome::Unknown { .. }));
    let handed_off = count(&all, |s| s.outcome == SampleOutcome::HandedOff);
    let stale_after = count(&all, |s| s.state_after == ConnectionState::Stale);
    let disconnected_after = count(&all, |s| s.state_after == ConnectionState::Disconnected);
    if skipped > 0 {
        reasons.push(Reason::Skipped(narrow(skipped)));
    }
    if unknown > 0 {
        reasons.push(Reason::TransportFailures(narrow(unknown)));
    }
    if probing && narrow(measured.len()) != run.workload.measured {
        reasons.push(Reason::MeasuredSamplesMissing {
            expected: run.workload.measured,
            recorded: narrow(measured.len()),
        });
    }

    let durations: Vec<u64> = measured
        .iter()
        .filter_map(|sample| sample.write_check_to_boundary_ns)
        .collect();
    let summary = summarize(&durations, MISS_NS);
    // Every measured sample must carry its duration: none may drop out of
    // the statistics.
    if probing && narrow(summary.count) != run.workload.measured {
        reasons.push(Reason::MeasuredTimingMissing {
            expected: run.workload.measured,
            recorded: narrow(summary.count),
        });
    }
    if summary.at_or_above_limit > 0 {
        reasons.push(Reason::DeadlineMisses(narrow(summary.at_or_above_limit)));
    }

    // The series belongs to the initial connection, each reconnect probe to
    // the connection of its reconnect.
    let mut scoped: Vec<(&Sample, &ConnectionFacts)> = Vec::new();
    if let Ok(initial) = &run.connect {
        let changed = run.samples.iter().any(|sample| {
            sample
                .generation
                .is_some_and(|generation| generation != initial.generation)
        });
        if changed {
            reasons.push(Reason::ConnectionChangedDuringSeries);
        }
        scoped.extend(run.samples.iter().map(|sample| (sample, initial)));
    }
    for cycle in &run.reconnects {
        if let (Ok(facts), Some(probe)) = (&cycle.result, &cycle.probe) {
            scoped.push((probe, facts));
        }
    }
    let issues: Vec<(&Sample, EvidenceIssue)> = scoped
        .iter()
        .filter_map(|(sample, scope)| handoff_issue(sample, scope).map(|issue| (*sample, issue)))
        .collect();
    if let Some((first, issue)) = issues.first() {
        reasons.push(Reason::InconsistentEvidence {
            samples: narrow(issues.len()),
            phase: first.phase,
            index: first.index,
            issue: *issue,
        });
    }

    // Reconnects.
    if probing && narrow(run.reconnects.len()) < run.workload.reconnect_cycles {
        reasons.push(Reason::ReconnectCyclesMissing {
            expected: run.workload.reconnect_cycles,
            recorded: narrow(run.reconnects.len()),
        });
    }
    for cycle in &run.reconnects {
        match &cycle.result {
            Err(_) => reasons.push(Reason::ReconnectFailed(cycle.cycle)),
            Ok(facts) => {
                let next = cycle.previous.map(|previous| previous.generation + 1);
                if next != Some(facts.generation) {
                    reasons.push(Reason::GenerationNotIncreased(cycle.cycle));
                }
                if cycle.previous_still_current {
                    reasons.push(Reason::PreviousFactsStillCurrent(cycle.cycle));
                }
                let probed = cycle.probe.as_ref().is_some_and(|probe| {
                    probe.outcome == SampleOutcome::HandedOff
                        && probe.response.is_some_and(ResponseClass::answered)
                        && probe.generation == Some(facts.generation)
                });
                if !probed {
                    reasons.push(Reason::ReconnectProbeFailed(cycle.cycle));
                }
            }
        }
    }

    let api = api_scope(run, &measured);
    let transport = if reasons.iter().any(|reason| reason.fails()) {
        Verdict::Fail
    } else if reasons.is_empty() {
        Verdict::Pass
    } else {
        Verdict::Incomplete
    };
    ProviderVerdict {
        transport,
        reasons,
        api,
        summary,
        handed_off,
        skipped,
        unknown,
        stale_after,
        disconnected_after,
    }
}

fn api_scope(run: &ProviderRun, measured: &[&Sample]) -> ApiScope {
    match run.auth {
        AuthScope::Missing => return ApiScope::AuthMissing,
        AuthScope::Invalid => return ApiScope::AuthInvalid,
        AuthScope::NotRequired | AuthScope::Present => {}
    }
    if measured.is_empty() {
        return ApiScope::NotMeasured;
    }
    let refused = measured
        .iter()
        .any(|sample| matches!(sample.response, Some(ResponseClass::HttpStatus(401 | 403))));
    if run.auth == AuthScope::Present && refused {
        return ApiScope::AuthRejected;
    }
    let answered = measured.iter().all(|sample| {
        sample.outcome == SampleOutcome::HandedOff
            && sample.response.is_some_and(ResponseClass::answered)
    });
    if answered {
        ApiScope::AllAnswered
    } else {
        ApiScope::NotAllAnswered
    }
}

/// The verdict over all providers: a pass needs a pass for both builders.
pub fn overall(runs: &[ProviderRun], verdicts: &[ProviderVerdict]) -> Verdict {
    if verdicts
        .iter()
        .any(|verdict| verdict.transport == Verdict::Fail)
    {
        return Verdict::Fail;
    }
    let present = |name: &str| runs.iter().any(|run| run.provider == name);
    let all_pass = verdicts
        .iter()
        .all(|verdict| verdict.transport == Verdict::Pass);
    if all_pass && present("club48") && present("blockrazor") {
        Verdict::Pass
    } else {
        Verdict::Incomplete
    }
}
