//! P6-LT: Opportunity-Record (Ledger-Vokabular). Eine Zeile JSONL pro Entscheidung.

use darkbot_common::timing::Timeline;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Decision {
    Skip,
    ShadowReady,
    Submitted,
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
    pub premium_bps: Option<u64>,
    pub t_profitable_unix_s: Option<u64>,
    pub tx_version: Option<String>,
    pub serialized_bytes: Option<usize>,
    pub locked_accounts: Option<usize>,
    pub path: Option<String>,
    pub funding: Option<String>,
    pub signature: Option<String>,
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
