//! Regressions of the capability records and the submission plan.

use alloy_primitives::{Address, B256, U256, keccak256};

use super::capability::testkit::{qualified, unqualified};
use super::capability::*;
use super::plan::*;
use super::release::*;
use crate::chain_state::SnapshotId;
use crate::economics::flap::native_curve20::{
    QUALIFIED_MAINNET_INPUT_WEI, QualifiedNativeState, WITNESSED_MAINNET_TERMS, admit_native_buy,
};
use crate::economics::policy::BuyPolicy;
use crate::intent::{
    AdapterId, AdapterSet, ArmedFence, Dispatch, IntentLane, PREPARE_SCRATCH_CAPACITY,
    SubmissionState, Ticket,
};
use crate::signer::Signer;
use crate::transactions::legacy::LegacyTx;
use crate::transactions::legacy::testdata::unhex;
use crate::venues::flap::abi::encode_new_token_v6;
use crate::venues::flap::binding::ExpectedBuy;
use crate::venues::flap::deployment::testkit::validated_mainnet;
use crate::venues::flap::profile::classify_launch;
use crate::venues::flap::profile::testkit::witnessed_native_launch;

const CHAIN: u64 = 56;
const FIRST_NONCE: u64 = 7;
const CODE_GENERATION: u64 = 7;
const POLICY_GENERATION: u64 = 3;
const CAPABILITY_GENERATION: u64 = 11;
const GAS_PRICE: u64 = 310_000_000;
const GAS_LIMIT: u64 = 1_500_000;
const LAUNCH_GAS_LIMIT: u64 = 6_603_270;
/// Last block the providers are asked to include in.
const MAX_BLOCK: u64 = 1_000;
/// Last head at which a write may start: one block earlier.
const LAST_START_HEAD: u64 = 999;

/// The buyer: signer of the BUY and recipient of the tokens.
fn buyer() -> Signer {
    Signer::from_secret_bytes(&[0x51u8; 32]).unwrap()
}

/// The launcher of a controlled self-launch: another wallet.
fn launcher() -> Signer {
    Signer::from_secret_bytes(&[0x61u8; 32]).unwrap()
}

fn portal() -> Address {
    validated_mainnet(CODE_GENERATION).deployment().portal
}

fn adapter(index: u8) -> AdapterId {
    AdapterId::new(index).unwrap()
}

fn set(indices: &[u8]) -> AdapterSet {
    let ids: Vec<AdapterId> = indices.iter().map(|i| adapter(*i)).collect();
    AdapterSet::new(&ids).unwrap()
}

/// The snapshot whose pending candidate is the launch `candidate`.
fn snapshot(candidate: B256) -> SnapshotId {
    SnapshotId {
        canonical_block_hash: B256::from_slice(&[0xb1u8; 32]),
        ordered_state_cursor: 41,
        pending_candidate: candidate,
        policy_generation: POLICY_GENERATION,
        codehash_generation: CODE_GENERATION,
    }
}

fn head(number: u64) -> ChainHead {
    ChainHead {
        number,
        timestamp: 1_800_000_000,
    }
}

/// The admitted BUY behind the launch transaction `candidate`, built
/// through the production path: expectation and calldata.
fn admitted_buy(candidate: B256, nonce: u64, gas_price: u64) -> (ExpectedBuy, [u8; 228]) {
    let deployment = validated_mainnet(CODE_GENERATION);
    let classified = classify_launch(&deployment, candidate, &witnessed_native_launch()).unwrap();
    let terms = WITNESSED_MAINNET_TERMS;
    let current = snapshot(candidate);
    let state = QualifiedNativeState::fresh_launch(&classified, &terms, 0, current).unwrap();
    let policy = BuyPolicy::new(
        POLICY_GENERATION,
        U256::from(QUALIFIED_MAINNET_INPUT_WEI),
        100,
        gas_price,
        GAS_LIMIT,
    )
    .unwrap();
    let admitted = admit_native_buy(&state, &policy).unwrap();
    let sender = buyer().address();
    let expected =
        ExpectedBuy::from_admitted(&deployment, &admitted, &policy, sender, nonce).unwrap();
    (expected, admitted.calldata().unwrap())
}

/// A reserved intent behind `candidate` and its expectation.
fn intent(candidate: B256, gas_price: u64) -> (Ticket, ExpectedBuy) {
    let fence = ArmedFence::for_tests(CHAIN, buyer().address(), FIRST_NONCE);
    let mut lane = IntentLane::armed(fence, 1).unwrap();
    let token = admitted_buy(candidate, FIRST_NONCE, gas_price).0.token();
    let ticket = lane.reserve(token).unwrap();
    (ticket, admitted_buy(candidate, ticket.nonce(), gas_price).0)
}

fn bind(index: u8, capability: CapabilityId) -> AdapterBinding {
    AdapterBinding {
        adapter: adapter(index),
        capability,
    }
}

/// 48Club at index 0, BlockRazor builder at 1, local geth at 2.
fn table() -> AdapterTable {
    AdapterTable::new(
        CHAIN,
        CAPABILITY_GENERATION,
        &[
            bind(0, CapabilityId::Club48BuilderV1),
            bind(1, CapabilityId::BlockRazorBuilderV1),
            bind(2, CapabilityId::LocalGethV1),
        ],
    )
    .unwrap()
}

fn buy_gas() -> U256 {
    U256::from(GAS_PRICE) * U256::from(GAS_LIMIT)
}

fn own_launch_gas() -> U256 {
    U256::from(GAS_PRICE) * U256::from(LAUNCH_GAS_LIMIT)
}

/// A budget that exactly fits the BUY, the own launch and nothing else.
fn budget() -> BudgetEnvelope {
    let input = U256::from(QUALIFIED_MAINNET_INPUT_WEI);
    BudgetEnvelope {
        max_buy_input_wei: input,
        max_buy_gas_wei: buy_gas(),
        max_own_launch_wei: own_launch_gas(),
        max_provider_payment_wei: U256::ZERO,
        max_total_wei: input + buy_gas() + own_launch_gas(),
    }
}

/// Stand-in for the signed BUY bytes where only a reference is needed.
fn buy_ref() -> TxRef {
    TxRef::of(&[0xb0u8; 343])
}

/// Stand-in for the original signed launch of a third party.
fn foreign_launch_raw() -> Vec<u8> {
    let mut raw = vec![0x02u8];
    raw.extend_from_slice(&[0x1au8; 1_123]);
    raw
}

fn foreign_launch() -> TxRef {
    TxRef::of(&foreign_launch_raw())
}

/// The signed bytes of an own launch.
fn own_launch_raw_with(chain_id: u64, to: Address, value_wei: u64) -> Vec<u8> {
    let mut calldata = Vec::new();
    encode_new_token_v6(&witnessed_native_launch(), &mut calldata);
    let tx = LegacyTx {
        chain_id,
        nonce: 3,
        gas_price: GAS_PRICE,
        gas_limit: LAUNCH_GAS_LIMIT,
        to,
        value: U256::from(value_wei),
        input: &calldata,
    };
    let mut scratch = Vec::new();
    let mut raw = Vec::new();
    launcher().sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
    raw
}

fn own_launch_raw(value_wei: u64) -> Vec<u8> {
    own_launch_raw_with(CHAIN, portal(), value_wei)
}

fn own_launch(raw: &[u8]) -> OwnLaunch {
    OwnLaunch::from_signed_legacy(raw, &mut Vec::new()).unwrap()
}

fn validity() -> ProviderValidity {
    ProviderValidity {
        max_block_number: MAX_BLOCK,
        min_timestamp: None,
        max_timestamp: None,
    }
}

/// A third party's launch followed by the BUY, through both builders.
fn public_inputs(ticket: Ticket) -> PlanInputs {
    PlanInputs {
        ticket,
        buy: buy_ref(),
        launch: Launch::PublicOriginal(foreign_launch()),
        mode: SubmissionMode::PublicFullLaunchBundle,
        profile: ReleaseProfile::BirthEntryV1,
        adapters: set(&[0, 1]),
        local_block_deadline: LocalBlockStartDeadline {
            last_head_block: LAST_START_HEAD,
        },
        provider_validity: Some(validity()),
        merge: MergePolicy::NoMerge,
        pre_submission: PreSubmission::No,
        provider_payment_wei: U256::ZERO,
    }
}

/// The intent behind the third party's launch.
fn public_intent() -> (Ticket, ExpectedBuy) {
    intent(foreign_launch().hash(), GAS_PRICE)
}

fn plan_with(change: impl FnOnce(&mut PlanInputs)) -> Result<SubmissionPlan, PlanError> {
    let (ticket, expected) = public_intent();
    let mut inputs = public_inputs(ticket);
    change(&mut inputs);
    SubmissionPlan::new(&table(), &expected, &budget(), inputs)
}

/// The intent behind the own launch `raw` and the inputs of its bundle.
fn self_launch(raw: &[u8]) -> (ExpectedBuy, PlanInputs) {
    let own = own_launch(raw);
    let (ticket, expected) = intent(own.tx().hash(), GAS_PRICE);
    let mut inputs = public_inputs(ticket);
    inputs.mode = SubmissionMode::ControlledSelfLaunchBundle;
    inputs.launch = Launch::ControlledOwn(own);
    (expected, inputs)
}

fn all_capabilities() -> [CapabilityId; 4] {
    [
        CapabilityId::LocalGethV1,
        CapabilityId::Club48BuilderV1,
        CapabilityId::BlockRazorBuilderV1,
        CapabilityId::BlockRazorRpcV1,
    ]
}

// ---------------------------------------------------------------------------
// Capability records
// ---------------------------------------------------------------------------

#[test]
fn club48_record_matches_the_frozen_scope() {
    let record = CapabilityId::Club48BuilderV1.record();
    assert_eq!(record.id(), CapabilityId::Club48BuilderV1);
    assert_eq!(record.kind(), AdapterKind::Club48Builder);
    assert_eq!(record.version(), 1);
    assert_eq!(record.chain_id(), 56);
    let bundle_modes = [
        SubmissionMode::PublicFullLaunchBundle,
        SubmissionMode::ControlledSelfLaunchBundle,
    ];
    for mode in bundle_modes {
        assert_eq!(record.mode(mode), Support::Enabled);
    }
    assert_eq!(record.max_block_number().support, Support::Enabled);
    assert_eq!(record.max_timestamp().support, Support::Enabled);
    assert_eq!(record.allowed_reverts(), Support::Enabled);
    assert_eq!(record.no_merge().support, Support::Enabled);
    // Documented concretely and required by the birth-entry profile.
    assert_eq!(record.position_first().support, Support::Enabled);
    // Documented, but not verified for this product.
    assert_eq!(
        record.mode(SubmissionMode::PrivateTarget),
        Support::Unqualified
    );
    assert_eq!(
        record.mode(SubmissionMode::SingleRawBuy),
        Support::Unqualified
    );
    // Not in the official parameter list.
    assert_eq!(record.min_timestamp().support, Support::Unqualified);
    assert_eq!(record.max_txs(), 2);
}

#[test]
fn a_documented_gas_price_rule_is_never_recorded_as_no_rule() {
    // 48Club: the FAQ says the average gas price of a bundle is checked;
    // no explicit numeric minimum is stated. BlockRazor: 0.05 gwei in two
    // different forms. Both are documented rules whose exact form is not
    // established; neither is "no rule".
    for id in [
        CapabilityId::Club48BuilderV1,
        CapabilityId::BlockRazorBuilderV1,
        CapabilityId::BlockRazorRpcV1,
    ] {
        assert_eq!(id.record().fee_floor(), FeeFloor::DocumentedRuleUnqualified);
    }
    // The own node has no provider reference.
    assert_eq!(
        CapabilityId::LocalGethV1.record().fee_floor(),
        FeeFloor::NotRecorded
    );
}

#[test]
fn blockrazor_builder_record_matches_the_frozen_scope() {
    let record = CapabilityId::BlockRazorBuilderV1.record();
    assert_eq!(record.kind(), AdapterKind::BlockRazorBuilder);
    assert_eq!(record.chain_id(), 56);
    assert_eq!(
        record.mode(SubmissionMode::PublicFullLaunchBundle),
        Support::Enabled
    );
    assert_eq!(
        record.mode(SubmissionMode::ControlledSelfLaunchBundle),
        Support::Enabled
    );
    assert_eq!(record.max_block_number().support, Support::Enabled);
    assert_eq!(record.min_timestamp().support, Support::Enabled);
    assert_eq!(record.max_timestamp().support, Support::Enabled);
    assert_eq!(record.allowed_reverts(), Support::Enabled);
    assert_eq!(record.no_merge().support, Support::Enabled);
    assert_eq!(record.position_first().support, Support::Unqualified);
    assert_eq!(
        record.mode(SubmissionMode::PrivateTarget),
        Support::Unqualified
    );
    assert_eq!(
        record.mode(SubmissionMode::SingleRawBuy),
        Support::Unqualified
    );
    // The provider states its floor in two different forms; no rule is
    // adopted.
    assert_eq!(record.fee_floor(), FeeFloor::DocumentedRuleUnqualified);
}

#[test]
fn bundle_form_and_pre_submission_are_separate_capabilities() {
    // Both builders accept the form of a self-launch bundle. Neither is
    // qualified for receiving it before the launch was broadcast.
    for id in [
        CapabilityId::Club48BuilderV1,
        CapabilityId::BlockRazorBuilderV1,
    ] {
        let record = id.record();
        assert_eq!(
            record.mode(SubmissionMode::ControlledSelfLaunchBundle),
            Support::Enabled
        );
        assert_eq!(
            record.pre_submission_before_launch_execution().support,
            Support::Unqualified
        );
    }
    for id in all_capabilities() {
        assert_ne!(
            id.record().pre_submission_before_launch_execution().support,
            Support::Enabled
        );
    }
}

#[test]
fn lifecycle_is_recorded_explicitly_and_not_guessed() {
    // BlockRazor direct: order, atomicity and same-block inclusion are
    // documented by the builder overview. What the builder does with the
    // transactions afterwards is not.
    let blockrazor = CapabilityId::BlockRazorBuilderV1.record();
    assert_eq!(blockrazor.preserves_order().knowledge, Knowledge::Yes);
    assert_eq!(blockrazor.atomic().knowledge, Knowledge::Yes);
    assert_eq!(blockrazor.same_block().knowledge, Knowledge::Yes);
    assert_eq!(
        blockrazor.extracts_transactions().knowledge,
        Knowledge::Unknown
    );
    assert_eq!(
        blockrazor.provider_rebroadcast().knowledge,
        Knowledge::Unknown
    );
    assert_eq!(
        blockrazor.provider_public_fallback().knowledge,
        Knowledge::Unknown
    );
    assert_eq!(
        blockrazor.reaches_other_builders().knowledge,
        Knowledge::Unknown
    );
    assert_eq!(
        blockrazor.public_mempool_exposure().knowledge,
        Knowledge::Unknown
    );

    // 48Club: the frozen pages state none of these as a guarantee.
    let club48 = CapabilityId::Club48BuilderV1.record();
    assert_eq!(club48.preserves_order().knowledge, Knowledge::Unknown);
    assert_eq!(club48.atomic().knowledge, Knowledge::Unknown);
    assert_eq!(club48.same_block().knowledge, Knowledge::Unknown);
    assert_eq!(club48.extracts_transactions().knowledge, Knowledge::Unknown);
    assert_eq!(
        club48.reaches_other_builders().knowledge,
        Knowledge::Unknown
    );

    // Documented: the RPC forwards to several builders.
    let rpc = CapabilityId::BlockRazorRpcV1.record();
    assert_eq!(rpc.reaches_other_builders().knowledge, Knowledge::Yes);
    assert_eq!(rpc.preserves_order().knowledge, Knowledge::Unknown);
    assert_eq!(rpc.atomic().knowledge, Knowledge::Unknown);

    // By construction: the local node broadcasts.
    let local = CapabilityId::LocalGethV1.record();
    assert_eq!(local.provider_rebroadcast().knowledge, Knowledge::Yes);
    assert_eq!(local.public_mempool_exposure().knowledge, Knowledge::Yes);
    assert_eq!(local.reaches_other_builders().knowledge, Knowledge::Yes);

    // No record claims that the bytes stay with one recipient.
    for id in all_capabilities() {
        let record = id.record();
        assert_ne!(record.extracts_transactions().knowledge, Knowledge::No);
        assert_ne!(record.provider_rebroadcast().knowledge, Knowledge::No);
        assert_ne!(record.provider_public_fallback().knowledge, Knowledge::No);
        assert_ne!(record.reaches_other_builders().knowledge, Knowledge::No);
        assert_ne!(record.public_mempool_exposure().knowledge, Knowledge::No);
    }
}

#[test]
fn documented_is_not_qualified_for_any_frozen_record() {
    // What a provider states and what this product qualified are separate.
    // Nothing has been qualified yet, for no property of any record.
    for id in all_capabilities() {
        let record = id.record();
        for property in [
            record.preserves_order(),
            record.atomic(),
            record.same_block(),
            record.extracts_transactions(),
            record.provider_rebroadcast(),
            record.provider_public_fallback(),
            record.reaches_other_builders(),
            record.public_mempool_exposure(),
            record.reorg_post_inclusion_public_exposure(),
        ] {
            assert_eq!(property.qualification, Qualification::Unqualified, "{id:?}");
            assert!(!property.established(Knowledge::Yes), "{id:?}");
            assert!(!property.established(Knowledge::No), "{id:?}");
        }
    }
    // BlockRazor's documented "yes" is exactly that: documented.
    let atomic = CapabilityId::BlockRazorBuilderV1.record().atomic();
    assert_eq!(atomic, unqualified(Knowledge::Yes));
    assert!(!atomic.established(Knowledge::Yes));

    // Only a statement that is also qualified is established.
    assert!(qualified(Knowledge::Yes).established(Knowledge::Yes));
    assert!(qualified(Knowledge::No).established(Knowledge::No));
    assert!(!qualified(Knowledge::Unknown).established(Knowledge::Yes));
    assert!(!qualified(Knowledge::Unknown).established(Knowledge::No));
    assert!(!qualified(Knowledge::No).established(Knowledge::Yes));
    assert!(!unqualified(Knowledge::Yes).established(Knowledge::Yes));
}

#[test]
fn reorg_exposure_is_not_a_provider_rebroadcast_or_public_fallback() {
    // 48Club documents that a reorg makes the transactions of a discarded
    // block public. It documents nothing about rebroadcasting them itself
    // or about a public fallback.
    let club48 = CapabilityId::Club48BuilderV1.record();
    assert_eq!(
        club48.reorg_post_inclusion_public_exposure().knowledge,
        Knowledge::Yes
    );
    assert_eq!(club48.provider_rebroadcast().knowledge, Knowledge::Unknown);
    assert_eq!(
        club48.provider_public_fallback().knowledge,
        Knowledge::Unknown
    );
    assert_eq!(
        club48.public_mempool_exposure().knowledge,
        Knowledge::Unknown
    );
    assert_ne!(
        club48.reorg_post_inclusion_public_exposure(),
        club48.provider_rebroadcast()
    );
    assert_ne!(
        club48.reorg_post_inclusion_public_exposure(),
        club48.provider_public_fallback()
    );

    // Where nothing is stated it stays unknown; it is not derived from
    // general knowledge about chains.
    let blockrazor = CapabilityId::BlockRazorBuilderV1.record();
    assert_eq!(
        blockrazor.reorg_post_inclusion_public_exposure().knowledge,
        Knowledge::Unknown
    );
}

#[test]
fn blockrazor_rpc_is_a_different_capability_and_enables_nothing() {
    let rpc = CapabilityId::BlockRazorRpcV1.record();
    let builder = CapabilityId::BlockRazorBuilderV1.record();
    assert_ne!(rpc.id(), builder.id());
    assert_ne!(rpc.kind(), builder.kind());
    assert_ne!(rpc, builder);
    for mode in [
        SubmissionMode::SingleRawBuy,
        SubmissionMode::PublicFullLaunchBundle,
        SubmissionMode::ControlledSelfLaunchBundle,
        SubmissionMode::PrivateTarget,
    ] {
        assert_ne!(rpc.mode(mode), Support::Enabled);
    }
    assert_eq!(rpc.handoff_boundary(), HandoffBoundary::NotDefined);
}

#[test]
fn local_geth_record_is_single_buy_with_deferred_boundary() {
    let record = CapabilityId::LocalGethV1.record();
    assert_eq!(record.mode(SubmissionMode::SingleRawBuy), Support::Enabled);
    for mode in [
        SubmissionMode::PublicFullLaunchBundle,
        SubmissionMode::ControlledSelfLaunchBundle,
        SubmissionMode::PrivateTarget,
    ] {
        assert_eq!(record.mode(mode), Support::Unsupported);
    }
    // No JSON-RPC call is declared the production boundary.
    assert_eq!(
        record.handoff_boundary(),
        HandoffBoundary::DeferredToNativeGeth
    );
}

#[test]
fn every_record_keeps_the_three_explicit_facts() {
    for id in all_capabilities() {
        let record = id.record();
        assert_eq!(record.id(), id);
        assert_eq!(record.chain_id(), 56);
        // No cancellation, no revocation by expiry, no qualified send time.
        assert_eq!(record.cancellation(), Support::Unsupported);
        assert!(!record.expiry_revokes_disclosed_bytes());
        assert!(!record.qualified_as_t_send());
        // Disabled for every adapter.
        assert_ne!(record.mode(SubmissionMode::PrivateTarget), Support::Enabled);
    }
    // A TLS adapter's boundary is the userspace write, nothing more.
    for id in [
        CapabilityId::Club48BuilderV1,
        CapabilityId::BlockRazorBuilderV1,
    ] {
        assert_eq!(
            id.record().handoff_boundary(),
            HandoffBoundary::TlsUserspaceWriteFlushReturned
        );
    }
}

#[test]
fn provider_evidence_is_frozen_and_pinned() {
    // The snapshots are part of the tree; these digests pin their bytes.
    let pinned: [(CapabilityId, &[u8], &str, &str); 3] = [
        (
            CapabilityId::Club48BuilderV1,
            include_bytes!("../../fixtures/inclusion/club48_builder_send_bundle.capability.json"),
            "629888e2e0524a9724f193502005506755c8412bc8091313d28e0f1357d3b54c",
            "299ca3ae02948219c5baf7f5df609b1145d14be54b9d5a3de18c00b899b6b10f",
        ),
        (
            CapabilityId::BlockRazorBuilderV1,
            include_bytes!(
                "../../fixtures/inclusion/blockrazor_builder_send_bundle.capability.json"
            ),
            "c92f87196feea6e60861fbaf10a13fb7fafdaa847d17ea5bc57b9b685cba7248",
            "266130637d0693f6e3d2bc347ced17055fb1e54a4066125ba91c0e7ea579b259",
        ),
        (
            CapabilityId::BlockRazorRpcV1,
            include_bytes!("../../fixtures/inclusion/blockrazor_rpc_send_bundle.capability.json"),
            "64d8b99cd62ea68d95a57a8a86a9591ac41752d4c9e60926151c3dd292e3ae9f",
            "aad4e5b3f4e399fa5b38e22245697265fcad6e60e41b817b499c63c7fdc72223",
        ),
    ];
    for (id, bytes, sha256, keccak) in pinned {
        let evidence = id.record().evidence();
        assert_eq!(evidence.len(), 1);
        // The record names the snapshot and its SHA-256.
        assert_eq!(evidence[0].fixture_sha256, sha256);
        assert_eq!(evidence[0].retrieved_on, "2026-10-04");
        assert!(evidence[0].fixture.ends_with(".capability.json"));
        // The file in the tree is the one that was frozen. The crate has
        // no SHA-256; Keccak-256 pins the same bytes.
        assert_eq!(keccak256(bytes).as_slice(), unhex(keccak).as_slice());
    }
    // The local node has no provider reference.
    assert!(CapabilityId::LocalGethV1.record().evidence().is_empty());
}

#[test]
fn adapter_table_binds_every_index_and_kind_once() {
    let table = table();
    assert_eq!(table.chain_id(), CHAIN);
    assert_eq!(table.generation(), CAPABILITY_GENERATION);
    assert_eq!(
        table.record(adapter(0)),
        Some(CapabilityId::Club48BuilderV1.record())
    );
    assert_eq!(
        table.record(adapter(1)),
        Some(CapabilityId::BlockRazorBuilderV1.record())
    );
    assert_eq!(
        table.record(adapter(2)),
        Some(CapabilityId::LocalGethV1.record())
    );
    assert_eq!(table.record(adapter(3)), None);

    let twice_index = [
        bind(0, CapabilityId::Club48BuilderV1),
        bind(0, CapabilityId::BlockRazorBuilderV1),
    ];
    assert_eq!(
        AdapterTable::new(CHAIN, 1, &twice_index),
        Err(CapabilityError::DuplicateAdapter)
    );

    let twice_kind = [
        bind(0, CapabilityId::BlockRazorBuilderV1),
        bind(1, CapabilityId::BlockRazorBuilderV1),
    ];
    assert_eq!(
        AdapterTable::new(CHAIN, 1, &twice_kind),
        Err(CapabilityError::DuplicateKind)
    );

    // The direct builder and the RPC are different kinds: both can be
    // bound, to different indices, and stay different capabilities.
    let both = [
        bind(0, CapabilityId::BlockRazorBuilderV1),
        bind(1, CapabilityId::BlockRazorRpcV1),
    ];
    let table = AdapterTable::new(CHAIN, 1, &both).unwrap();
    assert_ne!(table.record(adapter(0)), table.record(adapter(1)));
}

#[test]
fn mainnet_records_are_refused_for_another_chain() {
    // No frozen record is valid for chain 97, whatever ticket and
    // expectation would say.
    for id in all_capabilities() {
        assert_eq!(
            AdapterTable::new(97, 1, &[bind(0, id)]),
            Err(CapabilityError::WrongChain),
            "{id:?}"
        );
    }
    // The rule is the record's chain scope, not the number 56.
    let testnet_record = CapabilityId::Club48BuilderV1.record().with_chain_id(97);
    assert!(AdapterTable::for_tests(97, 1, &[(adapter(0), testnet_record)]).is_ok());
    assert_eq!(
        AdapterTable::for_tests(CHAIN, 1, &[(adapter(0), testnet_record)]),
        Err(CapabilityError::WrongChain)
    );
}

// ---------------------------------------------------------------------------
// Transaction references and the own launch: facts from bytes
// ---------------------------------------------------------------------------

#[test]
fn transaction_references_are_made_from_and_checked_against_bytes() {
    let raw = foreign_launch_raw();
    let reference = TxRef::of(&raw);
    assert_eq!(reference.hash(), keccak256(&raw));
    assert_eq!(reference.len(), raw.len());
    assert!(!reference.is_empty());
    assert!(reference.matches(&raw));

    // One changed byte, one byte more or one byte less is another
    // transaction.
    let mut changed = raw.clone();
    changed[10] ^= 0x01;
    assert!(!reference.matches(&changed));
    let mut longer = raw.clone();
    longer.push(0);
    assert!(!reference.matches(&longer));
    assert!(!reference.matches(&raw[..raw.len() - 1]));

    assert!(TxRef::of(&[]).is_empty());
}

#[test]
fn own_launch_facts_are_read_from_its_signed_bytes() {
    let raw = own_launch_raw(0);
    let own = own_launch(&raw);
    assert_eq!(own.tx(), TxRef::of(&raw));
    assert!(own.tx().matches(&raw));
    assert_eq!(own.chain_id(), CHAIN);
    assert_eq!(own.to(), portal());
    // The launcher is recovered from the signature. It is its own wallet,
    // not the buyer.
    assert_eq!(own.launcher(), launcher().address());
    assert_ne!(own.launcher(), buyer().address());
    // Gas limit times gas price, plus value.
    assert_eq!(own.cost_wei(), own_launch_gas());
    assert_eq!(
        own_launch(&own_launch_raw(5)).cost_wei(),
        own_launch_gas() + U256::from(5u64)
    );

    // Bytes that are not a signed legacy transaction are no own launch.
    let mut scratch = Vec::new();
    assert_eq!(
        OwnLaunch::from_signed_legacy(&raw[..raw.len() - 1], &mut scratch).err(),
        Some(PlanError::InvalidOwnLaunch)
    );
    assert_eq!(
        OwnLaunch::from_signed_legacy(&foreign_launch_raw(), &mut scratch).err(),
        Some(PlanError::InvalidOwnLaunch)
    );
}

// ---------------------------------------------------------------------------
// Submission plan: the three cases
// ---------------------------------------------------------------------------

#[test]
fn public_full_launch_fans_one_intent_out_and_charges_no_launch_cost() {
    let (ticket, expected) = public_intent();
    let plan = SubmissionPlan::new(&table(), &expected, &budget(), public_inputs(ticket)).unwrap();

    // One economic intent: one ticket, one nonce, one BUY hash.
    assert_eq!(plan.ticket(), ticket);
    assert_eq!(plan.ticket().nonce(), FIRST_NONCE);
    assert_eq!(plan.buy(), buy_ref());
    // The third party's launch first, referenced by hash and unchanged:
    // the runner can check the bytes it sends against the plan.
    assert_eq!(plan.launch(), Launch::PublicOriginal(foreign_launch()));
    assert!(plan.launch().tx().unwrap().matches(&foreign_launch_raw()));
    assert_eq!(plan.mode(), SubmissionMode::PublicFullLaunchBundle);
    assert_eq!(plan.adapters().count(), 2);
    assert!(plan.adapters().contains(adapter(0)));
    assert!(plan.adapters().contains(adapter(1)));
    assert_eq!(plan.merge(), MergePolicy::NoMerge);
    assert_eq!(plan.pre_submission(), PreSubmission::No);
    assert_eq!(plan.reverts(), RevertPolicy::NoneMayRevert);
    assert_eq!(plan.snapshot(), snapshot(foreign_launch().hash()));
    assert_eq!(plan.capability_generation(), CAPABILITY_GENERATION);

    // A foreign launch is not this product's cost.
    let costs = plan.costs();
    assert_eq!(costs.buy_input_wei, U256::from(QUALIFIED_MAINNET_INPUT_WEI));
    assert_eq!(costs.buy_gas_wei, buy_gas());
    assert_eq!(costs.own_launch_wei, U256::ZERO);
    assert_eq!(costs.provider_payment_wei, U256::ZERO);
    assert_eq!(costs.total_wei, costs.buy_input_wei + costs.buy_gas_wei);
}

#[test]
fn controlled_self_launch_charges_the_own_launch_cost() {
    let raw = own_launch_raw(0);
    let (expected, inputs) = self_launch(&raw);
    let plan = SubmissionPlan::new(&table(), &expected, &budget(), inputs).unwrap();
    assert_eq!(plan.mode(), SubmissionMode::ControlledSelfLaunchBundle);
    assert!(plan.launch().tx().unwrap().matches(&raw));
    let costs = plan.costs();
    assert_eq!(costs.own_launch_wei, own_launch_gas());
    assert_eq!(
        costs.total_wei,
        costs.buy_input_wei + costs.buy_gas_wei + own_launch_gas()
    );

    // The launch's value counts as well: five wei more exceed this budget.
    let (expected, inputs) = self_launch(&own_launch_raw(5));
    assert_eq!(
        SubmissionPlan::new(&table(), &expected, &budget(), inputs).err(),
        Some(PlanError::BudgetExceeded)
    );
}

#[test]
fn the_launch_must_be_the_pending_candidate() {
    // A third party's launch that is not the one the BUY was admitted
    // behind.
    let mut other_raw = foreign_launch_raw();
    other_raw[1] ^= 0x01;
    assert_eq!(
        plan_with(|i| i.launch = Launch::PublicOriginal(TxRef::of(&other_raw))).err(),
        Some(PlanError::LaunchNotCandidate)
    );

    // An own launch that is a valid transaction, but another one than the
    // candidate.
    let candidate_raw = own_launch_raw(0);
    let (expected, mut inputs) = self_launch(&candidate_raw);
    inputs.launch = Launch::ControlledOwn(own_launch(&own_launch_raw(5)));
    assert_eq!(
        SubmissionPlan::new(&table(), &expected, &budget(), inputs).err(),
        Some(PlanError::LaunchNotCandidate)
    );
}

#[test]
fn own_launch_must_be_on_the_buy_chain_and_call_the_buy_portal() {
    // Signed for another chain.
    let (expected, inputs) = self_launch(&own_launch_raw_with(97, portal(), 0));
    assert_eq!(
        SubmissionPlan::new(&table(), &expected, &budget(), inputs).err(),
        Some(PlanError::OwnLaunchWrongChain)
    );

    // Calls another contract than the Portal.
    let elsewhere = Address::from_slice(&[0x99u8; 20]);
    let (expected, inputs) = self_launch(&own_launch_raw_with(CHAIN, elsewhere, 0));
    assert_eq!(
        SubmissionPlan::new(&table(), &expected, &budget(), inputs).err(),
        Some(PlanError::OwnLaunchWrongPortal)
    );
}

#[test]
fn launch_kind_must_fit_the_mode() {
    use PlanError::*;
    let own = own_launch(&own_launch_raw(0));

    // The runner never has to guess which case it is.
    assert_eq!(
        plan_with(|i| i.launch = Launch::ControlledOwn(own)).err(),
        Some(LaunchKindMismatch)
    );
    assert_eq!(
        plan_with(|i| i.mode = SubmissionMode::ControlledSelfLaunchBundle).err(),
        Some(LaunchKindMismatch)
    );
    assert_eq!(
        plan_with(|i| i.launch = Launch::None).err(),
        Some(MissingLaunch)
    );
    assert_eq!(
        plan_with(|i| {
            i.mode = SubmissionMode::ControlledSelfLaunchBundle;
            i.launch = Launch::None;
        })
        .err(),
        Some(MissingLaunch)
    );
    assert_eq!(
        plan_with(|i| {
            i.mode = SubmissionMode::SingleRawBuy;
            i.adapters = set(&[2]);
        })
        .err(),
        Some(UnexpectedLaunch)
    );
    assert_eq!(
        plan_with(|i| i.launch = Launch::PublicOriginal(buy_ref())).err(),
        Some(LaunchIsBuy)
    );
    assert_eq!(
        plan_with(|i| i.buy = TxRef::of(&[])).err(),
        Some(EmptyTransaction)
    );
    assert_eq!(
        plan_with(|i| i.launch = Launch::PublicOriginal(TxRef::of(&[]))).err(),
        Some(EmptyTransaction)
    );
}

#[test]
fn self_launch_bundle_is_not_sent_before_the_launch_without_the_capability() {
    let raw = own_launch_raw(0);
    let (expected, mut inputs) = self_launch(&raw);

    // As a bundle whose launch is already pending it is admitted.
    assert!(SubmissionPlan::new(&table(), &expected, &budget(), inputs).is_ok());

    // Before the launch was broadcast: no frozen record allows it.
    inputs.pre_submission = PreSubmission::BeforeLaunchExecution;
    assert_eq!(
        SubmissionPlan::new(&table(), &expected, &budget(), inputs).err(),
        Some(PlanError::OptionNotEnabled)
    );
    // One qualified adapter is not enough; every selected one must be.
    let club48 = *CapabilityId::Club48BuilderV1.record();
    let blockrazor = *CapabilityId::BlockRazorBuilderV1.record();
    let one = [
        (adapter(0), club48.with_pre_submission(Support::Enabled)),
        (adapter(1), blockrazor),
    ];
    let table_one = AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &one).unwrap();
    assert_eq!(
        SubmissionPlan::new(&table_one, &expected, &budget(), inputs).err(),
        Some(PlanError::OptionNotEnabled)
    );
    let both = [
        (adapter(0), club48.with_pre_submission(Support::Enabled)),
        (adapter(1), blockrazor.with_pre_submission(Support::Enabled)),
    ];
    let table_both = AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &both).unwrap();
    let plan = SubmissionPlan::new(&table_both, &expected, &budget(), inputs).unwrap();
    assert_eq!(plan.pre_submission(), PreSubmission::BeforeLaunchExecution);

    // Pre-submission of anything but a controlled self-launch is not a
    // thing.
    assert_eq!(
        plan_with(|i| i.pre_submission = PreSubmission::BeforeLaunchExecution).err(),
        Some(PlanError::InvalidPreSubmission)
    );
}

#[test]
fn modes_are_admitted_only_where_the_record_enables_them() {
    use PlanError::*;

    // A private target is enabled nowhere, in any shape.
    assert_eq!(
        plan_with(|i| i.mode = SubmissionMode::PrivateTarget).err(),
        Some(ModeNotEnabled)
    );
    assert_eq!(
        plan_with(|i| {
            i.mode = SubmissionMode::PrivateTarget;
            i.launch = Launch::None;
        })
        .err(),
        Some(ModeNotEnabled)
    );

    // A single BUY through a builder is not qualified.
    assert_eq!(
        plan_with(|i| {
            i.mode = SubmissionMode::SingleRawBuy;
            i.launch = Launch::None;
        })
        .err(),
        Some(ModeNotEnabled)
    );

    // A bundle never falls back to the public path of the local node.
    assert_eq!(
        plan_with(|i| i.adapters = set(&[2])).err(),
        Some(ModeNotEnabled)
    );
    assert_eq!(
        plan_with(|i| i.adapters = set(&[0, 2])).err(),
        Some(ModeNotEnabled)
    );

    // The single BUY through the local node is admitted, explicitly, and
    // without a provider validity the node could not enforce.
    let single = plan_with(|i| {
        i.mode = SubmissionMode::SingleRawBuy;
        i.launch = Launch::None;
        i.adapters = set(&[2]);
        i.merge = MergePolicy::ProviderDefault;
        i.provider_validity = None;
    })
    .unwrap();
    assert_eq!(single.launch(), Launch::None);
    assert_eq!(single.provider_validity(), None);
    assert_eq!(
        plan_with(|i| {
            i.mode = SubmissionMode::SingleRawBuy;
            i.launch = Launch::None;
            i.adapters = set(&[2]);
            i.merge = MergePolicy::ProviderDefault;
        })
        .err(),
        Some(OptionNotEnabled)
    );

    // An adapter without a record is refused.
    assert_eq!(
        plan_with(|i| i.adapters = set(&[0, 5])).err(),
        Some(UnboundAdapter)
    );
}

#[test]
fn blockrazor_rpc_is_never_admitted_in_place_of_the_builder() {
    let (ticket, expected) = public_intent();
    let bindings = [
        bind(0, CapabilityId::Club48BuilderV1),
        bind(1, CapabilityId::BlockRazorRpcV1),
    ];
    let table = AdapterTable::new(CHAIN, CAPABILITY_GENERATION, &bindings).unwrap();
    assert_eq!(
        SubmissionPlan::new(&table, &expected, &budget(), public_inputs(ticket)).err(),
        Some(PlanError::ModeNotEnabled)
    );
}

#[test]
fn unsupported_option_fails_closed() {
    let (ticket, expected) = public_intent();
    let club48 = *CapabilityId::Club48BuilderV1.record();
    let blockrazor = *CapabilityId::BlockRazorBuilderV1.record();
    let table_of = |second: CapabilityRecord| {
        let records = [(adapter(0), club48), (adapter(1), second)];
        AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &records).unwrap()
    };
    let plan = |table: &AdapterTable, inputs: PlanInputs| {
        SubmissionPlan::new(table, &expected, &budget(), inputs).err()
    };

    // `noMerge` disabled on one adapter: not silently dropped, refused.
    let table = table_of(blockrazor.with_no_merge(Support::Unqualified));
    assert_eq!(
        plan(&table, public_inputs(ticket)),
        Some(PlanError::OptionNotEnabled)
    );
    // Without the request that adapter alone is admitted. (With 48Club in
    // the set the profile would demand the request; see the profile tests.)
    let mut inputs = public_inputs(ticket);
    inputs.merge = MergePolicy::ProviderDefault;
    inputs.adapters = set(&[1]);
    assert_eq!(plan(&table, inputs), None);

    // A bundle needs the provider validity by block to be enabled.
    let table = table_of(blockrazor.with_max_block_number(Support::Unsupported));
    assert_eq!(
        plan(&table, public_inputs(ticket)),
        Some(PlanError::OptionNotEnabled)
    );

    // More transactions than a record admits.
    let table = table_of(blockrazor.with_max_txs(1));
    assert_eq!(
        plan(&table, public_inputs(ticket)),
        Some(PlanError::TooManyTransactions)
    );

    // `minTimestamp` is enabled for BlockRazor and not for 48Club.
    let mut inputs = public_inputs(ticket);
    inputs.provider_validity = Some(ProviderValidity {
        max_block_number: MAX_BLOCK,
        min_timestamp: Some(1_700_000_000),
        max_timestamp: Some(1_900_000_000),
    });
    assert_eq!(
        plan(&self::table(), inputs),
        Some(PlanError::OptionNotEnabled)
    );
    inputs.adapters = set(&[1]);
    assert_eq!(plan(&self::table(), inputs), None);
    // `maxTimestamp` alone is enabled for both.
    inputs.adapters = set(&[0, 1]);
    inputs.provider_validity = Some(ProviderValidity {
        max_block_number: MAX_BLOCK,
        min_timestamp: None,
        max_timestamp: Some(1_900_000_000),
    });
    assert_eq!(plan(&self::table(), inputs), None);
}

#[test]
fn plan_binds_the_one_economic_identity() {
    let (ticket, expected) = public_intent();

    // An expectation for another nonce is another intent.
    let other = admitted_buy(foreign_launch().hash(), ticket.nonce() + 1, GAS_PRICE).0;
    assert_eq!(
        SubmissionPlan::new(&table(), &other, &budget(), public_inputs(ticket)).err(),
        Some(PlanError::IdentityMismatch)
    );

    // A table of another chain, even with a record of that chain.
    let testnet_record = CapabilityId::Club48BuilderV1.record().with_chain_id(97);
    let testnet = AdapterTable::for_tests(97, 1, &[(adapter(0), testnet_record)]).unwrap();
    assert_eq!(
        SubmissionPlan::new(&testnet, &expected, &budget(), public_inputs(ticket)).err(),
        Some(PlanError::WrongChain)
    );
}

// ---------------------------------------------------------------------------
// Binding to the intent lane's dispatch
// ---------------------------------------------------------------------------

#[test]
fn plan_is_bound_to_exactly_the_dispatch_of_the_intent_lane() {
    let candidate = foreign_launch().hash();
    let current = snapshot(candidate);
    let adapters = set(&[0, 1]);

    // The real sequence: reserve, sign, prepare, plan, final check,
    // dispatch, bind.
    let fence = ArmedFence::for_tests(CHAIN, buyer().address(), FIRST_NONCE);
    let mut lane = IntentLane::armed(fence, 1).unwrap();
    let token = admitted_buy(candidate, FIRST_NONCE, GAS_PRICE).0.token();
    let ticket = lane.reserve(token).unwrap();
    let (expected, calldata) = admitted_buy(candidate, ticket.nonce(), GAS_PRICE);
    let tx = LegacyTx {
        chain_id: expected.chain_id(),
        nonce: expected.nonce(),
        gas_price: expected.gas_price(),
        gas_limit: expected.gas_limit(),
        to: expected.portal(),
        value: expected.input_amount(),
        input: &calldata,
    };
    let mut scratch = Vec::with_capacity(PREPARE_SCRATCH_CAPACITY);
    let mut raw = Vec::new();
    buyer().sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
    let hash = lane
        .prepare(ticket, &raw, &expected, &current, &mut scratch)
        .unwrap();

    let mut inputs = public_inputs(ticket);
    inputs.buy = TxRef::of(&raw);
    let table = table();
    let plan = SubmissionPlan::new(&table, &expected, &budget(), inputs).unwrap();
    assert_eq!(plan.buy().hash(), hash);
    plan.check_dispatch(&table, &current, head(LAST_START_HEAD))
        .unwrap();

    let dispatch = lane.begin_send(&current, adapters, &plan.costs()).unwrap();
    let bound = plan.bind_dispatch(&dispatch).unwrap();
    assert_eq!(bound.ticket(), ticket);
    assert_eq!(bound.buy(), TxRef::of(&raw));
    assert_eq!(bound.adapters(), adapters);

    // The bytes the lane releases against a permit are the plan's BUY.
    let released = lane
        .start_write(ticket, adapter(0), bound.buy().hash())
        .unwrap();
    assert!(plan.buy().matches(released.bytes()));

    // Another ticket, another BUY hash or another adapter set is not this
    // plan's dispatch.
    let other_fence = ArmedFence::for_tests(CHAIN, buyer().address(), FIRST_NONCE + 1);
    let mut other_lane = IntentLane::armed(other_fence, 1).unwrap();
    let other_ticket = other_lane.reserve(token).unwrap();
    let mismatches = [
        Dispatch {
            ticket: other_ticket,
            ..dispatch
        },
        Dispatch {
            tx_hash: B256::ZERO,
            ..dispatch
        },
        Dispatch {
            adapters: set(&[0]),
            ..dispatch
        },
        Dispatch {
            adapters: set(&[0, 1, 2]),
            ..dispatch
        },
    ];
    for (index, other) in mismatches.iter().enumerate() {
        assert_eq!(
            plan.bind_dispatch(other).err(),
            Some(PlanError::DispatchMismatch),
            "case {index}"
        );
    }
}

// ---------------------------------------------------------------------------
// Budget
// ---------------------------------------------------------------------------

#[test]
fn every_budget_component_and_the_total_are_enforced() {
    let (ticket, expected) = public_intent();
    let one = U256::from(1u64);
    let exceeded = Some(PlanError::BudgetExceeded);
    let public = |budget: BudgetEnvelope| {
        SubmissionPlan::new(&table(), &expected, &budget, public_inputs(ticket)).err()
    };

    // Exactly at every bound passes.
    assert_eq!(public(budget()), None);

    let low = BudgetEnvelope {
        max_buy_input_wei: budget().max_buy_input_wei - one,
        ..budget()
    };
    assert_eq!(public(low), exceeded);

    let low = BudgetEnvelope {
        max_buy_gas_wei: budget().max_buy_gas_wei - one,
        ..budget()
    };
    assert_eq!(public(low), exceeded);

    // The total binds even when every component fits.
    let low = BudgetEnvelope {
        max_total_wei: U256::from(QUALIFIED_MAINNET_INPUT_WEI) + buy_gas() - one,
        ..budget()
    };
    assert_eq!(public(low), exceeded);

    // An own launch needs its own allowance; a public launch needs none.
    let no_launch_allowance = BudgetEnvelope {
        max_own_launch_wei: U256::ZERO,
        ..budget()
    };
    let (self_expected, self_inputs) = self_launch(&own_launch_raw(0));
    assert_eq!(
        SubmissionPlan::new(&table(), &self_expected, &no_launch_allowance, self_inputs).err(),
        exceeded
    );
    assert_eq!(public(no_launch_allowance), None);
}

#[test]
fn provider_payment_without_a_carrier_is_refused() {
    // No transaction carries a direct payment to a provider yet. A plan
    // that claims one is refused, whatever the budget would allow.
    let (ticket, expected) = public_intent();
    let generous = BudgetEnvelope {
        max_provider_payment_wei: U256::MAX,
        max_total_wei: U256::MAX,
        ..budget()
    };
    let mut inputs = public_inputs(ticket);
    inputs.provider_payment_wei = U256::from(1_000u64);
    assert_eq!(
        SubmissionPlan::new(&table(), &expected, &generous, inputs).err(),
        Some(PlanError::UnboundProviderPayment)
    );
    inputs.provider_payment_wei = U256::from(1u64);
    assert_eq!(
        SubmissionPlan::new(&table(), &expected, &generous, inputs).err(),
        Some(PlanError::UnboundProviderPayment)
    );
    // Without the claim the same plan is admitted and carries no payment.
    inputs.provider_payment_wei = U256::ZERO;
    let plan = SubmissionPlan::new(&table(), &expected, &generous, inputs).unwrap();
    assert_eq!(plan.costs().provider_payment_wei, U256::ZERO);
}

#[test]
fn no_per_buy_gas_floor_is_invented_for_blockrazor() {
    // The provider's two statements about 0.05 gwei differ; the plan adopts
    // neither. A BUY below 0.05 gwei is not refused by this product: what
    // the provider accepts is settled by qualification, not by a formula
    // made up here.
    let candidate = foreign_launch().hash();
    let (ticket, below) = intent(candidate, 49_999_999);
    assert!(SubmissionPlan::new(&table(), &below, &budget(), public_inputs(ticket)).is_ok());
    let (ticket, far_below) = intent(candidate, 1);
    assert!(SubmissionPlan::new(&table(), &far_below, &budget(), public_inputs(ticket)).is_ok());
    assert_eq!(
        CapabilityId::BlockRazorBuilderV1.record().fee_floor(),
        FeeFloor::DocumentedRuleUnqualified
    );
}

// ---------------------------------------------------------------------------
// Time: local start deadline, provider validity, transaction validity
// ---------------------------------------------------------------------------

#[test]
fn local_deadline_and_provider_validity_are_separate_bounds() {
    use PlanError::*;

    // A bundle needs a provider validity; it must be well-formed.
    assert_eq!(
        plan_with(|i| i.provider_validity = None).err(),
        Some(MissingProviderValidity)
    );
    assert_eq!(
        plan_with(|i| {
            i.provider_validity = Some(ProviderValidity {
                max_block_number: 0,
                min_timestamp: None,
                max_timestamp: None,
            });
            i.local_block_deadline.last_head_block = 0;
        })
        .err(),
        Some(InvalidProviderValidity)
    );
    assert_eq!(
        plan_with(|i| {
            i.adapters = set(&[1]);
            i.provider_validity = Some(ProviderValidity {
                max_block_number: MAX_BLOCK,
                min_timestamp: Some(20),
                max_timestamp: Some(10),
            });
        })
        .err(),
        Some(InvalidProviderValidity)
    );

    // The local block bound is a rule of its own. It may equal the
    // provider's last block; only a bound beyond it, which could never be
    // used, is refused.
    let equal = plan_with(|i| i.local_block_deadline.last_head_block = MAX_BLOCK).unwrap();
    assert!(equal.local_block_start_allowed(MAX_BLOCK));
    assert_eq!(
        plan_with(|i| i.local_block_deadline.last_head_block = MAX_BLOCK + 1).err(),
        Some(LocalDeadlineAfterProviderValidity)
    );
    // It may be stricter than that.
    let strict = plan_with(|i| i.local_block_deadline.last_head_block = MAX_BLOCK - 10).unwrap();
    assert!(strict.local_block_start_allowed(MAX_BLOCK - 10));
    assert!(!strict.local_block_start_allowed(MAX_BLOCK - 9));
    // The provider validity is unaffected by the stricter local rule.
    assert!(!strict.provider_expired(head(MAX_BLOCK - 9)));
}

#[test]
fn provider_validity_is_inclusive() {
    let plan = plan_with(|_| ()).unwrap();
    // The last valid block itself is not expired; the one after it is.
    assert!(!plan.provider_expired(head(MAX_BLOCK - 1)));
    assert!(!plan.provider_expired(head(MAX_BLOCK)));
    assert!(plan.provider_expired(head(MAX_BLOCK + 1)));

    // The same holds for a timestamp bound.
    let timed = plan_with(|i| {
        i.provider_validity = Some(ProviderValidity {
            max_block_number: MAX_BLOCK,
            min_timestamp: None,
            max_timestamp: Some(1_800_000_000),
        });
    })
    .unwrap();
    let at = |timestamp: u64| ChainHead {
        number: LAST_START_HEAD,
        timestamp,
    };
    assert!(!timed.provider_expired(at(1_799_999_999)));
    assert!(!timed.provider_expired(at(1_800_000_000)));
    assert!(timed.provider_expired(at(1_800_000_001)));

    // A single BUY through the local node has no provider validity and
    // never provider-expires.
    let single = plan_with(|i| {
        i.mode = SubmissionMode::SingleRawBuy;
        i.launch = Launch::None;
        i.adapters = set(&[2]);
        i.merge = MergePolicy::ProviderDefault;
        i.provider_validity = None;
    })
    .unwrap();
    assert!(!single.provider_expired(head(u64::MAX)));
    assert!(!single.local_block_start_allowed(LAST_START_HEAD + 1));
}

#[test]
fn the_signed_buy_has_no_time_bound_of_its_own() {
    let plan = plan_with(|_| ()).unwrap();
    assert_eq!(plan.buy_validity(), TransactionValidity::NoTimeBound);
    // Neither bound of the plan changes that: after both have passed, a
    // disclosed BUY may still execute.
    assert!(!plan.local_block_start_allowed(MAX_BLOCK + 5));
    assert!(plan.provider_expired(head(MAX_BLOCK + 5)));
    let after = disclosure(SubmissionState::HandedOff, true);
    assert!(after.may_still_execute());
}

#[test]
fn final_check_refuses_stale_state_and_passed_bounds() {
    let (ticket, expected) = public_intent();
    let table = table();
    let plan = SubmissionPlan::new(&table, &expected, &budget(), public_inputs(ticket)).unwrap();
    let current = snapshot(foreign_launch().hash());

    assert_eq!(
        plan.check_dispatch(&table, &current, head(LAST_START_HEAD)),
        Ok(())
    );

    // The local start deadline stops first, while the provider would
    // still accept.
    assert_eq!(
        plan.check_dispatch(&table, &current, head(MAX_BLOCK)),
        Err(PlanError::LocalDeadlinePassed)
    );
    assert!(!plan.provider_expired(head(MAX_BLOCK)));

    // A timestamp expiry can pass before the local block deadline.
    let mut inputs = public_inputs(ticket);
    inputs.provider_validity = Some(ProviderValidity {
        max_block_number: MAX_BLOCK,
        min_timestamp: None,
        max_timestamp: Some(1_799_999_999),
    });
    let timed = SubmissionPlan::new(&table, &expected, &budget(), inputs).unwrap();
    assert_eq!(
        timed.check_dispatch(&table, &current, head(LAST_START_HEAD)),
        Err(PlanError::ProviderExpired)
    );

    // Policy, code, candidate or chain state changed.
    let mut other = current;
    other.policy_generation += 1;
    assert_eq!(
        plan.check_dispatch(&table, &other, head(LAST_START_HEAD)),
        Err(PlanError::StaleSnapshot)
    );

    // The adapter configuration was validated again.
    let newer = AdapterTable::new(
        CHAIN,
        CAPABILITY_GENERATION + 1,
        &[bind(0, CapabilityId::Club48BuilderV1)],
    )
    .unwrap();
    assert_eq!(
        plan.check_dispatch(&newer, &current, head(LAST_START_HEAD)),
        Err(PlanError::StaleCapabilities)
    );
}

// ---------------------------------------------------------------------------
// Release profile: envelope options, block window, eligibility
// ---------------------------------------------------------------------------

#[test]
fn birth_entry_profile_fixes_the_club48_envelope() {
    let profile = ReleaseProfile::BirthEntryV1;
    assert_eq!(
        profile.required_options(AdapterKind::Club48Builder),
        RequiredOptions {
            no_merge: true,
            position_first: true,
        }
    );
    // The frozen evidence fixes no option for the other adapters.
    for kind in [
        AdapterKind::BlockRazorBuilder,
        AdapterKind::BlockRazorRpc,
        AdapterKind::LocalGeth,
    ] {
        assert_eq!(
            profile.required_options(kind),
            RequiredOptions {
                no_merge: false,
                position_first: false,
            }
        );
    }

    // The plan carries the derived envelope of every selected adapter.
    let plan = plan_with(|_| ()).unwrap();
    assert_eq!(plan.profile(), profile);
    assert_eq!(
        plan.envelope(adapter(0)),
        Some(EnvelopeOptions {
            no_merge: true,
            position_first: true,
        })
    );
    // BlockRazor: `noMerge` because the plan requests it; `positionFirst`
    // is never sent.
    assert_eq!(
        plan.envelope(adapter(1)),
        Some(EnvelopeOptions {
            no_merge: true,
            position_first: false,
        })
    );
    assert_eq!(plan.envelope(adapter(2)), None);
}

#[test]
fn provider_default_merging_is_refused_where_the_profile_requires_no_merge() {
    // 48Club under the birth-entry profile: the default is not enough, and
    // it is not silently upgraded.
    assert_eq!(
        plan_with(|i| i.merge = MergePolicy::ProviderDefault).err(),
        Some(PlanError::ProfileOptionMissing)
    );
    assert_eq!(
        plan_with(|i| {
            i.merge = MergePolicy::ProviderDefault;
            i.adapters = set(&[0]);
        })
        .err(),
        Some(PlanError::ProfileOptionMissing)
    );
    // BlockRazor alone has no such requirement.
    let plan = plan_with(|i| {
        i.merge = MergePolicy::ProviderDefault;
        i.adapters = set(&[1]);
    })
    .unwrap();
    assert_eq!(
        plan.envelope(adapter(1)),
        Some(EnvelopeOptions {
            no_merge: false,
            position_first: false,
        })
    );
}

#[test]
fn a_required_option_must_be_enabled_on_the_adapter() {
    let (ticket, expected) = public_intent();
    let club48 = *CapabilityId::Club48BuilderV1.record();
    // A 48Club record without `positionFirst` cannot carry the profile.
    let records = [(adapter(0), club48.with_position_first(Support::Unqualified))];
    let table = AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &records).unwrap();
    let mut inputs = public_inputs(ticket);
    inputs.adapters = set(&[0]);
    assert_eq!(
        SubmissionPlan::new(&table, &expected, &budget(), inputs).err(),
        Some(PlanError::OptionNotEnabled)
    );
    // Nor one without `noMerge`.
    let records = [(adapter(0), club48.with_no_merge(Support::Unqualified))];
    let table = AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &records).unwrap();
    assert_eq!(
        SubmissionPlan::new(&table, &expected, &budget(), inputs).err(),
        Some(PlanError::OptionNotEnabled)
    );
}

#[test]
fn provider_block_window_is_at_most_two_blocks_beyond_the_head() {
    let profile = ReleaseProfile::BirthEntryV1;
    assert_eq!(profile.max_block_window(), 2);
    let head = 5_000u64;

    // head, head + 1 and head + 2 are inside the window.
    for max_block in [head, head + 1, head + 2] {
        assert_eq!(
            profile.check_block_window(max_block, head),
            Ok(()),
            "{max_block}"
        );
    }
    // head + 3 and a long default such as +100 are not.
    for max_block in [head + 3, head + 100, u64::MAX] {
        assert_eq!(
            profile.check_block_window(max_block, head),
            Err(PlanError::ProviderWindowTooWide),
            "{max_block}"
        );
    }
    // A validity that lies before the head has expired.
    assert_eq!(
        profile.check_block_window(head - 1, head),
        Err(PlanError::ProviderExpired)
    );

    // At the upper end of u64 the window cannot be computed: fail closed.
    assert_eq!(
        profile.check_block_window(u64::MAX, u64::MAX),
        Err(PlanError::ProviderWindowTooWide)
    );
    assert_eq!(
        profile.check_block_window(u64::MAX, u64::MAX - 1),
        Err(PlanError::ProviderWindowTooWide)
    );
    assert_eq!(profile.check_block_window(u64::MAX, u64::MAX - 2), Ok(()));
}

#[test]
fn full_dispatch_check_accepts_exactly_head_to_head_plus_two() {
    // The complete final check, not the window function alone. The local
    // block bound is set equal to the provider's last block, so that only
    // the window decides.
    let (ticket, expected) = public_intent();
    let table = table();
    let current = snapshot(foreign_launch().hash());
    let dispatch_at = |head_number: u64, max_block: u64| {
        let mut inputs = public_inputs(ticket);
        inputs.provider_validity = Some(ProviderValidity {
            max_block_number: max_block,
            min_timestamp: None,
            max_timestamp: None,
        });
        inputs.local_block_deadline = LocalBlockStartDeadline {
            last_head_block: max_block,
        };
        SubmissionPlan::new(&table, &expected, &budget(), inputs)
            .unwrap()
            .check_dispatch(&table, &current, head(head_number))
    };
    let at = 5_000u64;

    // maxBlock = head, head + 1, head + 2: dispatched.
    assert_eq!(dispatch_at(at, at), Ok(()));
    assert_eq!(dispatch_at(at, at + 1), Ok(()));
    assert_eq!(dispatch_at(at, at + 2), Ok(()));
    // head + 3 and a long default such as +100: refused.
    assert_eq!(
        dispatch_at(at, at + 3),
        Err(PlanError::ProviderWindowTooWide)
    );
    assert_eq!(
        dispatch_at(at, at + 100),
        Err(PlanError::ProviderWindowTooWide)
    );
    // maxBlock < head: refused. With the local bound at maxBlock the local
    // rule sees it first.
    assert_eq!(dispatch_at(at, at - 1), Err(PlanError::LocalDeadlinePassed));
    // A provider validity that has passed by its timestamp while the local
    // block bound still admits the head is refused as such.
    let mut inputs = public_inputs(ticket);
    inputs.provider_validity = Some(ProviderValidity {
        max_block_number: at,
        min_timestamp: None,
        max_timestamp: Some(1_799_999_999),
    });
    inputs.local_block_deadline = LocalBlockStartDeadline {
        last_head_block: at,
    };
    let timed = SubmissionPlan::new(&table, &expected, &budget(), inputs).unwrap();
    assert_eq!(
        timed.check_dispatch(&table, &current, head(at)),
        Err(PlanError::ProviderExpired)
    );

    // At the upper end of u64 the window cannot be computed: fail closed,
    // through the full check.
    assert_eq!(
        dispatch_at(u64::MAX, u64::MAX),
        Err(PlanError::ProviderWindowTooWide)
    );
    assert_eq!(
        dispatch_at(u64::MAX - 1, u64::MAX),
        Err(PlanError::ProviderWindowTooWide)
    );
    assert_eq!(dispatch_at(u64::MAX - 2, u64::MAX), Ok(()));
}

#[test]
fn final_check_enforces_the_tight_block_window() {
    let (ticket, expected) = public_intent();
    let table = table();
    let current = snapshot(foreign_launch().hash());
    // Provider validity up to block 1,000, local block bound at head 999.
    let plan = SubmissionPlan::new(&table, &expected, &budget(), public_inputs(ticket)).unwrap();

    // Heads 998 and 999: block 1,000 is at most two blocks ahead.
    assert_eq!(
        plan.check_dispatch(&table, &current, head(MAX_BLOCK - 1)),
        Ok(())
    );
    assert_eq!(
        plan.check_dispatch(&table, &current, head(MAX_BLOCK - 2)),
        Ok(())
    );
    // Head 997 and earlier: the same validity is three or more blocks
    // ahead. Nothing is dispatched with a wide window.
    for early in [MAX_BLOCK - 3, MAX_BLOCK - 100, 0] {
        assert_eq!(
            plan.check_dispatch(&table, &current, head(early)),
            Err(PlanError::ProviderWindowTooWide),
            "{early}"
        );
    }
    // Head 1,000: the validity itself is still inside the window, but the
    // stricter local block bound has passed.
    assert_eq!(
        ReleaseProfile::BirthEntryV1.check_block_window(MAX_BLOCK, MAX_BLOCK),
        Ok(())
    );
    assert_eq!(
        plan.check_dispatch(&table, &current, head(MAX_BLOCK)),
        Err(PlanError::LocalDeadlinePassed)
    );

    // A plan with a long default can be formed, since the head is not
    // known then, but it can never be dispatched before its last blocks.
    let mut inputs = public_inputs(ticket);
    inputs.provider_validity = Some(ProviderValidity {
        max_block_number: MAX_BLOCK + 100,
        min_timestamp: None,
        max_timestamp: None,
    });
    let wide = SubmissionPlan::new(&table, &expected, &budget(), inputs).unwrap();
    assert_eq!(
        wide.check_dispatch(&table, &current, head(LAST_START_HEAD)),
        Err(PlanError::ProviderWindowTooWide)
    );
}

#[test]
fn no_frozen_adapter_counts_for_a_release_yet() {
    let profile = ReleaseProfile::BirthEntryV1;
    let bundle = SubmissionMode::PublicFullLaunchBundle;

    // 48Club: order is not even stated by the frozen evidence.
    assert_eq!(
        profile.release_blocker(CapabilityId::Club48BuilderV1.record(), bundle),
        Some(ReleaseBlocker::OrderNotEstablished)
    );
    // BlockRazor direct: order is documented, but documented is not
    // qualified.
    assert_eq!(
        profile.release_blocker(CapabilityId::BlockRazorBuilderV1.record(), bundle),
        Some(ReleaseBlocker::OrderNotEstablished)
    );
    // The RPC and the local node do not have the mode at all.
    assert_eq!(
        profile.release_blocker(CapabilityId::BlockRazorRpcV1.record(), bundle),
        Some(ReleaseBlocker::ModeNotEnabled)
    );
    assert_eq!(
        profile.release_blocker(CapabilityId::LocalGethV1.record(), bundle),
        Some(ReleaseBlocker::ModeNotEnabled)
    );
    // A single BUY gives no ordered placement behind the launch.
    assert_eq!(
        profile.release_blocker(
            CapabilityId::LocalGethV1.record(),
            SubmissionMode::SingleRawBuy
        ),
        Some(ReleaseBlocker::NoOrderedPlacement)
    );
    // Every frozen record has a blocker, in every mode.
    for id in all_capabilities() {
        for mode in [
            SubmissionMode::SingleRawBuy,
            SubmissionMode::PublicFullLaunchBundle,
            SubmissionMode::ControlledSelfLaunchBundle,
            SubmissionMode::PrivateTarget,
        ] {
            assert!(profile.release_blocker(id.record(), mode).is_some());
        }
    }
}

#[test]
fn a_required_property_must_be_stated_and_qualified() {
    let profile = ReleaseProfile::BirthEntryV1;
    let bundle = SubmissionMode::PublicFullLaunchBundle;
    // A record with every required property established and the send
    // boundary qualified.
    let grade = CapabilityId::BlockRazorBuilderV1.record().release_grade();
    assert_eq!(profile.release_blocker(&grade, bundle), None);

    // A property that must hold: only "yes and qualified" passes.
    let atomic = |property: Property| profile.release_blocker(&grade.with_atomic(property), bundle);
    let blocked = Some(ReleaseBlocker::AtomicityNotEstablished);
    assert_eq!(atomic(qualified(Knowledge::Yes)), None);
    assert_eq!(atomic(unqualified(Knowledge::Yes)), blocked);
    assert_eq!(atomic(qualified(Knowledge::Unknown)), blocked);
    assert_eq!(atomic(qualified(Knowledge::No)), blocked);
    assert_eq!(atomic(unqualified(Knowledge::Unknown)), blocked);

    // A property that must not hold: only "no and qualified" passes.
    let rebroadcast = |property: Property| {
        profile.release_blocker(&grade.with_provider_rebroadcast(property), bundle)
    };
    let blocked = Some(ReleaseBlocker::RebroadcastNotExcluded);
    assert_eq!(rebroadcast(qualified(Knowledge::No)), None);
    assert_eq!(rebroadcast(unqualified(Knowledge::No)), blocked);
    assert_eq!(rebroadcast(qualified(Knowledge::Unknown)), blocked);
    assert_eq!(rebroadcast(qualified(Knowledge::Yes)), blocked);

    // The public fallback is required independently of the rebroadcast.
    assert_eq!(
        profile.release_blocker(
            &grade.with_provider_public_fallback(unqualified(Knowledge::Unknown)),
            bundle
        ),
        Some(ReleaseBlocker::PublicFallbackNotExcluded)
    );

    // A qualified send boundary is its own requirement; it establishes
    // nothing else, and nothing else establishes it.
    assert_eq!(
        profile.release_blocker(&grade.with_send_time_qualified(false), bundle),
        Some(ReleaseBlocker::SendTimeNotQualified)
    );
    let frozen = *CapabilityId::BlockRazorBuilderV1.record();
    assert_eq!(
        profile.release_blocker(&frozen.with_send_time_qualified(true), bundle),
        Some(ReleaseBlocker::OrderNotEstablished)
    );
}

#[test]
fn a_property_the_profile_does_not_require_never_blocks() {
    // The reorg exposure after inclusion is a risk of the chain, handled by
    // recovery and reconciliation. The profile does not select adapters by
    // it: unknown, yes or unqualified, it does not block.
    let profile = ReleaseProfile::BirthEntryV1;
    let bundle = SubmissionMode::PublicFullLaunchBundle;
    let grade = CapabilityId::Club48BuilderV1.record().release_grade();
    for property in [
        unqualified(Knowledge::Unknown),
        unqualified(Knowledge::Yes),
        qualified(Knowledge::Yes),
        qualified(Knowledge::No),
    ] {
        assert_eq!(
            profile.release_blocker(&grade.with_reorg_exposure(property), bundle),
            None
        );
    }
}

#[test]
fn release_plan_exists_only_when_every_selected_adapter_is_established() {
    let (ticket, expected) = public_intent();
    let club48 = *CapabilityId::Club48BuilderV1.record();
    let blockrazor = *CapabilityId::BlockRazorBuilderV1.record();
    let table_of = |first: CapabilityRecord, second: CapabilityRecord| {
        let records = [(adapter(0), first), (adapter(1), second)];
        AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &records).unwrap()
    };
    let plan_on = |table: &AdapterTable| {
        SubmissionPlan::new(table, &expected, &budget(), public_inputs(ticket)).unwrap()
    };

    // The frozen adapters: a diagnostic plan is allowed, the same plan as
    // a release plan is refused, on the first selected adapter.
    let frozen = table_of(club48, blockrazor);
    let diagnostic = plan_on(&frozen);
    assert_eq!(
        diagnostic.qualify_for_release(&frozen),
        Err(ReleaseRefusal {
            adapter: Some(adapter(0)),
            blocker: ReleaseBlocker::OrderNotEstablished,
        })
    );

    // One established adapter and one that is not: refused, and the
    // refusal names the adapter.
    let mixed = table_of(club48.release_grade(), blockrazor);
    assert_eq!(
        plan_on(&mixed).qualify_for_release(&mixed),
        Err(ReleaseRefusal {
            adapter: Some(adapter(1)),
            blocker: ReleaseBlocker::OrderNotEstablished,
        })
    );
    let mixed = table_of(club48, blockrazor.release_grade());
    assert_eq!(
        plan_on(&mixed)
            .qualify_for_release(&mixed)
            .err()
            .map(|r| r.adapter),
        Some(Some(adapter(0)))
    );

    // Every selected adapter established: the release plan exists and is
    // the same plan.
    let established = table_of(club48.release_grade(), blockrazor.release_grade());
    let plan = plan_on(&established);
    let release = plan.qualify_for_release(&established).unwrap();
    assert_eq!(*release.plan(), plan);
    // Its final check is the plan's, block window included.
    let current = snapshot(foreign_launch().hash());
    assert_eq!(
        release
            .plan()
            .check_dispatch(&established, &current, head(MAX_BLOCK - 3)),
        Err(PlanError::ProviderWindowTooWide)
    );

    // One unknown required property on one adapter is enough to refuse.
    let unknown = unqualified(Knowledge::Unknown);
    let one_unknown = table_of(
        club48.release_grade(),
        blockrazor.release_grade().with_atomic(unknown),
    );
    assert_eq!(
        plan_on(&one_unknown).qualify_for_release(&one_unknown),
        Err(ReleaseRefusal {
            adapter: Some(adapter(1)),
            blocker: ReleaseBlocker::AtomicityNotEstablished,
        })
    );

    // A table of another generation than the plan's is refused before any
    // adapter is looked at.
    let records = [
        (adapter(0), club48.release_grade()),
        (adapter(1), blockrazor.release_grade()),
    ];
    let newer = AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION + 1, &records).unwrap();
    assert_eq!(
        plan.qualify_for_release(&newer),
        Err(ReleaseRefusal {
            adapter: None,
            blocker: ReleaseBlocker::StaleCapabilities,
        })
    );
}

#[test]
fn release_plan_requires_the_profile_envelope_and_enabled_pre_submission() {
    // 48Club without the required `positionFirst` in the envelope cannot
    // occur through plan creation; the release gate checks it again
    // against the plan's stored envelope.
    let (ticket, expected) = public_intent();
    let club48 = CapabilityId::Club48BuilderV1.record().release_grade();
    let blockrazor = CapabilityId::BlockRazorBuilderV1.record().release_grade();
    let records = [(adapter(0), club48), (adapter(1), blockrazor)];
    let table = AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &records).unwrap();

    // BlockRazor alone with the provider default: no required option, the
    // release plan exists.
    let mut inputs = public_inputs(ticket);
    inputs.merge = MergePolicy::ProviderDefault;
    inputs.adapters = set(&[1]);
    let plan = SubmissionPlan::new(&table, &expected, &budget(), inputs).unwrap();
    assert!(plan.qualify_for_release(&table).is_ok());

    // The same plan judged against a table that binds 48Club at that index
    // lacks the options 48Club requires.
    let swapped = [(adapter(1), club48)];
    let swapped = AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &swapped).unwrap();
    assert_eq!(
        plan.qualify_for_release(&swapped),
        Err(ReleaseRefusal {
            adapter: Some(adapter(1)),
            blocker: ReleaseBlocker::EnvelopeOptionMissing,
        })
    );

    // A pre-submitting self-launch. Pre-submission that is merely enabled
    // is documented, not qualified: enough for the diagnostic plan, not
    // for a release.
    let raw = own_launch_raw(0);
    let (expected, mut inputs) = self_launch(&raw);
    inputs.pre_submission = PreSubmission::BeforeLaunchExecution;
    let table_with = |pre: Qualification| {
        let records = [
            (
                adapter(0),
                club48
                    .with_pre_submission(Support::Enabled)
                    .pre_submission_qualified(pre),
            ),
            (
                adapter(1),
                blockrazor
                    .with_pre_submission(Support::Enabled)
                    .pre_submission_qualified(pre),
            ),
        ];
        AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &records).unwrap()
    };
    let enabled_only = table_with(Qualification::Unqualified);
    let plan = SubmissionPlan::new(&enabled_only, &expected, &budget(), inputs).unwrap();
    assert_eq!(
        plan.qualify_for_release(&enabled_only),
        Err(ReleaseRefusal {
            adapter: Some(adapter(0)),
            blocker: ReleaseBlocker::PreSubmissionNotEstablished,
        })
    );
    // Enabled and qualified on every adapter: the release plan exists.
    let established = table_with(Qualification::Qualified);
    assert!(plan.qualify_for_release(&established).is_ok());
    // Not enabled at all on the adapters of the judging table.
    assert_eq!(
        plan.qualify_for_release(&table),
        Err(ReleaseRefusal {
            adapter: Some(adapter(0)),
            blocker: ReleaseBlocker::PreSubmissionNotEstablished,
        })
    );
}

// ---------------------------------------------------------------------------
// Release: provider features need support and qualification
// ---------------------------------------------------------------------------

/// Judges the public bundle through both builders against the given
/// records for 48Club and BlockRazor.
fn release_with(
    club48: CapabilityRecord,
    blockrazor: CapabilityRecord,
    change: impl FnOnce(&mut PlanInputs),
) -> Result<QualifiedReleasePlan, ReleaseRefusal> {
    let (ticket, expected) = public_intent();
    let records = [(adapter(0), club48), (adapter(1), blockrazor)];
    let table = AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &records).unwrap();
    let mut inputs = public_inputs(ticket);
    change(&mut inputs);
    // The diagnostic plan must exist: these features are all enabled.
    let plan = SubmissionPlan::new(&table, &expected, &budget(), inputs).unwrap();
    plan.qualify_for_release(&table)
}

fn refused(index: u8, blocker: ReleaseBlocker) -> Result<QualifiedReleasePlan, ReleaseRefusal> {
    Err(ReleaseRefusal {
        adapter: Some(adapter(index)),
        blocker,
    })
}

#[test]
fn enabled_is_not_qualified_for_any_feature_of_a_frozen_record() {
    for id in all_capabilities() {
        let record = id.record();
        for feature in [
            record.max_block_number(),
            record.min_timestamp(),
            record.max_timestamp(),
            record.no_merge(),
            record.position_first(),
            record.pre_submission_before_launch_execution(),
        ] {
            assert_eq!(feature.qualification, Qualification::Unqualified, "{id:?}");
            assert!(!feature.established(), "{id:?}");
        }
    }
    // 48Club `noMerge`: offered by the API, usable by a diagnostic plan,
    // not established for a release.
    let no_merge = CapabilityId::Club48BuilderV1.record().no_merge();
    assert!(no_merge.enabled());
    assert!(!no_merge.established());
}

#[test]
fn every_feature_a_release_plan_uses_must_be_qualified() {
    use Qualification::Unqualified;
    let club48 = CapabilityId::Club48BuilderV1.record().release_grade();
    let blockrazor = CapabilityId::BlockRazorBuilderV1.record().release_grade();

    // Lifecycle, send boundary and the used features all established.
    assert!(release_with(club48, blockrazor, |_| ()).is_ok());

    // 48Club: each feature the profile makes it use, enabled but not
    // qualified, refuses the release.
    assert_eq!(
        release_with(club48.no_merge_qualified(Unqualified), blockrazor, |_| ()),
        refused(0, ReleaseBlocker::NoMergeNotEstablished)
    );
    assert_eq!(
        release_with(
            club48.position_first_qualified(Unqualified),
            blockrazor,
            |_| ()
        ),
        refused(0, ReleaseBlocker::PositionFirstNotEstablished)
    );
    // Every lifecycle property qualified, the expiry enforcement not.
    assert_eq!(
        release_with(
            club48.max_block_number_qualified(Unqualified),
            blockrazor,
            |_| ()
        ),
        refused(0, ReleaseBlocker::ExpiryByBlockNotEstablished)
    );

    // BlockRazor: the same holds for what the plan sends to it.
    assert_eq!(
        release_with(club48, blockrazor.no_merge_qualified(Unqualified), |_| ()),
        refused(1, ReleaseBlocker::NoMergeNotEstablished)
    );
    assert_eq!(
        release_with(
            club48,
            blockrazor.max_block_number_qualified(Unqualified),
            |_| ()
        ),
        refused(1, ReleaseBlocker::ExpiryByBlockNotEstablished)
    );
}

#[test]
fn a_feature_the_plan_does_not_use_never_blocks_a_release() {
    use Qualification::Unqualified;
    let club48 = CapabilityId::Club48BuilderV1.record().release_grade();
    let blockrazor = CapabilityId::BlockRazorBuilderV1.record().release_grade();

    // `positionFirst` is never sent to BlockRazor.
    assert!(
        release_with(
            club48,
            blockrazor.position_first_qualified(Unqualified),
            |_| ()
        )
        .is_ok()
    );
    // No timestamp bound in the plan: unqualified timestamps do not matter.
    let untimed = blockrazor
        .min_timestamp_qualified(Unqualified)
        .max_timestamp_qualified(Unqualified);
    assert!(release_with(club48, untimed, |_| ()).is_ok());
    // No pre-submission in the plan: unqualified pre-submission does not
    // matter.
    let no_pre = club48.pre_submission_qualified(Unqualified);
    assert!(release_with(no_pre, blockrazor, |_| ()).is_ok());
    // BlockRazor alone under the provider default: `noMerge` is not sent,
    // so its qualification is not required.
    let (ticket, expected) = public_intent();
    let lone = blockrazor.no_merge_qualified(Unqualified);
    let records = [(adapter(1), lone)];
    let table = AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &records).unwrap();
    let mut inputs = public_inputs(ticket);
    inputs.merge = MergePolicy::ProviderDefault;
    inputs.adapters = set(&[1]);
    let plan = SubmissionPlan::new(&table, &expected, &budget(), inputs).unwrap();
    assert!(plan.qualify_for_release(&table).is_ok());
}

#[test]
fn a_used_timestamp_bound_must_be_qualified() {
    use Qualification::{Qualified, Unqualified};
    let club48 = CapabilityId::Club48BuilderV1.record().release_grade();
    let blockrazor = CapabilityId::BlockRazorBuilderV1.record().release_grade();
    let with_max_timestamp = |i: &mut PlanInputs| {
        i.provider_validity = Some(ProviderValidity {
            max_block_number: MAX_BLOCK,
            min_timestamp: None,
            max_timestamp: Some(1_900_000_000),
        });
    };

    // Enabled on both builders, qualified on neither.
    assert_eq!(
        release_with(club48, blockrazor, with_max_timestamp),
        refused(0, ReleaseBlocker::MaxTimestampNotEstablished)
    );
    assert_eq!(
        release_with(
            club48.max_timestamp_qualified(Qualified),
            blockrazor.max_timestamp_qualified(Unqualified),
            with_max_timestamp
        ),
        refused(1, ReleaseBlocker::MaxTimestampNotEstablished)
    );
    assert!(
        release_with(
            club48.max_timestamp_qualified(Qualified),
            blockrazor.max_timestamp_qualified(Qualified),
            with_max_timestamp
        )
        .is_ok()
    );

    // A minimum timestamp, through BlockRazor alone (48Club does not
    // offer it).
    let with_min_timestamp = |i: &mut PlanInputs| {
        i.adapters = set(&[1]);
        i.provider_validity = Some(ProviderValidity {
            max_block_number: MAX_BLOCK,
            min_timestamp: Some(1_700_000_000),
            max_timestamp: None,
        });
    };
    assert_eq!(
        release_with(club48, blockrazor, with_min_timestamp),
        refused(1, ReleaseBlocker::MinTimestampNotEstablished)
    );
    assert!(
        release_with(
            club48,
            blockrazor.min_timestamp_qualified(Qualified),
            with_min_timestamp
        )
        .is_ok()
    );
}

#[test]
fn release_grade_qualifies_features_explicitly() {
    // The test model does not let "enabled" stand for "qualified": the
    // frozen records have the features enabled and unqualified; the
    // release-grade copy has exactly the three bundle features qualified.
    let frozen = *CapabilityId::Club48BuilderV1.record();
    let grade = frozen.release_grade();
    for (before, after) in [
        (frozen.no_merge(), grade.no_merge()),
        (frozen.position_first(), grade.position_first()),
        (frozen.max_block_number(), grade.max_block_number()),
    ] {
        assert!(before.enabled());
        assert!(!before.established());
        assert_eq!(after.support, before.support);
        assert!(after.established());
    }
    // Not part of it: timestamps and pre-submission stay unqualified.
    assert!(!grade.max_timestamp().established());
    assert!(!grade.pre_submission_before_launch_execution().established());

    // A feature that is not enabled cannot be established by qualifying
    // it.
    let blockrazor = CapabilityId::BlockRazorBuilderV1.record().release_grade();
    assert_eq!(blockrazor.position_first().support, Support::Unqualified);
    assert!(!blockrazor.position_first().established());
}

// ---------------------------------------------------------------------------
// Transport start deadline: a time limit, not a block bound
// ---------------------------------------------------------------------------

#[test]
fn transport_deadline_is_strict_and_bound_to_its_clock() {
    const CLOCK: u32 = 4;
    let profile = ReleaseProfile::BirthEntryV1;
    let origin = 10_000_000_000u64;
    let deadline = TransportStartDeadline::for_profile(profile, CLOCK, origin, 1_000_000).unwrap();
    assert_eq!(deadline.profile(), profile);
    assert_eq!(deadline.limit_ns(), 1_000_000);

    // 999,999 ns after the origin a write may start; at 1,000,000 ns and
    // beyond it is a miss.
    assert_eq!(deadline.check(CLOCK, origin), Ok(0));
    assert_eq!(deadline.check(CLOCK, origin + 999_999), Ok(999_999));
    assert_eq!(
        deadline.check(CLOCK, origin + 1_000_000),
        Err(TransportDeadlineError::Missed)
    );
    assert_eq!(
        deadline.check(CLOCK, origin + 1_000_001),
        Err(TransportDeadlineError::Missed)
    );
    assert_eq!(
        deadline.check(CLOCK, origin + 300_000_000),
        Err(TransportDeadlineError::Missed)
    );

    // A time from another clock, or from before the origin, is refused.
    assert_eq!(
        deadline.check(CLOCK + 1, origin + 1),
        Err(TransportDeadlineError::OtherClock)
    );
    assert_eq!(
        deadline.check(CLOCK, origin - 1),
        Err(TransportDeadlineError::BeforeOrigin)
    );

    // Near the end of the clock range nothing overflows.
    let late =
        TransportStartDeadline::for_profile(profile, CLOCK, u64::MAX - 10, 1_000_000).unwrap();
    assert_eq!(late.check(CLOCK, u64::MAX), Ok(10));
}

#[test]
fn transport_deadline_cannot_be_wider_than_the_profile_allows() {
    const CLOCK: u32 = 4;
    let profile = ReleaseProfile::BirthEntryV1;
    assert_eq!(profile.max_transport_start_limit_ns(), 1_000_000);
    assert_eq!(LOCAL_REACTION_LIMIT_NS, 1_000_000);
    let make = |limit_ns: u64| TransportStartDeadline::for_profile(profile, CLOCK, 0, limit_ns);

    // A tighter limit and the profile's own limit are allowed.
    assert_eq!(make(999_999).map(|d| d.limit_ns()), Ok(999_999));
    assert_eq!(make(1_000_000).map(|d| d.limit_ns()), Ok(1_000_000));
    assert_eq!(make(500_000).map(|d| d.limit_ns()), Ok(500_000));
    // One nanosecond more, 2 ms or anything larger cannot be made.
    for limit_ns in [1_000_001, 2_000_000, u64::MAX] {
        assert_eq!(
            make(limit_ns),
            Err(TransportDeadlineError::LimitAboveProfile),
            "{limit_ns}"
        );
    }
    assert_eq!(make(0), Err(TransportDeadlineError::ZeroLimit));

    // A tighter deadline is strict at its own limit.
    let tight = make(500_000).unwrap();
    assert_eq!(tight.check(CLOCK, 499_999), Ok(499_999));
    assert_eq!(
        tight.check(CLOCK, 500_000),
        Err(TransportDeadlineError::Missed)
    );
}

#[test]
fn the_block_bound_says_nothing_about_elapsed_time() {
    // Within one head the block bound is satisfied however late a write
    // would start; only the transport deadline sees the time.
    let plan = plan_with(|_| ()).unwrap();
    let profile = ReleaseProfile::BirthEntryV1;
    let deadline =
        TransportStartDeadline::for_profile(profile, 1, 0, LOCAL_REACTION_LIMIT_NS).unwrap();
    let three_hundred_ms = 300_000_000u64;
    assert!(plan.local_block_start_allowed(LAST_START_HEAD));
    assert_eq!(
        deadline.check(1, three_hundred_ms),
        Err(TransportDeadlineError::Missed)
    );
}

// ---------------------------------------------------------------------------
// After-handoff validity
// ---------------------------------------------------------------------------

#[test]
fn expiry_before_the_first_write_is_not_a_disclosure() {
    for state in [
        SubmissionState::NotSelected,
        SubmissionState::NotStarted,
        SubmissionState::Suppressed,
    ] {
        for provider_expired in [false, true] {
            let view = disclosure(state, provider_expired);
            assert_eq!(view, Disclosure::BeforeFirstWrite);
            assert!(!view.bytes_may_have_left());
            assert!(!view.may_still_execute());
        }
    }
}

#[test]
fn nothing_revokes_bytes_that_may_have_left() {
    let disclosed = [
        (SubmissionState::Writing, Disclosure::Writing),
        (SubmissionState::HandedOff, Disclosure::HandedOff),
        (SubmissionState::Acked, Disclosure::Acked),
        (SubmissionState::Rejected, Disclosure::Rejected),
        (SubmissionState::Unknown, Disclosure::Unknown),
    ];
    for (state, expected) in disclosed {
        let view = disclosure(state, false);
        assert_eq!(view, expected);
        assert!(view.bytes_may_have_left());
        // A rejection is no revocation either.
        assert!(view.may_still_execute());

        // After the provider validity passed it is recorded as expired
        // after disclosure, never as safely unsent.
        let expired = disclosure(state, true);
        assert_eq!(expired, Disclosure::ExpiredAfterDisclosure);
        assert!(expired.bytes_may_have_left());
        assert!(expired.may_still_execute());
    }
}

#[test]
fn one_adapter_expired_unstarted_while_another_disclosed() {
    // Adapter A handed off before the expiry, adapter B never started.
    let a = disclosure(SubmissionState::HandedOff, true);
    let b = disclosure(SubmissionState::NotStarted, true);
    assert_eq!(a, Disclosure::ExpiredAfterDisclosure);
    assert_eq!(b, Disclosure::BeforeFirstWrite);
    // The intent is unresolved because of A, whatever B did not do.
    assert!([a, b].iter().any(|view| view.may_still_execute()));
}

/// The findings that establish a birth entry on a builder, minus `skip`.
fn birth_entry_findings(
    skip: Option<super::qualification::PropertyName>,
) -> Vec<super::qualification::Finding> {
    use super::qualification::{FeatureName, Finding, PropertyName};
    [
        (PropertyName::PreservesOrder, Knowledge::Yes),
        (PropertyName::Atomic, Knowledge::Yes),
        (PropertyName::SameBlock, Knowledge::Yes),
        (PropertyName::ExtractsTransactions, Knowledge::No),
        (PropertyName::ProviderRebroadcast, Knowledge::No),
        (PropertyName::ProviderPublicFallback, Knowledge::No),
        (PropertyName::ReachesOtherBuilders, Knowledge::No),
        (PropertyName::PublicMempoolExposure, Knowledge::No),
    ]
    .into_iter()
    .filter(|(p, _)| Some(*p) != skip)
    .map(|(p, k)| Finding::Property(p, k))
    .chain([
        Finding::Feature(FeatureName::NoMerge),
        Finding::Feature(FeatureName::PositionFirst),
        Finding::Feature(FeatureName::MaxBlockNumber),
        Finding::SendTime,
    ])
    .collect()
}

/// Plan 28.10.: what this product measured about an adapter establishes
/// what its frozen record cannot, and exactly that; without it nothing is
/// released, as before.
#[test]
fn a_measured_qualification_releases_exactly_what_it_establishes() {
    use super::qualification::{AdapterQualification, FeatureName, Finding, PropertyName};
    let (ticket, expected) = public_intent();
    let frozen = table();
    let plan = SubmissionPlan::new(&frozen, &expected, &budget(), public_inputs(ticket)).unwrap();
    let q = |id: CapabilityId, findings: &[Finding]| {
        AdapterQualification::from_findings(id, findings).unwrap()
    };
    let club48 = q(CapabilityId::Club48BuilderV1, &birth_entry_findings(None));
    let blockrazor = q(
        CapabilityId::BlockRazorBuilderV1,
        &birth_entry_findings(None),
    );
    assert_eq!(
        plan.qualify_for_release_with(&frozen, &[]),
        Err(ReleaseRefusal {
            adapter: Some(adapter(0)),
            blocker: ReleaseBlocker::OrderNotEstablished,
        })
    );
    assert_eq!(
        plan.qualify_for_release_with(&frozen, std::slice::from_ref(&club48)),
        Err(ReleaseRefusal {
            adapter: Some(adapter(1)),
            blocker: ReleaseBlocker::OrderNotEstablished,
        })
    );
    let release = plan
        .qualify_for_release_with(&frozen, &[club48.clone(), blockrazor.clone()])
        .unwrap();
    assert_eq!(*release.plan(), plan);
    // Each property not measured blocks with its own reason.
    for (property, blocker) in [
        (
            PropertyName::PreservesOrder,
            ReleaseBlocker::OrderNotEstablished,
        ),
        (
            PropertyName::Atomic,
            ReleaseBlocker::AtomicityNotEstablished,
        ),
        (
            PropertyName::SameBlock,
            ReleaseBlocker::SameBlockNotEstablished,
        ),
        (
            PropertyName::ExtractsTransactions,
            ReleaseBlocker::ExtractionNotExcluded,
        ),
        (
            PropertyName::ProviderRebroadcast,
            ReleaseBlocker::RebroadcastNotExcluded,
        ),
        (
            PropertyName::ProviderPublicFallback,
            ReleaseBlocker::PublicFallbackNotExcluded,
        ),
        (
            PropertyName::ReachesOtherBuilders,
            ReleaseBlocker::DownstreamNotExcluded,
        ),
        (
            PropertyName::PublicMempoolExposure,
            ReleaseBlocker::PublicExposureNotExcluded,
        ),
    ] {
        let partial = q(
            CapabilityId::Club48BuilderV1,
            &birth_entry_findings(Some(property)),
        );
        assert_eq!(
            plan.qualify_for_release_with(&frozen, &[partial, blockrazor.clone()])
                .err()
                .map(|r| r.blocker),
            Some(blocker),
            "{}",
            property.name()
        );
    }
    // A property measured the unwanted way blocks as well.
    let mut leaky = birth_entry_findings(Some(PropertyName::ExtractsTransactions));
    leaky.push(Finding::Property(
        PropertyName::ExtractsTransactions,
        Knowledge::Yes,
    ));
    assert_eq!(
        plan.qualify_for_release_with(
            &frozen,
            &[q(CapabilityId::Club48BuilderV1, &leaky), blockrazor.clone()]
        )
        .err()
        .map(|r| r.blocker),
        Some(ReleaseBlocker::ExtractionNotExcluded)
    );
    // A feature the plan uses must be qualified, and so must the boundary.
    for (missing, blocker) in [
        (
            Finding::Feature(FeatureName::NoMerge),
            ReleaseBlocker::NoMergeNotEstablished,
        ),
        (
            Finding::Feature(FeatureName::PositionFirst),
            ReleaseBlocker::PositionFirstNotEstablished,
        ),
        (
            Finding::Feature(FeatureName::MaxBlockNumber),
            ReleaseBlocker::ExpiryByBlockNotEstablished,
        ),
        (Finding::SendTime, ReleaseBlocker::SendTimeNotQualified),
    ] {
        let without: Vec<_> = birth_entry_findings(None)
            .into_iter()
            .filter(|f| *f != missing)
            .collect();
        assert_eq!(
            plan.qualify_for_release_with(
                &frozen,
                &[
                    q(CapabilityId::Club48BuilderV1, &without),
                    blockrazor.clone()
                ]
            )
            .err()
            .map(|r| r.blocker),
            Some(blocker),
            "{}",
            missing.name()
        );
    }
    // A qualification of another record does not count for this one.
    let local = q(CapabilityId::LocalGethV1, &[Finding::SendTime]);
    assert!(
        plan.qualify_for_release_with(&frozen, &[local, blockrazor])
            .is_err()
    );
}

/// A single transaction (approval, SELL, a BUY on canonical state) claims
/// no placement: its mode and its send boundary must be qualified.
#[test]
fn a_single_transaction_needs_its_mode_and_send_boundary_qualified() {
    use super::qualification::{AdapterQualification, Finding};
    let single = SubmissionMode::SingleRawBuy;
    let profile = ReleaseProfile::SingleRawV1;
    let club48 = CapabilityId::Club48BuilderV1.record();
    let local = CapabilityId::LocalGethV1.record();
    let q = |id: CapabilityId, findings: &[Finding]| {
        AdapterQualification::from_findings(id, findings).unwrap()
    };
    assert_eq!(
        profile.release_blocker_with(club48, None, single),
        Some(ReleaseBlocker::ModeNotEnabled)
    );
    let mode = q(CapabilityId::Club48BuilderV1, &[Finding::Mode(single)]);
    assert_eq!(
        profile.release_blocker_with(club48, Some(&mode), single),
        Some(ReleaseBlocker::SendTimeNotQualified)
    );
    let both = q(
        CapabilityId::Club48BuilderV1,
        &[Finding::Mode(single), Finding::SendTime],
    );
    assert_eq!(
        profile.release_blocker_with(club48, Some(&both), single),
        None
    );
    assert_eq!(
        profile.release_blocker_with(club48, Some(&both), SubmissionMode::PublicFullLaunchBundle),
        Some(ReleaseBlocker::ModeNotEnabled)
    );
    assert_eq!(
        profile.release_blocker_with(local, None, single),
        Some(ReleaseBlocker::SendTimeNotQualified)
    );
    assert_eq!(
        profile.release_blocker_with(local, Some(&both), single),
        Some(ReleaseBlocker::SendTimeNotQualified),
        "another record's qualification is not this one's"
    );
    assert_eq!(
        ReleaseProfile::BirthEntryV1.release_blocker_with(club48, Some(&both), single),
        Some(ReleaseBlocker::NoOrderedPlacement)
    );
}

/// Findings read as they are named; what a record never offers cannot be
/// qualified, and nothing is measured twice.
#[test]
fn findings_cannot_qualify_what_a_record_never_offers() {
    use super::qualification::{AdapterQualification, FeatureName, Finding, PropertyName};
    for finding in birth_entry_findings(None).into_iter().chain([
        Finding::Mode(SubmissionMode::SingleRawBuy),
        Finding::Mode(SubmissionMode::PrivateTarget),
        Finding::Feature(FeatureName::PreSubmission),
        Finding::Property(PropertyName::Atomic, Knowledge::No),
    ]) {
        assert_eq!(
            Finding::parse(&finding.name()),
            Some(finding),
            "{}",
            finding.name()
        );
    }
    for bad in [
        "PROPERTY:ATOMIC=MAYBE",
        "PROPERTY:ATOMIC",
        "FEATURE:X",
        "MODE:X",
        "SEND",
    ] {
        assert_eq!(Finding::parse(bad), None, "{bad}");
    }
    let local = CapabilityId::LocalGethV1;
    assert!(
        AdapterQualification::from_findings(local, &[Finding::Feature(FeatureName::NoMerge)])
            .is_err()
    );
    assert!(
        AdapterQualification::from_findings(
            local,
            &[Finding::Mode(SubmissionMode::PublicFullLaunchBundle)]
        )
        .is_err()
    );
    let club48 = CapabilityId::Club48BuilderV1;
    assert!(
        AdapterQualification::from_findings(club48, &[Finding::SendTime, Finding::SendTime])
            .is_err()
    );
    assert!(
        AdapterQualification::from_findings(
            club48,
            &[
                Finding::Property(PropertyName::Atomic, Knowledge::Yes),
                Finding::Property(PropertyName::Atomic, Knowledge::No)
            ]
        )
        .is_err()
    );
}

/// Plan 28.10.: a builder's single transaction is formed once this product
/// qualified the mode for that builder, and released under `SingleRawV1`
/// once its send boundary and the expiry it relies on are qualified;
/// another builder's qualification forms nothing.
#[test]
fn a_qualified_single_transaction_forms_through_its_builder() {
    use super::qualification::{AdapterQualification, FeatureName, Finding};
    let single = SubmissionMode::SingleRawBuy;
    let (ticket, expected) = public_intent();
    let mut inputs = public_inputs(ticket);
    inputs.mode = single;
    inputs.launch = Launch::None;
    inputs.profile = ReleaseProfile::SingleRawV1;
    inputs.adapters = set(&[0]);
    inputs.merge = MergePolicy::ProviderDefault;
    let q = |id: CapabilityId, findings: &[Finding]| {
        AdapterQualification::from_findings(id, findings).unwrap()
    };
    let mode_only = q(
        CapabilityId::Club48BuilderV1,
        &[Finding::Mode(single), Finding::SendTime],
    );
    let full = q(
        CapabilityId::Club48BuilderV1,
        &[
            Finding::Mode(single),
            Finding::SendTime,
            Finding::Feature(FeatureName::MaxBlockNumber),
        ],
    );
    let other = q(
        CapabilityId::BlockRazorBuilderV1,
        &[Finding::Mode(single), Finding::SendTime],
    );
    let new = |quals: &[AdapterQualification]| {
        SubmissionPlan::new_with(&table(), quals, &expected, &budget(), inputs)
    };
    assert_eq!(
        SubmissionPlan::new(&table(), &expected, &budget(), inputs).err(),
        Some(PlanError::ModeNotEnabled)
    );
    assert_eq!(new(&[other]).err(), Some(PlanError::ModeNotEnabled));
    let plan = new(std::slice::from_ref(&mode_only)).unwrap();
    assert_eq!(plan.launch(), Launch::None);
    assert_eq!(
        plan.qualify_for_release(&table()).err().map(|r| r.blocker),
        Some(ReleaseBlocker::ModeNotEnabled)
    );
    assert_eq!(
        plan.qualify_for_release_with(&table(), &[mode_only])
            .err()
            .map(|r| r.blocker),
        Some(ReleaseBlocker::ExpiryByBlockNotEstablished)
    );
    assert!(plan.qualify_for_release_with(&table(), &[full]).is_ok());
}
