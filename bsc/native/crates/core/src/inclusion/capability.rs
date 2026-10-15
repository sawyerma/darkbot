//! Versioned, frozen capability records of the inclusion adapters.
//!
//! A record states what an adapter may be used for. Records exist only as
//! the constants of this module, are reached only through a
//! [`CapabilityId`] and have no public fields and no public constructor:
//! production code can neither build nor alter one. Changing a record means
//! adding a new version here, which is a reviewed change.
//!
//! A feature is used only when its record says [`Support::Enabled`]. A
//! feature that a provider documents, or that the Gate-0 harness once
//! sent, but whose exact meaning is not verified for this product is
//! [`Support::Unqualified`] and is never sent.
//!
//! `Enabled` means: stated by the provider's official reference, as frozen
//! in the evidence snapshot the record names, and admitted for use. It is
//! not a measurement and not a qualification of real behaviour.
//!
//! Lifecycle properties keep two things apart ([`Property`]): what the
//! frozen evidence *states* ([`Knowledge`]) and whether this product
//! *qualified* that very property ([`Qualification`]). A documented "yes"
//! is enough to build and test an adapter. Only a statement that is also
//! qualified counts for a release.
//!
//! The same holds for the provider features a request relies on
//! ([`Feature`]): expiry by block and by timestamp, `noMerge`,
//! `positionFirst` and pre-submission. An enabled feature may be used by a
//! diagnostic plan. A release plan needs every feature it actually uses to
//! be enabled *and* qualified.
//!
//! No property and no feature of any record is qualified today.
//!
//! Three facts hold for every record and are stated explicitly:
//!
//! - no adapter offers a cancellation;
//! - no provider expiry is evidence that signed bytes which were already
//!   disclosed can no longer execute;
//! - no handoff boundary is qualified as the product's send time yet.

use crate::intent::{AdapterId, MAX_ADAPTERS};

/// Chain of the current release scope: BSC mainnet.
const BSC_MAINNET: u64 = 56;

/// The kind of an inclusion adapter. Two paths of one company are two
/// kinds when they differ in where the signed bytes go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    /// The local pinned geth node.
    LocalGeth,
    /// 48Club puissant builder, submitted to directly.
    Club48Builder,
    /// BlockRazor block builder, submitted to directly.
    BlockRazorBuilder,
    /// BlockRazor RPC, which forwards a bundle to several builders.
    BlockRazorRpc,
}

/// Identifier of one frozen capability record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityId {
    /// Local pinned geth node, version 1.
    LocalGethV1,
    /// 48Club puissant builder, version 1.
    Club48BuilderV1,
    /// BlockRazor block builder (direct), version 1.
    BlockRazorBuilderV1,
    /// BlockRazor RPC, version 1.
    BlockRazorRpcV1,
}

impl CapabilityId {
    /// The frozen record behind this identifier.
    pub fn record(self) -> &'static CapabilityRecord {
        match self {
            Self::LocalGethV1 => &LOCAL_GETH_V1,
            Self::Club48BuilderV1 => &CLUB48_BUILDER_V1,
            Self::BlockRazorBuilderV1 => &BLOCKRAZOR_BUILDER_V1,
            Self::BlockRazorRpcV1 => &BLOCKRAZOR_RPC_V1,
        }
    }
}

/// How far a feature may be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// Stated by the official reference and admitted for use.
    Enabled,
    /// Documented or historically used, but not verified for this
    /// product. Disabled.
    Unqualified,
    /// Not offered by the adapter.
    Unsupported,
}

/// What is submitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionMode {
    /// The signed BUY alone.
    SingleRawBuy,
    /// The original signed launch of a third party, unchanged, followed by
    /// the signed BUY. The launch is never re-signed and its cost is not
    /// this product's.
    PublicFullLaunchBundle,
    /// This product's own prepared signed launch followed by the signed
    /// BUY; the launch's cost belongs to the budget. This is the form of
    /// the request only. Whether it may be sent before the launch has been
    /// broadcast or executed is a separate capability,
    /// [`CapabilityRecord::pre_submission_before_launch_execution`].
    ControlledSelfLaunchBundle,
    /// A BUY behind a private target the adapter identifies itself.
    PrivateTarget,
}

/// The local boundary at which an adapter's write is complete, as far as
/// it is defined today. A boundary is a diagnostic measuring point; whether
/// it may stand for the product's send time is a separate, per-record
/// statement ([`CapabilityRecord::qualified_as_t_send`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffBoundary {
    /// The userspace write and flush on the adapter's TLS stream returned.
    /// Says nothing about the kernel, the NIC or the wire.
    TlsUserspaceWriteFlushReturned,
    /// The native outbound handoff of the pinned geth node, which phase C1
    /// builds.
    DeferredToNativeGeth,
    /// No transport is defined for this adapter kind.
    NotDefined,
}

/// What the frozen evidence says about a yes-or-no property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knowledge {
    /// Stated, or true by construction.
    Yes,
    /// Stated not to be the case, or false by construction.
    No,
    /// Not stated. Nothing may be built on either answer.
    Unknown,
}

/// Whether a property was established by this product's own qualification
/// of the adapter on the target system.
///
/// Independent of [`Knowledge`]: a provider's reference can state a
/// property without it ever having been observed, and an observation is
/// worthless for a property nothing is stated about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qualification {
    /// Established by a recorded qualification of exactly this property.
    Qualified,
    /// Not qualified. The property is at most documented.
    Unqualified,
}

/// One lifecycle property of an adapter: what the frozen evidence states,
/// and whether that statement was qualified. Each property carries its own
/// qualification; there is no adapter-wide "qualified" flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Property {
    /// What the frozen evidence states.
    pub knowledge: Knowledge,
    /// Whether this very property was qualified.
    pub qualification: Qualification,
}

impl Property {
    /// Whether the property is stated as `wanted` and that was qualified.
    /// Only this counts for a release; a documented statement alone does
    /// not, and neither does a qualification of something unknown.
    #[inline]
    pub fn established(self, wanted: Knowledge) -> bool {
        self.knowledge == wanted && self.qualification == Qualification::Qualified
    }
}

/// A property as far as it is only documented: nothing is qualified.
const fn documented(knowledge: Knowledge) -> Property {
    Property {
        knowledge,
        qualification: Qualification::Unqualified,
    }
}

/// One provider feature of an adapter: whether the provider's API offers
/// it, and whether this product qualified how the provider really treats
/// it. Each feature carries its own qualification.
///
/// `support` alone decides whether a diagnostic plan may use the feature,
/// so that an adapter can be built and tested. For a release a feature the
/// plan relies on must be [`Feature::established`]: enabled *and*
/// qualified. `Support::Enabled` is a statement about the provider's
/// reference, never about its behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Feature {
    /// What the provider's API offers, per the frozen evidence.
    pub support: Support,
    /// Whether the provider's real treatment of this feature was qualified.
    pub qualification: Qualification,
}

impl Feature {
    /// Whether a diagnostic plan may use the feature.
    #[inline]
    pub fn enabled(self) -> bool {
        self.support == Support::Enabled
    }

    /// Whether the feature is enabled and its real behaviour qualified.
    /// Only this counts for a release.
    #[inline]
    pub fn established(self) -> bool {
        self.enabled() && self.qualification == Qualification::Qualified
    }
}

/// A feature as far as the provider's reference goes: nothing qualified.
const fn offered(support: Support) -> Feature {
    Feature {
        support,
        qualification: Qualification::Unqualified,
    }
}

/// What is known about a provider's gas-price rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeeFloor {
    /// No provider reference exists for this adapter. The own node's
    /// txpool rule is configuration, not recorded here.
    NotRecorded,
    /// The provider documents that it checks gas prices, but the exact
    /// rule, or an explicit numeric minimum, is not established for this
    /// product. No rule is enforced or assumed. This never means that no
    /// rule exists.
    DocumentedRuleUnqualified,
}

/// One frozen piece of provider evidence: a small structured snapshot of
/// the statements a record relies on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderEvidence {
    /// Repository path of the snapshot.
    pub fixture: &'static str,
    /// SHA-256 of the snapshot file.
    pub fixture_sha256: &'static str,
    /// Date on which the provider's reference was read.
    pub retrieved_on: &'static str,
}

/// The frozen capability record of one adapter kind. Reached only through
/// [`CapabilityId::record`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityRecord {
    id: CapabilityId,
    kind: AdapterKind,
    version: u32,
    chain_id: u64,
    evidence: &'static [ProviderEvidence],
    single_raw_buy: Support,
    public_full_launch_bundle: Support,
    controlled_self_launch_bundle: Support,
    private_target: Support,
    max_block_number: Feature,
    min_timestamp: Feature,
    max_timestamp: Feature,
    allowed_reverts: Support,
    no_merge: Feature,
    position_first: Feature,
    pre_submission_before_launch_execution: Feature,
    cancellation: Support,
    max_txs: u8,
    fee_floor: FeeFloor,
    expiry_revokes_disclosed_bytes: bool,
    preserves_order: Property,
    atomic: Property,
    same_block: Property,
    extracts_transactions: Property,
    provider_rebroadcast: Property,
    provider_public_fallback: Property,
    reaches_other_builders: Property,
    public_mempool_exposure: Property,
    reorg_post_inclusion_public_exposure: Property,
    handoff_boundary: HandoffBoundary,
    qualified_as_t_send: bool,
}

impl CapabilityRecord {
    /// Identifier of this record.
    pub fn id(&self) -> CapabilityId {
        self.id
    }

    /// Adapter kind the record describes.
    pub fn kind(&self) -> AdapterKind {
        self.kind
    }

    /// Version of this record.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The one chain this record is valid for.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Frozen provider evidence the record relies on.
    pub fn evidence(&self) -> &'static [ProviderEvidence] {
        self.evidence
    }

    /// Support of a submission mode.
    pub fn mode(&self, mode: SubmissionMode) -> Support {
        match mode {
            SubmissionMode::SingleRawBuy => self.single_raw_buy,
            SubmissionMode::PublicFullLaunchBundle => self.public_full_launch_bundle,
            SubmissionMode::ControlledSelfLaunchBundle => self.controlled_self_launch_bundle,
            SubmissionMode::PrivateTarget => self.private_target,
        }
    }

    /// Provider validity up to a block number.
    pub fn max_block_number(&self) -> Feature {
        self.max_block_number
    }

    /// Provider validity from a timestamp.
    pub fn min_timestamp(&self) -> Feature {
        self.min_timestamp
    }

    /// Provider validity up to a timestamp.
    pub fn max_timestamp(&self) -> Feature {
        self.max_timestamp
    }

    /// List of transactions that may revert.
    pub fn allowed_reverts(&self) -> Support {
        self.allowed_reverts
    }

    /// Request that the bundle is not merged with others.
    pub fn no_merge(&self) -> Feature {
        self.no_merge
    }

    /// Request strict ordering by priority fee.
    pub fn position_first(&self) -> Feature {
        self.position_first
    }

    /// Sending a controlled self-launch bundle before its launch has been
    /// broadcast or executed.
    pub fn pre_submission_before_launch_execution(&self) -> Feature {
        self.pre_submission_before_launch_execution
    }

    /// Cancellation of a submitted request.
    pub fn cancellation(&self) -> Support {
        self.cancellation
    }

    /// Largest number of transactions this record admits in one request.
    pub fn max_txs(&self) -> u8 {
        self.max_txs
    }

    /// What is known about the provider's gas-price rule.
    pub fn fee_floor(&self) -> FeeFloor {
        self.fee_floor
    }

    /// Whether a provider expiry is evidence that disclosed bytes can no
    /// longer execute. `false` for every record.
    pub fn expiry_revokes_disclosed_bytes(&self) -> bool {
        self.expiry_revokes_disclosed_bytes
    }

    /// Whether the transactions of a request are executed in the order
    /// submitted.
    pub fn preserves_order(&self) -> Property {
        self.preserves_order
    }

    /// Whether the transactions of a request are included all together or
    /// not at all.
    pub fn atomic(&self) -> Property {
        self.atomic
    }

    /// Whether the transactions of a request are included in one block.
    pub fn same_block(&self) -> Property {
        self.same_block
    }

    /// Whether the provider may take single transactions out of a request
    /// and use them on their own.
    pub fn extracts_transactions(&self) -> Property {
        self.extracts_transactions
    }

    /// Whether the provider itself rebroadcasts the transactions.
    pub fn provider_rebroadcast(&self) -> Property {
        self.provider_rebroadcast
    }

    /// Whether the provider itself falls back to a public path, such as
    /// the public mempool, for a request it cannot place.
    pub fn provider_public_fallback(&self) -> Property {
        self.provider_public_fallback
    }

    /// Whether the signed bytes reach other builders than the one
    /// submitted to: the downstream recipients.
    pub fn reaches_other_builders(&self) -> Property {
        self.reaches_other_builders
    }

    /// Whether submitting through this adapter makes the signed bytes
    /// public in normal operation.
    pub fn public_mempool_exposure(&self) -> Property {
        self.public_mempool_exposure
    }

    /// Whether transactions that were already included become public when
    /// their block is reorganised away. This is a property of the chain
    /// as the provider describes it, not a provider leak: it concerns
    /// recovery and reconciliation, and it is neither a rebroadcast nor a
    /// public fallback by the provider.
    pub fn reorg_post_inclusion_public_exposure(&self) -> Property {
        self.reorg_post_inclusion_public_exposure
    }

    /// The local boundary at which the adapter's write is complete.
    pub fn handoff_boundary(&self) -> HandoffBoundary {
        self.handoff_boundary
    }

    /// Whether that boundary is qualified to stand for the product's send
    /// time. `false` for every record: until an adapter is qualified on
    /// the target system its actual send time is UNKNOWN.
    pub fn qualified_as_t_send(&self) -> bool {
        self.qualified_as_t_send
    }
}

/// Local pinned geth node.
///
/// Defined here is only what is submitted: the one signed BUY, alone. That
/// exposes it to the public mempool, and thereby to every builder, so this
/// adapter is never a fallback for a bundle. Its transport is the native
/// outbound handoff that phase C1 builds; no JSON-RPC call is declared to
/// be the production boundary and no interim adapter exists.
static LOCAL_GETH_V1: CapabilityRecord = CapabilityRecord {
    id: CapabilityId::LocalGethV1,
    kind: AdapterKind::LocalGeth,
    version: 1,
    chain_id: BSC_MAINNET,
    evidence: &[],
    single_raw_buy: Support::Enabled,
    public_full_launch_bundle: Support::Unsupported,
    controlled_self_launch_bundle: Support::Unsupported,
    private_target: Support::Unsupported,
    max_block_number: offered(Support::Unsupported),
    min_timestamp: offered(Support::Unsupported),
    max_timestamp: offered(Support::Unsupported),
    allowed_reverts: Support::Unsupported,
    no_merge: offered(Support::Unsupported),
    position_first: offered(Support::Unsupported),
    pre_submission_before_launch_execution: offered(Support::Unsupported),
    cancellation: Support::Unsupported,
    max_txs: 1,
    fee_floor: FeeFloor::NotRecorded,
    expiry_revokes_disclosed_bytes: false,
    preserves_order: documented(Knowledge::Unknown),
    atomic: documented(Knowledge::Unknown),
    same_block: documented(Knowledge::Unknown),
    extracts_transactions: documented(Knowledge::Unknown),
    provider_rebroadcast: documented(Knowledge::Yes),
    provider_public_fallback: documented(Knowledge::Yes),
    reaches_other_builders: documented(Knowledge::Yes),
    public_mempool_exposure: documented(Knowledge::Yes),
    reorg_post_inclusion_public_exposure: documented(Knowledge::Yes),
    handoff_boundary: HandoffBoundary::DeferredToNativeGeth,
    qualified_as_t_send: false,
};

/// 48Club puissant builder.
///
/// The frozen snapshot lists `txs`, `backrunTarget`, `maxBlockNumber`,
/// `maxTimestamp`, `revertingTxHashes`, `noMerge`, `positionFirst` and
/// `48spSign`. It does not list `minTimestamp`, which stays unqualified.
/// `positionFirst` is documented concretely (strict placement by priority
/// fee instead of a possible late placement at the block tail) and is
/// enabled as an API feature; how the builder really places such a bundle
/// is a matter of qualification. `backrunTarget` stays unqualified, and
/// the single private transaction is a separate API that is not qualified.
///
/// The snapshot does not state order, atomicity or same-block inclusion as
/// a guarantee, nor what the builder itself does with the transactions:
/// extraction, rebroadcast, public fallback and downstream recipients are
/// all recorded as unknown. It does state that after a chain
/// reorganisation a bundle may land only partly and the transactions of
/// the discarded block become public. That is recorded as what it is, a
/// reorg exposure after inclusion, and not as a provider leak.
///
/// Gas price: the FAQ says the average gas price of a bundle is checked.
/// No explicit numeric minimum is stated. That is a documented rule whose
/// exact form is not established, not the absence of a rule. The 1 gwei of
/// the historical Stage-1 policy is this product's conservative choice,
/// not a 48Club minimum.
static CLUB48_BUILDER_V1: CapabilityRecord = CapabilityRecord {
    id: CapabilityId::Club48BuilderV1,
    kind: AdapterKind::Club48Builder,
    version: 1,
    chain_id: BSC_MAINNET,
    evidence: &[ProviderEvidence {
        fixture: "bsc/native/crates/core/fixtures/inclusion/\
                  club48_builder_send_bundle.capability.json",
        fixture_sha256: "629888e2e0524a9724f193502005506755c8412bc8091313d28e0f1357d3b54c",
        retrieved_on: "2026-10-04",
    }],
    single_raw_buy: Support::Unqualified,
    public_full_launch_bundle: Support::Enabled,
    controlled_self_launch_bundle: Support::Enabled,
    private_target: Support::Unqualified,
    max_block_number: offered(Support::Enabled),
    min_timestamp: offered(Support::Unqualified),
    max_timestamp: offered(Support::Enabled),
    allowed_reverts: Support::Enabled,
    no_merge: offered(Support::Enabled),
    position_first: offered(Support::Enabled),
    pre_submission_before_launch_execution: offered(Support::Unqualified),
    cancellation: Support::Unsupported,
    max_txs: 2,
    fee_floor: FeeFloor::DocumentedRuleUnqualified,
    expiry_revokes_disclosed_bytes: false,
    preserves_order: documented(Knowledge::Unknown),
    atomic: documented(Knowledge::Unknown),
    same_block: documented(Knowledge::Unknown),
    extracts_transactions: documented(Knowledge::Unknown),
    provider_rebroadcast: documented(Knowledge::Unknown),
    provider_public_fallback: documented(Knowledge::Unknown),
    reaches_other_builders: documented(Knowledge::Unknown),
    public_mempool_exposure: documented(Knowledge::Unknown),
    reorg_post_inclusion_public_exposure: documented(Knowledge::Yes),
    handoff_boundary: HandoffBoundary::TlsUserspaceWriteFlushReturned,
    qualified_as_t_send: false,
};

/// BlockRazor block builder, submitted to directly.
///
/// The frozen snapshot lists `txs`, `maxBlockNumber`, `minTimestamp`,
/// `maxTimestamp`, `revertingTxHashes`, `noMerge` and `positionFirst`, and
/// describes Send Bundle as serving order, atomicity and same-block
/// execution, which is recorded as documented. `positionFirst` is
/// documented as well but is not part of any frozen release profile for
/// this builder and stays unqualified; no private-target field is listed.
///
/// The provider documents a 0.05 gwei floor in two different forms: per
/// transaction of the bundle, and as an average over the transactions not
/// taken from the mempool with a builder tip counted in. Neither is adopted
/// as a rule here; the floor is recorded as documented and unqualified.
///
/// Submitting directly to this builder is not evidence that the bytes go
/// nowhere else; downstream recipients and public exposure are recorded
/// as unknown, and pre-submission of an unbroadcast launch as unqualified.
static BLOCKRAZOR_BUILDER_V1: CapabilityRecord = CapabilityRecord {
    id: CapabilityId::BlockRazorBuilderV1,
    kind: AdapterKind::BlockRazorBuilder,
    version: 1,
    chain_id: BSC_MAINNET,
    evidence: &[ProviderEvidence {
        fixture: "bsc/native/crates/core/fixtures/inclusion/\
                  blockrazor_builder_send_bundle.capability.json",
        fixture_sha256: "c92f87196feea6e60861fbaf10a13fb7fafdaa847d17ea5bc57b9b685cba7248",
        retrieved_on: "2026-10-04",
    }],
    single_raw_buy: Support::Unqualified,
    public_full_launch_bundle: Support::Enabled,
    controlled_self_launch_bundle: Support::Enabled,
    private_target: Support::Unqualified,
    max_block_number: offered(Support::Enabled),
    min_timestamp: offered(Support::Enabled),
    max_timestamp: offered(Support::Enabled),
    allowed_reverts: Support::Enabled,
    no_merge: offered(Support::Enabled),
    position_first: offered(Support::Unqualified),
    pre_submission_before_launch_execution: offered(Support::Unqualified),
    cancellation: Support::Unsupported,
    max_txs: 2,
    fee_floor: FeeFloor::DocumentedRuleUnqualified,
    expiry_revokes_disclosed_bytes: false,
    preserves_order: documented(Knowledge::Yes),
    atomic: documented(Knowledge::Yes),
    same_block: documented(Knowledge::Yes),
    extracts_transactions: documented(Knowledge::Unknown),
    provider_rebroadcast: documented(Knowledge::Unknown),
    provider_public_fallback: documented(Knowledge::Unknown),
    reaches_other_builders: documented(Knowledge::Unknown),
    public_mempool_exposure: documented(Knowledge::Unknown),
    reorg_post_inclusion_public_exposure: documented(Knowledge::Unknown),
    handoff_boundary: HandoffBoundary::TlsUserspaceWriteFlushReturned,
    qualified_as_t_send: false,
};

/// BlockRazor RPC.
///
/// A different path than the block builder: the RPC forwards a bundle to
/// several builders, so the signed bytes reach more recipients. It is
/// recorded so that it can never be mistaken for the direct builder. No
/// mode is enabled.
static BLOCKRAZOR_RPC_V1: CapabilityRecord = CapabilityRecord {
    id: CapabilityId::BlockRazorRpcV1,
    kind: AdapterKind::BlockRazorRpc,
    version: 1,
    chain_id: BSC_MAINNET,
    evidence: &[ProviderEvidence {
        fixture: "bsc/native/crates/core/fixtures/inclusion/\
                  blockrazor_rpc_send_bundle.capability.json",
        fixture_sha256: "64d8b99cd62ea68d95a57a8a86a9591ac41752d4c9e60926151c3dd292e3ae9f",
        retrieved_on: "2026-10-04",
    }],
    single_raw_buy: Support::Unqualified,
    public_full_launch_bundle: Support::Unqualified,
    controlled_self_launch_bundle: Support::Unqualified,
    private_target: Support::Unqualified,
    max_block_number: offered(Support::Unqualified),
    min_timestamp: offered(Support::Unsupported),
    max_timestamp: offered(Support::Unsupported),
    allowed_reverts: Support::Unqualified,
    no_merge: offered(Support::Unsupported),
    position_first: offered(Support::Unsupported),
    pre_submission_before_launch_execution: offered(Support::Unqualified),
    cancellation: Support::Unsupported,
    max_txs: 0,
    fee_floor: FeeFloor::DocumentedRuleUnqualified,
    expiry_revokes_disclosed_bytes: false,
    preserves_order: documented(Knowledge::Unknown),
    atomic: documented(Knowledge::Unknown),
    same_block: documented(Knowledge::Unknown),
    extracts_transactions: documented(Knowledge::Unknown),
    provider_rebroadcast: documented(Knowledge::Unknown),
    provider_public_fallback: documented(Knowledge::Unknown),
    reaches_other_builders: documented(Knowledge::Yes),
    public_mempool_exposure: documented(Knowledge::Unknown),
    reorg_post_inclusion_public_exposure: documented(Knowledge::Unknown),
    handoff_boundary: HandoffBoundary::NotDefined,
    qualified_as_t_send: false,
};

/// Binding of one adapter index of the intent lane to a frozen capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterBinding {
    /// Opaque adapter index used by the intent lane.
    pub adapter: AdapterId,
    /// Capability of the adapter behind that index.
    pub capability: CapabilityId,
}

/// Reason a set of bindings is not valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityError {
    /// One adapter index is bound twice.
    DuplicateAdapter,
    /// One adapter kind is bound to two indices.
    DuplicateKind,
    /// A record is valid for another chain than the table's.
    WrongChain,
}

impl core::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::DuplicateAdapter => "capabilities: adapter index bound twice",
            Self::DuplicateKind => "capabilities: adapter kind bound twice",
            Self::WrongChain => "capabilities: record is valid for another chain",
        })
    }
}

impl std::error::Error for CapabilityError {}

/// The validated bindings of adapter indices to frozen capability records
/// for one chain, with the generation under which they were validated.
///
/// Which adapters exist is operating configuration; a new table gets a new
/// generation, and a plan built against an older one is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterTable {
    chain_id: u64,
    generation: u64,
    records: [Option<CapabilityRecord>; MAX_ADAPTERS],
}

impl AdapterTable {
    /// Validates `bindings` for `chain_id`. Every index and every kind may
    /// occur once, and every record must be valid for that chain.
    pub fn new(
        chain_id: u64,
        generation: u64,
        bindings: &[AdapterBinding],
    ) -> Result<Self, CapabilityError> {
        let mut table = Self {
            chain_id,
            generation,
            records: [None; MAX_ADAPTERS],
        };
        for binding in bindings {
            table.bind(binding.adapter, *binding.capability.record())?;
        }
        Ok(table)
    }

    fn bind(
        &mut self,
        adapter: AdapterId,
        record: CapabilityRecord,
    ) -> Result<(), CapabilityError> {
        if record.chain_id != self.chain_id {
            return Err(CapabilityError::WrongChain);
        }
        if self.records[adapter.index()].is_some() {
            return Err(CapabilityError::DuplicateAdapter);
        }
        let kind = record.kind;
        if self
            .records
            .iter()
            .flatten()
            .any(|bound| bound.kind == kind)
        {
            return Err(CapabilityError::DuplicateKind);
        }
        self.records[adapter.index()] = Some(record);
        Ok(())
    }

    /// Chain the adapters submit to.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Generation of this table.
    #[inline]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Capability record bound to `adapter`, if any.
    #[inline]
    pub fn record(&self, adapter: AdapterId) -> Option<&CapabilityRecord> {
        self.records[adapter.index()].as_ref()
    }
}

/// Altered records and tables for negative tests. Production code has no
/// way to alter a record or to bind anything but a frozen capability.
/// Compiled for tests only: this crate's and, behind the `test-support`
/// feature, those of dependent crates. Never part of a production build.
#[cfg(any(test, feature = "test-support"))]
pub mod testkit {
    use super::*;

    impl CapabilityRecord {
        pub fn with_chain_id(mut self, chain_id: u64) -> Self {
            self.chain_id = chain_id;
            self
        }

        pub fn with_no_merge(mut self, support: Support) -> Self {
            self.no_merge.support = support;
            self
        }

        pub fn with_max_block_number(mut self, support: Support) -> Self {
            self.max_block_number.support = support;
            self
        }

        pub fn with_max_txs(mut self, max_txs: u8) -> Self {
            self.max_txs = max_txs;
            self
        }

        pub fn with_pre_submission(mut self, support: Support) -> Self {
            self.pre_submission_before_launch_execution.support = support;
            self
        }

        pub fn with_position_first(mut self, support: Support) -> Self {
            self.position_first.support = support;
            self
        }

        // Qualification of single features. Support is left as it is.

        pub fn no_merge_qualified(mut self, qualification: Qualification) -> Self {
            self.no_merge.qualification = qualification;
            self
        }

        pub fn position_first_qualified(mut self, qualification: Qualification) -> Self {
            self.position_first.qualification = qualification;
            self
        }

        pub fn max_block_number_qualified(mut self, qualification: Qualification) -> Self {
            self.max_block_number.qualification = qualification;
            self
        }

        pub fn min_timestamp_qualified(mut self, qualification: Qualification) -> Self {
            self.min_timestamp.qualification = qualification;
            self
        }

        pub fn max_timestamp_qualified(mut self, qualification: Qualification) -> Self {
            self.max_timestamp.qualification = qualification;
            self
        }

        pub fn pre_submission_qualified(mut self, qualification: Qualification) -> Self {
            self.pre_submission_before_launch_execution.qualification = qualification;
            self
        }

        /// A record as a fully qualified adapter would have it: every
        /// lifecycle property the birth-entry profile needs is stated and
        /// qualified, the send boundary is qualified, and the features a
        /// birth-entry bundle relies on (`noMerge`, `positionFirst`,
        /// expiry by block) are explicitly qualified. Being enabled does
        /// not make a feature qualified; it is set here, feature by
        /// feature. Timestamps and pre-submission are not part of it.
        pub fn release_grade(mut self) -> Self {
            self.no_merge.qualification = Qualification::Qualified;
            self.position_first.qualification = Qualification::Qualified;
            self.max_block_number.qualification = Qualification::Qualified;
            let yes = qualified(Knowledge::Yes);
            let no = qualified(Knowledge::No);
            self.preserves_order = yes;
            self.atomic = yes;
            self.same_block = yes;
            self.extracts_transactions = no;
            self.provider_rebroadcast = no;
            self.provider_public_fallback = no;
            self.reaches_other_builders = no;
            self.public_mempool_exposure = no;
            self.qualified_as_t_send = true;
            self
        }

        pub fn with_atomic(mut self, property: Property) -> Self {
            self.atomic = property;
            self
        }

        pub fn with_provider_rebroadcast(mut self, property: Property) -> Self {
            self.provider_rebroadcast = property;
            self
        }

        pub fn with_provider_public_fallback(mut self, property: Property) -> Self {
            self.provider_public_fallback = property;
            self
        }

        pub fn with_reorg_exposure(mut self, property: Property) -> Self {
            self.reorg_post_inclusion_public_exposure = property;
            self
        }

        pub fn with_send_time_qualified(mut self, qualified: bool) -> Self {
            self.qualified_as_t_send = qualified;
            self
        }
    }

    /// A property that is stated and qualified.
    pub fn qualified(knowledge: Knowledge) -> Property {
        Property {
            knowledge,
            qualification: Qualification::Qualified,
        }
    }

    /// A property that is stated and not qualified.
    pub fn unqualified(knowledge: Knowledge) -> Property {
        Property {
            knowledge,
            qualification: Qualification::Unqualified,
        }
    }

    impl AdapterTable {
        /// A table of arbitrary records, validated like a production one.
        pub fn for_tests(
            chain_id: u64,
            generation: u64,
            records: &[(AdapterId, CapabilityRecord)],
        ) -> Result<Self, CapabilityError> {
            let mut table = Self {
                chain_id,
                generation,
                records: [None; MAX_ADAPTERS],
            };
            for (adapter, record) in records {
                table.bind(*adapter, *record)?;
            }
            Ok(table)
        }
    }
}
