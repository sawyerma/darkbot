//! Review-model shapes. Reconcile with existing dn-types before any import.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PositionKey {
    pub account_uid: String,
    pub category: String,
    pub symbol: String,
    pub pos_side: String, // V1 expected: one-way; validate on adapter boundary
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side { Buy, Sell }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerBy { Mark, Last }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind { Market, Limit }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionProof {
    pub key: PositionKey,
    pub signed_lots: i64,
    pub generation: u64,
    pub applied_fill_epoch: u64,
    pub observed_mono_ms: u64,
    pub complete: bool,
    pub scope_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopExpectation {
    pub position_key: PositionKey,
    pub entry_intent_id: String,
    /// Journal-approved stop intent IDs, never arbitrary matching orders.
    pub bound_stop_intents: Vec<String>,
    pub trigger_ticks: i64,
    pub trigger_by: TriggerBy,
    pub required_kind: StopKind,
    pub established_after_fill_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopReadback {
    pub key: PositionKey,
    pub exchange_strategy_order_id: String,
    pub bound_stop_intent: Option<String>,
    /// True only after cross-check with persisted strategyId/clientOid mapping.
    pub binding_verified: bool,
    pub side: Side,
    pub reduce_only_verified: bool,
    pub kind: StopKind,
    pub trigger_ticks: i64,
    pub trigger_by: TriggerBy,
    pub remaining_lots: i64,
    pub exchange_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopsProof {
    pub key: PositionKey,
    pub generation: u64,
    pub after_position_generation: u64,
    pub after_fill_epoch: u64,
    pub observed_mono_ms: u64,
    pub complete: bool,
    pub scope_verified: bool,
    pub rows: Vec<StopReadback>,
}

pub fn is_fresh(now: u64, observed: u64, max_age: u64) -> bool {
    now.checked_sub(observed).is_some_and(|age| age <= max_age)
}

