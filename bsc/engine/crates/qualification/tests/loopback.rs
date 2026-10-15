//! The Q5A measurement end to end, over the frozen TLS transport, against a
//! loopback TLS server with test certificates. No real provider.

mod support;

use std::time::Duration;

use darkbot_bsc_execution::submission::tls::{AuthorizationToken, TLS_TRANSPORT_IMPLEMENTATION};
use darkbot_bsc_execution::submission::transport::{ConnectionState, MonotonicClock};
use darkbot_bsc_qualification::evidence::{
    AuthScope, Phase, ResponseClass, SampleOutcome, Workload, evidence_json,
};
use darkbot_bsc_qualification::identity::{EXPECTED_BASELINE, Identity};
use darkbot_bsc_qualification::measure::{Auth, Provider, QUALIFICATION_POLICY, run_provider};
use darkbot_bsc_qualification::verdict::{ApiScope, Reason, Verdict, evaluate, overall};
use support::{Answer, EchoServer, TEST_CA, UNTRUSTED_CA, trusting};

/// A short workload: the measurement as it is, the counts smaller.
const SHORT: Workload = Workload {
    warmup: 2,
    measured: 5,
    pacing: Duration::from_millis(1),
    reconnect_cycles: 3,
};

fn identity() -> Identity {
    Identity {
        baseline: EXPECTED_BASELINE.to_owned(),
        git_head: "b".repeat(40),
        git_dirty: "false".to_owned(),
        baseline_ancestor: "true".to_owned(),
        frozen_sources_unchanged: "true".to_owned(),
        rustc: "rustc 1.93.1".to_owned(),
        cargo: "cargo 1.93.1".to_owned(),
        rustls: "0.23.45".to_owned(),
        ring: "0.17.14".to_owned(),
        profile: "release".to_owned(),
        os: "test".to_owned(),
        arch: "test".to_owned(),
        transport_implementation: TLS_TRANSPORT_IMPLEMENTATION.to_owned(),
        harness: "test".to_owned(),
    }
}

fn provider(server: &EchoServer, auth: Auth) -> Provider {
    Provider {
        name: "club48",
        endpoint: server.endpoint(),
        auth,
    }
}

/// Reasons that only say the run was short, which is all a loopback run
/// may differ by. A handoff above 1 ms is possible on a loaded machine and
/// is tolerated here; the real run judges it.
fn transport_reasons(reasons: &[Reason]) -> Vec<Reason> {
    reasons
        .iter()
        .copied()
        .filter(|reason| {
            !matches!(
                reason,
                Reason::WorkloadNotQualification | Reason::DeadlineMisses(_)
            )
        })
        .collect()
}

#[test]
fn a_run_measures_every_sample_over_one_connection_and_reconnects_explicitly() {
    let server = EchoServer::start(Answer::RejectEcho);
    let clock = MonotonicClock::start(7);
    let run = run_provider(
        &provider(&server, Auth::NotRequired),
        &trusting(TEST_CA),
        QUALIFICATION_POLICY,
        SHORT,
        &clock,
    );

    let facts = run.connect.clone().unwrap();
    assert_eq!(facts.generation, 1);
    assert_eq!(run.samples.len(), 7);
    for (position, sample) in run.samples.iter().enumerate() {
        let phase = if position < 2 {
            Phase::Warmup
        } else {
            Phase::Measured
        };
        assert_eq!(sample.phase, phase);
        assert_eq!(sample.outcome, SampleOutcome::HandedOff, "{position}");
        assert_eq!(sample.generation, Some(1));
        assert_eq!(sample.peer, Some(server.address));
        assert_eq!(sample.state_before, ConnectionState::Ready);
        assert_eq!(sample.state_after, ConnectionState::Ready);
        assert_eq!(
            sample.response,
            Some(ResponseClass::Reject {
                code: Some(-32_602)
            })
        );
        let (check, boundary) = (
            sample.t_write_check_ns.unwrap(),
            sample.t_boundary_ns.unwrap(),
        );
        assert!(boundary >= check);
        assert_eq!(sample.write_check_to_boundary_ns, Some(boundary - check));
        assert!(sample.prepare_ns.is_some());
        assert!(sample.response_latency_ns.is_some());
        // The kernel took the TLS records: more than the plaintext.
        assert!(sample.wire_bytes.unwrap() > sample.request_bytes as u64);
        assert_eq!(sample.body_sha256.len(), 64);
    }
    // One connection for the series, then one per reconnect.
    assert_eq!(run.reconnects.len(), 3);
    for (cycle, reconnect) in run.reconnects.iter().enumerate() {
        let generation = cycle as u64 + 2;
        assert_eq!(reconnect.previous.unwrap().generation, generation - 1);
        assert_eq!(reconnect.result.clone().unwrap().generation, generation);
        assert!(!reconnect.previous_still_current);
        let probe = reconnect.probe.as_ref().unwrap();
        assert_eq!(probe.outcome, SampleOutcome::HandedOff);
        assert_eq!(probe.generation, Some(generation));
    }
    assert_eq!(server.accepted(), 4);
    assert_eq!(server.requests().len(), 10);

    let verdict = evaluate(&run, &identity());
    assert_eq!(transport_reasons(&verdict.reasons), Vec::<Reason>::new());
    assert!(verdict.reasons.contains(&Reason::WorkloadNotQualification));
    assert_ne!(verdict.transport, Verdict::Pass);
    assert_eq!(verdict.api, ApiScope::AllAnswered);
}

#[test]
fn the_authorization_is_sent_and_never_recorded() {
    const TOKEN: &str = "q5a-loopback-secret-token";
    let server = EchoServer::start(Answer::RejectEcho);
    let token = AuthorizationToken::new(TOKEN.to_owned()).unwrap();
    let mut provider = provider(&server, Auth::Token(token));
    provider.name = "blockrazor";
    let run = run_provider(
        &provider,
        &trusting(TEST_CA),
        QUALIFICATION_POLICY,
        SHORT,
        &MonotonicClock::start(7),
    );
    assert_eq!(run.auth, AuthScope::Present);
    let header = format!("\r\nAuthorization: {TOKEN}\r\n");
    let requests = server.requests();
    assert_eq!(requests.len(), 10);
    assert!(
        requests
            .iter()
            .all(|request| String::from_utf8_lossy(request).contains(&header))
    );

    let runs = [run];
    let verdicts: Vec<_> = runs.iter().map(|run| evaluate(run, &identity())).collect();
    let text = evidence_json(&identity(), &runs, &verdicts, overall(&runs, &verdicts)).render();
    assert!(!text.contains(TOKEN));
    assert!(!text.contains("Authorization"));
    assert!(text.contains("\"auth_scope\": \"PRESENT\""));
}

#[test]
fn without_an_authorization_only_the_handshake_is_measured() {
    let server = EchoServer::start(Answer::RejectEcho);
    let run = run_provider(
        &provider(&server, Auth::Missing),
        &trusting(TEST_CA),
        QUALIFICATION_POLICY,
        SHORT,
        &MonotonicClock::start(7),
    );
    assert!(run.connect.is_ok());
    assert!(run.samples.is_empty());
    assert!(run.reconnects.is_empty());
    assert!(server.requests().is_empty());
    let verdict = evaluate(&run, &identity());
    assert!(verdict.reasons.contains(&Reason::AuthorizationMissing));
    assert_eq!(verdict.transport, Verdict::Incomplete);
    assert_eq!(verdict.api, ApiScope::AuthMissing);
}

#[test]
fn a_refused_authorization_keeps_the_connection_and_is_reported_as_such() {
    let server = EchoServer::start(Answer::Unauthorized);
    let token = AuthorizationToken::new("wrong-token".to_owned()).unwrap();
    let run = run_provider(
        &provider(&server, Auth::Token(token)),
        &trusting(TEST_CA),
        QUALIFICATION_POLICY,
        SHORT,
        &MonotonicClock::start(7),
    );
    assert!(run.samples.iter().all(|sample| {
        sample.outcome == SampleOutcome::HandedOff
            && sample.response == Some(ResponseClass::HttpStatus(401))
            && sample.state_after == ConnectionState::Ready
    }));
    let verdict = evaluate(&run, &identity());
    assert_eq!(verdict.api, ApiScope::AuthRejected);
    assert_eq!(transport_reasons(&verdict.reasons), Vec::<Reason>::new());
}

#[test]
fn a_closed_connection_ends_the_series_without_repair() {
    let server = EchoServer::start(Answer::CloseAfterRequest);
    let run = run_provider(
        &provider(&server, Auth::NotRequired),
        &trusting(TEST_CA),
        QUALIFICATION_POLICY,
        SHORT,
        &MonotonicClock::start(7),
    );
    // The first probe finds the connection closed; every later one in the
    // series is skipped, nothing reconnects until the explicit cycles.
    assert_eq!(run.samples[0].response, Some(ResponseClass::Connection));
    assert_eq!(run.samples[0].state_after, ConnectionState::Disconnected);
    assert!(
        run.samples[1..]
            .iter()
            .all(|sample| sample.outcome == SampleOutcome::Skipped(ConnectionState::Disconnected))
    );
    assert_eq!(server.accepted(), 1 + 3);
    let verdict = evaluate(&run, &identity());
    assert_eq!(verdict.transport, Verdict::Fail);
    assert!(verdict.reasons.contains(&Reason::Skipped(6)));
}

#[test]
fn an_untrusted_certificate_fails_the_connection() {
    let server = EchoServer::start(Answer::RejectEcho);
    let run = run_provider(
        &provider(&server, Auth::NotRequired),
        &trusting(UNTRUSTED_CA),
        QUALIFICATION_POLICY,
        SHORT,
        &MonotonicClock::start(7),
    );
    assert!(run.connect.is_err());
    assert!(run.samples.is_empty());
    let verdict = evaluate(&run, &identity());
    assert_eq!(verdict.transport, Verdict::Fail);
    assert!(verdict.reasons.contains(&Reason::ConnectFailed));
}
