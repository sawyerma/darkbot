//! Release profiles: what a release requires of a submission, beyond what
//! an adapter merely supports.
//!
//! A capability record says what an adapter *can* be asked and what its
//! provider *states*. A release profile says what this product *must* ask
//! and what must be *established* about an adapter before it counts for a
//! release. The profile is core semantics: the runner frames what the plan
//! derived from it and decides nothing itself.
//!
//! # `BirthEntryV1`
//!
//! Frozen from the master plan §8 and the Gate-0 acceptance proposal v2
//! (`bundle_bytes`): launch then BUY, identical bytes across builders, no
//! transaction may revert, and
//!
//! - for 48Club `noMerge = true` and `positionFirst = true`, so that a late
//!   bundle is not placed at the block tail;
//! - for every armed builder a provider validity of at most two blocks
//!   beyond the head at dispatch; a long default such as +100 is never
//!   used;
//! - a transport start limit of at most 1,000,000 ns.
//!
//! For BlockRazor the frozen evidence fixes no envelope option. `noMerge`
//! is sent when the plan requests it; `positionFirst` is not sent.
//!
//! # Two kinds of plan
//!
//! A [`SubmissionPlan`](super::plan::SubmissionPlan) can be formed for an
//! adapter that is documented but not qualified, so that the adapter can
//! be built and tested. It is a diagnostic plan.
//!
//! A release needs a
//! [`QualifiedReleasePlan`](super::plan::QualifiedReleasePlan). It exists
//! only as the result of
//! [`SubmissionPlan::qualify_for_release`](super::plan::SubmissionPlan::qualify_for_release),
//! which refuses unless every selected adapter is established for the
//! profile. It is a type, not a flag: code that takes a qualified plan
//! cannot be handed a diagnostic one.
//!
//! "Established" means stated by the frozen evidence *and* qualified by
//! this product, per property. A documented "yes" alone is a blocker, an
//! unknown is a blocker, and a qualified send boundary establishes nothing
//! about order, atomicity or what a provider does with the bytes.
//!
//! The same applies to the provider features a plan uses: `noMerge`,
//! `positionFirst`, expiry by block or timestamp, pre-submission. An
//! enabled feature is one the provider's reference offers. A release plan
//! needs each feature it actually sends to be enabled and qualified; a
//! feature it does not send is not looked at.
//!
//! Today no property and no feature of any record is qualified, so no plan
//! can be qualified for a release.

use super::capability::{AdapterKind, CapabilityRecord, Knowledge, SubmissionMode, Support};
use super::plan::{LOCAL_REACTION_LIMIT_NS, PlanError};
use super::qualification::{AdapterQualification, FeatureName, PropertyName};
use crate::intent::AdapterId;

/// A versioned release profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseProfile {
    /// Same-block entry directly behind the launch, version 1.
    BirthEntryV1,
    /// One signed transaction alone (plan 28.10.): an approval, a SELL, or
    /// a BUY on canonical state (Strategy C), version 1. No placement is
    /// claimed; the adapter must take the single transaction and its send
    /// boundary be qualified.
    SingleRawV1,
}

/// Envelope options a profile requires of one adapter kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequiredOptions {
    /// The request must ask not to be merged.
    pub no_merge: bool,
    /// The request must ask for strict placement by priority fee.
    pub position_first: bool,
}

/// Why a plan, or one of its adapters, does not qualify for a release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseBlocker {
    /// The capability table changed since the plan was formed.
    StaleCapabilities,
    /// A selected adapter has no capability record.
    UnboundAdapter,
    /// The submission mode is not enabled for the adapter.
    ModeNotEnabled,
    /// The mode gives no ordered placement behind the launch.
    NoOrderedPlacement,
    /// The plan does not carry an envelope option the profile requires.
    EnvelopeOptionMissing,
    /// Order preservation is not both stated and qualified.
    OrderNotEstablished,
    /// Atomic inclusion is not both stated and qualified.
    AtomicityNotEstablished,
    /// Same-block inclusion is not both stated and qualified.
    SameBlockNotEstablished,
    /// It is not established that single transactions are not extracted.
    ExtractionNotExcluded,
    /// It is not established that the provider does not rebroadcast.
    RebroadcastNotExcluded,
    /// It is not established that the provider has no public fallback.
    PublicFallbackNotExcluded,
    /// It is not established that the bytes reach no other builder.
    DownstreamNotExcluded,
    /// It is not established that submitting does not make the bytes
    /// public.
    PublicExposureNotExcluded,
    /// The adapter's handoff boundary is not qualified as the send time.
    SendTimeNotQualified,
    /// The plan sends `noMerge`; the feature is not enabled and qualified.
    NoMergeNotEstablished,
    /// The plan sends `positionFirst`; the feature is not enabled and
    /// qualified.
    PositionFirstNotEstablished,
    /// The plan relies on expiry by block number; the provider's
    /// enforcement of it is not enabled and qualified.
    ExpiryByBlockNotEstablished,
    /// The plan sets a minimum timestamp; the feature is not enabled and
    /// qualified.
    MinTimestampNotEstablished,
    /// The plan sets a maximum timestamp; the feature is not enabled and
    /// qualified.
    MaxTimestampNotEstablished,
    /// The plan is sent before the launch executes; pre-submission is not
    /// enabled and qualified.
    PreSubmissionNotEstablished,
}

/// The provider features one plan uses on one adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsedFeatures {
    /// The envelope asks not to be merged.
    pub no_merge: bool,
    /// The envelope asks for strict placement by priority fee.
    pub position_first: bool,
    /// The request carries a last valid block.
    pub max_block_number: bool,
    /// The request carries a minimum timestamp.
    pub min_timestamp: bool,
    /// The request carries a maximum timestamp.
    pub max_timestamp: bool,
    /// The request is sent before the launch was broadcast or executed.
    pub pre_submission: bool,
}

/// A refused release qualification: the blocker and the adapter it was
/// found on, if it concerns one adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseRefusal {
    /// The adapter the blocker was found on.
    pub adapter: Option<AdapterId>,
    /// The first blocker found.
    pub blocker: ReleaseBlocker,
}

impl ReleaseProfile {
    /// Largest distance, in blocks, between the head at dispatch and the
    /// provider's last valid block.
    pub fn max_block_window(self) -> u64 {
        match self {
            Self::BirthEntryV1 | Self::SingleRawV1 => 2,
        }
    }

    /// Largest limit, in nanoseconds, a transport start deadline of this
    /// profile may have. A deadline may be tighter, never wider.
    pub fn max_transport_start_limit_ns(self) -> u64 {
        match self {
            Self::BirthEntryV1 | Self::SingleRawV1 => LOCAL_REACTION_LIMIT_NS,
        }
    }

    /// Envelope options the profile requires of an adapter kind.
    pub fn required_options(self, kind: AdapterKind) -> RequiredOptions {
        match (self, kind) {
            (Self::BirthEntryV1, AdapterKind::Club48Builder) => RequiredOptions {
                no_merge: true,
                position_first: true,
            },
            (Self::BirthEntryV1 | Self::SingleRawV1, _) => RequiredOptions {
                no_merge: false,
                position_first: false,
            },
        }
    }

    /// Checks the provider's last valid block against the head at
    /// dispatch: it must not lie before the head and at most
    /// [`ReleaseProfile::max_block_window`] blocks after it. A head so high
    /// that the window cannot be computed fails closed.
    pub fn check_block_window(
        self,
        max_block_number: u64,
        head_block_number: u64,
    ) -> Result<(), PlanError> {
        if max_block_number < head_block_number {
            return Err(PlanError::ProviderExpired);
        }
        let latest = head_block_number
            .checked_add(self.max_block_window())
            .ok_or(PlanError::ProviderWindowTooWide)?;
        if max_block_number > latest {
            return Err(PlanError::ProviderWindowTooWide);
        }
        Ok(())
    }

    /// The first reason `record` is not established for a release of
    /// `mode` under this profile, or `None` if it is.
    ///
    /// Required for a birth entry: order, atomicity and same-block
    /// inclusion established as given; extraction, provider rebroadcast,
    /// provider public fallback, downstream recipients and public exposure
    /// on submission established as absent; the send boundary qualified.
    ///
    /// Not required, and therefore never a blocker here: what happens to
    /// included transactions when their block is reorganised away. That is
    /// a risk of the chain which recovery and reconciliation handle, not a
    /// property an adapter can be chosen by.
    pub fn release_blocker(
        self,
        record: &CapabilityRecord,
        mode: SubmissionMode,
    ) -> Option<ReleaseBlocker> {
        self.release_blocker_with(record, None, mode)
    }

    /// [`ReleaseProfile::release_blocker`] with what this product qualified
    /// about the adapter (plan 28.10.): a property, mode or send boundary
    /// counts as established when the frozen record or the qualification
    /// establishes it. A single transaction (`SingleRawV1`) claims no
    /// placement: it needs its mode and its send boundary, nothing else.
    pub fn release_blocker_with(
        self,
        record: &CapabilityRecord,
        qualification: Option<&AdapterQualification>,
        mode: SubmissionMode,
    ) -> Option<ReleaseBlocker> {
        let q = qualification.filter(|q| q.id() == record.id());
        let property = |p: PropertyName, wanted: Knowledge| {
            p.of(record).established(wanted) || q.is_some_and(|q| q.property_established(p, wanted))
        };
        let mode_ok =
            record.mode(mode) == Support::Enabled || q.is_some_and(|q| q.mode_established(mode));
        let send_time =
            record.qualified_as_t_send() || q.is_some_and(|q| q.send_time_established());
        if !mode_ok {
            return Some(ReleaseBlocker::ModeNotEnabled);
        }
        if self == Self::SingleRawV1 {
            if mode != SubmissionMode::SingleRawBuy {
                return Some(ReleaseBlocker::ModeNotEnabled);
            }
            return (!send_time).then_some(ReleaseBlocker::SendTimeNotQualified);
        }
        if mode == SubmissionMode::SingleRawBuy {
            return Some(ReleaseBlocker::NoOrderedPlacement);
        }
        for (p, wanted, blocker) in [
            (
                PropertyName::PreservesOrder,
                Knowledge::Yes,
                ReleaseBlocker::OrderNotEstablished,
            ),
            (
                PropertyName::Atomic,
                Knowledge::Yes,
                ReleaseBlocker::AtomicityNotEstablished,
            ),
            (
                PropertyName::SameBlock,
                Knowledge::Yes,
                ReleaseBlocker::SameBlockNotEstablished,
            ),
            (
                PropertyName::ExtractsTransactions,
                Knowledge::No,
                ReleaseBlocker::ExtractionNotExcluded,
            ),
            (
                PropertyName::ProviderRebroadcast,
                Knowledge::No,
                ReleaseBlocker::RebroadcastNotExcluded,
            ),
            (
                PropertyName::ProviderPublicFallback,
                Knowledge::No,
                ReleaseBlocker::PublicFallbackNotExcluded,
            ),
            (
                PropertyName::ReachesOtherBuilders,
                Knowledge::No,
                ReleaseBlocker::DownstreamNotExcluded,
            ),
            (
                PropertyName::PublicMempoolExposure,
                Knowledge::No,
                ReleaseBlocker::PublicExposureNotExcluded,
            ),
        ] {
            if !property(p, wanted) {
                return Some(blocker);
            }
        }
        if !send_time {
            return Some(ReleaseBlocker::SendTimeNotQualified);
        }
        None
    }

    /// The first provider feature in `used` that is not established on
    /// `record`, or `None` if all of them are.
    ///
    /// A feature counts only if it is enabled *and* its real behaviour was
    /// qualified. Being documented is not enough: a release that relies on
    /// `noMerge`, `positionFirst`, the provider's expiry or pre-submission
    /// relies on what the provider does, not on what its reference says.
    /// A feature the plan does not use is never a blocker.
    pub fn feature_blocker(
        self,
        record: &CapabilityRecord,
        used: UsedFeatures,
    ) -> Option<ReleaseBlocker> {
        self.feature_blocker_with(record, None, used)
    }

    /// [`ReleaseProfile::feature_blocker`] with what this product qualified
    /// about the adapter: a feature the plan uses counts when the frozen
    /// record or the qualification establishes it.
    pub fn feature_blocker_with(
        self,
        record: &CapabilityRecord,
        qualification: Option<&AdapterQualification>,
        used: UsedFeatures,
    ) -> Option<ReleaseBlocker> {
        let q = qualification.filter(|q| q.id() == record.id());
        let established = |f: FeatureName| {
            f.of(record).established() || q.is_some_and(|q| q.feature_established(f))
        };
        for (in_use, feature, blocker) in [
            (
                used.no_merge,
                FeatureName::NoMerge,
                ReleaseBlocker::NoMergeNotEstablished,
            ),
            (
                used.position_first,
                FeatureName::PositionFirst,
                ReleaseBlocker::PositionFirstNotEstablished,
            ),
            (
                used.max_block_number,
                FeatureName::MaxBlockNumber,
                ReleaseBlocker::ExpiryByBlockNotEstablished,
            ),
            (
                used.min_timestamp,
                FeatureName::MinTimestamp,
                ReleaseBlocker::MinTimestampNotEstablished,
            ),
            (
                used.max_timestamp,
                FeatureName::MaxTimestamp,
                ReleaseBlocker::MaxTimestampNotEstablished,
            ),
            (
                used.pre_submission,
                FeatureName::PreSubmission,
                ReleaseBlocker::PreSubmissionNotEstablished,
            ),
        ] {
            if in_use && !established(feature) {
                return Some(blocker);
            }
        }
        None
    }
}
