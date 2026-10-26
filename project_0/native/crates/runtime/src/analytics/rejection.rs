//! DEC-12 (V2-14/P1-05): why a candidate was not ready, typed at its source. The pipeline moves
//! typed values; the text is made only where the record is written — the owner's and the
//! analytics' writer threads encode the typed code with its text, a human-readable view shows
//! the text — so no reason text is formatted on the owner's hot path. Every text is identical to
//! the one written before DEC-12; a record of an earlier writer keeps its text verbatim
//! (`Recorded` with `RejectionCode::Earlier`), never parsed into a cause.
use darkbot_project0::economics::Evaluation;
use darkbot_project0::funding::FundingClass;
use darkbot_project0::health::HealthError;
use darkbot_project0::safety::Deny;
use darkbot_solana::submit::{SimResult, SubmissionPath, SubmitError};
use darkbot_solana::tx::{BuildError, BuildVersion};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use solana_sdk::pubkey::Pubkey;
use std::borrow::Cow;

/// The typed rejection of one evaluation.
#[derive(Clone, Debug)]
pub enum Rejection {
    StateNotTradable,
    HealthUnavailable,
    /// A Health error (an integration reserve stale or any other refusal of the engine).
    Health(HealthError),
    NotLiquidatable,
    /// The pinned circuit-breaker eligibility refused the receivership.
    CircuitBreaker(HealthError),
    OpportunityStale,
    /// DEC-01: refused before planning on a stale or missing blockhash.
    BlockhashStaleEarly,
    RpcBudgetDrain,
    /// The network deadline passed during planning: the plans made, or the planning error text.
    RpcDeadlinePlanning(Result<usize, String>),
    NoFeasiblePlans,
    /// The planning error (its text as the builder made it).
    Plan(String),
    /// The late check before the build.
    BlockhashStaleLate,
    RpcDeadlineBeforeFeeQuote,
    RpcDeadlineInFeeQuote,
    RpcBudgetFeeQuote(&'static str),
    /// No plan was actionable; each rejected plan with its cause.
    PlansRejected(Vec<PlanRejection>),
    TechnicalProbe,
    Sign(BuildError),
    ExecutionDeadline,
    BundleWithoutTip,
    PrerequisiteBuild(BuildError),
    PrerequisiteMessage(&'static str),
    PrerequisiteSign(BuildError),
    TipMismatch(&'static str),
    TipBuild(SubmitError),
    RpcDeadlineBeforeSimulation,
    RpcDeadlineSimulation,
    RpcBudgetSimulation(&'static str),
    Simulation(SimResult),
    Safety(Deny),
    LiveSendRequiresFe04 {
        bundle: bool,
    },
    /// The FE-04 admission refused before anything was sent; `release` names a refused
    /// reservation release.
    SubmissionRefused {
        reason: String,
        release: Option<&'static str>,
    },
    /// The FE-04 handoff outcome is unknown (holds kept).
    SubmissionUnknown(String),
    /// Bad debt with zero equity assets reclassified the evaluation's cause.
    Bankruptcy {
        cause: crate::analytics::record::LossReason,
        reason: Option<Box<Rejection>>,
    },
    /// A decoded record: its persisted code and the text its writer made.
    Recorded {
        code: RejectionCode,
        text: String,
    },
}

/// Equal when written equal: the same code and the same text (a typed value and its decoded
/// record compare equal).
impl PartialEq for Rejection {
    fn eq(&self, other: &Self) -> bool {
        self.code() == other.code() && self.text() == other.text()
    }
}
impl Eq for Rejection {}

/// One rejected plan of an evaluation.
#[derive(Clone, Debug)]
pub struct PlanRejection {
    pub funding: FundingClass,
    pub cause: PlanCause,
}

/// Why the plan gate refused a plan's instructions.
#[derive(Clone, Copy, Debug)]
pub enum PlanInvalid {
    /// The receivership transaction's steps.
    Gate(darkbot_project0::planner::PlanError),
    /// A prerequisite transaction's steps.
    Prerequisite(darkbot_project0::planner::PlanError),
    /// The bundle of prerequisite and receivership transactions.
    Bundle(&'static str),
}

#[derive(Clone, Debug)]
pub enum PlanCause {
    Invalid(PlanInvalid),
    ProgramNotAllowlisted(Pubkey),
    PrerequisiteNotAllowed(Pubkey),
    Safety(Deny),
    TxVersion {
        version: BuildVersion,
        path: SubmissionPath,
    },
    NeedsBundle,
    Infeasible(BuildError),
    MessageDiffers(&'static str),
    Economics(Evaluation),
}

/// The persisted classification of a rejection (one per variant; `Earlier`: a text written
/// before DEC-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RejectionCode {
    StateNotTradable,
    HealthUnavailable,
    Health,
    NotLiquidatable,
    CircuitBreaker,
    OpportunityStale,
    BlockhashStaleEarly,
    RpcBudgetDrain,
    RpcDeadlinePlanning,
    NoFeasiblePlans,
    Plan,
    BlockhashStaleLate,
    RpcDeadlineBeforeFeeQuote,
    RpcDeadlineInFeeQuote,
    RpcBudgetFeeQuote,
    PlansRejected,
    TechnicalProbe,
    Sign,
    ExecutionDeadline,
    BundleWithoutTip,
    PrerequisiteBuild,
    PrerequisiteMessage,
    PrerequisiteSign,
    TipMismatch,
    TipBuild,
    RpcDeadlineBeforeSimulation,
    RpcDeadlineSimulation,
    RpcBudgetSimulation,
    Simulation,
    Safety,
    LiveSendRequiresFe04,
    SubmissionRefused,
    SubmissionUnknown,
    Bankruptcy,
    Earlier,
}

impl Rejection {
    pub fn code(&self) -> RejectionCode {
        use RejectionCode as C;
        match self {
            Self::StateNotTradable => C::StateNotTradable,
            Self::HealthUnavailable => C::HealthUnavailable,
            Self::Health(_) => C::Health,
            Self::NotLiquidatable => C::NotLiquidatable,
            Self::CircuitBreaker(_) => C::CircuitBreaker,
            Self::OpportunityStale => C::OpportunityStale,
            Self::BlockhashStaleEarly => C::BlockhashStaleEarly,
            Self::RpcBudgetDrain => C::RpcBudgetDrain,
            Self::RpcDeadlinePlanning(_) => C::RpcDeadlinePlanning,
            Self::NoFeasiblePlans => C::NoFeasiblePlans,
            Self::Plan(_) => C::Plan,
            Self::BlockhashStaleLate => C::BlockhashStaleLate,
            Self::RpcDeadlineBeforeFeeQuote => C::RpcDeadlineBeforeFeeQuote,
            Self::RpcDeadlineInFeeQuote => C::RpcDeadlineInFeeQuote,
            Self::RpcBudgetFeeQuote(_) => C::RpcBudgetFeeQuote,
            Self::PlansRejected(_) => C::PlansRejected,
            Self::TechnicalProbe => C::TechnicalProbe,
            Self::Sign(_) => C::Sign,
            Self::ExecutionDeadline => C::ExecutionDeadline,
            Self::BundleWithoutTip => C::BundleWithoutTip,
            Self::PrerequisiteBuild(_) => C::PrerequisiteBuild,
            Self::PrerequisiteMessage(_) => C::PrerequisiteMessage,
            Self::PrerequisiteSign(_) => C::PrerequisiteSign,
            Self::TipMismatch(_) => C::TipMismatch,
            Self::TipBuild(_) => C::TipBuild,
            Self::RpcDeadlineBeforeSimulation => C::RpcDeadlineBeforeSimulation,
            Self::RpcDeadlineSimulation => C::RpcDeadlineSimulation,
            Self::RpcBudgetSimulation(_) => C::RpcBudgetSimulation,
            Self::Simulation(_) => C::Simulation,
            Self::Safety(_) => C::Safety,
            Self::LiveSendRequiresFe04 { .. } => C::LiveSendRequiresFe04,
            Self::SubmissionRefused { .. } => C::SubmissionRefused,
            Self::SubmissionUnknown(_) => C::SubmissionUnknown,
            Self::Bankruptcy { .. } => C::Bankruptcy,
            Self::Recorded { code, .. } => *code,
        }
    }

    /// The record text (identical to the text written before DEC-12).
    pub fn text(&self) -> Cow<'_, str> {
        use crate::execution::pipeline::{BLOCKHASH_STALE_EARLY, LIVE_SEND_REQUIRES_FE04};
        Cow::Owned(match self {
            Self::StateNotTradable => return "state not tradable".into(),
            Self::HealthUnavailable => return "health engine unavailable".into(),
            Self::Health(e) | Self::CircuitBreaker(e) => e.to_string(),
            Self::NotLiquidatable => return "not liquidatable".into(),
            Self::OpportunityStale => return "opportunity stale before planning".into(),
            Self::BlockhashStaleEarly => return BLOCKHASH_STALE_EARLY.into(),
            Self::RpcBudgetDrain => {
                return "RPC_BUDGET: owner ingress queue behind; no network evaluation in this \
                        drain window"
                    .into()
            }
            Self::RpcDeadlinePlanning(planned) => format!(
                "RPC_DEADLINE: candidate network deadline during planning ({})",
                match planned {
                    Ok(n) => format!("{n} plan(s)"),
                    Err(text) => text.clone(),
                }
            ),
            Self::NoFeasiblePlans => return "no feasible execution candidates".into(),
            Self::Plan(text) => return Cow::Borrowed(text),
            Self::BlockhashStaleLate => return "blockhash stale".into(),
            Self::RpcDeadlineBeforeFeeQuote => {
                return "RPC_DEADLINE: candidate network deadline before the fee quote".into()
            }
            Self::RpcDeadlineInFeeQuote => {
                return "RPC_DEADLINE: candidate network deadline in the fee quote".into()
            }
            Self::RpcBudgetFeeQuote(code) => format!("RPC_BUDGET: fee quote refused ({code})"),
            Self::PlansRejected(plans) if plans.is_empty() => {
                return "no actionable execution candidate".into()
            }
            Self::PlansRejected(plans) => plans
                .iter()
                .map(PlanRejection::text)
                .collect::<Vec<_>>()
                .join(" | "),
            Self::TechnicalProbe => {
                return "technical shadow probe: cost or realization evidence incomplete".into()
            }
            Self::Sign(e) | Self::PrerequisiteSign(e) => e.to_string(),
            Self::ExecutionDeadline => return "execution deadline passed".into(),
            Self::BundleWithoutTip => return "bundle path without tip account".into(),
            Self::PrerequisiteBuild(e) => format!("prerequisite build: {e}"),
            Self::PrerequisiteMessage(code) => {
                format!("prerequisite compiled message differs from the validated one ({code})")
            }
            Self::TipMismatch(code) => format!("tip tx differs from its plan ({code})"),
            Self::TipBuild(e) => format!("tip tx: {e}"),
            Self::RpcDeadlineBeforeSimulation => {
                return "RPC_DEADLINE: candidate network deadline before the simulation".into()
            }
            Self::RpcDeadlineSimulation => {
                return "RPC_DEADLINE: candidate network deadline in the simulation".into()
            }
            Self::RpcBudgetSimulation(code) => format!("RPC_BUDGET: simulation refused ({code})"),
            Self::Simulation(sim) => format!("simulation not ok: {}", sim.record_text()),
            Self::Safety(d) => d.record_text(),
            Self::LiveSendRequiresFe04 { bundle } => format!(
                "{LIVE_SEND_REQUIRES_FE04}: simulated {} candidate is handed off only by the \
                 FE-04 one-shot owner",
                if *bundle { "bundle" } else { "single-TX" }
            ),
            Self::SubmissionRefused {
                reason,
                release: Some(code),
            } => format!("{reason}; {code}"),
            Self::SubmissionRefused {
                reason,
                release: None,
            }
            | Self::SubmissionUnknown(reason) => return Cow::Borrowed(reason),
            Self::Bankruptcy { cause, reason } => {
                format!(
                "bad debt with zero equity assets: bankruptcy/no-collateral case, not a normal \
                 receivership opportunity (reported as {:?}: {})",
                cause,
                reason.as_deref().map_or(Cow::Borrowed("-"), Rejection::text)
            )
            }
            Self::Recorded { text, .. } => return Cow::Borrowed(text),
        })
    }
}

impl PlanRejection {
    pub fn text(&self) -> String {
        let f = self.funding.as_str();
        match &self.cause {
            PlanCause::Invalid(PlanInvalid::Gate(e)) => {
                format!("{f}: plan invalid: {}", e.record_text())
            }
            PlanCause::Invalid(PlanInvalid::Prerequisite(e)) => {
                format!("{f}: plan invalid: prerequisite {}", e.record_text())
            }
            PlanCause::Invalid(PlanInvalid::Bundle(code)) => format!("{f}: plan invalid: {code}"),
            PlanCause::ProgramNotAllowlisted(p) => {
                format!("{f}: receivership program not allowlisted: {p}")
            }
            PlanCause::PrerequisiteNotAllowed(p) => {
                format!("{f}: prerequisite program not allowed: {p}")
            }
            PlanCause::Safety(d) => format!("{f}: safety {}", d.record_text()),
            PlanCause::TxVersion { version, path } => format!(
                "{f}: tx version {} not supported on {}",
                version.record_name(),
                path.as_str()
            ),
            PlanCause::NeedsBundle => format!("{f}: needs bundle path"),
            PlanCause::Infeasible(e) => format!("{f}: tx infeasible: {e}"),
            PlanCause::MessageDiffers(code) => {
                format!("{f}: compiled message differs from the validated instructions ({code})")
            }
            PlanCause::Economics(eval) => format!("{f}: economics {}", eval.record_text()),
        }
    }
}

/// The binary form: the typed code with the text the writer made.
#[derive(Serialize, Deserialize)]
struct Written<'a> {
    code: RejectionCode,
    text: Cow<'a, str>,
}

impl Serialize for Rejection {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.serialize_str(&self.text())
        } else {
            Written {
                code: self.code(),
                text: self.text(),
            }
            .serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for Rejection {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if d.is_human_readable() {
            Ok(Self::earlier(String::deserialize(d)?))
        } else {
            let w = Written::deserialize(d)?;
            Ok(Self::Recorded {
                code: w.code,
                text: w.text.into_owned(),
            })
        }
    }
}

impl Rejection {
    /// The text of a writer before DEC-12, kept verbatim.
    pub fn earlier(text: String) -> Self {
        Self::Recorded {
            code: RejectionCode::Earlier,
            text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::record::LossReason;

    /// DEC-12: every typed rejection writes exactly the text the pipeline wrote before DEC-12
    /// (pinned literals), under its own code; the binary form keeps code and text, the
    /// human-readable form is the text alone, and a text of an earlier writer stays verbatim.
    #[test]
    fn dec12_every_rejection_writes_its_earlier_text_under_its_code() {
        use darkbot_solana::tx::BuildError;
        let health = HealthError::IntegrationStale("bank".into());
        let cases: Vec<(Rejection, &str)> = vec![
            (Rejection::StateNotTradable, "state not tradable"),
            (Rejection::HealthUnavailable, "health engine unavailable"),
            (
                Rejection::Health(health.clone()),
                "health integration reserve stale: bank",
            ),
            (Rejection::NotLiquidatable, "not liquidatable"),
            (
                Rejection::CircuitBreaker(HealthError::Stale("cb".into())),
                "health stale: cb",
            ),
            (Rejection::OpportunityStale, "opportunity stale before planning"),
            (
                Rejection::BlockhashStaleEarly,
                "BLOCKHASH_STALE: refused before planning",
            ),
            (
                Rejection::RpcBudgetDrain,
                "RPC_BUDGET: owner ingress queue behind; no network evaluation in this drain window",
            ),
            (
                Rejection::RpcDeadlinePlanning(Ok(2)),
                "RPC_DEADLINE: candidate network deadline during planning (2 plan(s))",
            ),
            (
                Rejection::RpcDeadlinePlanning(Err("ROUTING: x".into())),
                "RPC_DEADLINE: candidate network deadline during planning (ROUTING: x)",
            ),
            (Rejection::NoFeasiblePlans, "no feasible execution candidates"),
            (Rejection::Plan("PLAN: y".into()), "PLAN: y"),
            (Rejection::BlockhashStaleLate, "blockhash stale"),
            (
                Rejection::RpcDeadlineBeforeFeeQuote,
                "RPC_DEADLINE: candidate network deadline before the fee quote",
            ),
            (
                Rejection::RpcDeadlineInFeeQuote,
                "RPC_DEADLINE: candidate network deadline in the fee quote",
            ),
            (
                Rejection::RpcBudgetFeeQuote("RPS"),
                "RPC_BUDGET: fee quote refused (RPS)",
            ),
            (
                Rejection::PlansRejected(vec![]),
                "no actionable execution candidate",
            ),
            (
                Rejection::PlansRejected(vec![
                    PlanRejection {
                        funding: FundingClass::ZeroPrefundSameTx,
                        cause: PlanCause::NeedsBundle,
                    },
                    PlanRejection {
                        funding: FundingClass::ZeroPrefundSameTx,
                        cause: PlanCause::MessageDiffers("E2"),
                    },
                ]),
                "ZERO_PREFUND_SAME_TX: needs bundle path | ZERO_PREFUND_SAME_TX: compiled message \
                 differs from the validated instructions (E2)",
            ),
            (
                Rejection::PlansRejected(vec![
                    PlanRejection {
                        funding: FundingClass::ZeroPrefundSameTx,
                        cause: PlanCause::Invalid(PlanInvalid::Gate(
                            darkbot_project0::planner::PlanError::OrderViolation(2, "repay"),
                        )),
                    },
                    PlanRejection {
                        funding: FundingClass::ZeroPrefundSameTx,
                        cause: PlanCause::Invalid(PlanInvalid::Prerequisite(
                            darkbot_project0::planner::PlanError::NoStart,
                        )),
                    },
                    PlanRejection {
                        funding: FundingClass::ZeroPrefundSameTx,
                        cause: PlanCause::Invalid(PlanInvalid::Bundle("BUNDLE_X")),
                    },
                ]),
                "ZERO_PREFUND_SAME_TX: plan invalid: OrderViolation(2, \"repay\") | \
                 ZERO_PREFUND_SAME_TX: plan invalid: prerequisite NoStart | ZERO_PREFUND_SAME_TX: \
                 plan invalid: BUNDLE_X",
            ),
            (
                Rejection::TechnicalProbe,
                "technical shadow probe: cost or realization evidence incomplete",
            ),
            (
                Rejection::Sign(BuildError::Sign("k".into())),
                "sign: k",
            ),
            (Rejection::ExecutionDeadline, "execution deadline passed"),
            (Rejection::BundleWithoutTip, "bundle path without tip account"),
            (
                Rejection::PrerequisiteBuild(BuildError::V1Unavailable),
                "prerequisite build: tx v1 signing/submission adapter not enabled; v1 capability \
                 measurement is available with feature txv1",
            ),
            (
                Rejection::PrerequisiteMessage("E2"),
                "prerequisite compiled message differs from the validated one (E2)",
            ),
            (
                Rejection::TipMismatch("TIP"),
                "tip tx differs from its plan (TIP)",
            ),
            (
                Rejection::TipBuild(SubmitError::Serialize("s".into())),
                "tip tx: submit serialize: s",
            ),
            (
                Rejection::RpcDeadlineBeforeSimulation,
                "RPC_DEADLINE: candidate network deadline before the simulation",
            ),
            (
                Rejection::RpcDeadlineSimulation,
                "RPC_DEADLINE: candidate network deadline in the simulation",
            ),
            (
                Rejection::RpcBudgetSimulation("RPS"),
                "RPC_BUDGET: simulation refused (RPS)",
            ),
            (
                Rejection::Simulation(SimResult::Unavailable("u".into())),
                "simulation not ok: Unavailable(\"u\")",
            ),
            (Rejection::Safety(Deny::DailyLoss), "DailyLoss"),
            (
                Rejection::LiveSendRequiresFe04 { bundle: false },
                "LIVE_SEND_REQUIRES_FE04_ONE_SHOT: simulated single-TX candidate is handed off \
                 only by the FE-04 one-shot owner",
            ),
            (
                Rejection::SubmissionRefused {
                    reason: "FE04_X".into(),
                    release: Some("RELEASE"),
                },
                "FE04_X; RELEASE",
            ),
            (
                Rejection::SubmissionRefused {
                    reason: "FE04_X".into(),
                    release: None,
                },
                "FE04_X",
            ),
            (
                Rejection::SubmissionUnknown("SUBMISSION_RESULT_UNKNOWN:X".into()),
                "SUBMISSION_RESULT_UNKNOWN:X",
            ),
            (
                Rejection::Bankruptcy {
                    cause: LossReason::Sizing,
                    reason: Some(Box::new(Rejection::Plan("SIZING:NO_PAIR".into()))),
                },
                "bad debt with zero equity assets: bankruptcy/no-collateral case, not a normal \
                 receivership opportunity (reported as Sizing: SIZING:NO_PAIR)",
            ),
        ];
        let mut codes = std::collections::HashSet::new();
        for (rejection, text) in &cases {
            assert_eq!(rejection.text(), *text, "{rejection:?}");
            codes.insert(rejection.code());
            let bytes = crate::analytics::payload::encode_wire(rejection).unwrap();
            let back: Rejection = crate::analytics::payload::decode_wire(&bytes).unwrap();
            assert!(matches!(back, Rejection::Recorded { .. }));
            assert_eq!(
                (back.code(), back.text()),
                (rejection.code(), (*text).into())
            );
            assert_eq!(&back, rejection);
            assert_eq!(
                serde_json::to_value(rejection).unwrap(),
                serde_json::json!(text)
            );
        }
        assert!(codes.len() >= 33, "one code per variant: {}", codes.len());
        // The persisted code is its variant index: append-only, pinned.
        use RejectionCode as C;
        let pinned = [
            C::StateNotTradable,
            C::HealthUnavailable,
            C::Health,
            C::NotLiquidatable,
            C::CircuitBreaker,
            C::OpportunityStale,
            C::BlockhashStaleEarly,
            C::RpcBudgetDrain,
            C::RpcDeadlinePlanning,
            C::NoFeasiblePlans,
            C::Plan,
            C::BlockhashStaleLate,
            C::RpcDeadlineBeforeFeeQuote,
            C::RpcDeadlineInFeeQuote,
            C::RpcBudgetFeeQuote,
            C::PlansRejected,
            C::TechnicalProbe,
            C::Sign,
            C::ExecutionDeadline,
            C::BundleWithoutTip,
            C::PrerequisiteBuild,
            C::PrerequisiteMessage,
            C::PrerequisiteSign,
            C::TipMismatch,
            C::TipBuild,
            C::RpcDeadlineBeforeSimulation,
            C::RpcDeadlineSimulation,
            C::RpcBudgetSimulation,
            C::Simulation,
            C::Safety,
            C::LiveSendRequiresFe04,
            C::SubmissionRefused,
            C::SubmissionUnknown,
            C::Bankruptcy,
            C::Earlier,
        ];
        for (index, code) in pinned.into_iter().enumerate() {
            assert_eq!(
                crate::analytics::payload::encode_wire(&code).unwrap(),
                (index as u32).to_le_bytes(),
                "{code:?}"
            );
            // Exhaustive: a new code must be appended to `pinned`.
            match code {
                C::StateNotTradable | C::HealthUnavailable | C::Health | C::NotLiquidatable => {}
                C::CircuitBreaker | C::OpportunityStale | C::BlockhashStaleEarly => {}
                C::RpcBudgetDrain | C::RpcDeadlinePlanning | C::NoFeasiblePlans | C::Plan => {}
                C::BlockhashStaleLate | C::RpcDeadlineBeforeFeeQuote | C::RpcDeadlineInFeeQuote => {
                }
                C::RpcBudgetFeeQuote | C::PlansRejected | C::TechnicalProbe | C::Sign => {}
                C::ExecutionDeadline | C::BundleWithoutTip | C::PrerequisiteBuild => {}
                C::PrerequisiteMessage | C::PrerequisiteSign | C::TipMismatch | C::TipBuild => {}
                C::RpcDeadlineBeforeSimulation | C::RpcDeadlineSimulation => {}
                C::RpcBudgetSimulation | C::Simulation | C::Safety | C::LiveSendRequiresFe04 => {}
                C::SubmissionRefused | C::SubmissionUnknown | C::Bankruptcy | C::Earlier => {}
            }
        }
        let earlier: Rejection = serde_json::from_value(serde_json::json!("old text")).unwrap();
        assert_eq!(
            (earlier.code(), earlier.text()),
            (RejectionCode::Earlier, "old text".into())
        );
    }
}
