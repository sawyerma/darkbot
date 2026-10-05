//! The release dispatch over qualified transports.
//!
//! Only these unit tests can make a [`QualifiedTransport`] or a
//! [`TransportQualification`]: their sole constructors are compiled for
//! them. They use the fixtures of the integration tests.

use darkbot_core::inclusion::capability::{
    AdapterKind, AdapterTable, CapabilityId, HandoffBoundary,
};
use darkbot_core::inclusion::plan::{QualifiedReleasePlan, SubmissionPlan};
use darkbot_core::intent::{AdapterId, IntentState, SubmissionState};

use super::frame::Frame;
use super::response::{ResponseBuffer, ResponseOutcome};
use super::send::{
    DispatchError, EvidenceClass, FanoutReport, Poll, ReleaseArmed, SendOutcome, SendTime,
    dispatch_release, poll_release_response,
};
use super::transport::{
    ConnectionState, QualifiedTransport, TransportQualification, TransportQualifications,
};
use crate::test_common::*;

const TLS: HandoffBoundary = HandoffBoundary::TlsUserspaceWriteFlushReturned;

/// The implementation id the test transports are qualified as.
const IMPLEMENTATION: &str = "test-double";
/// The transport qualification generation currently valid.
const CURRENT: u64 = 3;

fn qualification(implementation: &'static str, generation: u64) -> TransportQualification {
    TransportQualification::for_tests(implementation, generation)
}

/// The runner's current qualifications: generation 3 of the test double
/// for both builders.
fn current() -> TransportQualifications {
    let valid = qualification(IMPLEMENTATION, CURRENT);
    TransportQualifications::default()
        .with(adapter(CLUB48), valid)
        .with(adapter(BLOCKRAZOR), valid)
}

/// What a test binds the 48Club transport to.
#[derive(Clone, Copy)]
struct Bind {
    adapter: u8,
    kind: AdapterKind,
    generation: u64,
    boundary: HandoffBoundary,
    implementation: &'static str,
    transport_generation: u64,
}

const CLUB48_BIND: Bind = Bind {
    adapter: CLUB48,
    kind: AdapterKind::Club48Builder,
    generation: CAPABILITY_GENERATION,
    boundary: TLS,
    implementation: IMPLEMENTATION,
    transport_generation: CURRENT,
};

/// A qualified plan over synthetic release-grade records, with frames.
struct Release {
    world: World,
    table: AdapterTable,
    plan: SubmissionPlan,
    qualified: QualifiedReleasePlan,
    club48: Frame,
    blockrazor: Frame,
}

impl Release {
    fn new() -> Self {
        let world = World::prepared();
        let club48 = CapabilityId::Club48BuilderV1.record().release_grade();
        let blockrazor = CapabilityId::BlockRazorBuilderV1.record().release_grade();
        let table = table_of(&[(CLUB48, club48), (BLOCKRAZOR, blockrazor)]);
        let plan = both_builders(&world, &table);
        let qualified = plan.qualify_for_release(&table).unwrap();
        let club48 = club48_frame(&world, &plan, &table);
        let blockrazor = blockrazor_frame(&world, &plan, &table);
        Self {
            world,
            table,
            plan,
            qualified,
            club48,
            blockrazor,
        }
    }

    /// Dispatches over `a` bound as `bind` and `b` bound correctly,
    /// against the current qualifications.
    fn dispatch(
        &mut self,
        a: &mut FakeTransport,
        bind: Bind,
        b: &mut FakeTransport,
    ) -> Result<FanoutReport, DispatchError> {
        self.dispatch_against(a, bind, b, &current())
    }

    /// Dispatches over `a` bound as `bind` and `b` bound correctly,
    /// against `qualifications`.
    fn dispatch_against(
        &mut self,
        a: &mut FakeTransport,
        bind: Bind,
        b: &mut FakeTransport,
        qualifications: &TransportQualifications,
    ) -> Result<FanoutReport, DispatchError> {
        let current = self.world.snapshot;
        let mut armed = [
            ReleaseArmed {
                transport: QualifiedTransport::for_tests(
                    a,
                    adapter(bind.adapter),
                    bind.kind,
                    bind.generation,
                    bind.boundary,
                    qualification(bind.implementation, bind.transport_generation),
                ),
                frame: &mut self.club48,
            },
            ReleaseArmed {
                transport: QualifiedTransport::for_tests(
                    b,
                    adapter(BLOCKRAZOR),
                    AdapterKind::BlockRazorBuilder,
                    CAPABILITY_GENERATION,
                    TLS,
                    qualification(IMPLEMENTATION, CURRENT),
                ),
                frame: &mut self.blockrazor,
            },
        ];
        dispatch_release(
            &mut self.world.lane,
            &self.qualified,
            &context(&self.table, &current),
            &quick_clock(),
            &deadline(),
            qualifications,
            &mut armed,
        )
    }
}

fn record(report: &FanoutReport, index: u8) -> super::send::AdapterReport {
    *report.adapter(adapter(index)).unwrap()
}

#[test]
fn a_release_over_bound_transports_reports_the_qualified_send_time() {
    let mut release = Release::new();
    let (mut a, mut b) = (FakeTransport::ready(), FakeTransport::ready());
    let report = release.dispatch(&mut a, CLUB48_BIND, &mut b).unwrap();

    assert_eq!(report.evidence, EvidenceClass::Release);
    let club48 = record(&report, CLUB48);
    assert_eq!(club48.outcome, SendOutcome::HandedOff);
    assert!(club48.record_qualifies_boundary);
    assert!(club48.transport_qualified);
    // Reads: select 100, pre-permit 150, write check 200, boundary 300.
    assert_eq!(club48.send_time(), SendTime::Qualified(300));
    assert_eq!(club48.write_check_to_send_ns(), Some(100));
    assert_eq!(report.select_to_send_ns(adapter(CLUB48)), Some(200));
    assert_eq!(
        record(&report, BLOCKRAZOR).send_time(),
        SendTime::Qualified(500)
    );
    assert_eq!(report.lane_halt, None);
}

#[test]
fn a_transport_bound_to_something_else_is_refused_before_the_admission() {
    let wrong = [
        // Bound to another adapter kind.
        Bind {
            kind: AdapterKind::BlockRazorBuilder,
            ..CLUB48_BIND
        },
        // Qualified under another capability generation.
        Bind {
            generation: CAPABILITY_GENERATION + 1,
            ..CLUB48_BIND
        },
        // Qualified for another boundary than the record names.
        Bind {
            boundary: HandoffBoundary::NotDefined,
            ..CLUB48_BIND
        },
        // Everything else right, but an older transport qualification.
        Bind {
            transport_generation: CURRENT - 1,
            ..CLUB48_BIND
        },
        // Everything else right, but a newer one than the runner holds.
        Bind {
            transport_generation: CURRENT + 1,
            ..CLUB48_BIND
        },
        // Everything else right, but another implementation.
        Bind {
            implementation: "another-transport",
            ..CLUB48_BIND
        },
    ];
    for bind in wrong {
        let mut release = Release::new();
        let (mut a, mut b) = (FakeTransport::ready(), FakeTransport::ready());
        assert_eq!(
            release.dispatch(&mut a, bind, &mut b).err(),
            Some(DispatchError::TransportBindingMismatch)
        );
        assert!(a.untouched() && b.untouched());
        assert_eq!(
            release.world.lane.state(release.world.ticket),
            Ok(IntentState::Prepared)
        );
    }

    // Bound to another adapter index: the frame of 48Club does not belong
    // to it.
    let mut release = Release::new();
    let (mut a, mut b) = (FakeTransport::ready(), FakeTransport::ready());
    let other_index = Bind {
        adapter: LOCAL,
        ..CLUB48_BIND
    };
    assert!(matches!(
        release.dispatch(&mut a, other_index, &mut b).err(),
        Some(DispatchError::ArmedSetMismatch | DispatchError::FrameMismatch)
    ));
    assert!(a.untouched() && b.untouched());
}

#[test]
fn a_release_response_is_bound_to_its_request() {
    let mut release = Release::new();
    let id = request_id(&release.world);
    let (mut a, mut b) = (FakeTransport::ready(), FakeTransport::ready());
    a.response = http_json("HTTP/1.1 200 OK", &ack_body(&id));
    let mut report = release.dispatch(&mut a, CLUB48_BIND, &mut b).unwrap();
    assert_eq!(a.state, ConnectionState::AwaitingResponse);

    let mut slot = ReleaseArmed {
        transport: QualifiedTransport::for_tests(
            &mut a,
            adapter(CLUB48),
            AdapterKind::Club48Builder,
            CAPABILITY_GENERATION,
            TLS,
            qualification(IMPLEMENTATION, CURRENT),
        ),
        frame: &mut release.club48,
    };
    let mut buffer = ResponseBuffer::new(RESPONSE_CAPACITY);
    let clock = ScriptClock::new(CLOCK, &[900]);
    assert_eq!(
        poll_release_response(
            &mut release.world.lane,
            &mut slot,
            &mut buffer,
            &clock,
            &mut report
        ),
        Poll::Done(ResponseOutcome::Ack)
    );
    assert_eq!(
        release
            .world
            .lane
            .submission(release.world.ticket, adapter(CLUB48)),
        Ok(SubmissionState::Acked)
    );
    // An acknowledgement is no inclusion.
    assert_eq!(
        release.world.lane.state(release.world.ticket),
        Ok(IntentState::Sending)
    );
    assert_eq!(a.state, ConnectionState::Ready);
}

#[test]
fn the_same_qualified_plan_over_plain_transports_stays_diagnostic() {
    use super::send::{Armed, dispatch_diagnostic};

    let mut release = Release::new();
    let (mut a, mut b) = (FakeTransport::ready(), FakeTransport::ready());
    let current = release.world.snapshot;
    let mut armed = [
        Armed {
            adapter: adapter(CLUB48),
            transport: &mut a,
            frame: &mut release.club48,
        },
        Armed {
            adapter: adapter(BLOCKRAZOR),
            transport: &mut b,
            frame: &mut release.blockrazor,
        },
    ];
    let report = dispatch_diagnostic(
        &mut release.world.lane,
        &release.plan,
        &context(&release.table, &current),
        &quick_clock(),
        &deadline(),
        &mut armed,
    )
    .unwrap();
    assert_eq!(report.evidence, EvidenceClass::Diagnostic);
    assert_eq!(
        record(&report, CLUB48).send_time(),
        SendTime::TransportUnqualified
    );
}

#[test]
fn rotating_the_qualification_retires_the_transports_made_under_the_old_one() {
    // The transports were qualified under generation 3. The runner has
    // since moved to generation 4: they no longer count.
    let rotated = TransportQualifications::default()
        .with(adapter(CLUB48), qualification(IMPLEMENTATION, CURRENT + 1))
        .with(
            adapter(BLOCKRAZOR),
            qualification(IMPLEMENTATION, CURRENT + 1),
        );
    let mut release = Release::new();
    let (mut a, mut b) = (FakeTransport::ready(), FakeTransport::ready());
    assert_eq!(
        release
            .dispatch_against(&mut a, CLUB48_BIND, &mut b, &rotated)
            .err(),
        Some(DispatchError::TransportBindingMismatch)
    );
    assert!(a.untouched() && b.untouched());
    assert_eq!(
        release.world.lane.state(release.world.ticket),
        Ok(IntentState::Prepared)
    );

    // No current qualification for an adapter at all: refused as well.
    let only_blockrazor = TransportQualifications::default()
        .with(adapter(BLOCKRAZOR), qualification(IMPLEMENTATION, CURRENT));
    assert_eq!(
        release
            .dispatch_against(&mut a, CLUB48_BIND, &mut b, &only_blockrazor)
            .err(),
        Some(DispatchError::TransportBindingMismatch)
    );
    // Nor without any, which is all a production build can have today.
    assert_eq!(
        release
            .dispatch_against(
                &mut a,
                CLUB48_BIND,
                &mut b,
                &TransportQualifications::default()
            )
            .err(),
        Some(DispatchError::TransportBindingMismatch)
    );

    // The identical binding is admitted.
    assert!(release.dispatch(&mut a, CLUB48_BIND, &mut b).is_ok());
}

#[test]
fn a_binding_reports_what_it_is_bound_to() {
    let mut transport = FakeTransport::ready();
    let qualified = QualifiedTransport::for_tests(
        &mut transport,
        adapter(CLUB48),
        AdapterKind::Club48Builder,
        CAPABILITY_GENERATION,
        TLS,
        qualification(IMPLEMENTATION, CURRENT),
    );
    let binding = qualified.binding();
    assert_eq!(binding.adapter(), AdapterId::new(CLUB48).unwrap());
    assert_eq!(binding.kind(), AdapterKind::Club48Builder);
    assert_eq!(binding.capability_generation(), CAPABILITY_GENERATION);
    assert_eq!(binding.boundary(), TLS);
    assert_eq!(binding.qualification().implementation(), IMPLEMENTATION);
    assert_eq!(binding.qualification().generation(), CURRENT);
    assert_eq!(
        current().current(adapter(CLUB48)),
        Some(binding.qualification())
    );
    assert_eq!(current().current(adapter(LOCAL)), None);
}
