//! Regressions of the intent lane. Every intent in these tests is built
//! through the production path: classified launch, qualified state,
//! admission, expectation, real signature and final binding check.

use alloy_primitives::{Address, B256, U256};

use super::*;
use crate::chain_state::SnapshotId;
use crate::chain_state::canonical::{
    ExecutionStatus, FinalityProof, InclusionProof, OrphanProof, ReconciliationProof,
};
use crate::economics::flap::native_curve20::{
    QUALIFIED_MAINNET_INPUT_WEI, QualifiedNativeState, WITNESSED_MAINNET_TERMS, admit_native_buy,
};
use crate::economics::policy::BuyPolicy;
use crate::signer::Signer;
use crate::transactions::legacy::LegacyTx;
use crate::venues::flap::binding::{BindingError, ExpectedBuy};
use crate::venues::flap::deployment::testkit::validated_mainnet;
use crate::venues::flap::profile::testkit::witnessed_native_launch;
use crate::venues::flap::profile::{ClassifiedLaunch, classify_launch};

const CHAIN: u64 = 56;
const FIRST_NONCE: u64 = 7;
const CODE_GENERATION: u64 = 7;
const POLICY_GENERATION: u64 = 3;
const GAS_PRICE: u64 = 310_000_000;
const GAS_LIMIT: u64 = 1_500_000;

fn signer() -> Signer {
    Signer::from_secret_bytes(&[0x51u8; 32]).unwrap()
}

/// A scratch buffer as the runner preallocates it for `prepare`.
fn prepare_scratch() -> Vec<u8> {
    Vec::with_capacity(PREPARE_SCRATCH_CAPACITY)
}

/// The plan costs of every BUY these tests sign: the qualified input and the
/// gas of their one policy, no own launch, no provider payment.
fn buy_costs() -> PlanCosts {
    let buy_input_wei = U256::from(QUALIFIED_MAINNET_INPUT_WEI);
    let buy_gas_wei = U256::from(GAS_PRICE) * U256::from(GAS_LIMIT);
    PlanCosts {
        buy_input_wei,
        buy_gas_wei,
        own_launch_wei: U256::ZERO,
        provider_payment_wei: U256::ZERO,
        total_wei: buy_input_wei + buy_gas_wei,
    }
}

fn adapter(index: u8) -> AdapterId {
    AdapterId::new(index).unwrap()
}

fn adapters(indices: &[u8]) -> AdapterSet {
    let ids: Vec<AdapterId> = indices.iter().map(|i| adapter(*i)).collect();
    AdapterSet::new(&ids).unwrap()
}

/// A 32-byte word that encodes `id` and a marker.
fn word(id: u64, marker: u8) -> B256 {
    let mut bytes = [marker; 32];
    bytes[24..].copy_from_slice(&id.to_be_bytes());
    B256::from_slice(&bytes)
}

fn block(id: u64) -> B256 {
    word(id, 0xb7)
}

/// The witnessed launch with salt `id`, carried by its own transaction.
fn classified(id: u64) -> ClassifiedLaunch {
    let mut launch = witnessed_native_launch();
    launch.salt = word(id, 0x5a);
    classify_launch(&validated_mainnet(CODE_GENERATION), word(id, 0xca), &launch).unwrap()
}

fn token(id: u64) -> Address {
    classified(id).token()
}

fn snapshot(id: u64, policy_generation: u64) -> SnapshotId {
    SnapshotId {
        canonical_block_hash: B256::from_slice(&[0xb1u8; 32]),
        ordered_state_cursor: 41,
        pending_candidate: word(id, 0xca),
        policy_generation,
        codehash_generation: CODE_GENERATION,
    }
}

/// A signed BUY and everything needed to prepare it.
struct Signed {
    expected: ExpectedBuy,
    snapshot: SnapshotId,
    raw: Vec<u8>,
    hash: B256,
}

/// Builds and signs the admitted BUY of launch `id` with `nonce`.
fn signed_by(signer: &Signer, id: u64, nonce: u64, policy_generation: u64) -> Signed {
    let deployment = validated_mainnet(CODE_GENERATION);
    let current = snapshot(id, policy_generation);
    let terms = WITNESSED_MAINNET_TERMS;
    let state = QualifiedNativeState::fresh_launch(&classified(id), &terms, 0, current).unwrap();
    let policy = BuyPolicy::new(
        policy_generation,
        U256::from(QUALIFIED_MAINNET_INPUT_WEI),
        100,
        GAS_PRICE,
        GAS_LIMIT,
    )
    .unwrap();
    let admitted = admit_native_buy(&state, &policy).unwrap();
    let expected =
        ExpectedBuy::from_admitted(&deployment, &admitted, &policy, signer.address(), nonce)
            .unwrap();
    let calldata = admitted.calldata().unwrap();
    let tx = LegacyTx {
        chain_id: expected.chain_id(),
        nonce,
        gas_price: expected.gas_price(),
        gas_limit: expected.gas_limit(),
        to: expected.portal(),
        value: expected.input_amount(),
        input: &calldata,
    };
    let mut scratch = Vec::new();
    let mut raw = Vec::new();
    let hash = signer.sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
    Signed {
        expected,
        snapshot: current,
        raw,
        hash,
    }
}

fn fence(chain_id: u64, sender: Address, first_nonce: u64) -> ArmedFence {
    ArmedFence::for_tests(chain_id, sender, first_nonce)
}

fn armed(capacity: usize) -> IntentLane {
    IntentLane::armed(fence(CHAIN, signer().address(), FIRST_NONCE), capacity).unwrap()
}

/// Reserves and prepares the intent of launch `id`.
fn prepared(lane: &mut IntentLane, id: u64) -> (Ticket, Signed) {
    let ticket = lane.reserve(token(id)).unwrap();
    let signed = signed_by(&signer(), id, ticket.nonce(), POLICY_GENERATION);
    let mut scratch = prepare_scratch();
    let hash = lane
        .prepare(
            ticket,
            &signed.raw,
            &signed.expected,
            &signed.snapshot,
            &mut scratch,
        )
        .unwrap();
    assert_eq!(hash, signed.hash);
    (ticket, signed)
}

/// Reserves, prepares and dispatches the intent of launch `id`.
fn dispatched(lane: &mut IntentLane, id: u64, set: AdapterSet) -> (Ticket, Signed) {
    let (ticket, signed) = prepared(lane, id);
    let dispatch = lane
        .begin_send(&signed.snapshot, set, &buy_costs())
        .unwrap();
    assert_eq!(dispatch.ticket, ticket);
    assert_eq!(dispatch.tx_hash, signed.hash);
    (ticket, signed)
}

fn included(ticket: Ticket, tx_hash: B256, block_hash: B256) -> InclusionProof {
    InclusionProof::for_tests(
        CHAIN,
        ticket.sender(),
        ticket.nonce(),
        tx_hash,
        block_hash,
        ExecutionStatus::Succeeded,
    )
}

fn reverted(ticket: Ticket, tx_hash: B256, block_hash: B256) -> InclusionProof {
    InclusionProof::for_tests(
        CHAIN,
        ticket.sender(),
        ticket.nonce(),
        tx_hash,
        block_hash,
        ExecutionStatus::Reverted,
    )
}

fn finality(ticket: Ticket, block_hash: B256) -> FinalityProof {
    FinalityProof::for_tests(CHAIN, ticket.sender(), ticket.nonce(), block_hash)
}

fn orphan(ticket: Ticket, block_hash: B256) -> OrphanProof {
    OrphanProof::for_tests(CHAIN, ticket.sender(), ticket.nonce(), block_hash)
}

fn reconciled(next_nonce: u64) -> ReconciliationProof {
    ReconciliationProof::for_tests(CHAIN, signer().address(), next_nonce)
}

// ---------------------------------------------------------------------------
// Dedup, nonces, order (Gate-0 `intent_lane` regressions, matrix N1)
// ---------------------------------------------------------------------------

#[test]
fn duplicate_before_and_after_resolution_never_consumes_a_nonce() {
    let mut lane = armed(3);
    let (a, signed) = dispatched(&mut lane, 1, adapters(&[0]));
    assert_eq!(a.nonce(), FIRST_NONCE);
    assert_eq!(lane.reserve(token(1)), Err(LaneError::Duplicate));

    lane.apply_inclusion(&included(a, signed.hash, block(1)))
        .unwrap();
    lane.apply_finality(&finality(a, block(1))).unwrap();
    assert_eq!(lane.state(a), Ok(IntentState::Canonical));
    assert_eq!(lane.reserve(token(1)), Err(LaneError::Duplicate));

    assert_eq!(lane.reserve(token(2)).unwrap().nonce(), FIRST_NONCE + 1);
    assert_eq!(lane.stats().duplicates, 2);
}

#[test]
fn eight_thousand_signals_yield_one_thousand_contiguous_reservations() {
    // Gate-0: 8 workers x 1,000 tokens through a mutex. The lane has a
    // single owner, so the same signals arrive interleaved on one thread.
    let tokens: Vec<Address> = (0..1000).map(token).collect();
    let mut lane = armed(1000);
    let mut nonces = Vec::with_capacity(1000);
    for round in 0..8 {
        for (index, token) in tokens.iter().enumerate() {
            match lane.reserve(*token) {
                Ok(ticket) => {
                    assert_eq!(round, 0);
                    assert_eq!(ticket.nonce(), FIRST_NONCE + index as u64);
                    nonces.push(ticket.nonce());
                }
                Err(e) => assert_eq!(e, LaneError::Duplicate),
            }
        }
    }
    assert_eq!(nonces.len(), 1000);
    let expected: Vec<u64> = (FIRST_NONCE..FIRST_NONCE + 1000).collect();
    assert_eq!(nonces, expected);
    assert_eq!(lane.stats().duplicates, 7000);
    assert_eq!(lane.stats().capacity_rejections, 0);
}

#[test]
fn sixty_four_intents_dispatch_in_nonce_order_with_unique_hashes() {
    let mut lane = armed(64);
    let set = adapters(&[0]);
    let mut pending = Vec::new();
    for id in 0..64 {
        pending.push(prepared(&mut lane, id));
    }
    let mut hashes: Vec<B256> = Vec::new();
    for (index, (ticket, signed)) in pending.iter().enumerate() {
        let dispatch = lane
            .begin_send(&signed.snapshot, set, &buy_costs())
            .unwrap();
        assert_eq!(dispatch.ticket, *ticket);
        assert_eq!(dispatch.ticket.nonce(), FIRST_NONCE + index as u64);
        assert!(!hashes.contains(&dispatch.tx_hash));
        hashes.push(dispatch.tx_hash);
        // The next intent cannot leave before this nonce is consumed.
        assert_eq!(
            lane.begin_send(&signed.snapshot, set, &buy_costs()),
            Err(LaneError::NotReady)
        );
        lane.apply_inclusion(&included(*ticket, signed.hash, block(index as u64)))
            .unwrap();
    }
    assert_eq!(hashes.len(), 64);
    assert_eq!(
        lane.begin_send(&snapshot(0, POLICY_GENERATION), set, &buy_costs()),
        Err(LaneError::NotReady)
    );
}

#[test]
fn reversed_preparation_cannot_bypass_an_unready_or_unresolved_head() {
    let mut lane = armed(2);
    let set = adapters(&[0]);
    let a = lane.reserve(token(1)).unwrap();
    let b = lane.reserve(token(2)).unwrap();
    let mut scratch = prepare_scratch();

    // B is prepared first; the head A is not.
    let signed_b = signed_by(&signer(), 2, b.nonce(), POLICY_GENERATION);
    lane.prepare(
        b,
        &signed_b.raw,
        &signed_b.expected,
        &signed_b.snapshot,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(
        lane.begin_send(&signed_b.snapshot, set, &buy_costs()),
        Err(LaneError::NotReady)
    );

    let signed_a = signed_by(&signer(), 1, a.nonce(), POLICY_GENERATION);
    lane.prepare(
        a,
        &signed_a.raw,
        &signed_a.expected,
        &signed_a.snapshot,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(
        lane.begin_send(&signed_a.snapshot, set, &buy_costs())
            .unwrap()
            .ticket,
        a
    );
    // A is sent but unresolved: B still waits.
    assert_eq!(
        lane.begin_send(&signed_b.snapshot, set, &buy_costs()),
        Err(LaneError::NotReady)
    );
    // Evidence for B before A contradicts nonce order and stops the lane.
    assert_eq!(
        lane.apply_inclusion(&included(b, signed_b.hash, block(1))),
        Err(LaneError::ConflictingEvidence)
    );
    assert_eq!(lane.halt_reason(), Some(HaltReason::ConflictingEvidence));
}

#[test]
fn conflicting_preparation_is_rejected() {
    let mut lane = armed(1);
    let (ticket, signed) = prepared(&mut lane, 1);
    let mut scratch = prepare_scratch();
    // A second preparation of the same intent.
    assert_eq!(
        lane.prepare(
            ticket,
            &signed.raw,
            &signed.expected,
            &signed.snapshot,
            &mut scratch
        ),
        Err(LaneError::InvalidState)
    );
}

#[test]
fn capacity_and_nonce_exhaustion_fail_closed() {
    let mut lane = armed(1);
    lane.reserve(token(1)).unwrap();
    assert_eq!(lane.reserve(token(2)), Err(LaneError::CapacityExceeded));
    assert_eq!(lane.stats().capacity_rejections, 1);
    // The refused token did not enter the dedup set.
    assert_eq!(lane.reserve(token(2)), Err(LaneError::CapacityExceeded));

    let sender = signer().address();
    let mut lane = IntentLane::armed(fence(CHAIN, sender, u64::MAX), 1).unwrap();
    assert_eq!(lane.reserve(token(1)), Err(LaneError::NonceExhausted));

    assert_eq!(
        IntentLane::armed(fence(CHAIN, sender, 0), 0).err(),
        Some(LaneError::InvalidCapacity)
    );
    assert_eq!(
        IntentLane::armed(fence(CHAIN, sender, 0), MAX_LANE_CAPACITY + 1).err(),
        Some(LaneError::InvalidCapacity)
    );
}

// ---------------------------------------------------------------------------
// Recipient isolation and final binding (matrix N2)
// ---------------------------------------------------------------------------

#[test]
fn tickets_cannot_cross_sender_or_chain_lanes() {
    let mut lane = armed(1);
    let ticket = lane.reserve(token(1)).unwrap();
    let other_sender = Address::from_slice(&[0x77u8; 20]);
    for (chain_id, sender) in [(CHAIN, other_sender), (97, signer().address())] {
        let mut other = IntentLane::armed(fence(chain_id, sender, FIRST_NONCE), 1).unwrap();
        other.reserve(token(1)).unwrap();
        assert_eq!(other.state(ticket), Err(LaneError::UnknownTicket));
        assert_eq!(other.invalidate(ticket), Err(LaneError::UnknownTicket));
        assert_eq!(other.withdraw(ticket), Err(LaneError::UnknownTicket));
    }
}

#[test]
fn preparation_binds_sender_nonce_and_token_of_the_ticket() {
    let mut lane = armed(2);
    let ticket = lane.reserve(token(1)).unwrap();
    let mut scratch = prepare_scratch();

    // Signed by another key: the tokens would go to another recipient.
    let stranger = Signer::from_secret_bytes(&[0x52u8; 32]).unwrap();
    let foreign = signed_by(&stranger, 1, ticket.nonce(), POLICY_GENERATION);
    assert_eq!(
        lane.prepare(
            ticket,
            &foreign.raw,
            &foreign.expected,
            &foreign.snapshot,
            &mut scratch
        ),
        Err(LaneError::BindingMismatch)
    );

    // Our expectation, but bytes signed by the other key.
    let ours = signed_by(&signer(), 1, ticket.nonce(), POLICY_GENERATION);
    assert_eq!(
        lane.prepare(
            ticket,
            &foreign.raw,
            &ours.expected,
            &ours.snapshot,
            &mut scratch
        ),
        Err(LaneError::Binding(BindingError::WrongSender))
    );

    // Another nonce or another token than the ticket's.
    let wrong_nonce = signed_by(&signer(), 1, ticket.nonce() + 1, POLICY_GENERATION);
    assert_eq!(
        lane.prepare(
            ticket,
            &wrong_nonce.raw,
            &wrong_nonce.expected,
            &wrong_nonce.snapshot,
            &mut scratch
        ),
        Err(LaneError::BindingMismatch)
    );
    let wrong_token = signed_by(&signer(), 2, ticket.nonce(), POLICY_GENERATION);
    assert_eq!(
        lane.prepare(
            ticket,
            &wrong_token.raw,
            &wrong_token.expected,
            &wrong_token.snapshot,
            &mut scratch
        ),
        Err(LaneError::BindingMismatch)
    );

    // Nothing was bound by the refused attempts.
    assert_eq!(lane.state(ticket), Ok(IntentState::Reserved));
    lane.prepare(
        ticket,
        &ours.raw,
        &ours.expected,
        &ours.snapshot,
        &mut scratch,
    )
    .unwrap();
    // The bound bytes are exactly the verified ones.
    lane.begin_send(&ours.snapshot, adapters(&[0]), &buy_costs())
        .unwrap();
    assert_eq!(
        lane.start_write(ticket, adapter(0), ours.hash)
            .map(|write| write.bytes()),
        Ok(ours.raw.as_slice())
    );
}

// ---------------------------------------------------------------------------
// Fan-out: one intent, one nonce, identical bytes (matrix S1)
// ---------------------------------------------------------------------------

#[test]
fn fan_out_uses_one_nonce_and_identical_bytes_for_every_adapter() {
    let mut lane = armed(2);
    let set = adapters(&[0, 1, 2]);
    assert_eq!(set.count(), 3);
    let (ticket, signed) = dispatched(&mut lane, 1, set);

    // The economic admission is consumed once; it cannot be taken again.
    assert_eq!(
        lane.begin_send(&signed.snapshot, set, &buy_costs()),
        Err(LaneError::NotReady)
    );

    let mut written: Vec<Vec<u8>> = Vec::new();
    for index in 0..3u8 {
        assert_eq!(
            lane.submission(ticket, adapter(index)),
            Ok(SubmissionState::NotStarted)
        );
        // Consuming the permit is what hands the bytes to the adapter.
        let bytes = lane
            .start_write(ticket, adapter(index), signed.hash)
            .unwrap()
            .bytes()
            .to_vec();
        written.push(bytes);
        // Adapter B starts without waiting for adapter A's ACK.
        assert_eq!(
            lane.submission(ticket, adapter(index)),
            Ok(SubmissionState::Writing)
        );
    }
    assert!(written.iter().all(|bytes| *bytes == signed.raw));
    assert_eq!(lane.state(ticket), Ok(IntentState::Sending));

    // An adapter outside the selected set has no permit.
    assert_eq!(
        lane.start_write(ticket, adapter(3), signed.hash),
        Err(LaneError::AdapterNotSelected)
    );
    // Fan-out reserved nothing: the next intent gets the next nonce.
    assert_eq!(lane.reserve(token(2)).unwrap().nonce(), FIRST_NONCE + 1);
}

#[test]
fn a_permit_is_consumed_exactly_once() {
    let mut lane = armed(1);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0, 1]));
    lane.start_write(ticket, adapter(0), signed.hash).unwrap();
    assert_eq!(
        lane.start_write(ticket, adapter(0), signed.hash),
        Err(LaneError::PermitConsumed)
    );
    lane.write_completed(ticket, adapter(0)).unwrap();
    assert_eq!(
        lane.start_write(ticket, adapter(0), signed.hash),
        Err(LaneError::PermitConsumed)
    );
    // Lifecycle steps cannot be skipped or repeated.
    assert_eq!(
        lane.write_completed(ticket, adapter(0)),
        Err(LaneError::InvalidState)
    );
    assert_eq!(lane.acked(ticket, adapter(1)), Err(LaneError::InvalidState));
    assert_eq!(
        lane.write_failed(ticket, adapter(1)),
        Err(LaneError::InvalidState)
    );
}

#[test]
fn reversed_ack_order_and_ack_never_confirm_the_intent() {
    let mut lane = armed(2);
    let set = adapters(&[0, 1]);
    let (ticket, signed) = dispatched(&mut lane, 1, set);
    let (_, follower) = prepared(&mut lane, 2);
    for index in 0..2u8 {
        lane.start_write(ticket, adapter(index), signed.hash)
            .unwrap();
        lane.write_completed(ticket, adapter(index)).unwrap();
    }
    // B acknowledges before A.
    lane.acked(ticket, adapter(1)).unwrap();
    lane.acked(ticket, adapter(0)).unwrap();
    assert_eq!(
        lane.submission(ticket, adapter(0)),
        Ok(SubmissionState::Acked)
    );
    assert_eq!(
        lane.submission(ticket, adapter(1)),
        Ok(SubmissionState::Acked)
    );

    // Two ACKs are not an inclusion: the intent is still being sent and
    // the follower still waits behind it.
    assert_eq!(lane.state(ticket), Ok(IntentState::Sending));
    assert_eq!(lane.outcome(ticket), Ok(None));
    assert_eq!(
        lane.begin_send(&follower.snapshot, set, &buy_costs()),
        Err(LaneError::NotReady)
    );
    assert_eq!(lane.halt_reason(), None);
}

#[test]
fn rejection_leaves_the_intent_unresolved_and_late_execution_is_kept() {
    let mut lane = armed(2);
    let set = adapters(&[0, 1]);
    let (ticket, signed) = dispatched(&mut lane, 1, set);
    let (_, follower) = prepared(&mut lane, 2);
    for index in 0..2u8 {
        lane.start_write(ticket, adapter(index), signed.hash)
            .unwrap();
        lane.write_completed(ticket, adapter(index)).unwrap();
    }
    // One adapter rejects (or expires) while the other stays active.
    lane.rejected(ticket, adapter(0)).unwrap();
    lane.acked(ticket, adapter(1)).unwrap();
    assert_eq!(
        lane.submission(ticket, adapter(0)),
        Ok(SubmissionState::Rejected)
    );
    // Then the second one rejects as well, after its ACK.
    lane.rejected(ticket, adapter(1)).unwrap();

    // Rejections do not free the nonce or resolve the intent.
    assert_eq!(lane.state(ticket), Ok(IntentState::Sending));
    assert_eq!(
        lane.begin_send(&follower.snapshot, set, &buy_costs()),
        Err(LaneError::NotReady)
    );
    assert_eq!(lane.withdraw(ticket), Err(LaneError::NotTail));

    // The disclosed bytes execute later anyway: the acquisition is kept.
    assert_eq!(
        lane.apply_inclusion(&included(ticket, signed.hash, block(9))),
        Ok(IntentState::Included)
    );
    assert_eq!(lane.outcome(ticket), Ok(Some(Outcome::Included)));
}

// ---------------------------------------------------------------------------
// UNKNOWN: partial write, flush failure, wrong hash (matrix S1, SN08)
// ---------------------------------------------------------------------------

#[test]
fn failed_write_is_unknown_halts_and_suppresses_unstarted_permits() {
    let mut lane = armed(2);
    let set = adapters(&[0, 1, 2]);
    let (ticket, signed) = dispatched(&mut lane, 1, set);

    // A handed off, B fails mid-write, C has not started.
    lane.start_write(ticket, adapter(0), signed.hash).unwrap();
    lane.write_completed(ticket, adapter(0)).unwrap();
    lane.start_write(ticket, adapter(1), signed.hash).unwrap();
    lane.write_failed(ticket, adapter(1)).unwrap();

    assert_eq!(
        lane.submission(ticket, adapter(1)),
        Ok(SubmissionState::Unknown)
    );
    assert_eq!(
        lane.submission(ticket, adapter(2)),
        Ok(SubmissionState::Suppressed)
    );
    assert_eq!(lane.state(ticket), Ok(IntentState::Halted));
    assert_eq!(lane.halt_reason(), Some(HaltReason::UnknownSubmission));

    // No blind retry, no further fan-out, no new economic admission.
    assert_eq!(
        lane.start_write(ticket, adapter(1), signed.hash),
        Err(LaneError::PermitConsumed)
    );
    assert_eq!(
        lane.start_write(ticket, adapter(2), signed.hash),
        Err(LaneError::PermitSuppressed)
    );
    assert_eq!(
        lane.begin_send(&signed.snapshot, set, &buy_costs()),
        Err(LaneError::Halted)
    );
    assert_eq!(lane.reserve(token(2)), Err(LaneError::Halted));
    assert_eq!(lane.invalidate(ticket), Err(LaneError::Halted));
    assert_eq!(lane.withdraw(ticket), Err(LaneError::Halted));

    // The submission that already started is not cancelled by bookkeeping:
    // its real outcome is still recorded.
    assert_eq!(
        lane.submission(ticket, adapter(0)),
        Ok(SubmissionState::HandedOff)
    );
    lane.acked(ticket, adapter(0)).unwrap();
    assert_eq!(
        lane.submission(ticket, adapter(0)),
        Ok(SubmissionState::Acked)
    );

    // Reconciliation is refused while the outcome is unknown: the nonce
    // cannot be reused.
    assert_eq!(
        lane.resume(&reconciled(FIRST_NONCE)),
        Err(LaneError::UnresolvedIntents)
    );
}

#[test]
fn wrong_hash_at_the_permit_never_reaches_a_writer() {
    // Gate-0 `mismatched_request_permit_never_reaches_writer`.
    let mut lane = armed(1);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0, 1]));
    assert_eq!(
        lane.start_write(ticket, adapter(0), block(1)),
        Err(LaneError::WrongHash)
    );
    assert_eq!(lane.halt_reason(), Some(HaltReason::WrongHash));
    // No permit was consumed; both are withdrawn.
    assert_eq!(
        lane.submission(ticket, adapter(0)),
        Ok(SubmissionState::Suppressed)
    );
    assert_eq!(
        lane.submission(ticket, adapter(1)),
        Ok(SubmissionState::Suppressed)
    );
    assert_eq!(
        lane.start_write(ticket, adapter(1), signed.hash),
        Err(LaneError::PermitSuppressed)
    );
}

#[test]
fn external_halt_stops_admission_and_keeps_the_first_reason() {
    // Gate-0 `unknown_write_outcome_halts_without_retry_replacement_or_nonce_reuse`.
    let mut lane = armed(3);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0]));
    lane.halt(HaltReason::External);
    lane.halt(HaltReason::Reorg);
    assert_eq!(lane.halt_reason(), Some(HaltReason::External));
    assert_eq!(
        lane.begin_send(&signed.snapshot, adapters(&[0]), &buy_costs()),
        Err(LaneError::Halted)
    );
    assert_eq!(lane.reserve(token(2)), Err(LaneError::Halted));
    assert_eq!(
        lane.start_write(ticket, adapter(0), signed.hash),
        Err(LaneError::PermitSuppressed)
    );
}

#[test]
fn conflicting_evidence_outranks_an_earlier_reason_but_not_a_quarantine() {
    let mut lane = armed(1);
    lane.halt(HaltReason::External);
    lane.halt(HaltReason::ConflictingEvidence);
    assert_eq!(lane.halt_reason(), Some(HaltReason::ConflictingEvidence));
    // Later reasons do not hide it again.
    lane.halt(HaltReason::External);
    assert_eq!(lane.halt_reason(), Some(HaltReason::ConflictingEvidence));

    let mut lane = IntentLane::quarantined(CHAIN, signer().address(), 1).unwrap();
    lane.halt(HaltReason::ConflictingEvidence);
    assert_eq!(lane.halt_reason(), Some(HaltReason::RestartQuarantine));
}

// ---------------------------------------------------------------------------
// Head-of-line coupling (matrix N3)
// ---------------------------------------------------------------------------

#[test]
fn unresolved_head_blocks_every_prepared_follower_and_is_counted() {
    for resolve in [false, true] {
        let mut lane = armed(9);
        let set = adapters(&[0]);
        let (head, signed) = dispatched(&mut lane, 1, set);
        lane.start_write(head, adapter(0), signed.hash).unwrap();
        lane.write_completed(head, adapter(0)).unwrap();

        let mut followers = Vec::new();
        for id in 2..10 {
            let follower = prepared(&mut lane, id);
            assert_eq!(
                lane.begin_send(&follower.1.snapshot, set, &buy_costs()),
                Err(LaneError::NotReady)
            );
            followers.push(follower);
        }
        assert_eq!(lane.stats().head_of_line_blocks, 8);

        if resolve {
            lane.apply_inclusion(&included(head, signed.hash, block(1)))
                .unwrap();
            let next = lane
                .begin_send(&followers[0].1.snapshot, set, &buy_costs())
                .unwrap();
            assert_eq!(next.ticket.nonce(), FIRST_NONCE + 1);
        } else {
            lane.halt(HaltReason::External);
            assert_eq!(
                lane.begin_send(&followers[0].1.snapshot, set, &buy_costs()),
                Err(LaneError::Halted)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Prepared-state invalidation
// ---------------------------------------------------------------------------

#[test]
fn stale_preparation_is_not_dispatched_and_must_be_prepared_again() {
    let mut lane = armed(1);
    let set = adapters(&[0]);
    let (ticket, old) = prepared(&mut lane, 1);
    let mut scratch = prepare_scratch();

    // The operating policy changed: another snapshot identity.
    let current = snapshot(1, POLICY_GENERATION + 1);
    assert_eq!(
        lane.begin_send(&current, set, &buy_costs()),
        Err(LaneError::StalePreparation)
    );
    assert_eq!(lane.state(ticket), Ok(IntentState::Prepared));
    // Every other component of the identity is bound as well.
    let mut other = old.snapshot;
    other.pending_candidate = word(2, 0xca);
    assert_eq!(
        lane.begin_send(&other, set, &buy_costs()),
        Err(LaneError::StalePreparation)
    );
    let mut other = old.snapshot;
    other.codehash_generation += 1;
    assert_eq!(
        lane.begin_send(&other, set, &buy_costs()),
        Err(LaneError::StalePreparation)
    );
    let mut other = old.snapshot;
    other.ordered_state_cursor += 1;
    assert_eq!(
        lane.begin_send(&other, set, &buy_costs()),
        Err(LaneError::StalePreparation)
    );

    // The old bytes cannot be bound again for the new snapshot.
    lane.invalidate(ticket).unwrap();
    assert_eq!(lane.state(ticket), Ok(IntentState::Reserved));
    assert_eq!(
        lane.prepare(ticket, &old.raw, &old.expected, &current, &mut scratch),
        Err(LaneError::Binding(BindingError::StaleSnapshot))
    );

    // A new admission under the new policy, same nonce, is accepted.
    let fresh = signed_by(&signer(), 1, ticket.nonce(), POLICY_GENERATION + 1);
    lane.prepare(
        ticket,
        &fresh.raw,
        &fresh.expected,
        &fresh.snapshot,
        &mut scratch,
    )
    .unwrap();
    let dispatch = lane.begin_send(&current, set, &buy_costs()).unwrap();
    assert_eq!(dispatch.ticket.nonce(), FIRST_NONCE);
    assert_eq!(dispatch.tx_hash, fresh.hash);
}

#[test]
fn only_the_newest_undisclosed_intent_can_be_withdrawn() {
    let mut lane = armed(3);
    let (a, _) = prepared(&mut lane, 1);
    let b = lane.reserve(token(2)).unwrap();

    // A has a reservation behind it.
    assert_eq!(lane.withdraw(a), Err(LaneError::NotTail));
    lane.withdraw(b).unwrap();
    assert_eq!(lane.state(b), Err(LaneError::UnknownTicket));
    assert_eq!(lane.stats().withdrawn, 1);

    // The nonce returns to the lane; the token stays deduplicated.
    assert_eq!(lane.reserve(token(2)), Err(LaneError::Duplicate));
    let c = lane.reserve(token(3)).unwrap();
    assert_eq!(c.nonce(), b.nonce());
    // The old ticket does not address the new intent on that nonce.
    assert_eq!(lane.state(b), Err(LaneError::UnknownTicket));

    // A dispatched intent can never be withdrawn.
    lane.withdraw(c).unwrap();
    let signed_a = signed_by(&signer(), 1, a.nonce(), POLICY_GENERATION);
    lane.begin_send(&signed_a.snapshot, adapters(&[0]), &buy_costs())
        .unwrap();
    assert_eq!(lane.withdraw(a), Err(LaneError::InvalidState));
}

// ---------------------------------------------------------------------------
// Canonical evidence and reorg (interface to C2)
// ---------------------------------------------------------------------------

#[test]
fn inclusion_finality_and_reorg_transitions() {
    let mut lane = armed(2);
    let set = adapters(&[0, 1]);
    let (ticket, signed) = dispatched(&mut lane, 1, set);
    let (_, follower) = prepared(&mut lane, 2);
    lane.start_write(ticket, adapter(0), signed.hash).unwrap();
    lane.write_completed(ticket, adapter(0)).unwrap();

    // Included in block 1; the same evidence twice is harmless.
    let proof = included(ticket, signed.hash, block(1));
    assert_eq!(lane.apply_inclusion(&proof), Ok(IntentState::Included));
    assert_eq!(lane.apply_inclusion(&proof), Ok(IntentState::Included));
    assert_eq!(lane.halt_reason(), None);

    // Block 1 is orphaned: reorg after apparent resolution.
    assert_eq!(
        lane.apply_orphan(&orphan(ticket, block(2))),
        Err(LaneError::InvalidState)
    );
    lane.apply_orphan(&orphan(ticket, block(1))).unwrap();
    assert_eq!(lane.state(ticket), Ok(IntentState::Orphaned));
    assert_eq!(lane.halt_reason(), Some(HaltReason::Reorg));
    assert_eq!(
        lane.begin_send(&follower.snapshot, set, &buy_costs()),
        Err(LaneError::Halted)
    );
    assert_eq!(
        lane.resume(&reconciled(FIRST_NONCE)),
        Err(LaneError::UnresolvedIntents)
    );

    // The same bytes are included again in block 3 and become final.
    assert_eq!(
        lane.apply_inclusion(&included(ticket, signed.hash, block(3))),
        Ok(IntentState::Included)
    );
    assert_eq!(
        lane.apply_finality(&finality(ticket, block(1))),
        Err(LaneError::ConflictingEvidence)
    );
    // Included but not final is not enough to resume.
    assert_eq!(
        lane.resume(&reconciled(FIRST_NONCE + 1)),
        Err(LaneError::UnresolvedIntents)
    );
    lane.apply_finality(&finality(ticket, block(3))).unwrap();
    assert_eq!(lane.state(ticket), Ok(IntentState::Canonical));
    assert_eq!(lane.outcome(ticket), Ok(Some(Outcome::Included)));

    // Reconciliation with the wrong nonce is refused, the right one resumes.
    assert_eq!(
        lane.resume(&reconciled(FIRST_NONCE)),
        Err(LaneError::NonceMismatch)
    );
    lane.resume(&reconciled(FIRST_NONCE + 1)).unwrap();
    assert_eq!(lane.halt_reason(), None);
    assert_eq!(
        lane.resume(&reconciled(FIRST_NONCE + 1)),
        Err(LaneError::InvalidState)
    );
    let next = lane
        .begin_send(&follower.snapshot, set, &buy_costs())
        .unwrap();
    assert_eq!(next.ticket.nonce(), FIRST_NONCE + 1);
    assert_eq!(
        lane.submission(next.ticket, adapter(0)),
        Ok(SubmissionState::NotStarted)
    );
    assert_eq!(
        lane.submission(next.ticket, adapter(1)),
        Ok(SubmissionState::NotStarted)
    );

    // Even an intent already accepted as Canonical can later be orphaned by
    // C2. The lane rewinds and suppresses every still-unstarted permit of a
    // later intent that had already reached Sending.
    lane.apply_orphan(&orphan(ticket, block(3))).unwrap();
    assert_eq!(lane.state(ticket), Ok(IntentState::Orphaned));
    assert_eq!(lane.halt_reason(), Some(HaltReason::Reorg));
    assert_eq!(
        lane.submission(next.ticket, adapter(0)),
        Ok(SubmissionState::Suppressed)
    );
    assert_eq!(
        lane.submission(next.ticket, adapter(1)),
        Ok(SubmissionState::Suppressed)
    );
    assert_eq!(
        lane.resume(&reconciled(FIRST_NONCE)),
        Err(LaneError::UnresolvedIntents)
    );
}

#[test]
fn revert_consumes_the_nonce_and_stops_the_lane() {
    let mut lane = armed(2);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0]));
    assert_eq!(
        lane.apply_inclusion(&reverted(ticket, signed.hash, block(1))),
        Ok(IntentState::Reverted)
    );
    assert_eq!(lane.outcome(ticket), Ok(Some(Outcome::Reverted)));
    assert_eq!(lane.halt_reason(), Some(HaltReason::Reverted));
    assert_eq!(lane.reserve(token(2)), Err(LaneError::Halted));

    lane.apply_finality(&finality(ticket, block(1))).unwrap();
    lane.resume(&reconciled(FIRST_NONCE + 1)).unwrap();
    assert_eq!(lane.reserve(token(2)).unwrap().nonce(), FIRST_NONCE + 1);
    // The reverted token is not bought again in this session.
    assert_eq!(lane.reserve(token(1)), Err(LaneError::Duplicate));
}

#[test]
fn foreign_transaction_on_a_lane_nonce_is_replaced_and_stops_the_lane() {
    // On a dispatched intent.
    let mut lane = armed(2);
    let (ticket, _) = dispatched(&mut lane, 1, adapters(&[0, 1]));
    assert_eq!(
        lane.apply_inclusion(&included(ticket, block(77), block(1))),
        Ok(IntentState::Replaced)
    );
    assert_eq!(lane.outcome(ticket), Ok(Some(Outcome::Replaced)));
    assert_eq!(lane.halt_reason(), Some(HaltReason::NonceConsumedByOtherTx));
    // The unconsumed permits are gone.
    assert_eq!(
        lane.submission(ticket, adapter(1)),
        Ok(SubmissionState::Suppressed)
    );

    // On an intent that was never dispatched.
    let mut lane = armed(2);
    let reserved = lane.reserve(token(1)).unwrap();
    assert_eq!(
        lane.apply_inclusion(&included(reserved, block(77), block(1))),
        Ok(IntentState::Replaced)
    );
    assert_eq!(lane.halt_reason(), Some(HaltReason::NonceConsumedByOtherTx));

    // On a nonce the lane never assigned.
    let mut lane = armed(2);
    let sender = signer().address();
    let stray = InclusionProof::for_tests(
        CHAIN,
        sender,
        FIRST_NONCE,
        block(77),
        block(1),
        ExecutionStatus::Succeeded,
    );
    assert_eq!(lane.apply_inclusion(&stray), Err(LaneError::UnknownNonce));
    assert_eq!(lane.halt_reason(), Some(HaltReason::NonceConsumedByOtherTx));
}

#[test]
fn evidence_for_another_chain_or_sender_is_ignored() {
    let mut lane = armed(1);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0]));
    let other_sender = Address::from_slice(&[0x77u8; 20]);
    for (chain_id, sender) in [(97, ticket.sender()), (CHAIN, other_sender)] {
        let proof = InclusionProof::for_tests(
            chain_id,
            sender,
            ticket.nonce(),
            signed.hash,
            block(1),
            ExecutionStatus::Succeeded,
        );
        assert_eq!(
            lane.apply_inclusion(&proof),
            Err(LaneError::ForeignEvidence)
        );
        let proof = FinalityProof::for_tests(chain_id, sender, ticket.nonce(), block(1));
        assert_eq!(lane.apply_finality(&proof), Err(LaneError::ForeignEvidence));
        let proof = OrphanProof::for_tests(chain_id, sender, ticket.nonce(), block(1));
        assert_eq!(lane.apply_orphan(&proof), Err(LaneError::ForeignEvidence));
        let proof = ReconciliationProof::for_tests(chain_id, sender, FIRST_NONCE);
        assert_eq!(lane.resume(&proof), Err(LaneError::ForeignEvidence));
    }
    assert_eq!(lane.state(ticket), Ok(IntentState::Sending));
    assert_eq!(lane.halt_reason(), None);
}

#[test]
fn repeated_inclusion_evidence_must_repeat_the_execution_result() {
    // Recorded as Included; the same transaction and block as Reverted is a
    // contradiction, before and after the intent became Canonical.
    for make_final in [false, true] {
        let mut lane = armed(1);
        let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0]));
        let proof = included(ticket, signed.hash, block(1));
        assert_eq!(lane.apply_inclusion(&proof), Ok(IntentState::Included));
        let state = if make_final {
            lane.apply_finality(&finality(ticket, block(1))).unwrap();
            IntentState::Canonical
        } else {
            IntentState::Included
        };

        // Identical evidence stays idempotent and does not stop the lane.
        assert_eq!(lane.apply_inclusion(&proof), Ok(state));
        assert_eq!(lane.halt_reason(), None);

        assert_eq!(
            lane.apply_inclusion(&reverted(ticket, signed.hash, block(1))),
            Err(LaneError::ConflictingEvidence),
            "final {make_final}"
        );
        assert_eq!(
            lane.halt_reason(),
            Some(HaltReason::ConflictingEvidence),
            "final {make_final}"
        );
        // The record is unchanged.
        assert_eq!(lane.state(ticket), Ok(state));
        assert_eq!(lane.outcome(ticket), Ok(Some(Outcome::Included)));
    }

    // Recorded as Reverted; the same transaction and block as Succeeded is
    // a contradiction as well.
    for make_final in [false, true] {
        let mut lane = armed(1);
        let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0]));
        let proof = reverted(ticket, signed.hash, block(1));
        assert_eq!(lane.apply_inclusion(&proof), Ok(IntentState::Reverted));
        // A revert stops the lane on its own.
        assert_eq!(lane.halt_reason(), Some(HaltReason::Reverted));
        let state = if make_final {
            lane.apply_finality(&finality(ticket, block(1))).unwrap();
            IntentState::Canonical
        } else {
            IntentState::Reverted
        };

        assert_eq!(lane.apply_inclusion(&proof), Ok(state));

        assert_eq!(
            lane.apply_inclusion(&included(ticket, signed.hash, block(1))),
            Err(LaneError::ConflictingEvidence),
            "final {make_final}"
        );
        // The contradiction replaces the earlier stop reason.
        assert_eq!(
            lane.halt_reason(),
            Some(HaltReason::ConflictingEvidence),
            "final {make_final}"
        );
        // The record is unchanged.
        assert_eq!(lane.state(ticket), Ok(state));
        assert_eq!(lane.outcome(ticket), Ok(Some(Outcome::Reverted)));
    }
}

#[test]
fn repeated_evidence_with_another_transaction_or_block_is_conflicting() {
    let mut lane = armed(1);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0]));
    lane.apply_inclusion(&included(ticket, signed.hash, block(1)))
        .unwrap();
    // Same transaction in another block, without an orphan proof between.
    assert_eq!(
        lane.apply_inclusion(&included(ticket, signed.hash, block(2))),
        Err(LaneError::ConflictingEvidence)
    );
    assert_eq!(lane.halt_reason(), Some(HaltReason::ConflictingEvidence));

    let mut lane = armed(1);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0]));
    lane.apply_inclusion(&included(ticket, signed.hash, block(1)))
        .unwrap();
    // Another transaction on the same nonce in the same block.
    assert_eq!(
        lane.apply_inclusion(&included(ticket, block(77), block(1))),
        Err(LaneError::ConflictingEvidence)
    );
    assert_eq!(lane.halt_reason(), Some(HaltReason::ConflictingEvidence));
}

#[test]
fn repeated_evidence_of_a_foreign_transaction_ignores_its_status() {
    // The nonce was consumed by a transaction that is not the intent's.
    // Whether that transaction succeeded does not change the intent.
    let mut lane = armed(1);
    let (ticket, _) = dispatched(&mut lane, 1, adapters(&[0]));
    let foreign = block(77);
    assert_eq!(
        lane.apply_inclusion(&included(ticket, foreign, block(1))),
        Ok(IntentState::Replaced)
    );
    assert_eq!(
        lane.apply_inclusion(&reverted(ticket, foreign, block(1))),
        Ok(IntentState::Replaced)
    );
    assert_eq!(
        lane.apply_inclusion(&included(ticket, foreign, block(1))),
        Ok(IntentState::Replaced)
    );
    assert_eq!(lane.outcome(ticket), Ok(Some(Outcome::Replaced)));
    // Stopped by the foreign consumption, not by the repeated evidence.
    assert_eq!(lane.halt_reason(), Some(HaltReason::NonceConsumedByOtherTx));
}

// ---------------------------------------------------------------------------
// Restart (matrix N3, SN08)
// ---------------------------------------------------------------------------

#[test]
fn restart_from_every_phase_is_quarantined_and_cannot_be_released() {
    // Phases of the Gate-0 fence test; "Confirmed" is `Included` here.
    for phase in ["Reserved", "Prepared", "Sending", "Included"] {
        let mut old = armed(2);
        let set = adapters(&[0]);
        let (ticket, hash) = match phase {
            "Reserved" => (old.reserve(token(1)).unwrap(), B256::ZERO),
            "Prepared" => {
                let (ticket, signed) = prepared(&mut old, 1);
                (ticket, signed.hash)
            }
            _ => {
                let (ticket, signed) = dispatched(&mut old, 1, set);
                old.start_write(ticket, adapter(0), signed.hash).unwrap();
                if phase == "Included" {
                    old.apply_inclusion(&included(ticket, signed.hash, block(1)))
                        .unwrap();
                }
                (ticket, signed.hash)
            }
        };
        // The process ends; the in-memory lane is gone.
        drop(old);

        // The runner found the existing fence: quarantine.
        let mut lane = IntentLane::quarantined(CHAIN, signer().address(), 2).unwrap();
        assert_eq!(
            lane.halt_reason(),
            Some(HaltReason::RestartQuarantine),
            "{phase}"
        );
        assert_eq!(lane.reserve(token(1)), Err(LaneError::Halted), "{phase}");
        assert_eq!(lane.reserve(token(2)), Err(LaneError::Halted), "{phase}");
        assert_eq!(
            lane.begin_send(&snapshot(1, POLICY_GENERATION), set, &buy_costs()),
            Err(LaneError::Halted),
            "{phase}"
        );
        // Nothing of the old run is addressable or reusable.
        assert_eq!(lane.state(ticket), Err(LaneError::UnknownTicket), "{phase}");
        assert_eq!(
            lane.start_write(ticket, adapter(0), hash),
            Err(LaneError::UnknownTicket),
            "{phase}"
        );

        // A nonce reconciliation does not release the quarantine, whatever
        // nonce it reports: bytes disclosed before the crash may still
        // execute, and the old dedup records are gone.
        for next_nonce in [FIRST_NONCE, FIRST_NONCE + 1, 900] {
            assert_eq!(
                lane.resume(&reconciled(next_nonce)),
                Err(LaneError::QuarantineNotReleasable),
                "{phase}"
            );
        }
        // Evidence about the old nonce does not release it either.
        let _ = lane.apply_inclusion(&included(ticket, hash, block(1)));
        assert_eq!(
            lane.resume(&reconciled(FIRST_NONCE + 1)),
            Err(LaneError::QuarantineNotReleasable),
            "{phase}"
        );
        // A later stop reason does not replace the quarantine.
        lane.halt(HaltReason::External);
        assert_eq!(
            lane.halt_reason(),
            Some(HaltReason::RestartQuarantine),
            "{phase}"
        );
        assert_eq!(lane.reserve(token(2)), Err(LaneError::Halted), "{phase}");
    }
}

#[test]
fn crash_between_permits_cannot_continue_the_fan_out() {
    let mut old = armed(1);
    let (ticket, signed) = dispatched(&mut old, 1, adapters(&[0, 1]));
    old.start_write(ticket, adapter(0), signed.hash).unwrap();
    old.write_completed(ticket, adapter(0)).unwrap();
    // Crash before adapter B consumed its permit.
    drop(old);

    let mut lane = IntentLane::quarantined(CHAIN, signer().address(), 1).unwrap();
    // The restarted process holds no permit, cannot mint one and gets no
    // bytes: the remaining fan-out of the old intent never happens.
    assert_eq!(
        lane.start_write(ticket, adapter(1), signed.hash),
        Err(LaneError::UnknownTicket)
    );
    assert_eq!(lane.reserve(ticket.token()), Err(LaneError::Halted));
    assert_eq!(
        lane.begin_send(&signed.snapshot, adapters(&[1]), &buy_costs()),
        Err(LaneError::Halted)
    );
    assert_eq!(
        lane.resume(&reconciled(FIRST_NONCE)),
        Err(LaneError::QuarantineNotReleasable)
    );

    // A running lane that is not stopped refuses a reconciliation proof.
    let mut running = armed(1);
    assert_eq!(
        running.resume(&reconciled(FIRST_NONCE)),
        Err(LaneError::InvalidState)
    );
}

#[test]
fn quarantined_lane_offers_no_operation_that_admits_again() {
    // The only constructors of a lane are `armed`, which needs an
    // `ArmedFence` (no production constructor), and `quarantined`. This
    // walks every operation of a quarantined lane: none admits, none
    // clears the stop.
    let sender = signer().address();
    let mut lane = IntentLane::quarantined(CHAIN, sender, 2).unwrap();
    let set = adapters(&[0]);
    let quarantine = Some(HaltReason::RestartQuarantine);

    // A ticket and signed bytes from a healthy lane of the same sender.
    let mut healthy = armed(2);
    let (ticket, signed) = prepared(&mut healthy, 1);
    let mut scratch = prepare_scratch();

    assert_eq!(lane.reserve(token(1)), Err(LaneError::Halted));
    assert_eq!(
        lane.prepare(
            ticket,
            &signed.raw,
            &signed.expected,
            &signed.snapshot,
            &mut scratch
        ),
        Err(LaneError::Halted)
    );
    assert_eq!(lane.invalidate(ticket), Err(LaneError::Halted));
    assert_eq!(lane.withdraw(ticket), Err(LaneError::Halted));
    assert_eq!(
        lane.begin_send(&signed.snapshot, set, &buy_costs()),
        Err(LaneError::Halted)
    );
    assert_eq!(
        lane.start_write(ticket, adapter(0), signed.hash),
        Err(LaneError::UnknownTicket)
    );
    assert_eq!(
        lane.write_completed(ticket, adapter(0)),
        Err(LaneError::UnknownTicket)
    );
    assert_eq!(
        lane.write_failed(ticket, adapter(0)),
        Err(LaneError::UnknownTicket)
    );
    assert_eq!(
        lane.acked(ticket, adapter(0)),
        Err(LaneError::UnknownTicket)
    );
    assert_eq!(
        lane.rejected(ticket, adapter(0)),
        Err(LaneError::UnknownTicket)
    );
    assert_eq!(lane.halt_reason(), quarantine);

    // Evidence and reconciliation do not clear it.
    assert!(
        lane.apply_inclusion(&included(ticket, signed.hash, block(1)))
            .is_err()
    );
    assert!(lane.apply_finality(&finality(ticket, block(1))).is_err());
    assert!(lane.apply_orphan(&orphan(ticket, block(1))).is_err());
    assert_eq!(
        lane.resume(&reconciled(ticket.nonce())),
        Err(LaneError::QuarantineNotReleasable)
    );
    lane.halt(HaltReason::External);
    assert_eq!(lane.halt_reason(), quarantine);
    assert_eq!(lane.reserve(token(2)), Err(LaneError::Halted));
    assert_eq!(lane.stats(), LaneStats::default());
}

#[test]
fn in_process_stop_without_disclosure_resumes_at_the_same_nonce() {
    // A lane stopped while running keeps its entries; with nothing
    // dispatched it resumes exactly where it was.
    let mut lane = armed(2);
    let (ticket, signed) = prepared(&mut lane, 1);
    lane.halt(HaltReason::External);
    assert_eq!(
        lane.resume(&reconciled(FIRST_NONCE + 1)),
        Err(LaneError::NonceMismatch)
    );
    lane.resume(&reconciled(FIRST_NONCE)).unwrap();
    assert_eq!(lane.halt_reason(), None);
    // The session's dedup record survived the stop.
    assert_eq!(lane.reserve(token(1)), Err(LaneError::Duplicate));
    let dispatch = lane
        .begin_send(&signed.snapshot, adapters(&[0]), &buy_costs())
        .unwrap();
    assert_eq!(dispatch.ticket, ticket);
}

// ---------------------------------------------------------------------------
// Permit gate on the signed bytes
// ---------------------------------------------------------------------------

#[test]
fn bytes_are_released_only_by_a_successfully_consumed_permit() {
    let mut lane = armed(2);
    let set = adapters(&[0, 1, 2]);

    // Prepared, not dispatched: no permit exists, no bytes are released.
    let (ticket, signed) = prepared(&mut lane, 1);
    for index in 0..3u8 {
        assert_eq!(
            lane.start_write(ticket, adapter(index), signed.hash),
            Err(LaneError::AdapterNotSelected)
        );
    }
    assert_eq!(lane.state(ticket), Ok(IntentState::Prepared));

    lane.begin_send(&signed.snapshot, set, &buy_costs())
        .unwrap();
    // A permit that has not been consumed has released nothing.
    assert_eq!(
        lane.submission(ticket, adapter(0)),
        Ok(SubmissionState::NotStarted)
    );

    // An adapter outside the selected set gets no bytes.
    assert_eq!(
        lane.start_write(ticket, adapter(3), signed.hash),
        Err(LaneError::AdapterNotSelected)
    );

    // A successful consumption releases exactly the one signed byte string,
    // as the egress capability of this intent and adapter, and moves that
    // adapter, and only that adapter, to Writing.
    let write = lane.start_write(ticket, adapter(0), signed.hash).unwrap();
    assert_eq!(
        (
            write.ticket(),
            write.adapter(),
            write.tx_hash(),
            write.bytes()
        ),
        (ticket, adapter(0), signed.hash, signed.raw.as_slice())
    );
    assert_eq!(
        lane.submission(ticket, adapter(0)),
        Ok(SubmissionState::Writing)
    );
    assert_eq!(
        lane.submission(ticket, adapter(1)),
        Ok(SubmissionState::NotStarted)
    );

    // The permit cannot be consumed a second time, in no later state.
    assert_eq!(
        lane.start_write(ticket, adapter(0), signed.hash),
        Err(LaneError::PermitConsumed)
    );
    lane.write_completed(ticket, adapter(0)).unwrap();
    assert_eq!(
        lane.start_write(ticket, adapter(0), signed.hash),
        Err(LaneError::PermitConsumed)
    );
    lane.acked(ticket, adapter(0)).unwrap();
    assert_eq!(
        lane.start_write(ticket, adapter(0), signed.hash),
        Err(LaneError::PermitConsumed)
    );

    // A second adapter receives the identical bytes.
    assert_eq!(
        lane.start_write(ticket, adapter(1), signed.hash)
            .map(|write| write.bytes()),
        Ok(signed.raw.as_slice())
    );

    // After the lane stopped, the remaining permit is suppressed and
    // releases nothing.
    lane.halt(HaltReason::External);
    assert_eq!(
        lane.submission(ticket, adapter(2)),
        Ok(SubmissionState::Suppressed)
    );
    assert_eq!(
        lane.start_write(ticket, adapter(2), signed.hash),
        Err(LaneError::PermitSuppressed)
    );
}

#[test]
fn wrong_hash_or_resolved_intent_releases_no_bytes() {
    // A wrong hash consumes nothing and releases nothing.
    let mut lane = armed(1);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0]));
    assert_eq!(
        lane.start_write(ticket, adapter(0), block(1)),
        Err(LaneError::WrongHash)
    );
    assert_eq!(
        lane.start_write(ticket, adapter(0), signed.hash),
        Err(LaneError::PermitSuppressed)
    );

    // Once evidence resolved the intent, an unconsumed permit is void.
    let mut lane = armed(1);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0, 1]));
    lane.start_write(ticket, adapter(0), signed.hash).unwrap();
    lane.apply_inclusion(&included(ticket, signed.hash, block(1)))
        .unwrap();
    assert_eq!(
        lane.start_write(ticket, adapter(1), signed.hash),
        Err(LaneError::PermitSuppressed)
    );
}

// ---------------------------------------------------------------------------
// Allocation contract of `prepare`
// ---------------------------------------------------------------------------

#[test]
fn preallocated_scratch_never_grows_across_preparations() {
    let mut lane = armed(8);
    let mut scratch = prepare_scratch();
    let capacity = scratch.capacity();
    let pointer = scratch.as_ptr();
    assert!(capacity >= PREPARE_SCRATCH_CAPACITY);

    for id in 0..8 {
        let ticket = lane.reserve(token(id)).unwrap();
        let signed = signed_by(&signer(), id, ticket.nonce(), POLICY_GENERATION);
        lane.prepare(
            ticket,
            &signed.raw,
            &signed.expected,
            &signed.snapshot,
            &mut scratch,
        )
        .unwrap();
        assert_eq!(scratch.capacity(), capacity, "id {id}");
        assert_eq!(scratch.as_ptr(), pointer, "id {id}");
        // The working memory used is within the declared bound.
        assert!(scratch.len() <= PREPARE_SCRATCH_CAPACITY, "id {id}");
    }

    // A refused preparation does not grow it either.
    let again = lane.reserve(token(0));
    assert_eq!(again, Err(LaneError::Duplicate));
    let stranger = Signer::from_secret_bytes(&[0x52u8; 32]).unwrap();
    let mut other = armed(1);
    let ticket = other.reserve(token(9)).unwrap();
    let foreign = signed_by(&stranger, 9, ticket.nonce(), POLICY_GENERATION);
    let ours = signed_by(&signer(), 9, ticket.nonce(), POLICY_GENERATION);
    assert_eq!(
        other.prepare(
            ticket,
            &foreign.raw,
            &ours.expected,
            &ours.snapshot,
            &mut scratch
        ),
        Err(LaneError::Binding(BindingError::WrongSender))
    );
    assert_eq!(scratch.capacity(), capacity);
    assert_eq!(scratch.as_ptr(), pointer);
}

#[test]
fn too_small_scratch_is_refused_and_the_intent_stays_reserved() {
    let mut lane = armed(1);
    let ticket = lane.reserve(token(1)).unwrap();
    let signed = signed_by(&signer(), 1, ticket.nonce(), POLICY_GENERATION);

    let mut empty = Vec::new();
    assert_eq!(
        lane.prepare(
            ticket,
            &signed.raw,
            &signed.expected,
            &signed.snapshot,
            &mut empty
        ),
        Err(LaneError::ScratchTooSmall)
    );
    // Not grown, nothing bound.
    assert_eq!(empty.capacity(), 0);
    assert_eq!(lane.state(ticket), Ok(IntentState::Reserved));

    let mut short: Vec<u8> = Vec::with_capacity(PREPARE_SCRATCH_CAPACITY - 1);
    if short.capacity() < PREPARE_SCRATCH_CAPACITY {
        let before = short.capacity();
        assert_eq!(
            lane.prepare(
                ticket,
                &signed.raw,
                &signed.expected,
                &signed.snapshot,
                &mut short
            ),
            Err(LaneError::ScratchTooSmall)
        );
        assert_eq!(short.capacity(), before);
        assert_eq!(lane.state(ticket), Ok(IntentState::Reserved));
    }

    // With the declared capacity the same intent is prepared.
    let mut scratch = prepare_scratch();
    lane.prepare(
        ticket,
        &signed.raw,
        &signed.expected,
        &signed.snapshot,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(lane.state(ticket), Ok(IntentState::Prepared));
}

// ---------------------------------------------------------------------------
// Cumulative session exposure (E5-1)
// ---------------------------------------------------------------------------

/// A lane whose session cap admits exactly `intents` of the tests' BUYs.
fn capped(capacity: usize, intents: u64) -> IntentLane {
    let cap = buy_costs().total_wei * U256::from(intents);
    let fence = ArmedFence::for_tests_with_session_cap(CHAIN, signer().address(), FIRST_NONCE, cap);
    IntentLane::armed(fence, capacity).unwrap()
}

#[test]
fn the_session_cap_admits_exactly_its_intents_and_refuses_the_next() {
    for allowed in [1u64, 2, 3] {
        let mut lane = capped(5, allowed);
        // Fan-out of one intent to two adapters is one exposure.
        let set = adapters(&[0, 1]);
        for id in 1..=allowed {
            let (ticket, signed) = dispatched(&mut lane, id, set);
            for index in 0..2u8 {
                lane.start_write(ticket, adapter(index), signed.hash)
                    .unwrap();
                lane.write_completed(ticket, adapter(index)).unwrap();
            }
            // Inclusion neither credits the reservation back nor counts it
            // again.
            lane.apply_inclusion(&included(ticket, signed.hash, block(id)))
                .unwrap();
        }
        let reserved = buy_costs().total_wei * U256::from(allowed);
        assert_eq!(lane.session_exposure().reserved_wei, reserved);
        let (ticket, signed) = prepared(&mut lane, allowed + 1);
        assert_eq!(
            lane.begin_send(&signed.snapshot, set, &buy_costs()),
            Err(LaneError::SessionBudgetExceeded)
        );
        // Refused without any state change; nothing can be written.
        assert_eq!(lane.state(ticket), Ok(IntentState::Prepared));
        assert_eq!(
            lane.start_write(ticket, adapter(0), signed.hash),
            Err(LaneError::AdapterNotSelected)
        );
        assert_eq!(lane.session_exposure().reserved_wei, reserved);
        assert_eq!(lane.stats().session_budget_refusals, 1);
    }
}

#[test]
fn an_unknown_submission_keeps_its_reservation() {
    let mut lane = capped(2, 1);
    let (ticket, signed) = dispatched(&mut lane, 1, adapters(&[0]));
    lane.start_write(ticket, adapter(0), signed.hash).unwrap();
    lane.write_failed(ticket, adapter(0)).unwrap();
    assert_eq!(lane.state(ticket), Ok(IntentState::Halted));
    assert_eq!(lane.session_exposure().reserved_wei, buy_costs().total_wei);
}

#[test]
fn plan_costs_must_match_the_prepared_intent_and_cannot_overflow() {
    let base = buy_costs();
    let one = U256::from(1u64);
    let mut lane = armed(3);
    let (ticket, signed) = prepared(&mut lane, 1);
    let mut wrong_input = base;
    wrong_input.buy_input_wei += one;
    wrong_input.total_wei += one;
    let mut wrong_gas = base;
    wrong_gas.buy_gas_wei -= one;
    wrong_gas.total_wei -= one;
    // A total below the sum of its components hides a cost.
    let mut hidden = base;
    hidden.provider_payment_wei = one;
    let mut overflow = base;
    overflow.own_launch_wei = U256::MAX;
    overflow.total_wei = U256::MAX;
    for costs in [wrong_input, wrong_gas, hidden, overflow] {
        assert_eq!(
            lane.begin_send(&signed.snapshot, adapters(&[0]), &costs),
            Err(LaneError::CostMismatch)
        );
    }
    assert_eq!(lane.state(ticket), Ok(IntentState::Prepared));
    assert_eq!(lane.session_exposure().reserved_wei, U256::ZERO);

    // An own launch is part of the reserved total; the next intent's
    // reservation overflows the cap's arithmetic and is refused.
    let mut launch = base;
    launch.own_launch_wei = U256::MAX - base.total_wei;
    launch.total_wei = U256::MAX;
    lane.begin_send(&signed.snapshot, adapters(&[0]), &launch)
        .unwrap();
    assert_eq!(lane.session_exposure().reserved_wei, U256::MAX);
    lane.start_write(ticket, adapter(0), signed.hash).unwrap();
    lane.write_completed(ticket, adapter(0)).unwrap();
    lane.apply_inclusion(&included(ticket, signed.hash, block(1)))
        .unwrap();
    let (_, next) = prepared(&mut lane, 2);
    assert_eq!(
        lane.begin_send(&next.snapshot, adapters(&[0]), &base),
        Err(LaneError::SessionBudgetExceeded)
    );
}
