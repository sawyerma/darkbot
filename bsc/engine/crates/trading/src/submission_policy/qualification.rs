//! What this product qualified about one adapter (plan 28.10., `0a647175`):
//! measured findings that establish lifecycle properties, provider
//! features, a submission mode and the send boundary of one frozen
//! capability record, for a release.
//!
//! The frozen records stay as they are: they say what an adapter's
//! reference states and that nothing is qualified. A qualification is the
//! other half, a product measurement, and it is built only from findings a
//! verified evidence record names (the runner's LIVE qualification). Without
//! one, nothing is qualified and no plan can be released, exactly as before.
//!
//! A finding can only establish what the record could ever offer: a
//! feature or mode the record marks `Unsupported` cannot be qualified, and
//! a property is established with the value the measurement found (it may
//! well be the unwanted one).

use super::capability::{
    CapabilityId, CapabilityRecord, Feature, Knowledge, Property, SubmissionMode, Support,
};

/// A lifecycle property a release profile may require.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyName {
    PreservesOrder,
    Atomic,
    SameBlock,
    ExtractsTransactions,
    ProviderRebroadcast,
    ProviderPublicFallback,
    ReachesOtherBuilders,
    PublicMempoolExposure,
}

impl PropertyName {
    pub const ALL: [PropertyName; 8] = [
        Self::PreservesOrder,
        Self::Atomic,
        Self::SameBlock,
        Self::ExtractsTransactions,
        Self::ProviderRebroadcast,
        Self::ProviderPublicFallback,
        Self::ReachesOtherBuilders,
        Self::PublicMempoolExposure,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::PreservesOrder => "PRESERVES_ORDER",
            Self::Atomic => "ATOMIC",
            Self::SameBlock => "SAME_BLOCK",
            Self::ExtractsTransactions => "EXTRACTS_TRANSACTIONS",
            Self::ProviderRebroadcast => "PROVIDER_REBROADCAST",
            Self::ProviderPublicFallback => "PROVIDER_PUBLIC_FALLBACK",
            Self::ReachesOtherBuilders => "REACHES_OTHER_BUILDERS",
            Self::PublicMempoolExposure => "PUBLIC_MEMPOOL_EXPOSURE",
        }
    }

    /// The frozen record's statement of this property.
    pub fn of(self, record: &CapabilityRecord) -> Property {
        match self {
            Self::PreservesOrder => record.preserves_order(),
            Self::Atomic => record.atomic(),
            Self::SameBlock => record.same_block(),
            Self::ExtractsTransactions => record.extracts_transactions(),
            Self::ProviderRebroadcast => record.provider_rebroadcast(),
            Self::ProviderPublicFallback => record.provider_public_fallback(),
            Self::ReachesOtherBuilders => record.reaches_other_builders(),
            Self::PublicMempoolExposure => record.public_mempool_exposure(),
        }
    }
}

/// A provider feature a plan may rely on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureName {
    MaxBlockNumber,
    MinTimestamp,
    MaxTimestamp,
    NoMerge,
    PositionFirst,
    PreSubmission,
}

impl FeatureName {
    pub const ALL: [FeatureName; 6] = [
        Self::MaxBlockNumber,
        Self::MinTimestamp,
        Self::MaxTimestamp,
        Self::NoMerge,
        Self::PositionFirst,
        Self::PreSubmission,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::MaxBlockNumber => "MAX_BLOCK_NUMBER",
            Self::MinTimestamp => "MIN_TIMESTAMP",
            Self::MaxTimestamp => "MAX_TIMESTAMP",
            Self::NoMerge => "NO_MERGE",
            Self::PositionFirst => "POSITION_FIRST",
            Self::PreSubmission => "PRE_SUBMISSION",
        }
    }

    /// The frozen record's statement of this feature.
    pub fn of(self, record: &CapabilityRecord) -> Feature {
        match self {
            Self::MaxBlockNumber => record.max_block_number(),
            Self::MinTimestamp => record.min_timestamp(),
            Self::MaxTimestamp => record.max_timestamp(),
            Self::NoMerge => record.no_merge(),
            Self::PositionFirst => record.position_first(),
            Self::PreSubmission => record.pre_submission_before_launch_execution(),
        }
    }
}

const MODES: [(SubmissionMode, &str); 4] = [
    (SubmissionMode::SingleRawBuy, "SINGLE_RAW"),
    (
        SubmissionMode::PublicFullLaunchBundle,
        "PUBLIC_FULL_LAUNCH_BUNDLE",
    ),
    (
        SubmissionMode::ControlledSelfLaunchBundle,
        "CONTROLLED_SELF_LAUNCH_BUNDLE",
    ),
    (SubmissionMode::PrivateTarget, "PRIVATE_TARGET"),
];

/// One measured finding about an adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finding {
    /// The property was measured to be `Yes` or `No`.
    Property(PropertyName, Knowledge),
    /// The provider's real treatment of the feature was qualified.
    Feature(FeatureName),
    /// The submission mode was qualified end to end.
    Mode(SubmissionMode),
    /// The adapter's handoff boundary was qualified as the send time.
    SendTime,
}

impl Finding {
    /// `PROPERTY:<NAME>=YES|NO`, `FEATURE:<NAME>`, `MODE:<NAME>` or
    /// `SEND_TIME`.
    pub fn name(self) -> String {
        match self {
            Self::Property(p, k) => format!(
                "PROPERTY:{}={}",
                p.name(),
                if k == Knowledge::Yes { "YES" } else { "NO" }
            ),
            Self::Feature(f) => format!("FEATURE:{}", f.name()),
            Self::Mode(m) => format!(
                "MODE:{}",
                MODES.iter().find(|(x, _)| *x == m).map_or("?", |(_, n)| n)
            ),
            Self::SendTime => "SEND_TIME".into(),
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        if text == "SEND_TIME" {
            return Some(Self::SendTime);
        }
        if let Some(rest) = text.strip_prefix("PROPERTY:") {
            let (name, value) = rest.split_once('=')?;
            let property = PropertyName::ALL.into_iter().find(|p| p.name() == name)?;
            let knowledge = match value {
                "YES" => Knowledge::Yes,
                "NO" => Knowledge::No,
                _ => return None,
            };
            return Some(Self::Property(property, knowledge));
        }
        if let Some(name) = text.strip_prefix("FEATURE:") {
            return FeatureName::ALL
                .into_iter()
                .find(|f| f.name() == name)
                .map(Self::Feature);
        }
        let name = text.strip_prefix("MODE:")?;
        MODES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(m, _)| Self::Mode(*m))
    }
}

/// What this product qualified about one frozen capability record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterQualification {
    id: CapabilityId,
    properties: Vec<(PropertyName, Knowledge)>,
    features: Vec<FeatureName>,
    modes: Vec<SubmissionMode>,
    send_time: bool,
}

impl AdapterQualification {
    /// The qualification of `id` from the findings a verified evidence
    /// record names. Refused: a finding twice, a property with two values,
    /// a feature or mode the record does not offer at all.
    pub fn from_findings(id: CapabilityId, findings: &[Finding]) -> Result<Self, String> {
        let record = id.record();
        let mut q = AdapterQualification {
            id,
            properties: Vec::new(),
            features: Vec::new(),
            modes: Vec::new(),
            send_time: false,
        };
        for (i, finding) in findings.iter().enumerate() {
            if findings[..i].contains(finding) {
                return Err(format!("{} is named twice", finding.name()));
            }
            match *finding {
                Finding::Property(p, k) => {
                    if q.properties.iter().any(|(x, _)| *x == p) {
                        return Err(format!("{} is measured twice", p.name()));
                    }
                    q.properties.push((p, k));
                }
                Finding::Feature(f) => {
                    if f.of(record).support == Support::Unsupported {
                        return Err(format!("{} is not offered by this adapter", f.name()));
                    }
                    q.features.push(f);
                }
                Finding::Mode(m) => {
                    if record.mode(m) == Support::Unsupported {
                        return Err(format!("{} is not offered by this adapter", finding.name()));
                    }
                    q.modes.push(m);
                }
                Finding::SendTime => q.send_time = true,
            }
        }
        Ok(q)
    }

    pub fn id(&self) -> CapabilityId {
        self.id
    }

    /// Whether `property` is `wanted`, as measured or (never today) frozen
    /// as qualified.
    pub fn property_established(&self, property: PropertyName, wanted: Knowledge) -> bool {
        property.of(self.id.record()).established(wanted)
            || self.properties.contains(&(property, wanted))
    }

    /// Whether `feature` is offered and its real behaviour qualified.
    pub fn feature_established(&self, feature: FeatureName) -> bool {
        let frozen = feature.of(self.id.record());
        frozen.established()
            || (frozen.support != Support::Unsupported && self.features.contains(&feature))
    }

    /// Whether `mode` may be released through this adapter.
    pub fn mode_established(&self, mode: SubmissionMode) -> bool {
        self.modes.contains(&mode) && self.id.record().mode(mode) != Support::Unsupported
    }

    /// Whether the handoff boundary is qualified as the send time.
    pub fn send_time_established(&self) -> bool {
        self.id.record().qualified_as_t_send() || self.send_time
    }
}
