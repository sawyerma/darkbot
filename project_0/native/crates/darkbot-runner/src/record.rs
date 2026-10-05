//! P6-LT: Opportunity-Record (Ledger-Vokabular). Eine Zeile JSONL pro Entscheidung.

use darkbot_common::timing::Timeline;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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

#[derive(Clone, Debug, Serialize)]
pub struct OpportunityRecord {
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
        self.intervals_ns = t.standard_intervals().iter().map(|(n, v)| (n.to_string(), *v)).collect();
        self
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn health_json_preserves_full_protocol_fixed_point_range() {
        // R115 crashed in json! for an otherwise valid large Health value. These
        // are raw I80F48 bits, not floats: losing low bits also loses evidence.
        for bits in [i128::MIN, i64::MIN as i128 - 1, 0, u64::MAX as i128 + 1, i128::MAX] {
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
