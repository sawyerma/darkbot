//! P6-LT: Opportunity-Record (Ledger-Vokabular). Eine Zeile JSONL pro Entscheidung.

use darkbot_common::timing::Timeline;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Skip,
    ShadowReady,
    /// Technical shadow qualification: build/sign/sim succeeded, but economics are still Unknown.
    ShadowTechnicalReady,
    /// [M1] Shadow: economically known candidate built+signed, simulation failed.
    ShadowSimFailed,
    /// Technical shadow qualification candidate built+signed, simulation failed.
    ShadowTechnicalSimFailed,
    Submitted,
    /// [FIX-23] Handoff mit Transportfehler: Annahme unbekannt, Reconciliation entscheidet.
    AmbiguousSubmission,
    DeniedBySafety,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossReason {
    Ingress,
    State,
    Health,
    CircuitBreaker,
    Bankruptcy,
    IntegrationRefresh,
    Funding,
    MissingAta,
    UnsupportedIntegration,
    Sizing,
    Economics,
    Routing,
    Build,
    Submission,
    Tip,
    StateRace,
    EconomicSkip,
    ThresholdTooHigh,
    Unknown,
    /// B6: the owner's network budget for this drain window was used up; the candidate is
    /// recorded without routes, fee quote or simulation (no drop, no HALT).
    RpcBudget,
    /// B6: the candidate's network deadline passed before its next provider call.
    RpcDeadline,
}

impl LossReason {
    /// Every variant, in declaration order.
    pub const ALL: [LossReason; 21] = [
        LossReason::Ingress,
        LossReason::State,
        LossReason::Health,
        LossReason::CircuitBreaker,
        LossReason::Bankruptcy,
        LossReason::IntegrationRefresh,
        LossReason::Funding,
        LossReason::MissingAta,
        LossReason::UnsupportedIntegration,
        LossReason::Sizing,
        LossReason::Economics,
        LossReason::Routing,
        LossReason::Build,
        LossReason::Submission,
        LossReason::Tip,
        LossReason::StateRace,
        LossReason::EconomicSkip,
        LossReason::ThresholdTooHigh,
        LossReason::Unknown,
        LossReason::RpcBudget,
        LossReason::RpcDeadline,
    ];

    /// Q3: the persisted/report name (legacy bottleneck key, shadow-ledger day losses) —
    /// explicit, never the derived Debug layout; the names every report and journal so far carry.
    pub const fn record_name(self) -> &'static str {
        match self {
            LossReason::Ingress => "Ingress",
            LossReason::State => "State",
            LossReason::Health => "Health",
            LossReason::CircuitBreaker => "CircuitBreaker",
            LossReason::Bankruptcy => "Bankruptcy",
            LossReason::IntegrationRefresh => "IntegrationRefresh",
            LossReason::Funding => "Funding",
            LossReason::MissingAta => "MissingAta",
            LossReason::UnsupportedIntegration => "UnsupportedIntegration",
            LossReason::Sizing => "Sizing",
            LossReason::Economics => "Economics",
            LossReason::Routing => "Routing",
            LossReason::Build => "Build",
            LossReason::Submission => "Submission",
            LossReason::Tip => "Tip",
            LossReason::StateRace => "StateRace",
            LossReason::EconomicSkip => "EconomicSkip",
            LossReason::ThresholdTooHigh => "ThresholdTooHigh",
            LossReason::Unknown => "Unknown",
            LossReason::RpcBudget => "RpcBudget",
            LossReason::RpcDeadline => "RpcDeadline",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateCostRecord {
    pub required_working_capital_usd_micro: Option<u64>,
    pub profit_per_capital_bps: Option<i128>,
    pub gross_seized_realizable_usd_micro: Option<u64>,
    pub repay_usd_micro: Option<u64>,
    pub protocol_fees_usd_micro: Option<u64>,
    pub flat_sol_fee_usd_micro: Option<u64>,
    pub swap_fees_and_impact_usd_micro: Option<u64>,
    pub jito_tip_usd_micro: Option<u64>,
    pub funding: String,
    pub path: String,
    pub message_fee_quote: Option<darkbot_solana::submit::MessageFeeQuote>,
    pub transaction_fee_usd_micro: Option<u64>,
    pub failed_attempt_reserve_usd_micro: Option<u64>,
    pub rent_and_token_usd_micro: Option<u64>,
    pub economics_known: bool,
    /// [P1-C] True iff a ZERO_PREFUND candidate reached economics without
    /// a router-provided executable minimum output. Evidence only:
    /// this never makes the candidate actionable.
    pub route_min_out_unknown: bool,
    pub net_profit_usd_micro: Option<i128>,
}

/// Same-clock points; missing evidence stays absent. Slot provenance is separate.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TimingEvidence {
    pub t_rx_ns: Option<u64>,
    pub t_state_ns: Option<u64>,
    pub t_detect_ns: Option<u64>,
    pub t_ready_ns: Option<u64>,
    pub t_sign_ns: Option<u64>,
    pub t_handoff_ns: Option<u64>,
}

impl CandidateCostRecord {
    /// Conservative cash reserve, distinct from inventory. No unknown term is zero.
    /// Swap cost is embedded in the executable floor, not an extra upfront cash debit.
    pub fn fee_tip_reserve(&self) -> Option<u64> {
        self.transaction_fee_usd_micro?
            .checked_add(self.jito_tip_usd_micro?)?
            .checked_add(self.rent_and_token_usd_micro?)?
            .checked_add(self.flat_sol_fee_usd_micro?)?
            .checked_add(self.protocol_fees_usd_micro?)?
            .checked_add(self.failed_attempt_reserve_usd_micro?)
    }
    pub fn total_budget(&self) -> Option<u64> {
        self.required_working_capital_usd_micro?
            .checked_add(self.fee_tip_reserve()?)
    }
    pub fn from_input(
        input: &darkbot_project0::economics::CandidateInput,
        eval: darkbot_project0::economics::Evaluation,
        failed: Option<u64>,
        quote: Option<darkbot_solana::submit::MessageFeeQuote>,
    ) -> Self {
        use darkbot_project0::economics::Evaluation;
        let (net, ppc) = match eval {
            Evaluation::Known {
                net,
                profit_per_capital_bps,
            } => (Some(net), Some(profit_per_capital_bps)),
            Evaluation::Unknown => (None, None),
        };
        Self {
            required_working_capital_usd_micro: Some(input.required_capital_usd_micro),
            profit_per_capital_bps: ppc,
            gross_seized_realizable_usd_micro: input.gross_seized_realizable_usd_micro,
            repay_usd_micro: Some(input.repay_usd_micro),
            protocol_fees_usd_micro: Some(input.protocol_fees_usd_micro),
            flat_sol_fee_usd_micro: Some(input.flat_sol_fee_usd_micro),
            swap_fees_and_impact_usd_micro: Some(input.swap_fees_and_impact_usd_micro),
            jito_tip_usd_micro: Some(input.jito_tip_usd_micro),
            funding: input.funding.as_str().into(),
            path: input.path.record_name().into(),
            message_fee_quote: quote,
            transaction_fee_usd_micro: input.transaction_fee_usd_micro,
            failed_attempt_reserve_usd_micro: failed,
            rent_and_token_usd_micro: input.rent_and_token_usd_micro,
            economics_known: net.is_some(),
            route_min_out_unknown: input.funding
                == darkbot_project0::funding::FundingClass::ZeroPrefundSameTx
                && input.gross_seized_realizable_usd_micro.is_none(),
            net_profit_usd_micro: net,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PipelineProgress {
    pub health_liquidatable: Option<bool>,
    pub candidates_created: Option<bool>,
    pub plan_validated: Option<bool>,
    pub transaction_built: Option<bool>,
}

/// Identity of the actual selected production plan. Missing legacy evidence stays absent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketPlanIdentity {
    pub liquidation_type: String,
    pub asset_bank: String,
    pub liability_bank: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpportunityRecord {
    #[serde(default)]
    pub progress: PipelineProgress,
    #[serde(default)]
    pub market_plan_identity: Option<MarketPlanIdentity>,
    pub timing: TimingEvidence,
    pub rx_slot: Option<u64>,
    pub detect_slot: Option<u64>,
    pub selected_costs: Option<CandidateCostRecord>,
    pub simulation_result: Option<String>,
    pub account: String,
    pub trigger: String,
    pub decision: Decision,
    pub net_profit: Option<i128>,
    /// False means technical qualification only; unknown economics were never coerced to zero.
    pub economics_known: bool,
    pub premium_bps: Option<u64>,
    pub t_profitable_unix_s: Option<u64>,
    pub tx_version: Option<String>,
    pub serialized_bytes: Option<usize>,
    pub locked_accounts: Option<usize>,
    pub path: Option<String>,
    pub funding: Option<String>,
    pub signature: Option<String>,
    /// [M2] Slot zur Zeit von T_sign (für Winner-Vergleich).
    pub sign_slot: Option<u64>,
    /// [FIX-6] CU aus der Simulation derselben TX.
    pub simulated_cu: Option<u64>,
    pub candidate_costs: Vec<CandidateCostRecord>,
    /// [FIX-5] Reserviertes Kapital der Submission (für Reconciliation).
    pub exposure: Option<u64>,
    /// [FIX-18] Alle Signaturen des Handoffs + Bundle-ID (vollständige Reconciliation).
    pub all_signatures: Vec<String>,
    pub bundle_id: Option<String>,
    pub submitted_tip: Option<crate::execution::pending::SubmittedTip>,
    pub intervals_ns: Vec<(String, Option<u64>)>,
    pub reason: Option<String>,
    pub loss: Option<LossReason>,
}

impl OpportunityRecord {
    pub fn with_timeline(mut self, t: &Timeline) -> Self {
        use darkbot_common::timing::Stage;
        self.timing = TimingEvidence {
            t_rx_ns: t.get(Stage::Rx),
            t_state_ns: t.get(Stage::State),
            t_detect_ns: t.get(Stage::Detect),
            t_ready_ns: t.get(Stage::Ready),
            t_sign_ns: t.get(Stage::Sign),
            t_handoff_ns: t.get(Stage::Handoff),
        };
        self.intervals_ns = t
            .standard_intervals()
            .iter()
            .map(|(n, v)| (n.to_string(), *v))
            .collect();
        self
    }
}

impl super::payload::Payload for OpportunityRecord {
    const KIND: u16 = super::journal::kind::OWNER_OPPORTUNITY;
}

/// F1: the simulation evidence of one failed candidate (journal only, never stderr/Display).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationFailureRecord {
    pub account: String,
    pub rx_slot: Option<u64>,
    pub failure: darkbot_solana::submit::SimulationFailure,
}
impl super::payload::Payload for SimulationFailureRecord {
    const KIND: u16 = super::journal::kind::OWNER_SIMULATION_FAILURE;
}

#[cfg(test)]
mod tests {
    /// Q3: every value written into journals and reports for these enums comes from an explicit
    /// name, never from a derived Debug layout, and equals what was written so far: the full
    /// name lists are pinned, and every value of the real historical evidence
    /// (`docs/evidence/*.json`: path `Rpc`, tx_version `V0`, losses Bankruptcy, StateRace,
    /// EconomicSkip, MissingAta, Routing, Sizing) reads back as a name.
    #[test]
    fn q3_persisted_names_are_explicit_and_unchanged() {
        use super::LossReason as L;
        use darkbot_project0::census::Unsupported as U;
        use darkbot_project0::economics::Path;
        use darkbot_solana::submit::{confirmation_status_name, SimResult, SimulationFailure};
        use darkbot_solana::tx::BuildVersion;
        use solana_transaction_status::TransactionConfirmationStatus as C;
        // `ALL` holds each variant exactly once (the match is exhaustive: a new variant fails to
        // compile here until it is listed and named).
        let index = |l: L| match l {
            L::Ingress => 0,
            L::State => 1,
            L::Health => 2,
            L::CircuitBreaker => 3,
            L::Bankruptcy => 4,
            L::IntegrationRefresh => 5,
            L::Funding => 6,
            L::MissingAta => 7,
            L::UnsupportedIntegration => 8,
            L::Sizing => 9,
            L::Economics => 10,
            L::Routing => 11,
            L::Build => 12,
            L::Submission => 13,
            L::Tip => 14,
            L::StateRace => 15,
            L::EconomicSkip => 16,
            L::ThresholdTooHigh => 17,
            L::Unknown => 18,
            L::RpcBudget => 19,
            L::RpcDeadline => 20,
        };
        for (i, l) in L::ALL.into_iter().enumerate() {
            assert_eq!(index(l), i);
        }
        assert_eq!(
            L::ALL.map(L::record_name),
            [
                "Ingress",
                "State",
                "Health",
                "CircuitBreaker",
                "Bankruptcy",
                "IntegrationRefresh",
                "Funding",
                "MissingAta",
                "UnsupportedIntegration",
                "Sizing",
                "Economics",
                "Routing",
                "Build",
                "Submission",
                "Tip",
                "StateRace",
                "EconomicSkip",
                "ThresholdTooHigh",
                "Unknown",
                "RpcBudget",
                "RpcDeadline"
            ]
        );
        let names = L::ALL.map(L::record_name);
        for historical in [
            "Bankruptcy",
            "StateRace",
            "EconomicSkip",
            "MissingAta",
            "Routing",
            "Sizing",
        ] {
            assert!(names.contains(&historical), "{historical}");
        }
        assert_eq!(
            [Path::Rpc, Path::JitoSingle, Path::JitoBundle].map(Path::record_name),
            ["Rpc", "JitoSingle", "JitoBundle"]
        );
        assert_eq!(
            [BuildVersion::Legacy, BuildVersion::V0, BuildVersion::V1]
                .map(BuildVersion::record_name),
            ["Legacy", "V0", "V1"]
        );
        let failure = SimulationFailure {
            code: "RPC_SIMULATION_FAILED InstructionError(3, Custom(6009))".into(),
            units_consumed: Some(7),
            logs: vec!["Program log: SENTINEL".into()],
            log_lines: Some(1),
            logs_truncated: false,
        };
        assert_eq!(
            [
                SimResult::Ok {
                    units_consumed: Some(1)
                },
                SimResult::Failed(failure),
                SimResult::Unavailable("RPC_SIMULATION_DECODE".into()),
                SimResult::Unavailable("quote \"x\"".into()),
                SimResult::Deadline,
                SimResult::BudgetRejected("RPC_RATE_LIMIT"),
            ]
            .map(|r| r.record_text()),
            [
                "PASS",
                "Failed(\"RPC_SIMULATION_FAILED InstructionError(3, Custom(6009))\")",
                "Unavailable(\"RPC_SIMULATION_DECODE\")",
                "Unavailable(\"quote \\\"x\\\"\")",
                "Deadline",
                "BudgetRejected(\"RPC_RATE_LIMIT\")",
            ]
        );
        assert_eq!(
            [C::Processed, C::Confirmed, C::Finalized].map(|c| confirmation_status_name(&c)),
            ["Processed", "Confirmed", "Finalized"]
        );
        assert_eq!(
            [
                U::UnknownLayout,
                U::Unsupported,
                U::Stale,
                U::MissingAccount,
                U::Crate,
                U::Unavailable,
                U::Premium,
                U::Facts,
                U::ValueRange,
                U::AccountLayout,
            ]
            .map(U::record_name),
            [
                "UnknownLayout",
                "Unsupported",
                "Stale",
                "MissingAccount",
                "Crate",
                "Unavailable",
                "Premium",
                "Facts",
                "ValueRange",
                "AccountLayout",
            ]
        );
    }

    #[test]
    fn health_json_preserves_full_protocol_fixed_point_range() {
        // R115 crashed in json! for an otherwise valid large Health value. These
        // are raw I80F48 bits, not floats: losing low bits also loses evidence.
        for bits in [
            i128::MIN,
            i64::MIN as i128 - 1,
            0,
            u64::MAX as i128 + 1,
            i128::MAX,
        ] {
            let row = serde_json::json!({
                "maint_health_bits": bits,
                "equity_assets_bits": bits,
            });
            let encoded = serde_json::to_string(&row).unwrap();
            let decoded: std::collections::BTreeMap<String, i128> =
                serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded["maint_health_bits"], bits);
            assert_eq!(decoded["equity_assets_bits"], bits);
            assert_eq!(row["maint_health_bits"].to_string(), bits.to_string());
        }
    }
}
