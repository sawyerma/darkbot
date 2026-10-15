//! The TLS transport against a loopback TLS server with test certificates:
//! real handshakes, real sockets, the real dispatch. No real provider is
//! contacted, and nothing here qualifies anything.

mod common;
mod tls_support;

use std::thread;
use std::time::{Duration, Instant};

use common::*;
use darkbot_core::inclusion::capability::{AdapterKind, AdapterTable, CapabilityId};
use darkbot_core::inclusion::plan::TransportDeadlineError;
use darkbot_core::inclusion::plan::{MergePolicy, SubmissionPlan};
use darkbot_core::intent::{HaltReason, IntentState, SubmissionState};
use darkbot_runner::inclusion::frame::Frame;
use darkbot_runner::inclusion::response::{ProtocolFailure, ResponseBuffer, ResponseOutcome};
use darkbot_runner::inclusion::send::{
    Armed, DispatchError, FanoutReport, Poll, SendOutcome, SendTime, UnknownCause,
    dispatch_diagnostic, poll_response,
};
use darkbot_runner::inclusion::tls::{
    AuthorizationToken, BLOCKRAZOR_DOCUMENTED_HOST, CLUB48_DOCUMENTED_HOST, ConnectError,
    HTTPS_PORT, HttpVersion, Maintenance, PolicyError, ProviderTarget, SetupError,
    TLS_TRANSPORT_IMPLEMENTATION, TargetError, TlsConnectionOwner, TlsEndpoint, TlsPolicy,
    TlsSetup, tls_output_capacity,
};
use darkbot_runner::inclusion::transport::{ConnectionState, Transport};
use darkbot_runner::inclusion::{blockrazor, club48};
use tls_support::*;

const TOKEN: &str = "test-token";

fn club48_target(server: &TestServer) -> ProviderTarget {
    ProviderTarget::club48(endpoint(server))
}

fn blockrazor_target(server: &TestServer) -> ProviderTarget {
    let token = AuthorizationToken::new(TOKEN.to_owned()).unwrap();
    ProviderTarget::blockrazor(endpoint(server), token)
}

/// The frame of `target` for `plan`.
fn frame_for(
    world: &World,
    plan: &SubmissionPlan,
    table: &AdapterTable,
    target: &ProviderTarget,
    capacity: usize,
) -> Frame {
    let endpoint = target.frame_endpoint();
    match target.kind() {
        AdapterKind::Club48Builder => club48::frame(
            plan,
            table,
            adapter(CLUB48),
            &endpoint,
            &world.launch_raw,
            capacity,
        )
        .unwrap(),
        AdapterKind::BlockRazorBuilder => blockrazor::frame(
            plan,
            table,
            adapter(BLOCKRAZOR),
            &endpoint,
            &world.launch_raw,
            capacity,
        )
        .unwrap(),
        other => panic!("no builder: {other:?}"),
    }
}

/// The public bundle through 48Club alone.
fn club48_plan(world: &World, table: &AdapterTable) -> SubmissionPlan {
    plan(
        world,
        table,
        bundle_inputs(world, &[CLUB48], MergePolicy::NoMerge),
    )
}

/// Dispatches 48Club alone over the owner's transport, reading `times`
/// from the clock: select, pre-permit check, write check, boundary.
fn try_dispatch_club48(
    world: &mut World,
    plan: &SubmissionPlan,
    table: &AdapterTable,
    owner: &mut TlsConnectionOwner,
    frame: &mut Frame,
    times: &[u64],
) -> Result<FanoutReport, DispatchError> {
    let current = world.snapshot;
    let mut armed = [Armed {
        adapter: adapter(CLUB48),
        transport: owner.transport(),
        frame,
    }];
    dispatch_diagnostic(
        &mut world.lane,
        plan,
        &context(table, &current),
        &ScriptClock::new(CLOCK, times),
        &deadline(),
        &mut armed,
    )
}

/// Dispatches 48Club alone, well within the deadline.
fn dispatch_club48(
    world: &mut World,
    plan: &SubmissionPlan,
    table: &AdapterTable,
    owner: &mut TlsConnectionOwner,
    frame: &mut Frame,
) -> FanoutReport {
    try_dispatch_club48(world, plan, table, owner, frame, &[100, 150, 200, 300]).unwrap()
}

/// Polls the response of `index` until it is no longer pending, at most
/// five seconds. Returns it and how many polls were pending first.
fn poll_until_done(
    world: &mut World,
    owner: &mut TlsConnectionOwner,
    index: u8,
    frame: &mut Frame,
    report: &mut FanoutReport,
) -> (Poll, usize) {
    let mut buffer = ResponseBuffer::new(RESPONSE_CAPACITY);
    let clock = ScriptClock::new(CLOCK, &[900]);
    let started = Instant::now();
    let mut pending = 0;
    loop {
        let mut slot = Armed {
            adapter: adapter(index),
            transport: owner.transport(),
            frame: &mut *frame,
        };
        let poll = poll_response(&mut world.lane, &mut slot, &mut buffer, &clock, report);
        if poll != Poll::Pending || started.elapsed() > Duration::from_secs(5) {
            return (poll, pending);
        }
        pending += 1;
        thread::sleep(Duration::from_millis(2));
    }
}

fn answering(body: String) -> Behaviour {
    Behaviour::Answer {
        pieces: vec![http_json("HTTP/1.1 200 OK", &body)],
        gap: Duration::ZERO,
    }
}

fn submission(world: &World, index: u8) -> SubmissionState {
    world.lane.submission(world.ticket, adapter(index)).unwrap()
}

/// A world whose request is larger than any socket buffer: a 12 MiB
/// launch, hex-encoded into a request of about 24 MiB.
fn huge_world() -> World {
    World::prepared_with_launch(vec![0x02; 12 << 20])
}

const HUGE_FRAME: usize = 32 << 20;

// ---------------------------------------------------------------------------
// Establishing: outside any dispatch, complete, HTTP/1.1, verified
// ---------------------------------------------------------------------------

#[test]
fn the_owner_connects_outside_any_dispatch_and_is_ready_only_after_the_handshake() {
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let mut owner = owner(&server);
    // Making the owner performs no I/O.
    assert_eq!(owner.state(), ConnectionState::NotReady);
    assert_eq!(owner.generation(), 0);
    assert!(owner.profile().is_none());
    thread::sleep(Duration::from_millis(20));
    assert_eq!(server.accepted(), 0);

    let facts = owner.connect().unwrap();
    assert_eq!(owner.state(), ConnectionState::Ready);
    assert_eq!(facts.generation, 1);
    assert_eq!(facts.http, HttpVersion::Http11);
    assert!(facts.alpn_selected);
    assert_eq!(facts.tls_version, rustls::ProtocolVersion::TLSv1_3);
    assert_eq!(facts.peer, server.address);
    assert!(owner.is_current(&facts));
    assert!(wait_until(|| server.accepted() == 1));
}

#[test]
fn a_server_without_alpn_is_taken_as_http11() {
    let server = TestServer::start(Served::Localhost, ServerAlpn::Absent, Behaviour::Hold);
    let mut owner = owner(&server);
    let facts = owner.connect().unwrap();
    assert!(!facts.alpn_selected);
    assert_eq!(facts.http, HttpVersion::Http11);
    assert_eq!(owner.state(), ConnectionState::Ready);
}

#[test]
fn a_server_that_only_speaks_h2_is_refused() {
    let server = TestServer::start(Served::Localhost, ServerAlpn::H2Only, Behaviour::Hold);
    let mut owner = owner(&server);
    let error = owner.connect().unwrap_err();
    assert!(
        matches!(error, ConnectError::Tls(_) | ConnectError::Closed),
        "{error:?}"
    );
    assert_ne!(owner.state(), ConnectionState::Ready);
    assert!(owner.transport().facts().is_none());
}

#[test]
fn a_certificate_for_another_name_is_refused() {
    let server = TestServer::start(Served::OtherName, ServerAlpn::Http11, Behaviour::Hold);
    let mut owner = owner(&server);
    let error = owner.connect().unwrap_err();
    assert!(matches!(error, ConnectError::Certificate(_)), "{error:?}");
    assert_ne!(owner.state(), ConnectionState::Ready);
}

#[test]
fn an_untrusted_issuer_is_refused() {
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let mut owner = owner_with(&server, trusting(UNTRUSTED_CA), policy());
    let error = owner.connect().unwrap_err();
    assert!(matches!(error, ConnectError::Certificate(_)), "{error:?}");
    assert_ne!(owner.state(), ConnectionState::Ready);
}

#[test]
fn a_handshake_that_never_completes_times_out() {
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::SilentTcp);
    let quick = TlsPolicy {
        handshake_timeout: Duration::from_millis(200),
        ..policy()
    };
    let mut owner = owner_with(&server, trusting(TEST_CA), quick);
    let started = Instant::now();
    assert_eq!(owner.connect(), Err(ConnectError::HandshakeTimeout));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_ne!(owner.state(), ConnectionState::Ready);
}

#[test]
fn nothing_listening_is_a_connect_failure() {
    let address = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let endpoint = TlsEndpoint::new("localhost", address.port())
        .unwrap()
        .with_address(address);
    let mut owner = TlsConnectionOwner::new(endpoint, trusting(TEST_CA), policy()).unwrap();
    assert_eq!(owner.connect(), Err(ConnectError::Connect));
    assert_eq!(owner.state(), ConnectionState::NotReady);
}

// ---------------------------------------------------------------------------
// Dispatch and response over TLS
// ---------------------------------------------------------------------------

#[test]
fn a_dispatch_over_tls_hands_off_and_a_complete_response_makes_it_ready_again() {
    let mut world = World::prepared();
    let id = request_id(&world);
    let server = TestServer::start(
        Served::Localhost,
        ServerAlpn::Http11,
        answering(ack_body(&id)),
    );
    let mut owner = owner(&server);
    owner.connect().unwrap();
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );

    let mut report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);
    let record = *report.adapter(adapter(CLUB48)).unwrap();
    assert_eq!(record.outcome, SendOutcome::HandedOff);
    assert_eq!(owner.state(), ConnectionState::AwaitingResponse);
    // The frozen record does not qualify the boundary; the send time is
    // unknown.
    assert_eq!(record.send_time(), SendTime::BoundaryUnqualified);

    let (poll, _) = poll_until_done(&mut world, &mut owner, CLUB48, &mut frame, &mut report);
    assert_eq!(poll, Poll::Done(ResponseOutcome::Ack));
    assert_eq!(owner.state(), ConnectionState::Ready);
    assert_eq!(submission(&world, CLUB48), SubmissionState::Acked);
    // An acknowledgement is no inclusion.
    assert_eq!(world.lane.state(world.ticket), Ok(IntentState::Sending));

    // The server received exactly the framed request, for `localhost`.
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with(b"POST / HTTP/1.1\r\nHost: localhost\r\n"));
    let (launch, buy) = bundle_txs(&requests[0]);
    assert_eq!(launch, world.launch_raw);
    assert_eq!(buy, world.raw_buy);
    assert_eq!(server.accepted(), 1);
}

#[test]
fn both_builders_receive_the_same_bytes_over_tls() {
    let mut world = World::prepared();
    let id = request_id(&world);
    let club48_server = TestServer::start(
        Served::Localhost,
        ServerAlpn::Http11,
        answering(ack_body(&id)),
    );
    let blockrazor_server = TestServer::start(
        Served::Localhost,
        ServerAlpn::Http11,
        answering(ack_body(&id)),
    );
    let mut club48_owner = owner(&club48_server);
    let mut blockrazor_owner = owner(&blockrazor_server);
    club48_owner.connect().unwrap();
    blockrazor_owner.connect().unwrap();
    let table = frozen_table();
    let plan = both_builders(&world, &table);
    let club48_target = club48_target(&club48_server);
    let blockrazor_target = blockrazor_target(&blockrazor_server);
    let mut club48_frame = frame_for(&world, &plan, &table, &club48_target, FRAME_CAPACITY);
    let mut blockrazor_frame = frame_for(&world, &plan, &table, &blockrazor_target, FRAME_CAPACITY);

    let current = world.snapshot;
    let mut armed = [
        Armed {
            adapter: adapter(CLUB48),
            transport: club48_owner.transport(),
            frame: &mut club48_frame,
        },
        Armed {
            adapter: adapter(BLOCKRAZOR),
            transport: blockrazor_owner.transport(),
            frame: &mut blockrazor_frame,
        },
    ];
    let report = dispatch_diagnostic(
        &mut world.lane,
        &plan,
        &context(&table, &current),
        &quick_clock(),
        &deadline(),
        &mut armed,
    )
    .unwrap();
    assert_eq!(
        report.adapter(adapter(CLUB48)).unwrap().outcome,
        SendOutcome::HandedOff
    );
    assert_eq!(
        report.adapter(adapter(BLOCKRAZOR)).unwrap().outcome,
        SendOutcome::HandedOff
    );

    assert!(wait_until(|| {
        club48_server.requests().len() == 1 && blockrazor_server.requests().len() == 1
    }));
    let club48_request = &club48_server.requests()[0];
    let blockrazor_request = &blockrazor_server.requests()[0];
    // The same launch and the same BUY bytes, in the same order.
    assert_eq!(bundle_txs(club48_request), bundle_txs(blockrazor_request));
    assert_eq!(bundle_txs(club48_request).1, world.raw_buy);
    // BlockRazor's request carries its authorization; 48Club's none.
    let authorization = format!("\r\nAuthorization: {TOKEN}\r\n");
    assert!(String::from_utf8_lossy(blockrazor_request).contains(&authorization));
    assert!(!String::from_utf8_lossy(club48_request).contains("Authorization"));
}

#[test]
fn a_fragmented_response_is_pending_until_complete() {
    let mut world = World::prepared();
    let response = http_json("HTTP/1.1 200 OK", &ack_body(&request_id(&world)));
    let (first, rest) = response.split_at(10);
    let (second, third) = rest.split_at(rest.len() / 2);
    let server = TestServer::start(
        Served::Localhost,
        ServerAlpn::Http11,
        Behaviour::Answer {
            pieces: vec![first.to_vec(), second.to_vec(), third.to_vec()],
            gap: Duration::from_millis(30),
        },
    );
    let mut owner = owner(&server);
    owner.connect().unwrap();
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let mut report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);

    let (poll, pending) = poll_until_done(&mut world, &mut owner, CLUB48, &mut frame, &mut report);
    assert_eq!(poll, Poll::Done(ResponseOutcome::Ack));
    assert!(pending >= 1, "{pending}");
    assert_eq!(owner.state(), ConnectionState::Ready);
}

#[test]
fn a_malformed_response_makes_the_connection_stale_and_maintenance_replaces_it() {
    let mut world = World::prepared();
    let server = TestServer::start(
        Served::Localhost,
        ServerAlpn::Http11,
        answering("{]".to_owned()),
    );
    let mut owner = owner(&server);
    let first = owner.connect().unwrap();
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let mut report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);

    let (poll, _) = poll_until_done(&mut world, &mut owner, CLUB48, &mut frame, &mut report);
    assert_eq!(
        poll,
        Poll::Done(ResponseOutcome::Protocol(ProtocolFailure::MalformedJson))
    );
    assert_eq!(owner.state(), ConnectionState::Stale);
    assert_eq!(submission(&world, CLUB48), SubmissionState::HandedOff);

    // Maintenance, not the dispatch, replaces the connection.
    let Maintenance::Connected(second) = owner.maintain() else {
        panic!("not reconnected");
    };
    assert_eq!(second.generation, first.generation + 1);
    assert!(!owner.is_current(&first));
    assert!(wait_until(|| server.accepted() == 2));
}

#[test]
fn end_of_stream_without_a_response_disconnects() {
    let mut world = World::prepared();
    let server = TestServer::start(
        Served::Localhost,
        ServerAlpn::Http11,
        Behaviour::CloseAfterRequest,
    );
    let mut owner = owner(&server);
    owner.connect().unwrap();
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let mut report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);

    let (poll, _) = poll_until_done(&mut world, &mut owner, CLUB48, &mut frame, &mut report);
    assert_eq!(poll, Poll::Done(ResponseOutcome::Connection));
    assert_eq!(owner.state(), ConnectionState::Disconnected);
    assert_eq!(submission(&world, CLUB48), SubmissionState::HandedOff);
}

#[test]
fn an_overdue_response_gives_the_connection_up_in_maintenance() {
    let mut world = World::prepared();
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let short = TlsPolicy {
        response_timeout: Duration::from_millis(50),
        ..policy()
    };
    let mut owner = owner_with(&server, trusting(TEST_CA), short);
    let first = owner.connect().unwrap();
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let mut report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);

    assert_eq!(owner.maintain(), Maintenance::Awaiting);
    thread::sleep(Duration::from_millis(100));
    let Maintenance::Connected(second) = owner.maintain() else {
        panic!("overdue connection kept");
    };
    assert_eq!(second.generation, first.generation + 1);
    // The answer is no longer awaited on the new connection; the
    // submission stays handed off, its outcome for the chain owner.
    let (poll, _) = poll_until_done(&mut world, &mut owner, CLUB48, &mut frame, &mut report);
    assert_eq!(poll, Poll::NotExpected);
    assert_eq!(submission(&world, CLUB48), SubmissionState::HandedOff);
}

// ---------------------------------------------------------------------------
// Only a ready, idle-fresh, drained connection is eligible
// ---------------------------------------------------------------------------

#[test]
fn an_idle_connection_is_not_eligible() {
    let mut world = World::prepared();
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let short = TlsPolicy {
        max_idle: Duration::from_millis(50),
        ..policy()
    };
    let mut owner = owner_with(&server, trusting(TEST_CA), short);
    owner.connect().unwrap();
    thread::sleep(Duration::from_millis(100));
    assert_eq!(owner.state(), ConnectionState::Stale);

    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);
    assert!(!report.admitted());
    assert_eq!(world.lane.state(world.ticket), Ok(IntentState::Prepared));
    assert!(server.requests().is_empty());
    assert!(matches!(owner.maintain(), Maintenance::Connected(_)));
}

#[test]
fn no_second_request_while_a_response_is_outstanding() {
    let mut first_world = World::prepared();
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let mut owner = owner(&server);
    owner.connect().unwrap();
    let table = frozen_table();
    let first_plan = club48_plan(&first_world, &table);
    let mut first_frame = frame_for(
        &first_world,
        &first_plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    dispatch_club48(
        &mut first_world,
        &first_plan,
        &table,
        &mut owner,
        &mut first_frame,
    );
    assert_eq!(owner.state(), ConnectionState::AwaitingResponse);

    // Another intent, over the same connection: skipped, nothing admitted.
    let mut second_world = World::prepared();
    let second_plan = club48_plan(&second_world, &table);
    let mut second_frame = frame_for(
        &second_world,
        &second_plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let report = dispatch_club48(
        &mut second_world,
        &second_plan,
        &table,
        &mut owner,
        &mut second_frame,
    );
    assert!(!report.admitted());
    assert_eq!(
        report.adapter(adapter(CLUB48)).unwrap().outcome,
        SendOutcome::SkippedConnection(ConnectionState::AwaitingResponse)
    );
    assert!(wait_until(|| server.requests().len() == 1));
    thread::sleep(Duration::from_millis(50));
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn a_disconnected_transport_is_skipped_and_only_maintenance_reconnects() {
    let mut world = World::prepared();
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let mut owner = owner(&server);
    let first = owner.connect().unwrap();
    owner.transport().mark(ConnectionState::Disconnected);

    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);
    assert!(!report.admitted());
    // The dispatch did not connect.
    thread::sleep(Duration::from_millis(50));
    assert_eq!(server.accepted(), 1);
    assert_eq!(owner.generation(), 1);

    let Maintenance::Connected(second) = owner.maintain() else {
        panic!("not reconnected");
    };
    assert_eq!(second.generation, 2);
    assert!(!owner.is_current(&first));
    assert!(owner.is_current(&second));
    assert!(wait_until(|| server.accepted() == 2));
}

#[test]
fn maintenance_finds_a_peer_that_closed_an_idle_connection() {
    let server = TestServer::start(
        Served::Localhost,
        ServerAlpn::Http11,
        Behaviour::CloseAfterHandshake,
    );
    let mut owner = owner(&server);
    owner.connect().unwrap();
    thread::sleep(Duration::from_millis(100));
    // The dispatch could not know; maintenance does, and replaces it.
    let Maintenance::Connected(second) = owner.maintain() else {
        panic!("closed connection kept");
    };
    assert_eq!(second.generation, 2);
}

#[test]
fn maintenance_keeps_a_warm_connection() {
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let mut owner = owner(&server);
    owner.connect().unwrap();
    thread::sleep(Duration::from_millis(50));
    assert_eq!(owner.maintain(), Maintenance::Warm);
    assert_eq!(owner.maintain(), Maintenance::Warm);
    assert_eq!(owner.generation(), 1);
    assert_eq!(server.accepted(), 1);
}

// ---------------------------------------------------------------------------
// Backpressure, partial writes and closes: before and after the permit
// ---------------------------------------------------------------------------

#[test]
fn backpressure_before_the_permit_skips_without_disclosure() {
    let mut world = World::prepared();
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::NeverRead);
    let mut owner = owner(&server);
    owner.connect().unwrap();

    // Fill the socket while the peer reads nothing, until a flush reports
    // WouldBlock. It returns at once: nothing is waited for or hidden.
    let junk = vec![0u8; 256 << 10];
    let mut blocked = false;
    for _ in 0..512 {
        let transport = owner.transport();
        assert_eq!(transport.write(&junk).unwrap(), junk.len());
        let started = Instant::now();
        match transport.flush() {
            Ok(()) => {}
            Err(error) => {
                assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
                assert!(started.elapsed() < Duration::from_secs(1));
                blocked = true;
                break;
            }
        }
    }
    assert!(blocked);

    // Undrained ciphertext: the transport is not ready, and a dispatch
    // skips it before any permit.
    assert_eq!(owner.state(), ConnectionState::Stale);
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);
    assert!(!report.admitted());
    assert_eq!(world.lane.state(world.ticket), Ok(IntentState::Prepared));
}

#[test]
fn backpressure_after_the_permit_is_unknown() {
    let mut world = huge_world();
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::NeverRead);
    let mut owner = owner(&server);
    owner.connect().unwrap();
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(&world, &plan, &table, &club48_target(&server), HUGE_FRAME);
    let frame_len = frame.len();

    let report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);
    // The whole request went to TLS; its ciphertext did not all fit the
    // socket. Bytes may have left: unknown, never "unsent", never retried.
    assert_eq!(
        report.adapter(adapter(CLUB48)).unwrap().outcome,
        SendOutcome::Unknown {
            bytes_accepted: frame_len,
            cause: UnknownCause::Flush,
        }
    );
    assert_eq!(submission(&world, CLUB48), SubmissionState::Unknown);
    assert_eq!(
        world.lane.halt_reason(),
        Some(HaltReason::UnknownSubmission)
    );
    assert_eq!(owner.state(), ConnectionState::Disconnected);
    // No reconnect happened in the dispatch.
    assert_eq!(owner.generation(), 1);
    assert_eq!(server.accepted(), 1);
}

#[test]
fn a_peer_that_closed_before_the_write_makes_it_unknown() {
    let mut world = huge_world();
    let server = TestServer::start(
        Served::Localhost,
        ServerAlpn::Http11,
        Behaviour::CloseAfterHandshake,
    );
    let mut owner = owner(&server);
    owner.connect().unwrap();
    thread::sleep(Duration::from_millis(100));
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(&world, &plan, &table, &club48_target(&server), HUGE_FRAME);

    let report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);
    let outcome = report.adapter(adapter(CLUB48)).unwrap().outcome;
    // The first write to the wire or the drain after it fails, depending on
    // when the reset arrives: unknown either way.
    assert!(
        matches!(
            outcome,
            SendOutcome::Unknown {
                cause: UnknownCause::Write | UnknownCause::Flush,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(
        world.lane.halt_reason(),
        Some(HaltReason::UnknownSubmission)
    );
}

// ---------------------------------------------------------------------------
// Two stages: TLS preparation, then the last check, then the wire
// ---------------------------------------------------------------------------

#[test]
fn a_late_check_after_tls_preparation_writes_nothing_to_the_wire() {
    let mut world = World::prepared();
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let mut owner = owner(&server);
    owner.connect().unwrap();
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );

    // In time before the permit; the request is then prepared, encrypted
    // into TLS records, and the last check finds the deadline reached.
    let report = try_dispatch_club48(
        &mut world,
        &plan,
        &table,
        &mut owner,
        &mut frame,
        &[100, 999_900, 1_000_000],
    )
    .unwrap();
    let record = *report.adapter(adapter(CLUB48)).unwrap();
    // The permit is consumed: unknown, with nothing on the wire.
    assert_eq!(
        record.outcome,
        SendOutcome::Unknown {
            bytes_accepted: 0,
            cause: UnknownCause::WriteDeadline(TransportDeadlineError::Missed),
        }
    );
    assert_eq!(record.t_write_check_ns, None);
    assert_eq!(owner.transport().wire_bytes(), 0);
    assert_eq!(
        world.lane.halt_reason(),
        Some(HaltReason::UnknownSubmission)
    );
    // The prepared records stay unsent: the connection is not eligible,
    // the server received nothing, and maintenance drops them with it.
    assert_eq!(owner.state(), ConnectionState::Stale);
    thread::sleep(Duration::from_millis(50));
    assert!(server.requests().is_empty());
    assert!(matches!(owner.maintain(), Maintenance::Connected(_)));
    assert_eq!(owner.transport().wire_bytes(), 0);
}

#[test]
fn the_last_check_lies_between_tls_preparation_and_the_first_socket_write() {
    let mut world = World::prepared();
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let mut owner = owner(&server);
    owner.connect().unwrap();
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let frame_len = frame.len();

    // Prepared and checked at 999,999 ns: the first socket write follows;
    // the complete handoff is measured separately.
    let report = try_dispatch_club48(
        &mut world,
        &plan,
        &table,
        &mut owner,
        &mut frame,
        &[100, 500, 999_999, 1_250_000],
    )
    .unwrap();
    let record = *report.adapter(adapter(CLUB48)).unwrap();
    assert_eq!(record.outcome, SendOutcome::HandedOff);
    assert_eq!(record.t_prepermit_check_ns, Some(500));
    assert_eq!(record.t_write_check_ns, Some(999_999));
    assert_eq!(record.t_socket_write_start_ns(), Some(999_999));
    assert_eq!(record.elapsed_at_write_check_ns, Some(999_999));
    assert_eq!(record.t_boundary_ns, Some(1_250_000));
    assert_eq!(record.write_check_to_boundary_ns(), Some(250_001));
    // Every ciphertext byte reached the kernel: more than the plaintext.
    assert!(owner.transport().wire_bytes() > frame_len as u64);
    assert!(wait_until(|| server.requests().len() == 1));
    let (launch, buy) = bundle_txs(&server.requests()[0]);
    assert_eq!(
        (launch, buy),
        (world.launch_raw.clone(), world.raw_buy.clone())
    );
}

#[test]
fn a_request_larger_than_the_transport_takes_is_refused_before_the_admission() {
    let mut world = World::prepared();
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let table = frozen_table();
    let plan = club48_plan(&world, &table);
    let target = club48_target(&server);
    let mut frame = frame_for(&world, &plan, &table, &target, FRAME_CAPACITY);
    let limit = |max: usize| TlsPolicy {
        max_plaintext_request_len: max,
        ..policy()
    };

    // One byte over the transport's limit: refused before any permit, with
    // nothing disclosed, and not as an unknown submission.
    let mut tight = owner_with(&server, trusting(TEST_CA), limit(frame.len() - 1));
    tight.connect().unwrap();
    assert_eq!(tight.transport().max_request_len(), frame.len() - 1);
    let refused = try_dispatch_club48(
        &mut world,
        &plan,
        &table,
        &mut tight,
        &mut frame,
        &[100, 150, 200, 300],
    );
    assert_eq!(refused.err(), Some(DispatchError::RequestTooLarge));
    assert_eq!(world.lane.state(world.ticket), Ok(IntentState::Prepared));
    assert_eq!(world.lane.halt_reason(), None);
    assert_eq!(tight.transport().wire_bytes(), 0);
    assert_eq!(tight.state(), ConnectionState::Ready);

    // Exactly at the limit: taken whole and sent.
    let mut exact = owner_with(&server, trusting(TEST_CA), limit(frame.len()));
    exact.connect().unwrap();
    let report = dispatch_club48(&mut world, &plan, &table, &mut exact, &mut frame);
    assert_eq!(
        report.adapter(adapter(CLUB48)).unwrap().outcome,
        SendOutcome::HandedOff
    );
    assert!(wait_until(|| server.requests().len() == 1));
}

#[test]
fn the_tls_output_buffer_is_sized_above_the_plaintext_limit() {
    // Room for the plaintext, the overhead of every record it needs and
    // some slack, never less than the plaintext itself.
    for plaintext in [1usize, 16_384, 16_385, 3_000, 24 << 20] {
        let records = plaintext.div_ceil(16_384);
        let capacity = tls_output_capacity(plaintext);
        assert!(capacity >= plaintext + records * 30, "{plaintext}");
    }
    assert_eq!(tls_output_capacity(usize::MAX), usize::MAX);
}

// ---------------------------------------------------------------------------
// Not qualified, no secrets, pinned
// ---------------------------------------------------------------------------

#[test]
fn a_tls_transport_never_reports_a_qualified_send_time() {
    // Even under synthetic records that qualify the boundary, a TLS
    // transport in a dispatch is not a qualified transport.
    let mut world = World::prepared();
    let id = request_id(&world);
    let server = TestServer::start(
        Served::Localhost,
        ServerAlpn::Http11,
        answering(ack_body(&id)),
    );
    let mut owner = owner(&server);
    owner.connect().unwrap();
    let grade = CapabilityId::Club48BuilderV1.record().release_grade();
    let table = table_of(&[(CLUB48, grade)]);
    let plan = club48_plan(&world, &table);
    let mut frame = frame_for(
        &world,
        &plan,
        &table,
        &club48_target(&server),
        FRAME_CAPACITY,
    );
    let report = dispatch_club48(&mut world, &plan, &table, &mut owner, &mut frame);
    let record = report.adapter(adapter(CLUB48)).unwrap();
    assert_eq!(record.outcome, SendOutcome::HandedOff);
    assert!(record.record_qualifies_boundary);
    assert_eq!(record.send_time(), SendTime::TransportUnqualified);
    assert_eq!(record.t_send_ns(), None);
}

#[test]
fn no_credential_in_debug_output_or_errors() {
    const SECRET: &str = "secret-token-for-this-test";
    let token = AuthorizationToken::new(SECRET.to_owned()).unwrap();
    assert!(!format!("{token:?}").contains(SECRET));
    let endpoint = TlsEndpoint::new("localhost", 1).unwrap();
    let target = ProviderTarget::blockrazor(endpoint.clone(), token);
    let shown = format!("{target:?}");
    assert!(!shown.contains(SECRET));
    assert!(shown.contains("redacted"));
    // The frame endpoint is where it is used, and only there.
    assert_eq!(target.frame_endpoint().authorization, Some(SECRET));

    // A failed connection and its owner show no credential either.
    let address = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let mut owner =
        TlsConnectionOwner::new(endpoint.with_address(address), trusting(TEST_CA), policy())
            .unwrap();
    let error = owner.connect().unwrap_err();
    assert!(!format!("{error:?} {error}").contains(SECRET));
    assert!(!format!("{owner:?}").contains(SECRET));
}

#[test]
fn endpoints_tokens_and_policies_are_validated() {
    for host in ["", "bad host", "a\r\nb", "127.0.0.1", "::1"] {
        assert_eq!(
            TlsEndpoint::new(host, HTTPS_PORT).err(),
            Some(TargetError::InvalidHost),
            "{host:?}"
        );
    }
    assert_eq!(
        TlsEndpoint::new("localhost", 0).err(),
        Some(TargetError::InvalidPort)
    );
    for host in [CLUB48_DOCUMENTED_HOST, BLOCKRAZOR_DOCUMENTED_HOST] {
        let endpoint = TlsEndpoint::new(host, HTTPS_PORT).unwrap();
        assert_eq!(endpoint.host(), host);
        assert_eq!(endpoint.connect_to(), None);
    }
    for token in ["", "a\r\nb", "tab\there"] {
        assert_eq!(
            AuthorizationToken::new(token.to_owned()).err(),
            Some(TargetError::InvalidAuthorization),
            "{token:?}"
        );
    }
    let zero = TlsPolicy {
        max_idle: Duration::ZERO,
        ..policy()
    };
    assert_eq!(zero.validate(), Err(PolicyError::Zero));
    let endpoint = TlsEndpoint::new("localhost", HTTPS_PORT).unwrap();
    assert_eq!(
        TlsConnectionOwner::new(endpoint, trusting(TEST_CA), zero).err(),
        Some(PolicyError::Zero)
    );
}

#[test]
fn trust_anchors_are_required_and_the_mozilla_roots_are_built_in() {
    assert!(TlsSetup::web_pki_roots().unwrap().trust_anchors() > 100);
    assert_eq!(
        TlsSetup::with_trust_anchors(&[]).err(),
        Some(SetupError::NoTrustAnchors)
    );
    let garbage: &[u8] = b"not a certificate";
    assert_eq!(
        TlsSetup::with_trust_anchors(&[garbage]).err(),
        Some(SetupError::InvalidTrustAnchor)
    );
}

#[test]
fn a_reconnect_makes_a_new_generation_and_retires_the_old_facts() {
    let server = TestServer::start(Served::Localhost, ServerAlpn::Http11, Behaviour::Hold);
    let mut owner = owner(&server);
    let first = owner.connect().unwrap();
    let second = owner.connect().unwrap();
    assert_eq!((first.generation, second.generation), (1, 2));
    assert_ne!(first, second);
    assert!(!owner.is_current(&first));
    assert!(owner.is_current(&second));

    let profile = owner.profile().unwrap();
    assert_eq!(profile.implementation, TLS_TRANSPORT_IMPLEMENTATION);
    assert_eq!(profile.host, "localhost");
    assert_eq!(profile.connection, second);
    assert_eq!(profile.http, HttpVersion::Http11);
    assert_eq!(profile.policy, policy());
    assert_eq!(profile.target.0, std::env::consts::OS);
}

#[test]
fn the_implementation_id_names_the_pinned_rustls() {
    let workspace = include_str!("../../../Cargo.toml");
    assert!(workspace.contains("rustls = { version = \"=0.23.45\""));
    assert!(TLS_TRANSPORT_IMPLEMENTATION.contains("rustls-0.23.45"));
    assert!(TLS_TRANSPORT_IMPLEMENTATION.contains("http/1.1"));
}
