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
