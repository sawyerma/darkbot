//! The immutable submission plan of one economic intent.
//!
//! A plan says what is submitted, through which adapters, under which
//! merge and revert policy, within which bounds of time and within which
//! budget. It refers to transactions by hash and length only. The signed
//! BUY bytes are held by the intent lane and handed to an adapter when it
//! consumes its permit; a plan cannot be used to obtain or to replace them.
//!
//! # Three cases, not one
//!
//! - [`SubmissionMode::SingleRawBuy`]: the BUY alone.
//! - [`SubmissionMode::PublicFullLaunchBundle`]: the original signed launch
//!   of a third party, referenced unchanged, then the BUY. The launch's
//!   cost is not charged to the budget.
//! - [`SubmissionMode::ControlledSelfLaunchBundle`]: this product's own
//!   signed launch, then the BUY. Chain, Portal, launcher and cost are read
//!   from the launch's signed bytes; the cost is charged to the budget.
//!   Sending it before the launch was broadcast is a separate capability
//!   ([`PreSubmission`]) that no adapter has enabled today.
//!
//! In both bundle cases the launch must be the very transaction the
//! snapshot names as its pending candidate: a plan cannot pair the BUY with
//! any other launch.
//!
//! # Nothing from assertion
//!
//! Transaction references are made from bytes ([`TxRef::of`]) and can be
//! checked against bytes ([`TxRef::matches`]). An own launch's facts are
//! read from its signed bytes. A direct payment to a provider has no
//! transaction that carries it yet, so a plan that claims one is refused.
//! After the intent lane dispatched, [`SubmissionPlan::bind_dispatch`]
//! ties the plan to exactly that dispatch before any adapter may write.
//!
//! # Bounds of time, kept apart
//!
//! - [`LocalBlockStartDeadline`]: the last chain head at which an adapter
//!   may still start a write. A placement bound in blocks. It says nothing
//!   about elapsed time and never stands for the 1 ms deadline. It is a
//!   rule of its own and may equal the provider's last block.
//! - [`TransportStartDeadline`]: a deadline on one monotonic clock, counted
//!   from a declared origin, that the runner checks for every adapter
//!   immediately before it consumes the permit. It is made for a release
//!   profile and cannot be wider than the profile allows. Its origin is
//!   known only at run time, so it is not part of the plan.
//! - [`ProviderValidity`]: what the provider is asked to enforce for the
//!   request (`maxBlockNumber`, optional timestamps). It is inclusive: the
//!   request is provider-expired only once the head is beyond it. The
//!   release profile additionally limits how far it may lie beyond the
//!   head at dispatch.
//! - [`TransactionValidity`]: what the signed BUY itself enforces. Today's
//!   legacy BUY has no bound of its own.
//!
//! None of them revokes bytes that were disclosed. No adapter can cancel.
//! [`Disclosure`] therefore distinguishes only whether bytes may have left
//! the process; once they may have, the BUY may still execute, whatever
//! the adapter answered and whatever expired, until the chain-state owner
//! provides evidence.
//!
//! A plan is valid for one snapshot identity and one capability
//! generation. [`SubmissionPlan::check_dispatch`] is the final check
//! before the economic admission of the intent lane is consumed.

use alloy_primitives::{Address, B256, U256};

use super::capability::{AdapterTable, SubmissionMode, Support};
use super::qualification::AdapterQualification;
use super::release::{ReleaseBlocker, ReleaseProfile, ReleaseRefusal, UsedFeatures};
use crate::chain_state::SnapshotId;
use crate::economics::arith::add;
use crate::intent::{AdapterId, AdapterSet, Dispatch, MAX_ADAPTERS, SubmissionState, Ticket};
use crate::signer::recover_legacy_sender;
use crate::transactions::legacy::{decode_signed, tx_hash};
use crate::venues::flap::binding::ExpectedTx;

/// Reference to one signed transaction: its hash and its encoded length.
/// Made from the bytes only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxRef {
    hash: B256,
    len: usize,
}

impl TxRef {
    /// Reference to the signed transaction `raw`. The bytes are hashed, not
    /// kept and not altered.
    pub fn of(raw: &[u8]) -> Self {
        Self {
            hash: tx_hash(raw),
            len: raw.len(),
        }
    }

    /// Keccak-256 of the signed transaction bytes.
    #[inline]
    pub fn hash(&self) -> B256 {
        self.hash
    }

    /// Length of the signed transaction bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the reference is to an empty byte string.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether `raw` is byte for byte the referenced transaction.
    pub fn matches(&self, raw: &[u8]) -> bool {
        raw.len() == self.len && tx_hash(raw) == self.hash
    }
}

/// This product's own signed launch, as read from its signed bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnLaunch {
    tx: TxRef,
    chain_id: u64,
    to: Address,
    launcher: Address,
    cost_wei: U256,
}

impl OwnLaunch {
    /// Reads an own launch from its signed bytes: chain, call target,
    /// recovered launcher, and the cost bound of gas limit times gas price
    /// plus value. Only a canonical signed legacy transaction with a valid
    /// signature is accepted.
    ///
    /// `scratch` is cleared and reused for the signing payload.
    pub fn from_signed_legacy(raw: &[u8], scratch: &mut Vec<u8>) -> Result<Self, PlanError> {
        let signed = decode_signed(raw).map_err(|_| PlanError::InvalidOwnLaunch)?;
        let chain_id = signed.tx.chain_id;
        let launcher = recover_legacy_sender(&signed, chain_id, scratch)
            .map_err(|_| PlanError::InvalidOwnLaunch)?;
        let gas = U256::from(signed.tx.gas_price) * U256::from(signed.tx.gas_limit);
        let cost_wei = add(gas, signed.tx.value).map_err(|_| PlanError::InvalidOwnLaunch)?;
        Ok(Self {
            tx: TxRef::of(raw),
            chain_id,
            to: signed.tx.to,
            launcher,
            cost_wei,
        })
    }

    /// Reference to the signed launch.
    #[inline]
    pub fn tx(&self) -> TxRef {
        self.tx
    }

    /// Chain the launch is signed for.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Contract the launch calls.
    #[inline]
    pub fn to(&self) -> Address {
        self.to
    }

    /// Signer of the launch. A wallet of its own; it need not be, and is
    /// not required to be, the buyer.
    #[inline]
    pub fn launcher(&self) -> Address {
        self.launcher
    }

    /// Upper bound of what the launch can cost, in wei.
    #[inline]
    pub fn cost_wei(&self) -> U256 {
        self.cost_wei
    }
}

/// The launch that precedes the BUY in a request, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Launch {
    /// No launch: the BUY is submitted alone.
    None,
    /// The original signed launch of a third party. Never re-signed.
    PublicOriginal(TxRef),
    /// This product's own signed launch.
    ControlledOwn(OwnLaunch),
}

impl Launch {
    /// Reference to the launch transaction, if there is one.
    pub fn tx(&self) -> Option<TxRef> {
        match self {
            Self::None => None,
            Self::PublicOriginal(tx) => Some(*tx),
            Self::ControlledOwn(own) => Some(own.tx),
        }
    }
}

/// Whether a controlled self-launch bundle is sent before its launch has
/// been broadcast or executed anywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreSubmission {
    /// No: the launch is already pending or executed when the request is
    /// sent.
    No,
    /// Yes. Requires the pre-submission capability on every selected
    /// adapter, which none has enabled today.
    BeforeLaunchExecution,
}

/// The intent lane's dispatch, verified to be the one this plan describes.
/// Made only by [`SubmissionPlan::bind_dispatch`]; an adapter may write
/// only with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundDispatch {
    ticket: Ticket,
    buy: TxRef,
    adapters: AdapterSet,
}

impl BoundDispatch {
    /// The dispatched intent.
    #[inline]
    pub fn ticket(&self) -> Ticket {
        self.ticket
    }

    /// The intent's signed BUY; its hash is presented when a permit is
    /// consumed.
    #[inline]
    pub fn buy(&self) -> TxRef {
        self.buy
    }

    /// The adapters that hold a permit.
    #[inline]
    pub fn adapters(&self) -> AdapterSet {
        self.adapters
    }
}

/// The last chain head at which an adapter may still start a write.
///
/// A placement bound in blocks, not a time limit: within one head a write
/// could start arbitrarily late and still satisfy it. The time limit is
/// [`TransportStartDeadline`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalBlockStartDeadline {
    /// Highest head block number at which a start is allowed.
    pub last_head_block: u64,
}

/// Product limit of the complete local reaction, in nanoseconds. Strict:
/// an elapsed time equal to it is a miss.
pub const LOCAL_REACTION_LIMIT_NS: u64 = 1_000_000;

/// Reason a transport deadline cannot be made, or a write must not start
/// under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportDeadlineError {
    /// The limit is zero.
    ZeroLimit,
    /// The limit is wider than the release profile allows.
    LimitAboveProfile,
    /// The time was read on another clock than the deadline's.
    OtherClock,
    /// The time lies before the deadline's origin.
    BeforeOrigin,
    /// The limit is reached or exceeded.
    Missed,
}

impl core::fmt::Display for TransportDeadlineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ZeroLimit => "transport deadline: zero limit",
            Self::LimitAboveProfile => "transport deadline: limit wider than the profile allows",
            Self::OtherClock => "transport deadline: time from another clock",
            Self::BeforeOrigin => "transport deadline: time before the origin",
            Self::Missed => "transport deadline: missed",
        })
    }
}

impl std::error::Error for TransportDeadlineError {}

/// A deadline for starting an adapter's write, on one monotonic clock, for
/// one release profile.
///
/// `origin_ns` is the declared start of the measured interval on the clock
/// `clock_id`, for example the earliest local observation of the trigger;
/// `limit_ns` is how long after it a write may still start. The limit can
/// be tighter than the profile's, never wider: there is no way to make a
/// deadline of a profile with a larger limit.
///
/// The runner reads the clock and checks the deadline for every adapter
/// immediately before it consumes the permit; that read and its placement
/// are the runner's. A passed check is a statement about the moment of the
/// check on that clock, not about when bytes reach the network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportStartDeadline {
    profile: ReleaseProfile,
    clock_id: u32,
    origin_ns: u64,
    limit_ns: u64,
}

impl TransportStartDeadline {
    /// A deadline of `limit_ns` after `origin_ns` on clock `clock_id` for
    /// `profile`. Fails for a zero limit and for a limit above
    /// [`ReleaseProfile::max_transport_start_limit_ns`].
    pub fn for_profile(
        profile: ReleaseProfile,
        clock_id: u32,
        origin_ns: u64,
        limit_ns: u64,
    ) -> Result<Self, TransportDeadlineError> {
        if limit_ns == 0 {
            return Err(TransportDeadlineError::ZeroLimit);
        }
        if limit_ns > profile.max_transport_start_limit_ns() {
            return Err(TransportDeadlineError::LimitAboveProfile);
        }
        Ok(Self {
            profile,
            clock_id,
            origin_ns,
            limit_ns,
        })
    }

    /// The release profile the deadline was made for.
    #[inline]
    pub fn profile(&self) -> ReleaseProfile {
        self.profile
    }

    /// The limit, in nanoseconds after the origin.
    #[inline]
    pub fn limit_ns(&self) -> u64 {
        self.limit_ns
    }

    /// Checks whether a write may start at `now_ns`, read on clock
    /// `clock_id`, and returns the elapsed time. Fails for another clock,
    /// for a time before the origin, and once the elapsed time has reached
    /// the limit: an elapsed time equal to the limit is a miss.
    pub fn check(&self, clock_id: u32, now_ns: u64) -> Result<u64, TransportDeadlineError> {
        if clock_id != self.clock_id {
            return Err(TransportDeadlineError::OtherClock);
        }
        let elapsed = now_ns
            .checked_sub(self.origin_ns)
            .ok_or(TransportDeadlineError::BeforeOrigin)?;
        if elapsed >= self.limit_ns {
            return Err(TransportDeadlineError::Missed);
        }
        Ok(elapsed)
    }
}

/// Envelope options derived for one adapter of a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvelopeOptions {
    /// Ask the provider not to merge the request.
    pub no_merge: bool,
    /// Ask the provider for strict placement by priority fee.
    pub position_first: bool,
}

/// What the provider is asked to enforce for the request. Inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderValidity {
    /// Last block in which the request may be included.
    pub max_block_number: u64,
    /// Earliest block timestamp, in unix seconds, if requested.
    pub min_timestamp: Option<u64>,
    /// Latest block timestamp, in unix seconds, if requested.
    pub max_timestamp: Option<u64>,
}

/// What the signed BUY itself enforces about time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionValidity {
    /// Nothing. A legacy transaction stays executable for as long as its
    /// nonce is unused.
    NoTimeBound,
}

/// The chain head as far as time bounds need it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainHead {
    /// Block number of the head.
    pub number: u64,
    /// Timestamp of the head, in unix seconds.
    pub timestamp: u64,
}

/// Which transactions of a request may revert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevertPolicy {
    /// None. A bundle in which any transaction reverts must not be
    /// included.
    NoneMayRevert,
}

/// Whether the provider may merge the request with others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergePolicy {
    /// The provider's default applies; nothing is requested.
    ProviderDefault,
    /// The request must not be merged. Requires the capability on every
    /// selected adapter.
    NoMerge,
}

/// Upper bounds of what one submission may cost. Operating policy; the
/// values come from validated configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetEnvelope {
    /// Largest native amount spent on the token.
    pub max_buy_input_wei: U256,
    /// Largest BUY gas cost at the signed gas price and limit.
    pub max_buy_gas_wei: U256,
    /// Largest cost of an own launch: gas plus value. Zero admits none.
    pub max_own_launch_wei: U256,
    /// Largest direct payment to a provider: bid or tip. Zero admits none.
    /// No transaction carries such a payment yet, so no plan can use it.
    pub max_provider_payment_wei: U256,
    /// Largest total of all of the above.
    pub max_total_wei: U256,
}

/// What a plan can cost, by component. Upper bounds, in wei.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanCosts {
    /// Native amount spent on the token.
    pub buy_input_wei: U256,
    /// BUY gas at the signed gas price and limit.
    pub buy_gas_wei: U256,
    /// Own launch: gas plus value. Zero unless the launch is this
    /// product's own.
    pub own_launch_wei: U256,
    /// Direct payment to a provider. Zero until a transaction that
    /// carries one exists and is bound.
    pub provider_payment_wei: U256,
    /// Sum of the components.
    pub total_wei: U256,
}

/// Reason a plan cannot be formed or dispatched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The adapters submit to another chain than the intent's.
    WrongChain,
    /// Ticket and expectation describe different intents.
    IdentityMismatch,
    /// A transaction reference has length zero.
    EmptyTransaction,
    /// The own launch is not a canonical signed legacy transaction, or its
    /// cost is not representable.
    InvalidOwnLaunch,
    /// A bundle needs its launch transaction.
    MissingLaunch,
    /// A single BUY must not carry a launch transaction.
    UnexpectedLaunch,
    /// The launch is of the other kind than the mode requires.
    LaunchKindMismatch,
    /// Launch and BUY are the same transaction.
    LaunchIsBuy,
    /// The launch is not the pending candidate of the snapshot.
    LaunchNotCandidate,
    /// The own launch is signed for another chain than the BUY.
    OwnLaunchWrongChain,
    /// The own launch does not call the Portal the BUY calls.
    OwnLaunchWrongPortal,
    /// Pre-submission is requested for something that is not a controlled
    /// self-launch bundle.
    InvalidPreSubmission,
    /// A provider payment is claimed, but no transaction carries it.
    UnboundProviderPayment,
    /// The intent lane's dispatch is not the one this plan describes.
    DispatchMismatch,
    /// A bundle needs a provider validity.
    MissingProviderValidity,
    /// The provider validity is zero or contradictory.
    InvalidProviderValidity,
    /// The local block bound lies beyond the provider's last block.
    LocalDeadlineAfterProviderValidity,
    /// The release profile requires an envelope option the plan does not
    /// request.
    ProfileOptionMissing,
    /// The provider's last valid block lies further beyond the head than
    /// the release profile allows.
    ProviderWindowTooWide,
    /// A selected adapter has no capability record.
    UnboundAdapter,
    /// A selected adapter does not have the submission mode enabled.
    ModeNotEnabled,
    /// A selected adapter does not have a requested option enabled.
    OptionNotEnabled,
    /// The request has more transactions than a selected adapter admits.
    TooManyTransactions,
    /// A cost component or the total exceeds the budget.
    BudgetExceeded,
    /// The capability table changed since the plan was formed.
    StaleCapabilities,
    /// The snapshot identity changed since the plan was formed.
    StaleSnapshot,
    /// The local start deadline passed.
    LocalDeadlinePassed,
    /// The provider validity passed.
    ProviderExpired,
}

impl core::fmt::Display for PlanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::WrongChain => "submission plan: adapters are on another chain",
            Self::IdentityMismatch => "submission plan: ticket and expectation differ",
            Self::EmptyTransaction => "submission plan: empty transaction reference",
            Self::InvalidOwnLaunch => "submission plan: invalid own launch",
            Self::MissingLaunch => "submission plan: bundle without launch",
            Self::UnexpectedLaunch => "submission plan: single BUY with a launch",
            Self::LaunchKindMismatch => "submission plan: launch kind does not fit the mode",
            Self::LaunchIsBuy => "submission plan: launch and BUY are identical",
            Self::LaunchNotCandidate => "submission plan: launch is not the pending candidate",
            Self::OwnLaunchWrongChain => "submission plan: own launch on another chain",
            Self::OwnLaunchWrongPortal => "submission plan: own launch calls another contract",
            Self::InvalidPreSubmission => "submission plan: pre-submission without self-launch",
            Self::UnboundProviderPayment => "submission plan: provider payment without carrier",
            Self::DispatchMismatch => "submission plan: dispatch does not match the plan",
            Self::MissingProviderValidity => "submission plan: bundle without provider validity",
            Self::InvalidProviderValidity => "submission plan: invalid provider validity",
            Self::LocalDeadlineAfterProviderValidity => {
                "submission plan: local block bound beyond provider validity"
            }
            Self::ProfileOptionMissing => "submission plan: option required by the profile",
            Self::ProviderWindowTooWide => "submission plan: provider validity too far ahead",
            Self::UnboundAdapter => "submission plan: adapter without capability record",
            Self::ModeNotEnabled => "submission plan: mode not enabled for an adapter",
            Self::OptionNotEnabled => "submission plan: option not enabled for an adapter",
            Self::TooManyTransactions => "submission plan: too many transactions",
            Self::BudgetExceeded => "submission plan: budget exceeded",
            Self::StaleCapabilities => "submission plan: capability table changed",
            Self::StaleSnapshot => "submission plan: snapshot changed",
            Self::LocalDeadlinePassed => "submission plan: local start deadline passed",
            Self::ProviderExpired => "submission plan: provider validity passed",
        })
    }
}

impl std::error::Error for PlanError {}

/// What the caller chooses for one submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanInputs {
    /// The reserved intent.
    pub ticket: Ticket,
    /// The intent's signed BUY, as bound by the lane.
    pub buy: TxRef,
    /// The launch that precedes the BUY; its kind must fit the mode.
    pub launch: Launch,
    /// What is submitted.
    pub mode: SubmissionMode,
    /// The release profile whose envelope options and block window apply.
    pub profile: ReleaseProfile,
    /// The adapters to fan out to.
    pub adapters: AdapterSet,
    /// Last head at which an adapter may start a write.
    pub local_block_deadline: LocalBlockStartDeadline,
    /// What the providers are asked to enforce. Required for a bundle.
    pub provider_validity: Option<ProviderValidity>,
    /// Merge policy. Must request `NoMerge` where the profile requires it.
    pub merge: MergePolicy,
    /// Whether the request is sent before the launch was broadcast.
    pub pre_submission: PreSubmission,
    /// Direct payment to a provider. Must be zero: no transaction carries
    /// one yet.
    pub provider_payment_wei: U256,
}

/// The immutable submission plan of one economic intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubmissionPlan {
    ticket: Ticket,
    buy: TxRef,
    launch: Launch,
    mode: SubmissionMode,
    profile: ReleaseProfile,
    adapters: AdapterSet,
    envelopes: [Option<EnvelopeOptions>; MAX_ADAPTERS],
    local_block_deadline: LocalBlockStartDeadline,
    provider_validity: Option<ProviderValidity>,
    merge: MergePolicy,
    pre_submission: PreSubmission,
    reverts: RevertPolicy,
    costs: PlanCosts,
    snapshot: SnapshotId,
    capability_generation: u64,
}

impl SubmissionPlan {
    /// Forms the plan of an intent.
    ///
    /// Fails closed unless the ticket and the expectation describe the
    /// same intent, the launch fits the mode, the time bounds are
    /// consistent, every selected adapter has the mode and every requested
    /// option enabled, and every cost component and the total fit the
    /// budget.
    pub fn new<'e>(
        table: &AdapterTable,
        expected: impl Into<ExpectedTx<'e>>,
        budget: &BudgetEnvelope,
        inputs: PlanInputs,
    ) -> Result<Self, PlanError> {
        Self::new_with(table, &[], expected, budget, inputs)
    }

    /// [`SubmissionPlan::new`] where a mode the frozen record leaves
    /// unqualified counts as enabled once this product qualified it for
    /// the adapter (plan 28.10.: a builder's single transaction). Nothing
    /// else changes: an `Unsupported` mode never qualifies, and the release
    /// still needs [`SubmissionPlan::qualify_for_release_with`].
    pub fn new_with<'e>(
        table: &AdapterTable,
        qualifications: &[AdapterQualification],
        expected: impl Into<ExpectedTx<'e>>,
        budget: &BudgetEnvelope,
        inputs: PlanInputs,
    ) -> Result<Self, PlanError> {
        let expected: ExpectedTx<'e> = expected.into();
        let ticket = inputs.ticket;
        if table.chain_id() != ticket.chain_id() {
            return Err(PlanError::WrongChain);
        }
        if expected.chain_id() != ticket.chain_id()
            || expected.sender() != ticket.sender()
            || expected.nonce() != ticket.nonce()
            || expected.token() != ticket.token()
        {
            return Err(PlanError::IdentityMismatch);
        }

        // Launch and mode.
        let (tx_count, own_launch_wei): (u8, U256) = match (inputs.mode, inputs.launch) {
            // No request shape is defined for a private target, and no
            // capability record enables it.
            (SubmissionMode::PrivateTarget, _) => return Err(PlanError::ModeNotEnabled),
            (SubmissionMode::SingleRawBuy, Launch::None) => (1, U256::ZERO),
            (SubmissionMode::SingleRawBuy, _) => return Err(PlanError::UnexpectedLaunch),
            (_, Launch::None) => return Err(PlanError::MissingLaunch),
            (SubmissionMode::PublicFullLaunchBundle, Launch::PublicOriginal(_)) => (2, U256::ZERO),
            (SubmissionMode::ControlledSelfLaunchBundle, Launch::ControlledOwn(own)) => {
                (2, own.cost_wei)
            }
            (SubmissionMode::PublicFullLaunchBundle, Launch::ControlledOwn(_))
            | (SubmissionMode::ControlledSelfLaunchBundle, Launch::PublicOriginal(_)) => {
                return Err(PlanError::LaunchKindMismatch);
            }
        };
        if inputs.buy.is_empty() {
            return Err(PlanError::EmptyTransaction);
        }
        if let Some(launch) = inputs.launch.tx() {
            if launch.is_empty() {
                return Err(PlanError::EmptyTransaction);
            }
            if launch.hash == inputs.buy.hash {
                return Err(PlanError::LaunchIsBuy);
            }
            // The launch must be the transaction the BUY was admitted
            // behind, not merely some launch.
            if launch.hash != expected.snapshot().pending_candidate {
                return Err(PlanError::LaunchNotCandidate);
            }
        }
        if let Launch::ControlledOwn(own) = inputs.launch {
            if own.chain_id != expected.chain_id() {
                return Err(PlanError::OwnLaunchWrongChain);
            }
            if own.to != expected.portal() {
                return Err(PlanError::OwnLaunchWrongPortal);
            }
        }
        let pre_submitted = inputs.pre_submission == PreSubmission::BeforeLaunchExecution;
        if pre_submitted && inputs.mode != SubmissionMode::ControlledSelfLaunchBundle {
            return Err(PlanError::InvalidPreSubmission);
        }
        if !inputs.provider_payment_wei.is_zero() {
            return Err(PlanError::UnboundProviderPayment);
        }

        // Time bounds.
        if let Some(validity) = inputs.provider_validity {
            let contradictory = match (validity.min_timestamp, validity.max_timestamp) {
                (Some(min), Some(max)) => min > max,
                _ => false,
            };
            if validity.max_block_number == 0 || contradictory {
                return Err(PlanError::InvalidProviderValidity);
            }
            // The two bounds are separate rules and may coincide. A local
            // bound beyond the provider's last block could never be used
            // and is refused as a contradiction.
            if inputs.local_block_deadline.last_head_block > validity.max_block_number {
                return Err(PlanError::LocalDeadlineAfterProviderValidity);
            }
        } else if tx_count > 1 {
            return Err(PlanError::MissingProviderValidity);
        }

        // Capabilities of every selected adapter, and the envelope options
        // the release profile and the plan derive for it.
        let mut envelopes: [Option<EnvelopeOptions>; MAX_ADAPTERS] = [None; MAX_ADAPTERS];
        for (index, envelope) in envelopes.iter_mut().enumerate() {
            let Some(adapter) = member(inputs.adapters, index) else {
                continue;
            };
            let record = table.record(adapter).ok_or(PlanError::UnboundAdapter)?;
            let qualified = qualifications
                .iter()
                .any(|q| q.id() == record.id() && q.mode_established(inputs.mode));
            if record.mode(inputs.mode) != Support::Enabled && !qualified {
                return Err(PlanError::ModeNotEnabled);
            }
            if tx_count > record.max_txs() {
                return Err(PlanError::TooManyTransactions);
            }
            if let Some(validity) = inputs.provider_validity {
                let unsupported = !record.max_block_number().enabled()
                    || (validity.min_timestamp.is_some() && !record.min_timestamp().enabled())
                    || (validity.max_timestamp.is_some() && !record.max_timestamp().enabled());
                if unsupported {
                    return Err(PlanError::OptionNotEnabled);
                }
            }
            let no_merge = inputs.merge == MergePolicy::NoMerge;
            let required = inputs.profile.required_options(record.kind());
            if required.no_merge && !no_merge {
                // Not silently upgraded: the caller must ask for it.
                return Err(PlanError::ProfileOptionMissing);
            }
            if no_merge && !record.no_merge().enabled() {
                return Err(PlanError::OptionNotEnabled);
            }
            if required.position_first && !record.position_first().enabled() {
                return Err(PlanError::OptionNotEnabled);
            }
            if pre_submitted && !record.pre_submission_before_launch_execution().enabled() {
                return Err(PlanError::OptionNotEnabled);
            }
            *envelope = Some(EnvelopeOptions {
                no_merge,
                position_first: required.position_first,
            });
        }

        // Budget, by component and in total.
        // An exit's approval or SELL carries no value: gas only.
        let buy_input_wei = expected.value_wei();
        let buy_gas_wei = expected.gas_wei();
        let provider_payment_wei = inputs.provider_payment_wei;
        let total_wei = add(buy_input_wei, buy_gas_wei)
            .and_then(|sum| add(sum, own_launch_wei))
            .and_then(|sum| add(sum, provider_payment_wei))
            .map_err(|_| PlanError::BudgetExceeded)?;
        if buy_input_wei > budget.max_buy_input_wei
            || buy_gas_wei > budget.max_buy_gas_wei
            || own_launch_wei > budget.max_own_launch_wei
            || provider_payment_wei > budget.max_provider_payment_wei
            || total_wei > budget.max_total_wei
        {
            return Err(PlanError::BudgetExceeded);
        }

        Ok(Self {
            ticket,
            buy: inputs.buy,
            launch: inputs.launch,
            mode: inputs.mode,
            profile: inputs.profile,
            adapters: inputs.adapters,
            envelopes,
            local_block_deadline: inputs.local_block_deadline,
            provider_validity: inputs.provider_validity,
            merge: inputs.merge,
            pre_submission: inputs.pre_submission,
            reverts: RevertPolicy::NoneMayRevert,
            costs: PlanCosts {
                buy_input_wei,
                buy_gas_wei,
                own_launch_wei,
                provider_payment_wei,
                total_wei,
            },
            snapshot: expected.snapshot(),
            capability_generation: table.generation(),
        })
    }

    /// The economic intent: chain, sender, nonce and token.
    #[inline]
    pub fn ticket(&self) -> Ticket {
        self.ticket
    }

    /// The intent's signed BUY. Its hash is what an adapter presents when
    /// it consumes its permit.
    #[inline]
    pub fn buy(&self) -> TxRef {
        self.buy
    }

    /// The launch that precedes the BUY.
    #[inline]
    pub fn launch(&self) -> Launch {
        self.launch
    }

    /// What is submitted.
    #[inline]
    pub fn mode(&self) -> SubmissionMode {
        self.mode
    }

    /// The adapters the intent fans out to.
    #[inline]
    pub fn adapters(&self) -> AdapterSet {
        self.adapters
    }

    /// The release profile the plan was formed under.
    #[inline]
    pub fn profile(&self) -> ReleaseProfile {
        self.profile
    }

    /// The envelope options derived for `adapter`, or `None` if it is not
    /// part of the plan. The runner frames exactly these.
    #[inline]
    pub fn envelope(&self, adapter: AdapterId) -> Option<EnvelopeOptions> {
        self.envelopes[adapter.index()]
    }

    /// Last head at which an adapter may start a write. A placement bound,
    /// not a time limit.
    #[inline]
    pub fn local_block_deadline(&self) -> LocalBlockStartDeadline {
        self.local_block_deadline
    }

    /// What the providers are asked to enforce.
    #[inline]
    pub fn provider_validity(&self) -> Option<ProviderValidity> {
        self.provider_validity
    }

    /// What the signed BUY itself enforces about time: nothing.
    #[inline]
    pub fn buy_validity(&self) -> TransactionValidity {
        TransactionValidity::NoTimeBound
    }

    /// Merge policy.
    #[inline]
    pub fn merge(&self) -> MergePolicy {
        self.merge
    }

    /// Whether the request is sent before the launch was broadcast.
    #[inline]
    pub fn pre_submission(&self) -> PreSubmission {
        self.pre_submission
    }

    /// Revert policy.
    #[inline]
    pub fn reverts(&self) -> RevertPolicy {
        self.reverts
    }

    /// What the plan can cost, by component.
    #[inline]
    pub fn costs(&self) -> PlanCosts {
        self.costs
    }

    /// Snapshot identity the plan is valid for.
    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }

    /// Generation of the capability table the plan was validated against.
    #[inline]
    pub fn capability_generation(&self) -> u64 {
        self.capability_generation
    }

    /// Whether the local block bound still allows an adapter to start a
    /// write when the chain head is `head_block_number`. This is a
    /// placement check; elapsed time is checked separately with a
    /// [`TransportStartDeadline`].
    #[inline]
    pub fn local_block_start_allowed(&self, head_block_number: u64) -> bool {
        head_block_number <= self.local_block_deadline.last_head_block
    }

    /// Whether the provider validity has passed at `head`. Inclusive: a
    /// head equal to `max_block_number` or `max_timestamp` is not expired.
    /// A plan without provider validity never provider-expires.
    pub fn provider_expired(&self, head: ChainHead) -> bool {
        match self.provider_validity {
            None => false,
            Some(validity) => {
                head.number > validity.max_block_number
                    || validity
                        .max_timestamp
                        .is_some_and(|max| head.timestamp > max)
            }
        }
    }

    /// Final check before the economic admission of the intent is
    /// consumed. Nothing has been disclosed when it fails.
    ///
    /// Checks the capability generation, the snapshot, the local block
    /// bound, the provider validity and the release profile's block window
    /// at `head`. It does not check elapsed time; that is the
    /// [`TransportStartDeadline`], checked per adapter right before the
    /// permit.
    pub fn check_dispatch(
        &self,
        table: &AdapterTable,
        current: &SnapshotId,
        head: ChainHead,
    ) -> Result<(), PlanError> {
        if table.generation() != self.capability_generation {
            return Err(PlanError::StaleCapabilities);
        }
        if *current != self.snapshot {
            return Err(PlanError::StaleSnapshot);
        }
        if !self.local_block_start_allowed(head.number) {
            return Err(PlanError::LocalDeadlinePassed);
        }
        if self.provider_expired(head) {
            return Err(PlanError::ProviderExpired);
        }
        if let Some(validity) = self.provider_validity {
            // Only now is the head known: the provider validity must be
            // tight around it.
            self.profile
                .check_block_window(validity.max_block_number, head.number)?;
        }
        Ok(())
    }
}

impl SubmissionPlan {
    /// Ties the plan to the dispatch the intent lane returned from
    /// `begin_send`. The dispatch must be for this plan's intent, this
    /// plan's BUY and exactly this plan's adapters. No adapter may consume
    /// a permit before this succeeded.
    pub fn bind_dispatch(&self, dispatch: &Dispatch) -> Result<BoundDispatch, PlanError> {
        if dispatch.ticket != self.ticket
            || dispatch.tx_hash != self.buy.hash
            || dispatch.adapters != self.adapters
        {
            return Err(PlanError::DispatchMismatch);
        }
        Ok(BoundDispatch {
            ticket: self.ticket,
            buy: self.buy,
            adapters: self.adapters,
        })
    }
}

/// A plan that is established for a release under its profile.
///
/// It exists only as the result of
/// [`SubmissionPlan::qualify_for_release`]. A release path takes this
/// type; a plain [`SubmissionPlan`] is a diagnostic plan and cannot stand
/// in for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedReleasePlan {
    plan: SubmissionPlan,
}

impl QualifiedReleasePlan {
    /// The qualified plan. Its final check before dispatch, including the
    /// profile's block window, and its binding to the intent lane's
    /// dispatch are those of the plan.
    #[inline]
    pub fn plan(&self) -> &SubmissionPlan {
        &self.plan
    }
}

impl SubmissionPlan {
    /// Qualifies the plan for a release under its profile.
    ///
    /// Refuses unless the capability table is the one the plan was formed
    /// against and, for every selected adapter,
    ///
    /// - the plan carries every envelope option the profile requires;
    /// - the adapter's record is established for the profile: every
    ///   lifecycle property it requires is stated as needed and qualified,
    ///   and the send boundary is qualified;
    /// - every provider feature this plan uses on that adapter (`noMerge`,
    ///   `positionFirst`, expiry by block or timestamp, pre-submission) is
    ///   enabled *and* qualified. An enabled feature is documented, not
    ///   proven; that suffices for a diagnostic plan and never for a
    ///   release.
    ///
    /// One blocker on one adapter refuses the whole plan.
    ///
    /// The block window of the profile is bound through the plan's final
    /// check, which the qualified plan shares.
    pub fn qualify_for_release(
        &self,
        table: &AdapterTable,
    ) -> Result<QualifiedReleasePlan, ReleaseRefusal> {
        self.qualify_for_release_with(table, &[])
    }

    /// [`SubmissionPlan::qualify_for_release`] with what this product
    /// qualified about the adapters (plan 28.10.): each adapter is judged
    /// with the qualification of its own record, if one is given. Without
    /// one, exactly as before: nothing is qualified, nothing is released.
    pub fn qualify_for_release_with(
        &self,
        table: &AdapterTable,
        qualifications: &[AdapterQualification],
    ) -> Result<QualifiedReleasePlan, ReleaseRefusal> {
        if table.generation() != self.capability_generation {
            return Err(ReleaseRefusal {
                adapter: None,
                blocker: ReleaseBlocker::StaleCapabilities,
            });
        }
        for (index, envelope) in self.envelopes.iter().enumerate() {
            let Some(adapter) = member(self.adapters, index) else {
                continue;
            };
            let refuse = |blocker: ReleaseBlocker| ReleaseRefusal {
                adapter: Some(adapter),
                blocker,
            };
            let Some(record) = table.record(adapter) else {
                return Err(refuse(ReleaseBlocker::UnboundAdapter));
            };
            let required = self.profile.required_options(record.kind());
            let carried = envelope.is_some_and(|options| {
                (!required.no_merge || options.no_merge)
                    && (!required.position_first || options.position_first)
            });
            if !carried {
                return Err(refuse(ReleaseBlocker::EnvelopeOptionMissing));
            }
            let qualification = qualifications.iter().find(|q| q.id() == record.id());
            if let Some(blocker) =
                self.profile
                    .release_blocker_with(record, qualification, self.mode)
            {
                return Err(refuse(blocker));
            }
            // The provider features this very plan relies on for this
            // adapter. Each must be enabled and qualified; a feature the
            // plan does not use is not looked at.
            let validity = self.provider_validity;
            let used = UsedFeatures {
                no_merge: envelope.is_some_and(|options| options.no_merge),
                position_first: envelope.is_some_and(|options| options.position_first),
                max_block_number: validity.is_some(),
                min_timestamp: validity.is_some_and(|v| v.min_timestamp.is_some()),
                max_timestamp: validity.is_some_and(|v| v.max_timestamp.is_some()),
                pre_submission: self.pre_submission == PreSubmission::BeforeLaunchExecution,
            };
            if let Some(blocker) = self
                .profile
                .feature_blocker_with(record, qualification, used)
            {
                return Err(refuse(blocker));
            }
        }
        Ok(QualifiedReleasePlan { plan: *self })
    }
}

/// The adapter with index `index`, if it is in `set`.
fn member(set: AdapterSet, index: usize) -> Option<AdapterId> {
    let adapter = AdapterId::new(u8::try_from(index).ok()?)?;
    if set.contains(adapter) {
        Some(adapter)
    } else {
        None
    }
}

/// What is known about the disclosure of the signed bytes through one
/// adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disclosure {
    /// No write was started. Nothing left the process through this adapter.
    BeforeFirstWrite,
    /// A write was started and has not completed.
    Writing,
    /// The local write completed.
    HandedOff,
    /// The adapter acknowledged the request.
    Acked,
    /// The adapter rejected the request.
    Rejected,
    /// A write failed after it was started.
    Unknown,
    /// The provider validity passed after bytes may have left the process.
    ExpiredAfterDisclosure,
}

impl Disclosure {
    /// Whether bytes may have left the process through this adapter.
    #[inline]
    pub fn bytes_may_have_left(self) -> bool {
        self != Self::BeforeFirstWrite
    }

    /// Whether the submitted BUY may still execute as far as this adapter
    /// is concerned. No rejection, expiry or local bookkeeping changes
    /// that once bytes may have left; only chain evidence does.
    #[inline]
    pub fn may_still_execute(self) -> bool {
        self.bytes_may_have_left()
    }
}

/// Classifies the submission state of one adapter.
///
/// `provider_expired` says whether the plan's provider validity has
/// passed. An expiry before any write is not a disclosure; an expiry after
/// one is recorded as such and never as "safely unsent".
pub fn disclosure(submission: SubmissionState, provider_expired: bool) -> Disclosure {
    let state = match submission {
        SubmissionState::NotSelected
        | SubmissionState::NotStarted
        | SubmissionState::Suppressed => return Disclosure::BeforeFirstWrite,
        SubmissionState::Writing => Disclosure::Writing,
        SubmissionState::HandedOff => Disclosure::HandedOff,
        SubmissionState::Acked => Disclosure::Acked,
        SubmissionState::Rejected => Disclosure::Rejected,
        SubmissionState::Unknown => Disclosure::Unknown,
    };
    if provider_expired {
        Disclosure::ExpiredAfterDisclosure
    } else {
        state
    }
}
