//! Dispatch: one admission, permits, two deadline checks, unknown after
//! the permit, no permit left behind, no waiting between adapters.

mod common;

use std::rc::Rc;

use alloy_primitives::keccak256;
use common::*;
use darkbot_core::inclusion::capability::{AdapterTable, HandoffBoundary};
use darkbot_core::inclusion::plan::{
    MergePolicy, PlanError, ProviderValidity, SubmissionPlan, TransportDeadlineError, TxRef,
    disclosure,
};
use darkbot_core::intent::{HaltReason, IntentState, LaneError, SubmissionState};
use darkbot_core::tx::legacy::decode_signed;
use darkbot_runner::inclusion::frame::Frame;
use darkbot_runner::inclusion::response::{ProtocolFailure, ResponseBuffer, ResponseOutcome};
use darkbot_runner::inclusion::send::{
    Armed, DispatchError, EvidenceClass, FanoutReport, Poll, SendOutcome, SendTime, UnknownCause,
    dispatch_diagnostic, poll_response,
};
use darkbot_runner::inclusion::transport::ConnectionState;

/// The public bundle through both builders, with its frames.
struct Fixture {
    world: World,
    table: AdapterTable,
    plan: SubmissionPlan,
    club48: Frame,
    blockrazor: Frame,
}

impl Fixture {
    fn both() -> Self {
        let world = World::prepared();
        let table = frozen_table();
        let plan = both_builders(&world, &table);
        let club48 = club48_frame(&world, &plan, &table);
        let blockrazor = blockrazor_frame(&world, &plan, &table);
        Self {
            world,
            table,
            plan,
            club48,
            blockrazor,
        }
    }

    /// Dispatches over `a` (48Club) and `b` (BlockRazor) with `clock`.
    fn dispatch(
        &mut self,
        a: &mut FakeTransport,
        b: &mut FakeTransport,
        clock: &ScriptClock,
    ) -> Result<FanoutReport, DispatchError> {
        let current = self.world.snapshot;
        let mut armed = [
            Armed {
                adapter: adapter(CLUB48),
                transport: a,
                frame: &mut self.club48,
            },
            Armed {
                adapter: adapter(BLOCKRAZOR),
                transport: b,
                frame: &mut self.blockrazor,
            },
        ];
        dispatch_diagnostic(
            &mut self.world.lane,
            &self.plan,
            &context(&self.table, &current),
            clock,
            &deadline(),
            &mut armed,
        )
    }

    fn submission(&self, index: u8) -> SubmissionState {
        self.world
            .lane
            .submission(self.world.ticket, adapter(index))
            .unwrap()
    }

    fn state(&self) -> IntentState {
        self.world.lane.state(self.world.ticket).unwrap()
    }

    fn halt(&self) -> Option<HaltReason> {
        self.world.lane.halt_reason()
    }
}

fn outcome(report: &FanoutReport, index: u8) -> SendOutcome {
    report.adapter(adapter(index)).unwrap().outcome
}

fn ready_pair() -> (FakeTransport, FakeTransport) {
    (FakeTransport::ready(), FakeTransport::ready())
}

// ---------------------------------------------------------------------------
// Same bytes, one admission
// ---------------------------------------------------------------------------

#[test]
fn fan_out_sends_the_same_buy_bytes_nonce_and_hash() {
    let mut fx = Fixture::both();
    let (mut a, mut b) = ready_pair();
    let report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();

    assert_eq!(report.evidence, EvidenceClass::Diagnostic);
    assert_eq!(report.ticket, Some(fx.world.ticket));
    assert_eq!(outcome(&report, CLUB48), SendOutcome::HandedOff);
    assert_eq!(outcome(&report, BLOCKRAZOR), SendOutcome::HandedOff);

    let (launch_a, buy_a) = bundle_txs(&a.written);
    let (launch_b, buy_b) = bundle_txs(&b.written);
    // The same launch, unchanged and first; the same BUY bytes second.
    assert_eq!(launch_a, fx.world.launch_raw);
    assert_eq!(launch_b, fx.world.launch_raw);
    assert_eq!(buy_a, fx.world.raw_buy);
    assert_eq!(buy_b, fx.world.raw_buy);
    // Hence the same hash, the plan's, and the same nonce, the ticket's.
    assert_eq!(keccak256(&buy_a), fx.plan.buy().hash());
    assert_eq!(keccak256(&buy_b), fx.plan.buy().hash());
    let nonce = decode_signed(&buy_a).unwrap().tx.nonce;
    assert_eq!(nonce, fx.world.ticket.nonce());
    assert_eq!(decode_signed(&buy_b).unwrap().tx.nonce, nonce);

    assert_eq!(fx.submission(CLUB48), SubmissionState::HandedOff);
    assert_eq!(fx.submission(BLOCKRAZOR), SubmissionState::HandedOff);
    assert_eq!(fx.state(), IntentState::Sending);
    // Nothing was skipped, so nothing stops the lane.
    assert_eq!(report.permits_suppressed_at_end, 0);
    assert_eq!(report.lane_halt, None);
    assert!(!report.admitted_without_disclosure);
    // Both connections now wait for their response.
    assert_eq!(a.state, ConnectionState::AwaitingResponse);
    assert_eq!(b.state, ConnectionState::AwaitingResponse);
}

#[test]
fn the_real_request_carries_the_authorization_and_the_request_id() {
    let mut fx = Fixture::both();
    let (mut a, mut b) = ready_pair();
    fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();
    let written = String::from_utf8(b.written.clone()).unwrap();
    // The secret goes to the transport, and only there.
    assert!(written.contains("\r\nAuthorization: test-token\r\n"));
    let id = format!("\"id\":\"{}\"", request_id(&fx.world));
    assert!(written.contains(&id));
    assert!(String::from_utf8(a.written.clone()).unwrap().contains(&id));
}

#[test]
fn the_permit_releases_the_buy_and_the_frame_gives_it_back() {
    let mut fx = Fixture::both();
    let buy_hex = hex(&fx.world.raw_buy);
    // Before the dispatch the frame holds no BUY: only the permit can
    // release it.
    assert!(!String::from_utf8_lossy(&fx.club48.redacted_request()).contains(&buy_hex));

    let (mut a, mut b) = ready_pair();
    fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();

    // The first write already carried the complete request with the BUY,
    // so the permit was consumed before it.
    assert_eq!(a.write_calls, 1);
    assert_eq!(a.flush_calls, 1);
    assert!(String::from_utf8_lossy(&a.written).contains(&buy_hex));
    // Afterwards the frame no longer holds the BUY.
    assert!(!fx.club48.is_filled());
    assert!(!String::from_utf8_lossy(&fx.club48.redacted_request()).contains(&buy_hex));
}

#[test]
fn no_second_economic_admission() {
    let mut fx = Fixture::both();
    let (mut a, mut b) = ready_pair();
    fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();

    // Fresh connections change nothing: the intent is being sent and
    // cannot be admitted again.
    let (mut a2, mut b2) = ready_pair();
    assert_eq!(
        fx.dispatch(&mut a2, &mut b2, &quick_clock()).err(),
        Some(DispatchError::Lane(LaneError::NotReady))
    );
    assert!(a2.untouched() && b2.untouched());
    assert_eq!(fx.submission(CLUB48), SubmissionState::HandedOff);
}

// ---------------------------------------------------------------------------
// Two deadline checks
// ---------------------------------------------------------------------------

#[test]
fn the_write_check_is_the_last_clock_read_before_the_first_write() {
    let mut fx = Fixture::both();
    // select, pre-permit A at 999,000, write check A at 999,999, boundary,
    // pre-permit B late.
    let clock = Rc::new(ScriptClock::new(
        CLOCK,
        &[100, 999_000, 999_999, 1_000_500, 1_000_600],
    ));
    let (mut a, mut b) = ready_pair();
    a.clock = Some(Rc::clone(&clock));
    let report = fx.dispatch(&mut a, &mut b, &clock).unwrap();

    let record = report.adapter(adapter(CLUB48)).unwrap();
    assert_eq!(record.outcome, SendOutcome::HandedOff);
    assert_eq!(record.t_prepermit_check_ns, Some(999_000));
    assert_eq!(record.t_write_check_ns, Some(999_999));
    assert_eq!(record.elapsed_at_write_check_ns, Some(999_999));
    // Three reads (select, pre-permit, write check) happened before the
    // first write, and none between the write check and the write: the
    // recorded time is the final check before the write, not the permit.
    assert_eq!(a.clock_reads_at_first_write, Some(3));
    assert_eq!(record.t_boundary_ns, Some(1_000_500));
}

#[test]
fn a_late_write_check_writes_nothing_and_is_unknown() {
    for late in [1_000_000, 1_000_001] {
        let mut fx = Fixture::both();
        // In time at the pre-permit check, late at the write check.
        let clock = ScriptClock::new(CLOCK, &[100, 999_900, late, late + 100]);
        let (mut a, mut b) = ready_pair();
        let report = fx.dispatch(&mut a, &mut b, &clock).unwrap();

        // No byte was written, but the permit was consumed: unknown, never
        // "unsent".
        assert_eq!(
            outcome(&report, CLUB48),
            SendOutcome::Unknown {
                bytes_accepted: 0,
                cause: UnknownCause::WriteDeadline(TransportDeadlineError::Missed),
            },
            "{late}"
        );
        assert!(a.untouched());
        assert!(a.marks.is_empty());
        let record = report.adapter(adapter(CLUB48)).unwrap();
        assert_eq!(record.t_write_check_ns, None);
        assert_eq!(fx.submission(CLUB48), SubmissionState::Unknown);
        assert_eq!(fx.state(), IntentState::Halted);
        assert_eq!(fx.halt(), Some(HaltReason::UnknownSubmission));
        assert_eq!(report.lane_halt, Some(HaltReason::UnknownSubmission));
        // The other adapter is not started.
        assert_eq!(outcome(&report, BLOCKRAZOR), SendOutcome::Suppressed);
        assert_eq!(fx.submission(BLOCKRAZOR), SubmissionState::Suppressed);
        assert!(b.untouched());
        assert_eq!(report.deadline_misses, 1);
    }
}

#[test]
fn a_late_pre_permit_check_after_another_adapter_was_sent_leaves_no_permit() {
    for late in [1_000_000, 1_000_001, 5_000_000] {
        let mut fx = Fixture::both();
        let clock = ScriptClock::new(CLOCK, &[100, 150, 999_999, 1_000_100, late]);
        let (mut a, mut b) = ready_pair();
        let report = fx.dispatch(&mut a, &mut b, &clock).unwrap();

        assert_eq!(outcome(&report, CLUB48), SendOutcome::HandedOff);
        // The late adapter is skipped before its permit and writes nothing.
        assert_eq!(
            outcome(&report, BLOCKRAZOR),
            SendOutcome::SkippedDeadline(TransportDeadlineError::Missed),
            "{late}"
        );
        assert!(b.untouched());
        // Its permit does not outlive the dispatch: the lane is stopped and
        // the permit suppressed. The sent adapter stays as it is.
        assert_eq!(fx.submission(BLOCKRAZOR), SubmissionState::Suppressed);
        assert_eq!(fx.submission(CLUB48), SubmissionState::HandedOff);
        assert_eq!(report.permits_suppressed_at_end, 1);
        assert_eq!(fx.halt(), Some(HaltReason::External));
        assert_eq!(report.lane_halt, Some(HaltReason::External));
        assert_eq!(report.deadline_misses, 1);
    }
}

#[test]
fn a_deadline_passed_before_the_admission_consumes_nothing() {
    let mut fx = Fixture::both();
    let (mut a, mut b) = ready_pair();
    let late = ScriptClock::new(CLOCK, &[1_000_000]);
    let report = fx.dispatch(&mut a, &mut b, &late).unwrap();

    assert!(!report.admitted());
    assert_eq!(report.deadline_misses, 2);
    for index in [CLUB48, BLOCKRAZOR] {
        assert_eq!(
            outcome(&report, index),
            SendOutcome::SkippedDeadline(TransportDeadlineError::Missed)
        );
    }
    assert!(a.untouched() && b.untouched());
    // The intent is still prepared and can be dispatched in time.
    assert_eq!(fx.state(), IntentState::Prepared);
    assert_eq!(fx.halt(), None);
    let report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();
    assert!(report.admitted());
    assert_eq!(outcome(&report, CLUB48), SendOutcome::HandedOff);
}

#[test]
fn a_deadline_passing_after_the_admission_stops_the_lane_without_disclosure() {
    let mut fx = Fixture::both();
    let (mut a, mut b) = ready_pair();
    // In time at selection, late at both pre-permit checks.
    let clock = ScriptClock::new(CLOCK, &[10, 1_000_000, 1_000_001]);
    let report = fx.dispatch(&mut a, &mut b, &clock).unwrap();

    assert!(report.admitted());
    assert!(report.admitted_without_disclosure);
    assert!(a.untouched() && b.untouched());
    // The admission cannot be taken back: the lane is stopped and every
    // permit suppressed.
    assert_eq!(fx.halt(), Some(HaltReason::External));
    assert_eq!(fx.submission(CLUB48), SubmissionState::Suppressed);
    assert_eq!(fx.submission(BLOCKRAZOR), SubmissionState::Suppressed);
    assert_eq!(report.permits_suppressed_at_end, 2);
}

#[test]
fn a_time_from_another_clock_is_no_start() {
    let mut fx = Fixture::both();
    let (mut a, mut b) = ready_pair();
    let other = ScriptClock::new(CLOCK + 1, &[100]);
    let report = fx.dispatch(&mut a, &mut b, &other).unwrap();
    assert!(!report.admitted());
    assert_eq!(
        outcome(&report, CLUB48),
        SendOutcome::SkippedDeadline(TransportDeadlineError::OtherClock)
    );
    assert!(a.untouched() && b.untouched());
}

// ---------------------------------------------------------------------------
// Warm, idle connections only
// ---------------------------------------------------------------------------

#[test]
fn a_connection_that_is_not_ready_is_skipped_and_leaves_no_permit() {
    for state in [
        ConnectionState::NotReady,
        ConnectionState::AwaitingResponse,
        ConnectionState::Stale,
        ConnectionState::Disconnected,
    ] {
        let mut fx = Fixture::both();
        let mut a = FakeTransport::with_state(state);
        let mut b = FakeTransport::ready();
        let report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();

        assert_eq!(
            outcome(&report, CLUB48),
            SendOutcome::SkippedConnection(state)
        );
        // Nothing touched it: no write, no read, no state change, no
        // reconnect.
        assert!(a.untouched());
        assert!(a.marks.is_empty());
        assert_eq!(a.state, state);
        assert_eq!(outcome(&report, BLOCKRAZOR), SendOutcome::HandedOff);
        // Its permit is suppressed at the end of the fan-out.
        assert_eq!(fx.submission(CLUB48), SubmissionState::Suppressed);
        assert_eq!(report.permits_suppressed_at_end, 1);
        assert_eq!(fx.halt(), Some(HaltReason::External));
    }
}

#[test]
fn no_ready_connection_consumes_no_admission() {
    let mut fx = Fixture::both();
    let mut a = FakeTransport::with_state(ConnectionState::Stale);
    let mut b = FakeTransport::with_state(ConnectionState::AwaitingResponse);
    let report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();
    assert!(!report.admitted());
    assert!(a.untouched() && b.untouched());
    assert_eq!(fx.state(), IntentState::Prepared);
    assert_eq!(fx.halt(), None);
}

// ---------------------------------------------------------------------------
// After the permit: unknown, stop, no retry
// ---------------------------------------------------------------------------

/// Checks the common consequences of an unknown first adapter.
fn assert_unknown_stops_everything(fx: &Fixture, b: &FakeTransport, report: &FanoutReport) {
    assert_eq!(fx.submission(CLUB48), SubmissionState::Unknown);
    assert_eq!(fx.state(), IntentState::Halted);
    assert_eq!(fx.halt(), Some(HaltReason::UnknownSubmission));
    // The second adapter is not started.
    assert_eq!(outcome(report, BLOCKRAZOR), SendOutcome::Suppressed);
    assert_eq!(fx.submission(BLOCKRAZOR), SubmissionState::Suppressed);
    assert!(b.untouched());
    assert!(
        report
            .adapter(adapter(CLUB48))
            .unwrap()
            .bytes_may_have_left()
    );
    assert!(!fx.club48.is_filled());
}

#[test]
fn a_partial_write_is_unknown_and_stops_everything() {
    let mut fx = Fixture::both();
    let mut a = FakeTransport::ready();
    a.accept_limit = Some(10);
    let mut b = FakeTransport::ready();
    let report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();

    assert_eq!(
        outcome(&report, CLUB48),
        SendOutcome::Unknown {
            bytes_accepted: 10,
            cause: UnknownCause::Write,
        }
    );
    assert_unknown_stops_everything(&fx, &b, &report);
    // Ten bytes, then one failing call. Nothing was retried.
    assert_eq!(a.write_calls, 2);
    assert_eq!(a.state, ConnectionState::Disconnected);
}

#[test]
fn a_failed_write_after_the_permit_is_unknown_even_with_nothing_accepted() {
    let mut fx = Fixture::both();
    let mut a = FakeTransport::ready();
    a.accept_limit = Some(0);
    let mut b = FakeTransport::ready();
    let report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();

    // Never "unsent": the permit was consumed.
    assert_eq!(
        outcome(&report, CLUB48),
        SendOutcome::Unknown {
            bytes_accepted: 0,
            cause: UnknownCause::Write,
        }
    );
    assert_unknown_stops_everything(&fx, &b, &report);
    assert_eq!(a.write_calls, 1);
    assert_eq!(a.state, ConnectionState::Disconnected);
}

#[test]
fn a_failed_flush_after_the_write_is_unknown() {
    let mut fx = Fixture::both();
    let mut a = FakeTransport::ready();
    a.fail_flush = true;
    let mut b = FakeTransport::ready();
    let report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();

    assert_eq!(
        outcome(&report, CLUB48),
        SendOutcome::Unknown {
            bytes_accepted: fx.club48.len(),
            cause: UnknownCause::Flush,
        }
    );
    assert_unknown_stops_everything(&fx, &b, &report);
    assert_eq!(a.flush_calls, 1);
}

#[test]
fn nothing_is_retried_after_an_unknown_submission() {
    let mut fx = Fixture::both();
    let mut a = FakeTransport::ready();
    a.accept_limit = Some(10);
    let mut b = FakeTransport::ready();
    fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();
    let calls = (a.write_calls, b.write_calls);

    // Fresh connections change nothing: the lane admits nothing more.
    let (mut a2, mut b2) = ready_pair();
    assert_eq!(
        fx.dispatch(&mut a2, &mut b2, &quick_clock()).err(),
        Some(DispatchError::Lane(LaneError::Halted))
    );
    assert!(a2.untouched() && b2.untouched());
    assert_eq!((a.write_calls, b.write_calls), calls);
}

// ---------------------------------------------------------------------------
// Binding before the first write
// ---------------------------------------------------------------------------

#[test]
fn a_plan_for_another_buy_fails_before_any_write() {
    let mut world = World::prepared();
    let table = frozen_table();
    // A plan whose BUY is other bytes than the lane prepared.
    let mut other = world.raw_buy.clone();
    let last = other.len() - 1;
    other[last] ^= 0x01;
    let mut inputs = bundle_inputs(&world, &[CLUB48, BLOCKRAZOR], MergePolicy::NoMerge);
    inputs.buy = TxRef::of(&other);
    let plan = plan(&world, &table, inputs);
    let mut club48 = club48_frame(&world, &plan, &table);
    let mut blockrazor = blockrazor_frame(&world, &plan, &table);
    let (mut a, mut b) = ready_pair();
    let current = world.snapshot;
    let mut armed = [
        Armed {
            adapter: adapter(CLUB48),
            transport: &mut a,
            frame: &mut club48,
        },
        Armed {
            adapter: adapter(BLOCKRAZOR),
            transport: &mut b,
            frame: &mut blockrazor,
        },
    ];
    let result = dispatch_diagnostic(
        &mut world.lane,
        &plan,
        &context(&table, &current),
        &quick_clock(),
        &deadline(),
        &mut armed,
    );
    assert_eq!(
        result.err(),
        Some(DispatchError::DispatchBinding(PlanError::DispatchMismatch))
    );
    assert!(a.untouched() && b.untouched());
    // The admission was consumed by the lane; it is stopped and no permit
    // can be used.
    assert_eq!(world.lane.halt_reason(), Some(HaltReason::External));
    assert_eq!(
        world.lane.submission(world.ticket, adapter(CLUB48)),
        Ok(SubmissionState::Suppressed)
    );
}

#[test]
fn frames_and_adapters_must_be_the_plans_before_the_admission() {
    let mut fx = Fixture::both();
    let current = fx.world.snapshot;
    let (mut a, mut b) = ready_pair();

    // Frames of another plan: the same bundle with a nearer provider
    // validity.
    let mut nearer = bundle_inputs(&fx.world, &[CLUB48, BLOCKRAZOR], MergePolicy::NoMerge);
    nearer.provider_validity = Some(ProviderValidity {
        max_block_number: HEAD,
        min_timestamp: None,
        max_timestamp: None,
    });
    let other_plan = plan(&fx.world, &fx.table, nearer);
    let mut foreign = club48_frame(&fx.world, &other_plan, &fx.table);
    {
        let mut armed = [
            Armed {
                adapter: adapter(CLUB48),
                transport: &mut a,
                frame: &mut foreign,
            },
            Armed {
                adapter: adapter(BLOCKRAZOR),
                transport: &mut b,
                frame: &mut fx.blockrazor,
            },
        ];
        let result = dispatch_diagnostic(
            &mut fx.world.lane,
            &fx.plan,
            &context(&fx.table, &current),
            &quick_clock(),
            &deadline(),
            &mut armed,
        );
        assert_eq!(result.err(), Some(DispatchError::FrameMismatch));
    }

    // Not all of the plan's adapters armed.
    {
        let mut armed = [Armed {
            adapter: adapter(CLUB48),
            transport: &mut a,
            frame: &mut fx.club48,
        }];
        let result = dispatch_diagnostic(
            &mut fx.world.lane,
            &fx.plan,
            &context(&fx.table, &current),
            &quick_clock(),
            &deadline(),
            &mut armed,
        );
        assert_eq!(result.err(), Some(DispatchError::ArmedSetMismatch));
    }

    // A frame under another adapter index.
    {
        let mut armed = [
            Armed {
                adapter: adapter(BLOCKRAZOR),
                transport: &mut a,
                frame: &mut fx.club48,
            },
            Armed {
                adapter: adapter(CLUB48),
                transport: &mut b,
                frame: &mut fx.blockrazor,
            },
        ];
        let result = dispatch_diagnostic(
            &mut fx.world.lane,
            &fx.plan,
            &context(&fx.table, &current),
            &quick_clock(),
            &deadline(),
            &mut armed,
        );
        assert_eq!(result.err(), Some(DispatchError::FrameMismatch));
    }

    // Nothing was consumed or written.
    assert!(a.untouched() && b.untouched());
    assert_eq!(fx.state(), IntentState::Prepared);
}

#[test]
fn the_plans_final_check_runs_before_the_admission() {
    let mut fx = Fixture::both();
    let current = fx.world.snapshot;
    let (mut a, mut b) = ready_pair();
    let mut early = context(&fx.table, &current);
    // Three blocks before the provider's last block: the window is too
    // wide for the birth-entry profile.
    early.head.number = MAX_BLOCK - 3;
    let mut armed = [
        Armed {
            adapter: adapter(CLUB48),
            transport: &mut a,
            frame: &mut fx.club48,
        },
        Armed {
            adapter: adapter(BLOCKRAZOR),
            transport: &mut b,
            frame: &mut fx.blockrazor,
        },
    ];
    let result = dispatch_diagnostic(
        &mut fx.world.lane,
        &fx.plan,
        &early,
        &quick_clock(),
        &deadline(),
        &mut armed,
    );
    assert_eq!(
        result.err(),
        Some(DispatchError::Plan(PlanError::ProviderWindowTooWide))
    );
    assert!(a.untouched() && b.untouched());
    assert_eq!(fx.state(), IntentState::Prepared);
}

// ---------------------------------------------------------------------------
// Responses: bound to the request, no waiting, no inclusion
// ---------------------------------------------------------------------------

fn poll(
    fx: &mut Fixture,
    index: u8,
    transport: &mut FakeTransport,
    buffer: &mut ResponseBuffer,
    report: &mut FanoutReport,
) -> Poll {
    let frame = if index == CLUB48 {
        &mut fx.club48
    } else {
        &mut fx.blockrazor
    };
    let mut slot = Armed {
        adapter: adapter(index),
        transport,
        frame,
    };
    let clock = ScriptClock::new(CLOCK, &[900]);
    poll_response(&mut fx.world.lane, &mut slot, buffer, &clock, report)
}

fn ok(body: &str) -> Vec<u8> {
    http_json("HTTP/1.1 200 OK", body)
}

#[test]
fn a_delayed_response_does_not_hold_back_the_other_adapter() {
    let mut fx = Fixture::both();
    let id = request_id(&fx.world);
    let (mut a, mut b) = ready_pair();
    b.response = ok(&ack_body(&id));
    let mut report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();

    // Both were written before anything was read.
    assert_eq!(outcome(&report, CLUB48), SendOutcome::HandedOff);
    assert_eq!(outcome(&report, BLOCKRAZOR), SendOutcome::HandedOff);
    assert_eq!(a.read_calls + b.read_calls, 0);

    let mut buffer_a = ResponseBuffer::new(RESPONSE_CAPACITY);
    let mut buffer_b = ResponseBuffer::new(RESPONSE_CAPACITY);
    // 48Club has not answered: pending, without waiting.
    assert_eq!(
        poll(&mut fx, CLUB48, &mut a, &mut buffer_a, &mut report),
        Poll::Pending
    );
    assert_eq!(
        poll(&mut fx, BLOCKRAZOR, &mut b, &mut buffer_b, &mut report),
        Poll::Done(ResponseOutcome::Ack)
    );
    assert_eq!(
        poll(&mut fx, CLUB48, &mut a, &mut buffer_a, &mut report),
        Poll::Pending
    );
    assert_eq!(fx.submission(CLUB48), SubmissionState::HandedOff);
    assert_eq!(fx.submission(BLOCKRAZOR), SubmissionState::Acked);
    // Only the answered connection is ready again.
    assert_eq!(a.state, ConnectionState::AwaitingResponse);
    assert_eq!(b.state, ConnectionState::Ready);
}

#[test]
fn an_acknowledgement_is_not_an_inclusion() {
    let mut fx = Fixture::both();
    let id = request_id(&fx.world);
    let (mut a, mut b) = ready_pair();
    a.response = ok(&ack_body(&id));
    let mut report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();
    let mut buffer = ResponseBuffer::new(RESPONSE_CAPACITY);

    assert_eq!(
        poll(&mut fx, CLUB48, &mut a, &mut buffer, &mut report),
        Poll::Done(ResponseOutcome::Ack)
    );
    assert_eq!(fx.submission(CLUB48), SubmissionState::Acked);
    // The intent is still being sent; nothing is included, nothing is
    // canonical.
    assert_eq!(fx.state(), IntentState::Sending);
    assert_eq!(fx.world.lane.outcome(fx.world.ticket), Ok(None));
    let record = report.adapter(adapter(CLUB48)).unwrap();
    assert_eq!(record.t_ack_ns(), Some(900));
    // The response is complete: the connection may carry the next request.
    assert_eq!(a.state, ConnectionState::Ready);
    // It is recorded once.
    assert_eq!(
        poll(&mut fx, CLUB48, &mut a, &mut buffer, &mut report),
        Poll::NotExpected
    );
}

#[test]
fn a_rejection_is_not_proof_of_non_execution() {
    let mut fx = Fixture::both();
    let id = request_id(&fx.world);
    let (mut a, mut b) = ready_pair();
    a.response = ok(&reject_body(&id));
    let mut report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();
    let mut buffer = ResponseBuffer::new(RESPONSE_CAPACITY);

    assert_eq!(
        poll(&mut fx, CLUB48, &mut a, &mut buffer, &mut report),
        Poll::Done(ResponseOutcome::Reject {
            code: Some(-38_000)
        })
    );
    assert_eq!(fx.submission(CLUB48), SubmissionState::Rejected);
    assert_eq!(fx.state(), IntentState::Sending);
    // The bytes left; the core's view: they may still execute.
    assert!(
        report
            .adapter(adapter(CLUB48))
            .unwrap()
            .bytes_may_have_left()
    );
    assert!(disclosure(SubmissionState::Rejected, false).may_still_execute());
}

#[test]
fn a_response_to_another_request_moves_nothing() {
    let mut fx = Fixture::both();
    let other_id = format!("0x{}", "11".repeat(32));
    let (mut a, mut b) = ready_pair();
    // An acknowledgement, but of another request: a stale or foreign one.
    a.response = ok(&ack_body(&other_id));
    // An acknowledgement without the JSON-RPC version.
    let id = request_id(&fx.world);
    b.response = ok(&format!(r#"{{"id":"{id}","result":"0x01"}}"#));
    let mut report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();
    let mut buffer_a = ResponseBuffer::new(RESPONSE_CAPACITY);
    let mut buffer_b = ResponseBuffer::new(RESPONSE_CAPACITY);

    assert_eq!(
        poll(&mut fx, CLUB48, &mut a, &mut buffer_a, &mut report),
        Poll::Done(ResponseOutcome::Protocol(ProtocolFailure::WrongId))
    );
    assert_eq!(
        poll(&mut fx, BLOCKRAZOR, &mut b, &mut buffer_b, &mut report),
        Poll::Done(ResponseOutcome::Protocol(ProtocolFailure::WrongVersion))
    );
    // Neither submission moved; neither connection is trusted again.
    assert_eq!(fx.submission(CLUB48), SubmissionState::HandedOff);
    assert_eq!(fx.submission(BLOCKRAZOR), SubmissionState::HandedOff);
    assert_eq!(a.state, ConnectionState::Stale);
    assert_eq!(b.state, ConnectionState::Stale);
}

#[test]
fn an_unreadable_response_changes_no_submission_state() {
    let mut fx = Fixture::both();
    let (mut a, mut b) = ready_pair();
    a.response = http_json("HTTP/1.1 500 Internal Server Error", "{}");
    b.response = b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n{".to_vec();
    b.close_after_response = true;
    let mut report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();
    let mut buffer_a = ResponseBuffer::new(RESPONSE_CAPACITY);
    let mut buffer_b = ResponseBuffer::new(RESPONSE_CAPACITY);

    assert_eq!(
        poll(&mut fx, CLUB48, &mut a, &mut buffer_a, &mut report),
        Poll::Done(ResponseOutcome::Protocol(ProtocolFailure::HttpStatus(500)))
    );
    assert_eq!(fx.submission(CLUB48), SubmissionState::HandedOff);
    assert_eq!(a.state, ConnectionState::Stale);

    // The connection ends in the middle of the body.
    assert_eq!(
        poll(&mut fx, BLOCKRAZOR, &mut b, &mut buffer_b, &mut report),
        Poll::Done(ResponseOutcome::Connection)
    );
    assert_eq!(fx.submission(BLOCKRAZOR), SubmissionState::HandedOff);
    assert_eq!(b.state, ConnectionState::Disconnected);
}

#[test]
fn a_response_in_pieces_is_pending_until_complete() {
    let mut fx = Fixture::both();
    let full = ok(&ack_body(&request_id(&fx.world)));
    let (mut a, mut b) = ready_pair();
    a.response = full[..20].to_vec();
    let mut report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();
    let mut buffer = ResponseBuffer::new(RESPONSE_CAPACITY);

    assert_eq!(
        poll(&mut fx, CLUB48, &mut a, &mut buffer, &mut report),
        Poll::Pending
    );
    a.response = full;
    assert_eq!(
        poll(&mut fx, CLUB48, &mut a, &mut buffer, &mut report),
        Poll::Done(ResponseOutcome::Ack)
    );
}

// ---------------------------------------------------------------------------
// What is measured, and what is not claimed
// ---------------------------------------------------------------------------

#[test]
fn boundaries_are_measured_but_no_unqualified_one_is_a_send_time() {
    let mut fx = Fixture::both();
    let (mut a, mut b) = ready_pair();
    let report = fx.dispatch(&mut a, &mut b, &quick_clock()).unwrap();

    assert_eq!(report.clock_id, CLOCK);
    assert_eq!(report.t_select_ns, 100);
    let record = report.adapter(adapter(CLUB48)).unwrap();
    assert_eq!(record.t_prepermit_check_ns, Some(150));
    assert_eq!(record.t_write_check_ns, Some(200));
    assert_eq!(record.t_boundary_ns, Some(300));
    assert_eq!(record.write_check_to_boundary_ns(), Some(100));
    assert_eq!(
        record.boundary,
        HandoffBoundary::TlsUserspaceWriteFlushReturned
    );
    // The frozen record does not qualify that boundary, and the transport
    // of a diagnostic dispatch is never qualified: the send time is
    // unknown, and nothing stands in for it.
    assert!(!record.record_qualifies_boundary);
    assert!(!record.transport_qualified);
    assert_eq!(record.send_time(), SendTime::BoundaryUnqualified);
    assert_eq!(record.t_send_ns(), None);
    assert_eq!(record.write_check_to_send_ns(), None);
    assert_eq!(report.select_to_send_ns(adapter(CLUB48)), None);
    assert_eq!(record.t_queue_commit_ns(), None);
}

#[test]
fn local_geth_sends_the_raw_buy_and_its_send_time_is_deferred_to_c1() {
    let mut world = World::prepared();
    let table = frozen_table();
    let plan = plan(&world, &table, single_inputs(&world));
    let mut frame = local_frame(&plan, &table);
    let mut node = FakeTransport::ready();
    let current = world.snapshot;
    let mut armed = [Armed {
        adapter: adapter(LOCAL),
        transport: &mut node,
        frame: &mut frame,
    }];
    let mut report = dispatch_diagnostic(
        &mut world.lane,
        &plan,
        &context(&table, &current),
        &ScriptClock::new(CLOCK, &[100, 200, 250, 300]),
        &deadline(),
        &mut armed,
    )
    .unwrap();

    let record = *report.adapter(adapter(LOCAL)).unwrap();
    assert_eq!(record.outcome, SendOutcome::HandedOff);
    assert_eq!(record.t_write_check_ns, Some(250));
    assert_eq!(record.boundary, HandoffBoundary::DeferredToNativeGeth);
    assert_eq!(record.send_time(), SendTime::TrackedDeferredC1);
    assert_eq!(record.t_send_ns(), None);
    // No response protocol is defined for the local node.
    let mut buffer = ResponseBuffer::new(RESPONSE_CAPACITY);
    let clock = ScriptClock::new(CLOCK, &[400]);
    assert_eq!(
        poll_response(
            &mut world.lane,
            &mut armed[0],
            &mut buffer,
            &clock,
            &mut report
        ),
        Poll::NotExpected
    );
    // Exactly the signed BUY, nothing around it; nothing awaited.
    assert_eq!(node.written, world.raw_buy);
    assert_eq!(node.state, ConnectionState::Ready);
}
