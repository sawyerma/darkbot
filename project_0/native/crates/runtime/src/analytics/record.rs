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
    /// V17: a planning failure without a cause of its own (state, layout, instruction build
    /// before the plan gate); attributed to `Routing` before V17.
    Plan,
    /// V9: a Safety limit refused the candidate (LIVE preview or admission); before V9 the
    /// preview counted as an economic skip and the admission had no loss.
    Safety,
}

impl LossReason {
    /// Every variant, in declaration order.
    pub const ALL: [LossReason; 23] = [
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
        LossReason::Plan,
        LossReason::Safety,
    ];

    /// V9/V17: causes a record of the earlier attribution (ECON_SHADOW_EVALUATION schema 2 and
    /// before) cannot carry; the variants were appended, so every earlier value keeps its index.
    pub const fn since_exhaustive_v2(self) -> bool {
        matches!(self, LossReason::Plan | LossReason::Safety)
    }

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
            LossReason::Plan => "Plan",
            LossReason::Safety => "Safety",
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
    /// DEC-12: typed at its source; its text is made by the writer (`Rejection::text`).
    pub reason: Option<super::rejection::Rejection>,
    pub loss: Option<LossReason>,
}

impl OpportunityRecord {
    /// DEC-12: the record text of the typed rejection (what the writer writes).
    pub fn reason_text(&self) -> Option<String> {
        self.reason.as_ref().map(|r| r.text().into_owned())
    }
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
    /// DEC-12: 3 — the typed rejection and the native message hash.
    const SCHEMA: u16 = 3;
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<Self>> {
        (schema == super::payload::TYPED).then(|| {
            super::payload::decode_wire::<OpportunityRecordV2>(bytes).and_then(TryInto::try_into)
        })
    }
}

/// DEC-12: the record layout before DEC-12 (`OWNER_OPPORTUNITY` schema 2, embedded in
/// `ECON_SHADOW_EVALUATION` ≤ 4, `ECON_CANONICAL` ≤ 4, `ECON_OPPORTUNITY` ≤ 3): the reason as
/// its text, the quoted message hash as its base58 text. Read only; its text stays verbatim
/// (`RejectionCode::Earlier`), the hash text must be a hash.
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
pub(crate) struct OpportunityRecordV2 {
    #[serde(default)]
    progress: PipelineProgress,
    #[serde(default)]
    market_plan_identity: Option<MarketPlanIdentity>,
    timing: TimingEvidence,
    rx_slot: Option<u64>,
    detect_slot: Option<u64>,
    selected_costs: Option<CandidateCostRecordV1>,
    simulation_result: Option<String>,
    account: String,
    trigger: String,
    decision: Decision,
    net_profit: Option<i128>,
    economics_known: bool,
    premium_bps: Option<u64>,
    t_profitable_unix_s: Option<u64>,
    tx_version: Option<String>,
    serialized_bytes: Option<usize>,
    locked_accounts: Option<usize>,
    path: Option<String>,
    funding: Option<String>,
    signature: Option<String>,
    sign_slot: Option<u64>,
    simulated_cu: Option<u64>,
    candidate_costs: Vec<CandidateCostRecordV1>,
    exposure: Option<u64>,
    all_signatures: Vec<String>,
    bundle_id: Option<String>,
    submitted_tip: Option<crate::execution::pending::SubmittedTip>,
    intervals_ns: Vec<(String, Option<u64>)>,
    reason: Option<String>,
    pub(crate) loss: Option<LossReason>,
}

/// DEC-12: `CandidateCostRecord` before DEC-12 (the message hash as text).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct CandidateCostRecordV1 {
    required_working_capital_usd_micro: Option<u64>,
    profit_per_capital_bps: Option<i128>,
    gross_seized_realizable_usd_micro: Option<u64>,
    repay_usd_micro: Option<u64>,
    protocol_fees_usd_micro: Option<u64>,
    flat_sol_fee_usd_micro: Option<u64>,
    swap_fees_and_impact_usd_micro: Option<u64>,
    jito_tip_usd_micro: Option<u64>,
    funding: String,
    path: String,
    message_fee_quote: Option<MessageFeeQuoteV1>,
    transaction_fee_usd_micro: Option<u64>,
    failed_attempt_reserve_usd_micro: Option<u64>,
    rent_and_token_usd_micro: Option<u64>,
    economics_known: bool,
    route_min_out_unknown: bool,
    net_profit_usd_micro: Option<i128>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct MessageFeeQuoteV1 {
    message_hash: String,
    context_slot: u64,
    lamports: u64,
}

impl TryFrom<CandidateCostRecordV1> for CandidateCostRecord {
    type Error = std::io::Error;
    fn try_from(v: CandidateCostRecordV1) -> std::io::Result<Self> {
        let message_fee_quote = match v.message_fee_quote {
            Some(q) => Some(darkbot_solana::submit::MessageFeeQuote {
                message_hash: q
                    .message_hash
                    .parse()
                    .map_err(|_| super::journal::coded("PAYLOAD_DECODE"))?,
                context_slot: q.context_slot,
                lamports: q.lamports,
            }),
            None => None,
        };
        Ok(Self {
            required_working_capital_usd_micro: v.required_working_capital_usd_micro,
            profit_per_capital_bps: v.profit_per_capital_bps,
            gross_seized_realizable_usd_micro: v.gross_seized_realizable_usd_micro,
            repay_usd_micro: v.repay_usd_micro,
            protocol_fees_usd_micro: v.protocol_fees_usd_micro,
            flat_sol_fee_usd_micro: v.flat_sol_fee_usd_micro,
            swap_fees_and_impact_usd_micro: v.swap_fees_and_impact_usd_micro,
            jito_tip_usd_micro: v.jito_tip_usd_micro,
            funding: v.funding,
            path: v.path,
            message_fee_quote,
            transaction_fee_usd_micro: v.transaction_fee_usd_micro,
            failed_attempt_reserve_usd_micro: v.failed_attempt_reserve_usd_micro,
            rent_and_token_usd_micro: v.rent_and_token_usd_micro,
            economics_known: v.economics_known,
            route_min_out_unknown: v.route_min_out_unknown,
            net_profit_usd_micro: v.net_profit_usd_micro,
        })
    }
}

impl TryFrom<OpportunityRecordV2> for OpportunityRecord {
    type Error = std::io::Error;
    fn try_from(v: OpportunityRecordV2) -> std::io::Result<Self> {
        Ok(Self {
            progress: v.progress,
            market_plan_identity: v.market_plan_identity,
            timing: v.timing,
            rx_slot: v.rx_slot,
            detect_slot: v.detect_slot,
            selected_costs: v.selected_costs.map(TryInto::try_into).transpose()?,
            simulation_result: v.simulation_result,
            account: v.account,
            trigger: v.trigger,
            decision: v.decision,
            net_profit: v.net_profit,
            economics_known: v.economics_known,
            premium_bps: v.premium_bps,
            t_profitable_unix_s: v.t_profitable_unix_s,
            tx_version: v.tx_version,
            serialized_bytes: v.serialized_bytes,
            locked_accounts: v.locked_accounts,
            path: v.path,
            funding: v.funding,
            signature: v.signature,
            sign_slot: v.sign_slot,
            simulated_cu: v.simulated_cu,
            candidate_costs: v
                .candidate_costs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<std::io::Result<_>>()?,
            exposure: v.exposure,
            all_signatures: v.all_signatures,
            bundle_id: v.bundle_id,
            submitted_tip: v.submitted_tip,
            intervals_ns: v.intervals_ns,
            reason: v.reason.map(super::rejection::Rejection::earlier),
            loss: v.loss,
        })
    }
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

/// DEC-12 (tests): the record bytes of the writer before DEC-12 — the pinned schema-2 shadow
/// evaluation (no previous offset, then the record).
#[cfg(test)]
pub(crate) fn earlier_writer_record_bytes() -> Vec<u8> {
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/analytics/fixtures/shadow_evaluations_schema2.p0j");
    let mut reader = super::journal::Reader::open(&fixture).unwrap();
    let raw = loop {
        let f = reader.next_frame().unwrap().unwrap();
        if f.kind != super::journal::MANIFEST {
            break f.payload;
        }
    };
    assert_eq!(raw[0], 0, "no previous offset");
    raw[1..].to_vec()
}

#[cfg(test)]
mod dec12_tests {
    use super::*;
    use crate::analytics::journal::kind::OWNER_OPPORTUNITY;
    use crate::analytics::payload;
    use crate::analytics::rejection::RejectionCode;
    use solana_sdk::hash::Hash;

    /// DEC-12: records of the writer before DEC-12 stay readable. The mirror of the earlier
    /// layout re-encodes the earlier writer's record byte for byte; its reason text stays
    /// verbatim (`Earlier`); its quoted message hash text becomes the native hash and a text
    /// that is no hash is refused; the current schema writes the hash as its 32 bytes and shows
    /// the base58 text and the reason text in a human-readable view.
    #[test]
    fn dec12_records_of_the_earlier_writer_stay_readable() {
        let bytes = earlier_writer_record_bytes();
        let mut v2: OpportunityRecordV2 = payload::decode_wire(&bytes).unwrap();
        assert_eq!(
            payload::encode_wire(&v2).unwrap(),
            bytes,
            "mirror = earlier writer"
        );
        let rec: OpportunityRecord = payload::decode(OWNER_OPPORTUNITY, 2, &bytes).unwrap();
        assert_eq!(
            rec.reason
                .as_ref()
                .map(|r| (r.code(), r.text().into_owned())),
            Some((RejectionCode::Earlier, "mint not in state".to_string()))
        );
        let hash = Hash::new_unique();
        let quoted = |message_hash: String| CandidateCostRecordV1 {
            required_working_capital_usd_micro: Some(1),
            profit_per_capital_bps: None,
            gross_seized_realizable_usd_micro: None,
            repay_usd_micro: Some(2),
            protocol_fees_usd_micro: Some(3),
            flat_sol_fee_usd_micro: Some(4),
            swap_fees_and_impact_usd_micro: Some(5),
            jito_tip_usd_micro: Some(0),
            funding: "ZERO_PREFUND_SAME_TX".into(),
            path: "Rpc".into(),
            message_fee_quote: Some(MessageFeeQuoteV1 {
                message_hash,
                context_slot: 7,
                lamports: 5_000,
            }),
            transaction_fee_usd_micro: Some(6),
            failed_attempt_reserve_usd_micro: None,
            rent_and_token_usd_micro: Some(0),
            economics_known: false,
            route_min_out_unknown: true,
            net_profit_usd_micro: None,
        };
        v2.candidate_costs = vec![quoted(hash.to_string())];
        let earlier = payload::encode_wire(&v2).unwrap();
        let rec: OpportunityRecord = payload::decode(OWNER_OPPORTUNITY, 2, &earlier).unwrap();
        let quote = rec.candidate_costs[0].message_fee_quote.clone().unwrap();
        assert_eq!((quote.message_hash, quote.context_slot), (hash, 7));
        v2.candidate_costs = vec![quoted("not a hash".into())];
        let forged = payload::encode_wire(&v2).unwrap();
        assert!(payload::decode::<OpportunityRecord>(OWNER_OPPORTUNITY, 2, &forged).is_err());
        // The current schema: 32 hash bytes, round trip, human-readable texts.
        let current = payload::encode(&rec).unwrap();
        let back: OpportunityRecord = payload::decode(OWNER_OPPORTUNITY, 3, &current).unwrap();
        assert_eq!(
            back.candidate_costs[0]
                .message_fee_quote
                .as_ref()
                .unwrap()
                .message_hash,
            hash
        );
        assert_eq!(back.reason, rec.reason);
        assert_eq!(
            payload::encoded_len(&quote_only(hash)).unwrap(),
            32 + 8 + 8,
            "the hash is its 32 bytes"
        );
        let view = serde_json::to_value(&back).unwrap();
        assert_eq!(
            view["candidate_costs"][0]["message_fee_quote"]["message_hash"],
            hash.to_string()
        );
        assert_eq!(view["reason"], "mint not in state");
    }
    fn quote_only(hash: Hash) -> QuoteOnly {
        QuoteOnly(darkbot_solana::submit::MessageFeeQuote {
            message_hash: hash,
            context_slot: 7,
            lamports: 5_000,
        })
    }
    #[derive(Serialize, Deserialize)]
    struct QuoteOnly(darkbot_solana::submit::MessageFeeQuote);
    impl payload::Payload for QuoteOnly {
        const KIND: u16 = OWNER_OPPORTUNITY;
    }
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
            L::Plan => 21,
            L::Safety => 22,
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
                "RpcDeadline",
                "Plan",
                "Safety"
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
                    units_consumed: Some(1),
                    context_slot: None,
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

    /// V18 (Q3 review addendum): the own types inside the free-text `reason` — Safety denials
    /// (with every halt reason), plan-gate refusals and economics evaluations — come from
    /// explicit texts equal to what the derived Debug wrote so far, pinned here.
    #[test]
    fn q3_reason_texts_of_own_types_are_explicit_and_unchanged() {
        use darkbot_common::error::HaltReason as H;
        use darkbot_project0::economics::Evaluation;
        use darkbot_project0::planner::PlanError as P;
        use darkbot_project0::safety::Deny as D;
        let denies = [
            D::TotalCapital,
            D::Inventory,
            D::ConcurrentPositions,
            D::AssetNotAllowed,
            D::ExecutionClassNotAllowed,
            D::Exposure,
            D::DailyLoss,
            D::FailedAttempts,
            D::Tip,
            D::Slippage,
            D::Outstanding,
            D::LiveDisabled,
            D::Halted(H::FeedGap),
            D::Halted(H::StateGap),
            D::Halted(H::ReconcileFailure),
            D::Halted(H::ProgramUpgrade),
            D::Halted(H::UnknownLayout),
            D::Halted(H::UnknownOracle),
            D::Halted(H::UnknownIntegration),
            D::Halted(H::UnknownEconomics),
            D::Halted(H::UnknownVersion),
            D::Halted(H::LimitBreached),
            D::Halted(H::Shutdown),
            D::Halted(H::LedgerInvariant),
        ];
        assert_eq!(
            denies.map(D::record_text),
            [
                "TotalCapital",
                "Inventory",
                "ConcurrentPositions",
                "AssetNotAllowed",
                "ExecutionClassNotAllowed",
                "Exposure",
                "DailyLoss",
                "FailedAttempts",
                "Tip",
                "Slippage",
                "Outstanding",
                "LiveDisabled",
                "Halted(FeedGap)",
                "Halted(StateGap)",
                "Halted(ReconcileFailure)",
                "Halted(ProgramUpgrade)",
                "Halted(UnknownLayout)",
                "Halted(UnknownOracle)",
                "Halted(UnknownIntegration)",
                "Halted(UnknownEconomics)",
                "Halted(UnknownVersion)",
                "Halted(LimitBreached)",
                "Halted(Shutdown)",
                "Halted(LedgerInvariant)",
            ]
        );
        let plans = [
            P::NoStart,
            P::NoEnd,
            P::EndNotLast,
            P::StepBeforeStartNotAllowed(3),
            P::ProgramNotAllowlisted(1),
            P::OrderViolation(2, "repay before \"withdraw\""),
            P::RepayMissing,
            P::PaidRouteBeforeP7,
            P::UnknownInstruction(4),
        ];
        assert_eq!(
            plans.each_ref().map(P::record_text),
            [
                "NoStart",
                "NoEnd",
                "EndNotLast",
                "StepBeforeStartNotAllowed(3)",
                "ProgramNotAllowlisted(1)",
                "OrderViolation(2, \"repay before \\\"withdraw\\\"\")",
                "RepayMissing",
                "PaidRouteBeforeP7",
                "UnknownInstruction(4)",
            ]
        );
        let evaluations = [
            Evaluation::Known {
                net: -5,
                profit_per_capital_bps: 12,
            },
            Evaluation::Unknown,
        ];
        assert_eq!(
            evaluations.map(Evaluation::record_text),
            ["Known { net: -5, profit_per_capital_bps: 12 }", "Unknown"]
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
