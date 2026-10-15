//! Use of the test support from outside the crate, as a dependent crate's
//! tests will use it. Compiled only with the `test-support` feature;
//! without it this file contains no tests.
#![cfg(feature = "test-support")]

use alloy_primitives::Address;
use darkbot_bsc_trading::intent::{AdapterId, ArmedFence, HaltReason, IntentLane};
use darkbot_bsc_trading::submission_policy::capability::testkit::{qualified, unqualified};
use darkbot_bsc_trading::submission_policy::capability::{
    AdapterTable, CapabilityError, CapabilityId, Knowledge, Qualification,
};

const CHAIN: u64 = 56;

fn sender() -> Address {
    Address::from_slice(&[0x5eu8; 20])
}

#[test]
fn a_dependent_crate_can_arm_a_lane_for_its_tests() {
    let token = Address::from_slice(&[0x77u8; 20]);
    let mut lane = IntentLane::armed(ArmedFence::for_tests(CHAIN, sender(), 7), 2).unwrap();
    assert_eq!(lane.halt_reason(), None);
    let ticket = lane.reserve(token).unwrap();
    assert_eq!(ticket.chain_id(), CHAIN);
    assert_eq!(ticket.sender(), sender());
    assert_eq!(ticket.nonce(), 7);
    assert_eq!(ticket.token(), token);

    // The production constructor for a restart is unaffected: it still
    // yields a lane that admits nothing.
    let mut quarantined = IntentLane::quarantined(CHAIN, sender(), 2).unwrap();
    assert_eq!(
        quarantined.halt_reason(),
        Some(HaltReason::RestartQuarantine)
    );
    assert!(quarantined.reserve(token).is_err());
}

#[test]
fn a_dependent_crate_can_build_synthetic_capability_records() {
    let adapter = AdapterId::new(0).unwrap();
    let frozen = *CapabilityId::Club48BuilderV1.record();
    let grade = frozen.release_grade();
    assert!(grade.no_merge().established());

    // A synthetic record is a copy. The frozen record is untouched and
    // still has nothing qualified.
    let still_frozen = CapabilityId::Club48BuilderV1.record();
    assert_eq!(*still_frozen, frozen);
    assert!(still_frozen.no_merge().enabled());
    assert!(!still_frozen.no_merge().established());

    assert!(qualified(Knowledge::Yes).established(Knowledge::Yes));
    assert!(!unqualified(Knowledge::Yes).established(Knowledge::Yes));
    assert_eq!(
        unqualified(Knowledge::No).qualification,
        Qualification::Unqualified
    );

    let table = AdapterTable::for_tests(CHAIN, 1, &[(adapter, grade)]).unwrap();
    assert_eq!(table.record(adapter), Some(&grade));
    // A synthetic table is validated like a production one.
    assert_eq!(
        AdapterTable::for_tests(97, 1, &[(adapter, grade)]),
        Err(CapabilityError::WrongChain)
    );
}
