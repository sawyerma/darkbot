//! Landing-Telemetrie (`OwnerEvidence::Reconcile`, Kind OWNER_RECONCILE).
//! F-04/C02: einziger Erzeuger ist das FE-04-Settlement (`state_owner/fe04.rs`,
//! `SettlementBooking::landing_record`) — kein zweiter RAM-Reconcile-Pfad. Der finanzielle
//! Abschluss jedes Versuchs bleibt das Settlement des FE-04-Wallet-Journals.

use serde::Serialize;

/// Only constructed from our signed, single-transfer tip transaction.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct SubmittedTip {
    pub signature: String,
    pub lamports: u64,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct ReconcileRecord {
    pub signature: String,
    pub path: std::borrow::Cow<'static, str>,
    pub outcome: String,
    pub landed_slot: Option<u64>,
    pub fee_lamports: Option<u64>,
    pub compute_units_consumed: Option<u64>,
    pub paid_tip_lamports: Option<u64>,
    pub network_and_tip_lamports: Option<u64>,
    pub error: Option<String>,
    pub exposure_released: u64,
    pub pnl_complete: bool,
}

impl crate::analytics::payload::Payload for ReconcileRecord {
    const KIND: u16 = crate::analytics::journal::kind::OWNER_RECONCILE;
}
