//! The release entry point: a qualified plan over qualified transports,
//! and nothing else. Outside the crate's unit tests neither can exist
//! today; the release dispatch itself is tested in `release_tests`.

mod common;

use common::*;
use darkbot_core::inclusion::capability::CapabilityId;
use darkbot_core::inclusion::plan::{QualifiedReleasePlan, TransportStartDeadline};
use darkbot_core::inclusion::release::ReleaseBlocker;
use darkbot_core::intent::IntentLane;
use darkbot_runner::inclusion::send::{
    Armed, DispatchContext, DispatchError, EvidenceClass, FanoutReport, ReleaseArmed, SendOutcome,
    SendTime, dispatch_diagnostic, dispatch_release,
};
use darkbot_runner::inclusion::transport::{Clock, TransportQualifications};

/// The signature of the release entry point: the qualified plan type, the
/// runner's current transport qualifications and the qualified-transport
/// slot type. (The compile-fail doctests of `dispatch_release` show that
/// the plain types are refused.)
type ReleaseEntry = fn(
    &mut IntentLane,
    &QualifiedReleasePlan,
    &DispatchContext<'_>,
    &dyn Clock,
    &TransportStartDeadline,
    &TransportQualifications,
    &mut [ReleaseArmed<'_>],
) -> Result<FanoutReport, DispatchError>;

#[test]
fn the_release_entry_point_takes_only_qualified_types() {
    let entry: ReleaseEntry = dispatch_release;
    let _ = entry;
    // Outside the crate's unit tests the current qualifications can only be
    // the empty set: no adapter has one.
    let none = TransportQualifications::default();
    for index in [CLUB48, BLOCKRAZOR, LOCAL] {
        assert_eq!(none.current(adapter(index)), None);
    }
}

#[test]
fn the_frozen_records_give_no_qualified_plan() {
    let world = World::prepared();
    let table = frozen_table();
    let plan = both_builders(&world, &table);
    // Nothing to hand to the release entry point: the core refuses.
    let refusal = plan.qualify_for_release(&table).unwrap_err();
    assert_eq!(refusal.adapter, Some(adapter(CLUB48)));
    assert_eq!(refusal.blocker, ReleaseBlocker::OrderNotEstablished);
}

#[test]
fn qualified_records_over_fake_transports_give_no_qualified_send_time() {
    // Synthetic records with everything qualified, including the send
    // boundary, and even a qualified release plan. Dispatched over fake
    // transports, which is only possible diagnostically, the send time is
    // still unknown: a record alone never makes a transport qualified.
    let mut world = World::prepared();
    let club48 = CapabilityId::Club48BuilderV1.record().release_grade();
    let blockrazor = CapabilityId::BlockRazorBuilderV1.record().release_grade();
    let table = table_of(&[(CLUB48, club48), (BLOCKRAZOR, blockrazor)]);
    let plan = both_builders(&world, &table);
    let qualified = plan.qualify_for_release(&table).unwrap();

    let mut club48_frame = club48_frame(&world, &plan, &table);
    let mut blockrazor_frame = blockrazor_frame(&world, &plan, &table);
    let (mut a, mut b) = (FakeTransport::ready(), FakeTransport::ready());
    let current = world.snapshot;
    let mut armed = [
        Armed {
            adapter: adapter(CLUB48),
            transport: &mut a,
            frame: &mut club48_frame,
        },
        Armed {
            adapter: adapter(BLOCKRAZOR),
            transport: &mut b,
            frame: &mut blockrazor_frame,
        },
    ];
    let report = dispatch_diagnostic(
        &mut world.lane,
        qualified.plan(),
        &context(&table, &current),
        &quick_clock(),
        &deadline(),
        &mut armed,
    )
    .unwrap();

    assert_eq!(report.evidence, EvidenceClass::Diagnostic);
    for index in [CLUB48, BLOCKRAZOR] {
        let record = report.adapter(adapter(index)).unwrap();
        assert_eq!(record.outcome, SendOutcome::HandedOff);
        assert!(record.record_qualifies_boundary);
        assert!(!record.transport_qualified);
        assert_eq!(record.send_time(), SendTime::TransportUnqualified);
        assert_eq!(record.t_send_ns(), None);
        assert_eq!(report.select_to_send_ns(adapter(index)), None);
    }
}
