//! Offline tests of the Q5A harness: probe, statistics, evidence form and
//! the acceptance rules. No network.

use std::net::SocketAddr;
use std::time::Duration;

use darkbot_bsc_execution::submission::response::{ProtocolFailure, ResponseOutcome};
use darkbot_bsc_execution::submission::tls::{
    ConnectionFacts, HttpVersion, TLS_TRANSPORT_IMPLEMENTATION,
};
use darkbot_bsc_execution::submission::transport::ConnectionState;
use darkbot_bsc_qualification::evidence::{
    AuthScope, MISS_NS, Phase, ProviderRun, QUALIFICATION_WORKLOAD, ReconnectCycle, ResponseClass,
    SCHEMA, Sample, SampleOutcome, Stage, Workload, contains_secret, evidence_json,
};
use darkbot_bsc_qualification::identity::{EXPECTED_BASELINE, Identity, rfc3339};
use darkbot_bsc_qualification::json::Json;
use darkbot_bsc_qualification::measure::{QUALIFICATION_POLICY, classify, state_after_response};
use darkbot_bsc_qualification::probe::{frame_request, probe_body, sha256_hex};
use darkbot_bsc_qualification::stats::{nearest_rank, summarize};
use darkbot_bsc_qualification::verdict::{
    ApiScope, EvidenceIssue, Reason, Verdict, evaluate, handoff_issue, overall,
};

type SampleMutation = fn(&mut Sample);
type IdentityMutation = fn(&mut Identity);
type SampleCase = (SampleMutation, EvidenceIssue);
type IdentityCase = (IdentityMutation, Reason);

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

#[test]
fn the_probe_carries_no_transaction_and_a_last_block_of_zero() {
    let body = probe_body("q5a-club48-m-007");
    assert_eq!(
        body,
        "{\"jsonrpc\":\"2.0\",\"id\":\"q5a-club48-m-007\",\"method\":\"eth_sendBundle\",\
         \"params\":[{\"txs\":[],\"maxBlockNumber\":0,\"noMerge\":true,\"positionFirst\":true}]}"
    );
    assert_eq!(body.len(), 144);
    assert_eq!(
        sha256_hex(body.as_bytes()),
        "399bfcedbfd005a8a664159fe159c8fd9a8b4624fdad91560a0e8881f908dca5"
    );
    // Nothing hex-encoded at all: no transaction could be in it.
    assert!(!body.contains("0x"));
}

#[test]
fn the_probe_is_framed_like_the_runner_frames_a_bundle() {
    let body = probe_body("q5a-club48-w-000");
    let request = frame_request("puissant-builder.48.club", None, &body);
    let expected = format!(
        "POST / HTTP/1.1\r\nHost: puissant-builder.48.club\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    assert_eq!(request, expected.into_bytes());

    let with = frame_request("virginia.builder.blockrazor.io", Some("token-value"), &body);
    let text = String::from_utf8(with).unwrap();
    assert!(text.contains("\r\nAuthorization: token-value\r\n\r\n"));
    assert!(text.ends_with(&body));
}

#[test]
fn sha256_is_the_standard_one() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

#[test]
fn percentiles_are_nearest_rank_and_the_limit_itself_is_a_miss() {
    let values: Vec<u64> = (1..=100).collect();
    let summary = summarize(&values, 100);
    assert_eq!(summary.count, 100);
    assert_eq!(summary.p50, Some(50));
    assert_eq!(summary.p95, Some(95));
    assert_eq!(summary.p99, Some(99));
    assert_eq!(summary.max, Some(100));
    // 100 is at the limit: a miss.
    assert_eq!(summary.at_or_above_limit, 1);
    assert!((summary.fraction_at_or_above_limit - 0.01).abs() < 1e-12);

    assert_eq!(nearest_rank(&[], 50), None);
    assert_eq!(nearest_rank(&[7], 1), Some(7));
    assert_eq!(nearest_rank(&[1, 2, 3], 50), Some(2));
    let empty = summarize(&[], MISS_NS);
    assert_eq!(
        (empty.count, empty.max, empty.at_or_above_limit),
        (0, None, 0)
    );
    assert!(empty.fraction_at_or_above_limit.abs() < f64::EPSILON);
}

#[test]
fn no_percentile_hides_a_maximum_miss() {
    let mut values = vec![50_000u64; 99];
    values.push(5_000_000);
    let summary = summarize(&values, MISS_NS);
    assert_eq!(summary.p99, Some(50_000));
    assert_eq!(summary.max, Some(5_000_000));
    assert_eq!(summary.at_or_above_limit, 1);
}

// ---------------------------------------------------------------------------
// JSON and time
// ---------------------------------------------------------------------------

#[test]
fn json_escapes_and_renders_deterministically() {
    let value = Json::object([
        ("text", Json::str("a\"b\\c\nd\u{1}")),
        (
            "list",
            Json::Array(vec![Json::U64(1), Json::Null, Json::Bool(false)]),
        ),
        ("empty", Json::Array(Vec::new())),
        ("fraction", Json::F64(0.25)),
        ("nan", Json::F64(f64::NAN)),
        ("negative", Json::I64(-38_000)),
    ]);
    assert_eq!(
        value.render(),
        "{\n  \"text\": \"a\\\"b\\\\c\\nd\\u0001\",\n  \"list\": [\n    1,\n    null,\n    \
         false\n  ],\n  \"empty\": [],\n  \"fraction\": 0.250000,\n  \"nan\": null,\n  \
         \"negative\": -38000\n}\n"
    );
}

#[test]
fn timestamps_are_rfc3339_utc() {
    assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
    assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(rfc3339(1_709_164_800), "2024-02-29T00:00:00Z");
    assert_eq!(rfc3339(1_759_622_400), "2025-10-05T00:00:00Z");
    assert_eq!(rfc3339(4_102_444_799), "2099-12-31T23:59:59Z");
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

#[test]
fn a_complete_answer_keeps_the_connection_and_anything_else_ends_it() {
    let cases = [
        (
            ResponseOutcome::Ack,
            ResponseClass::Ack,
            ConnectionState::Ready,
        ),
        (
            ResponseOutcome::Reject {
                code: Some(-38_000),
            },
            ResponseClass::Reject {
                code: Some(-38_000),
            },
            ConnectionState::Ready,
        ),
        (
            ResponseOutcome::RateLimited,
            ResponseClass::RateLimited,
            ConnectionState::Ready,
        ),
        (
            ResponseOutcome::Protocol(ProtocolFailure::HttpStatus(400)),
            ResponseClass::HttpStatus(400),
            ConnectionState::Ready,
        ),
        (
            ResponseOutcome::Protocol(ProtocolFailure::WrongId),
            ResponseClass::Malformed(ProtocolFailure::WrongId),
            ConnectionState::Stale,
        ),
        (
            ResponseOutcome::Connection,
            ResponseClass::Connection,
            ConnectionState::Disconnected,
        ),
    ];
    for (outcome, class, state) in cases {
        assert_eq!(classify(outcome), class);
        assert_eq!(state_after_response(class), state);
    }
    assert_eq!(
        state_after_response(ResponseClass::Timeout),
        ConnectionState::Stale
    );
    assert!(!ResponseClass::Timeout.answered());
}

// ---------------------------------------------------------------------------
// Acceptance rules on synthetic runs
// ---------------------------------------------------------------------------

fn peer() -> SocketAddr {
    "192.0.2.1:443".parse().unwrap()
}

fn facts(generation: u64) -> ConnectionFacts {
    ConnectionFacts {
        generation,
        peer: peer(),
        tls_version: rustls::ProtocolVersion::TLSv1_3,
        cipher_suite: rustls::CipherSuite::TLS13_AES_128_GCM_SHA256,
        http: HttpVersion::Http11,
        alpn_selected: true,
    }
}

fn handed(phase: Phase, index: u32, generation: u64, handoff_ns: u64) -> Sample {
    Sample {
        phase,
        index,
        generation: Some(generation),
        peer: Some(peer()),
        state_before: ConnectionState::Ready,
        request_bytes: 230,
        body_bytes: 144,
        body_sha256: "00".repeat(32),
        outcome: SampleOutcome::HandedOff,
        prepare_ns: Some(20_000),
        t_write_check_ns: Some(1_000),
        t_boundary_ns: Some(1_000 + handoff_ns),
        write_check_to_boundary_ns: Some(handoff_ns),
        wire_bytes: Some(260),
        response: Some(ResponseClass::Reject {
            code: Some(-38_000),
        }),
        response_latency_ns: Some(2_000_000),
        state_after: ConnectionState::Ready,
    }
}

fn identity() -> Identity {
    Identity {
        baseline: EXPECTED_BASELINE.to_owned(),
        git_head: "a".repeat(40),
        git_dirty: "false".to_owned(),
        baseline_ancestor: "true".to_owned(),
        frozen_sources_unchanged: "true".to_owned(),
        rustc: "rustc 1.93.1".to_owned(),
        cargo: "cargo 1.93.1".to_owned(),
        rustls: "0.23.45".to_owned(),
        ring: "0.17.14".to_owned(),
        profile: "release".to_owned(),
        os: "macos".to_owned(),
        arch: "aarch64".to_owned(),
        transport_implementation: TLS_TRANSPORT_IMPLEMENTATION.to_owned(),
        harness: "u5_tls_qualify v1".to_owned(),
    }
}

/// A run that meets every rule.
fn passing(provider: &'static str, auth: AuthScope) -> ProviderRun {
    let mut samples = Vec::new();
    for index in 0..10 {
        samples.push(handed(Phase::Warmup, index, 1, 40_000));
    }
    for index in 0..100 {
        samples.push(handed(Phase::Measured, index, 1, 30_000 + u64::from(index)));
    }
    let reconnects = (0..3)
        .map(|cycle| ReconnectCycle {
            cycle,
            previous: Some(facts(u64::from(cycle) + 1)),
            result: Ok(facts(u64::from(cycle) + 2)),
            previous_still_current: false,
            probe: Some(handed(
                Phase::Reconnect,
                cycle,
                u64::from(cycle) + 2,
                35_000,
            )),
        })
        .collect();
    ProviderRun {
        provider,
        host: "builder.example".to_owned(),
        port: 443,
        auth,
        policy: QUALIFICATION_POLICY,
        workload: QUALIFICATION_WORKLOAD,
        connect: Ok(facts(1)),
        started_at: "2026-10-05T00:00:00Z".to_owned(),
        ended_at: "2026-10-05T00:00:30Z".to_owned(),
        samples,
        reconnects,
    }
}

fn measured(run: &mut ProviderRun, index: usize) -> &mut Sample {
    &mut run.samples[10 + index]
}

/// Gives a sample a handoff of `ns`, timestamps and duration together.
fn set_handoff(sample: &mut Sample, ns: u64) {
    let check = sample.t_write_check_ns.unwrap();
    sample.t_boundary_ns = Some(check + ns);
    sample.write_check_to_boundary_ns = Some(ns);
}

fn verdict_of(run: &ProviderRun) -> (Verdict, Vec<Reason>) {
    let verdict = evaluate(run, &identity());
    (verdict.transport, verdict.reasons)
}

/// Asserts the verdict and exactly these reasons, in any order.
fn judged(run: &ProviderRun, verdict: Verdict, reasons: &[Reason]) {
    let (got, got_reasons) = verdict_of(run);
    assert_eq!(got, verdict, "{got_reasons:?}");
    assert_eq!(got_reasons.len(), reasons.len(), "{got_reasons:?}");
    for reason in reasons {
        assert!(
            got_reasons.contains(reason),
            "{reason:?} in {got_reasons:?}"
        );
    }
}

#[test]
fn a_run_that_meets_every_rule_passes() {
    let run = passing("club48", AuthScope::NotRequired);
    let verdict = evaluate(&run, &identity());
    assert_eq!(verdict.transport, Verdict::Pass);
    assert!(verdict.reasons.is_empty());
    assert_eq!(verdict.api, ApiScope::AllAnswered);
    assert_eq!(verdict.summary.count, 100);
    assert_eq!(verdict.summary.max, Some(30_099));
    assert_eq!(verdict.handed_off, 113);
}

#[test]
fn a_handoff_of_one_millisecond_fails_and_one_nanosecond_less_does_not() {
    let mut run = passing("club48", AuthScope::NotRequired);
    set_handoff(measured(&mut run, 57), 999_999);
    assert_eq!(verdict_of(&run).0, Verdict::Pass);
    set_handoff(measured(&mut run, 57), 1_000_000);
    assert_eq!(
        verdict_of(&run),
        (Verdict::Fail, vec![Reason::DeadlineMisses(1)])
    );
}

#[test]
fn a_single_slow_handoff_fails_even_below_p99() {
    let mut run = passing("club48", AuthScope::NotRequired);
    set_handoff(measured(&mut run, 3), 5_000_000);
    let verdict = evaluate(&run, &identity());
    assert!(verdict.summary.p99.unwrap() < MISS_NS);
    assert_eq!(verdict.summary.max, Some(5_000_000));
    assert_eq!(verdict.transport, Verdict::Fail);
}

#[test]
fn a_skip_or_an_unknown_anywhere_fails() {
    let mut run = passing("club48", AuthScope::NotRequired);
    let sample = measured(&mut run, 10);
    sample.outcome = SampleOutcome::Skipped(ConnectionState::Stale);
    sample.write_check_to_boundary_ns = None;
    judged(
        &run,
        Verdict::Fail,
        &[
            Reason::Skipped(1),
            Reason::MeasuredTimingMissing {
                expected: 100,
                recorded: 99,
            },
        ],
    );

    let mut run = passing("club48", AuthScope::NotRequired);
    run.samples[2].outcome = SampleOutcome::Unknown {
        stage: Stage::Flush,
        error: std::io::ErrorKind::WouldBlock,
    };
    assert_eq!(
        verdict_of(&run),
        (Verdict::Fail, vec![Reason::TransportFailures(1)])
    );
}

#[test]
fn fewer_measured_samples_or_another_workload_is_incomplete() {
    let mut run = passing("club48", AuthScope::NotRequired);
    run.samples.pop();
    judged(
        &run,
        Verdict::Incomplete,
        &[
            Reason::MeasuredSamplesMissing {
                expected: 100,
                recorded: 99,
            },
            Reason::MeasuredTimingMissing {
                expected: 100,
                recorded: 99,
            },
        ],
    );

    let mut run = passing("club48", AuthScope::NotRequired);
    run.workload = Workload {
        pacing: Duration::from_millis(100),
        ..QUALIFICATION_WORKLOAD
    };
    assert_eq!(
        verdict_of(&run),
        (Verdict::Incomplete, vec![Reason::WorkloadNotQualification])
    );
}

#[test]
fn a_failure_outweighs_incompleteness() {
    let mut run = passing("club48", AuthScope::NotRequired);
    run.samples.pop();
    set_handoff(measured(&mut run, 0), 2_000_000);
    assert_eq!(verdict_of(&run).0, Verdict::Fail);
}

#[test]
fn a_missing_authorization_measures_the_handshake_only_and_stays_incomplete() {
    let mut run = passing("blockrazor", AuthScope::Missing);
    run.samples.clear();
    run.reconnects.clear();
    let verdict = evaluate(&run, &identity());
    assert_eq!(verdict.transport, Verdict::Incomplete);
    assert_eq!(verdict.reasons, vec![Reason::AuthorizationMissing]);
    assert_eq!(verdict.api, ApiScope::AuthMissing);
}

#[test]
fn a_failed_connection_fails() {
    let mut run = passing("club48", AuthScope::NotRequired);
    run.connect = Err("tls connect: certificate refused".to_owned());
    run.samples.clear();
    run.reconnects.clear();
    assert_eq!(
        verdict_of(&run),
        (Verdict::Fail, vec![Reason::ConnectFailed])
    );
}

#[test]
fn a_handed_off_sample_without_its_timing_is_never_a_pass() {
    // 100 measured samples, all handed off, one without its duration: it
    // must not quietly drop out of the statistics.
    let mut run = passing("club48", AuthScope::NotRequired);
    measured(&mut run, 42).write_check_to_boundary_ns = None;
    let verdict = evaluate(&run, &identity());
    assert_ne!(verdict.transport, Verdict::Pass);
    assert_eq!(verdict.summary.count, 99);
    assert!(verdict.reasons.contains(&Reason::MeasuredTimingMissing {
        expected: 100,
        recorded: 99,
    }));
    assert!(verdict.reasons.contains(&Reason::InconsistentEvidence {
        samples: 1,
        phase: Phase::Measured,
        index: 42,
        issue: EvidenceIssue::MissingDuration,
    }));
}

#[test]
fn every_handoff_must_be_complete_and_consistent() {
    let scope = facts(1);
    let other_peer: SocketAddr = "198.51.100.7:443".parse().unwrap();
    let cases: [SampleCase; 9] = [
        (|s| s.generation = None, EvidenceIssue::MissingGeneration),
        (|s| s.peer = None, EvidenceIssue::MissingPeer),
        (
            |s| s.t_write_check_ns = None,
            EvidenceIssue::MissingWriteCheck,
        ),
        (|s| s.t_boundary_ns = None, EvidenceIssue::MissingBoundary),
        (|s| s.wire_bytes = None, EvidenceIssue::MissingWireBytes),
        (
            |s| s.t_boundary_ns = Some(500),
            EvidenceIssue::BoundaryBeforeWriteCheck,
        ),
        (
            |s| s.t_boundary_ns = s.t_boundary_ns.map(|t| t + 1),
            EvidenceIssue::DurationMismatch,
        ),
        (
            |s| s.write_check_to_boundary_ns = Some(1),
            EvidenceIssue::DurationMismatch,
        ),
        (|s| s.wire_bytes = Some(0), EvidenceIssue::ZeroWireBytes),
    ];
    for (change, issue) in cases {
        let mut sample = handed(Phase::Measured, 3, 1, 40_000);
        assert_eq!(handoff_issue(&sample, &scope), None);
        change(&mut sample);
        assert_eq!(handoff_issue(&sample, &scope), Some(issue));

        // The same contradiction in a full run is never a pass.
        let mut run = passing("club48", AuthScope::NotRequired);
        change(measured(&mut run, 3));
        assert_ne!(verdict_of(&run).0, Verdict::Pass, "{issue:?}");
    }
    let mut sample = handed(Phase::Measured, 3, 1, 40_000);
    sample.peer = Some(other_peer);
    assert_eq!(
        handoff_issue(&sample, &scope),
        Some(EvidenceIssue::PeerMismatch)
    );
    // Only a handoff is judged.
    sample.outcome = SampleOutcome::Skipped(ConnectionState::Stale);
    assert_eq!(handoff_issue(&sample, &scope), None);
}

#[test]
fn a_reconnect_probe_belongs_to_its_new_connection() {
    let mut run = passing("club48", AuthScope::NotRequired);
    if let Some(probe) = run.reconnects[1].probe.as_mut() {
        probe.peer = Some("198.51.100.7:443".parse().unwrap());
    }
    judged(
        &run,
        Verdict::Incomplete,
        &[Reason::InconsistentEvidence {
            samples: 1,
            phase: Phase::Reconnect,
            index: 1,
            issue: EvidenceIssue::PeerMismatch,
        }],
    );
}

#[test]
fn a_build_flag_with_an_unexpected_value_is_never_accepted() {
    let run = passing("club48", AuthScope::NotRequired);
    let cases: [IdentityCase; 6] = [
        (
            |i| i.baseline_ancestor = "banana".to_owned(),
            Reason::IdentityInvalid("baseline_ancestor"),
        ),
        (
            |i| i.git_dirty = "banana".to_owned(),
            Reason::IdentityInvalid("git_dirty"),
        ),
        (
            |i| i.frozen_sources_unchanged = "banana".to_owned(),
            Reason::IdentityInvalid("frozen_sources_unchanged"),
        ),
        (
            |i| i.git_head = "not-a-commit".to_owned(),
            Reason::IdentityInvalid("git_head"),
        ),
        (
            |i| i.transport_implementation = "another transport".to_owned(),
            Reason::IdentityInvalid("transport_implementation"),
        ),
        (|i| i.profile = "banana".to_owned(), Reason::NotReleaseBuild),
    ];
    for (change, reason) in cases {
        let mut changed = identity();
        change(&mut changed);
        let verdict = evaluate(&run, &changed);
        assert_eq!(verdict.transport, Verdict::Incomplete, "{reason:?}");
        assert_eq!(verdict.reasons, vec![reason]);
    }
    // "unknown" is unknown, for each flag.
    let mut unknown = identity();
    unknown.git_dirty = "unknown".to_owned();
    unknown.baseline_ancestor = "unknown".to_owned();
    unknown.frozen_sources_unchanged = "unknown".to_owned();
    let verdict = evaluate(&run, &unknown);
    assert_eq!(verdict.transport, Verdict::Incomplete);
    assert_eq!(
        verdict.reasons,
        vec![
            Reason::IdentityUnknown("git_dirty"),
            Reason::IdentityUnknown("baseline_ancestor"),
            Reason::IdentityUnknown("frozen_sources_unchanged"),
        ]
    );
}

#[test]
fn every_gap_in_the_identity_leaves_the_run_incomplete() {
    let run = passing("club48", AuthScope::NotRequired);
    let cases: [IdentityCase; 7] = [
        (
            |i| i.git_dirty = "true".to_owned(),
            Reason::WorkingTreeDirty,
        ),
        (
            |i| i.frozen_sources_unchanged = "false".to_owned(),
            Reason::FrozenSourcesChanged,
        ),
        (|i| i.profile = "debug".to_owned(), Reason::NotReleaseBuild),
        (|i| i.rustls = "0.23.46".to_owned(), Reason::RustlsVersion),
        (|i| i.ring = "0.17.15".to_owned(), Reason::RingVersion),
        (
            |i| i.baseline_ancestor = "false".to_owned(),
            Reason::BaselineNotAncestor,
        ),
        (
            |i| i.git_head = "unknown".to_owned(),
            Reason::IdentityUnknown("git_head"),
        ),
    ];
    for (change, reason) in cases {
        let mut changed = identity();
        change(&mut changed);
        let verdict = evaluate(&run, &changed);
        assert_eq!(verdict.transport, Verdict::Incomplete, "{reason:?}");
        assert_eq!(verdict.reasons, vec![reason]);
    }
}

#[test]
fn every_reconnect_must_advance_retire_and_probe() {
    let mut run = passing("club48", AuthScope::NotRequired);
    run.reconnects[1].result = Ok(facts(2));
    if let Some(probe) = run.reconnects[1].probe.as_mut() {
        probe.generation = Some(2);
    }
    assert_eq!(
        verdict_of(&run),
        (Verdict::Fail, vec![Reason::GenerationNotIncreased(1)])
    );

    let mut run = passing("club48", AuthScope::NotRequired);
    run.reconnects[0].previous_still_current = true;
    assert_eq!(
        verdict_of(&run),
        (Verdict::Fail, vec![Reason::PreviousFactsStillCurrent(0)])
    );

    let mut run = passing("club48", AuthScope::NotRequired);
    if let Some(probe) = run.reconnects[2].probe.as_mut() {
        probe.response = Some(ResponseClass::Timeout);
    }
    assert_eq!(
        verdict_of(&run),
        (Verdict::Fail, vec![Reason::ReconnectProbeFailed(2)])
    );

    let mut run = passing("club48", AuthScope::NotRequired);
    run.reconnects[2].result = Err("tls connect: handshake timed out".to_owned());
    run.reconnects[2].probe = None;
    assert_eq!(
        verdict_of(&run),
        (Verdict::Fail, vec![Reason::ReconnectFailed(2)])
    );

    let mut run = passing("club48", AuthScope::NotRequired);
    run.reconnects.pop();
    assert_eq!(
        verdict_of(&run),
        (
            Verdict::Incomplete,
            vec![Reason::ReconnectCyclesMissing {
                expected: 3,
                recorded: 2
            }]
        )
    );
}

#[test]
fn the_measured_series_must_stay_on_one_connection() {
    let mut run = passing("club48", AuthScope::NotRequired);
    measured(&mut run, 50).generation = Some(2);
    assert_eq!(
        verdict_of(&run),
        (Verdict::Fail, vec![Reason::ConnectionChangedDuringSeries])
    );
    // Also against the initial connection, not only within the series.
    let mut run = passing("club48", AuthScope::NotRequired);
    run.connect = Ok(facts(5));
    for cycle in &mut run.reconnects {
        cycle.previous = cycle.previous.map(|f| facts(f.generation + 4));
        cycle.result = cycle.result.clone().map(|f| facts(f.generation + 4));
        if let Some(probe) = cycle.probe.as_mut() {
            probe.generation = probe.generation.map(|g| g + 4);
        }
    }
    assert_eq!(
        verdict_of(&run),
        (Verdict::Fail, vec![Reason::ConnectionChangedDuringSeries])
    );
}

#[test]
fn the_api_scope_is_reported_beside_the_transport_verdict() {
    let mut run = passing("blockrazor", AuthScope::Present);
    measured(&mut run, 0).response = Some(ResponseClass::HttpStatus(401));
    let verdict = evaluate(&run, &identity());
    // The transport handed everything off; the builder refused the
    // authorization. Two separate statements.
    assert_eq!(verdict.transport, Verdict::Pass);
    assert_eq!(verdict.api, ApiScope::AuthRejected);

    let mut run = passing("club48", AuthScope::NotRequired);
    measured(&mut run, 9).response = Some(ResponseClass::Timeout);
    assert_eq!(evaluate(&run, &identity()).api, ApiScope::NotAllAnswered);
}

#[test]
fn the_overall_verdict_needs_both_builders_to_pass() {
    let club48 = passing("club48", AuthScope::NotRequired);
    let blockrazor = passing("blockrazor", AuthScope::Present);
    let both = [club48.clone(), blockrazor.clone()];
    let verdicts: Vec<_> = both.iter().map(|run| evaluate(run, &identity())).collect();
    assert_eq!(overall(&both, &verdicts), Verdict::Pass);

    let one = [club48.clone()];
    let verdicts: Vec<_> = one.iter().map(|run| evaluate(run, &identity())).collect();
    assert_eq!(overall(&one, &verdicts), Verdict::Incomplete);

    let mut failing = blockrazor;
    set_handoff(&mut failing.samples[20], MISS_NS);
    let mixed = [club48, failing];
    let verdicts: Vec<_> = mixed.iter().map(|run| evaluate(run, &identity())).collect();
    assert_eq!(overall(&mixed, &verdicts), Verdict::Fail);
}

// ---------------------------------------------------------------------------
// Evidence document
// ---------------------------------------------------------------------------

#[test]
fn the_evidence_binds_identity_workload_and_verdict_and_holds_no_secret() {
    let runs = [
        passing("club48", AuthScope::NotRequired),
        passing("blockrazor", AuthScope::Present),
    ];
    let verdicts: Vec<_> = runs.iter().map(|run| evaluate(run, &identity())).collect();
    let text = evidence_json(&identity(), &runs, &verdicts, overall(&runs, &verdicts)).render();
    for needle in [
        SCHEMA,
        EXPECTED_BASELINE,
        "\"tls_transport_qualification\": \"PASS\"",
        "\"auth_scope\": \"PRESENT\"",
        "\"transport_verdict\": \"PASS\"",
        "\"body_sha256\"",
        "\"write_check_to_boundary_ns\"",
        "\"not_qualified_by_this_evidence\"",
        "\"pacing_ms\": 200",
        "\"rustls\": \"0.23.45\"",
    ] {
        assert!(text.contains(needle), "{needle}");
    }
    assert!(!text.contains("Authorization"));
    assert!(contains_secret(&text, "PRESENT"));
    assert!(!contains_secret(&text, "a-token-that-was-never-written"));
    assert!(!contains_secret(&text, ""));
}

/// Plan 27.10.: the Q5A builder probe sends Mainnet `eth_sendBundle`
/// requests, so it needs BSC_ECON_SEND ENABLED in the native owner state of
/// this boot; without it (no state under HOME) it stops before any file,
/// connection or request.
#[test]
fn the_q5a_probe_refuses_without_the_send_switch() {
    let home = std::env::temp_dir().join(format!("q5a-switch-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let evidence = home.join("evidence.json");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_u5_tls_qualify"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("Q5A_EVIDENCE_PATH", &evidence)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    // A HOME other than the account home is never the switch state.
    assert!(
        err.contains("refused by the native switches: CONTROL_STATE_UNREADABLE")
            && err.contains("HOME is not the account home")
            && err.contains("nothing sent"),
        "{err}"
    );
    assert!(!evidence.exists(), "no evidence file reserved");
    std::fs::remove_dir_all(&home).unwrap();
}

/// Plan 27.10.: the Q5A probe requires BSC_ECON_SEND before it reserves its
/// evidence file or opens any connection (the process test above only
/// reaches the HOME refusal).
#[test]
fn the_q5a_probe_requires_the_send_switch_before_any_egress() {
    let source = include_str!("../src/bin/u5_tls_qualify.rs");
    let run = &source[source.find("fn run()").unwrap()..];
    let send = run.find("SwitchId::Send").unwrap();
    for later in ["OpenOptions::new()", "run_provider(", "TlsEndpoint"] {
        if let Some(at) = run.find(later) {
            assert!(send < at, "{later}");
        }
    }
    assert!(run[..send + 200].contains("nothing sent"));
}
