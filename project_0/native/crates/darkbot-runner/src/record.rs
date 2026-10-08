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
            path: format!("{:?}", input.path),
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpportunityRecord {
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
    pub submitted_tip: Option<crate::pending::SubmittedTip>,
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

#[cfg(test)]
mod tests {
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
