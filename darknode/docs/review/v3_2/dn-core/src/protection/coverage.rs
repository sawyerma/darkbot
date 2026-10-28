//! V3.2: pure, read-only stop coverage decision. NEVER dispatch orders here.
//! B02/B06/B24/B35: full identity + source/snapshot proof, no repeat actions.

use std::collections::HashSet;
use crate::model::{is_fresh, PositionProof, Side, StopExpectation, StopsProof};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Coverage {
    Unknown(&'static str),
    ConfirmedFlat,
    ConfirmedFull,
    ConfirmedPartial { covered: i64, missing: i64 },
    ConfirmedMissing { missing: i64 },
    Conflict(&'static str),
}

/// Returning ConfirmedFull means *this identity* has a complete, fresh,
/// cross-correlated stop readback; a REST acknowledgment does not qualify.
/// Evidence must be typed by the Bitget adapter, not self-attested by UI.
pub fn evaluate(
    expected: Option<&StopExpectation>,
    position: Option<&PositionProof>,
    stops: Option<&StopsProof>,
    required_fill_epoch: u64,
    now_mono_ms: u64,
    max_age_ms: u64,
) -> Coverage {
    let Some(pos) = position else { return Coverage::Unknown("POSITION_NOT_READ"); };
    if !pos.complete || !pos.scope_verified || !is_fresh(now_mono_ms, pos.observed_mono_ms, max_age_ms)
        || pos.applied_fill_epoch < required_fill_epoch
    {
        return Coverage::Unknown("POSITION_SNAPSHOT_UNQUALIFIED");
    }
    let Some(stop_book) = stops else { return Coverage::Unknown("STOPS_NOT_READ"); };
    if !stop_book.complete || !stop_book.scope_verified || stop_book.key != pos.key
        || stop_book.after_position_generation < pos.generation
        || stop_book.after_fill_epoch < required_fill_epoch
        || !is_fresh(now_mono_ms, stop_book.observed_mono_ms, max_age_ms)
    {
        return Coverage::Unknown("STOP_SNAPSHOT_UNQUALIFIED");
    }
    if pos.signed_lots == 0 {
        if stop_book.rows.iter().any(|s| s.exchange_active && s.key == pos.key) {
            return Coverage::Conflict("LIVE_STOP_ON_VERIFIED_FLAT");
        }
        return Coverage::ConfirmedFlat;
    }
    if pos.signed_lots == i64::MIN { return Coverage::Conflict("INVALID_POSITION_MAGNITUDE"); }
    let required = pos.signed_lots.abs();
    let Some(exp) = expected else { return Coverage::Unknown("EXPECTED_STOP_NOT_RECOVERED"); };
    if exp.position_key != pos.key || exp.entry_intent_id.is_empty()
        || exp.established_after_fill_epoch > pos.applied_fill_epoch
    {
        return Coverage::Unknown("EXPECTED_STOP_NOT_VERIFIED");
    }
    let closing_side = if pos.signed_lots > 0 { Side::Sell } else { Side::Buy };
    let mut covered: i64 = 0;
    let mut seen_ids = HashSet::new();
    let mut foreign_or_wrong_same_scope = false;
    for row in stop_book.rows.iter().filter(|s| s.exchange_active && s.key == pos.key) {
        if row.exchange_strategy_order_id.is_empty()
            || !seen_ids.insert(row.exchange_strategy_order_id.as_str())
        {
            return Coverage::Conflict("DUPLICATE_OR_UNIDENTIFIED_EXCHANGE_STOP");
        }
        let linked = row.binding_verified
            && row.bound_stop_intent.as_ref().is_some_and(|id| exp.bound_stop_intents.contains(id));
        let exact = linked && row.side == closing_side && row.reduce_only_verified
            && row.kind == exp.required_kind && row.trigger_by == exp.trigger_by
            && row.trigger_ticks == exp.trigger_ticks && row.remaining_lots > 0;
        if exact {
            let Some(sum) = covered.checked_add(row.remaining_lots) else {
                return Coverage::Conflict("STOP_QUANTITY_OVERFLOW");
            };
            covered = sum;
        } else {
            // Do not treat a foreign or malformed strategy stop as ours.
            // Do not auto-cancel such stops: ownership is not established.
            foreign_or_wrong_same_scope = true;
        }
    }
    if foreign_or_wrong_same_scope {
        return Coverage::Conflict("FOREIGN_OR_WRONG_STOP_IN_POSITION_SCOPE");
    }
    if covered > required { return Coverage::Conflict("STOP_OVERCOVERAGE"); }
    if covered == required { return Coverage::ConfirmedFull; }
    if covered == 0 { return Coverage::ConfirmedMissing { missing: required }; }
    Coverage::ConfirmedPartial { covered, missing: required - covered }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    fn key() -> PositionKey { PositionKey { account_uid: "uid".into(), category: "USDT-FUTURES".into(), symbol:"BTCUSDT".into(), pos_side:"one_way_mode".into() } }
    fn pos(lots: i64) -> PositionProof { PositionProof { key:key(), signed_lots:lots, generation:9, applied_fill_epoch:7, observed_mono_ms:100, complete:true, scope_verified:true } }
    fn exp() -> StopExpectation { StopExpectation { position_key:key(), entry_intent_id:"entry7".into(), bound_stop_intents:vec!["stop7".into()], trigger_ticks:90, trigger_by:TriggerBy::Mark, required_kind:StopKind::Market, established_after_fill_epoch:7 } }
    fn stop() -> StopReadback { StopReadback { key:key(), exchange_strategy_order_id:"s1".into(), bound_stop_intent:Some("stop7".into()), binding_verified:true, side:Side::Sell, reduce_only_verified:true, kind:StopKind::Market, trigger_ticks:90, trigger_by:TriggerBy::Mark, remaining_lots:4, exchange_active:true } }
    fn book(rows:Vec<StopReadback>) -> StopsProof { StopsProof { key:key(), generation:10, after_position_generation:9, after_fill_epoch:7, observed_mono_ms:101, complete:true, scope_verified:true, rows } }
    #[test] fn linked_stop_covers_position() { assert_eq!(evaluate(Some(&exp()),Some(&pos(4)),Some(&book(vec![stop()])),7,102,100),Coverage::ConfirmedFull); }
    #[test] fn unknown_expected_never_means_missing() { assert_eq!(evaluate(None,Some(&pos(4)),Some(&book(vec![])),7,102,100),Coverage::Unknown("EXPECTED_STOP_NOT_RECOVERED")); }
    #[test] fn foreign_matching_price_is_not_our_stop() { let mut s=stop(); s.bound_stop_intent=Some("other-bot".into());assert!(matches!(evaluate(Some(&exp()),Some(&pos(4)),Some(&book(vec![s])),7,102,100),Coverage::Conflict(_))); }
    #[test] fn stale_position_never_falsely_flat() { let mut p=pos(0);p.complete=false;assert!(matches!(evaluate(None,Some(&p),Some(&book(vec![])),7,102,100),Coverage::Unknown(_))); }
    #[test] fn unconfirmed_position_missing_stop_requires_full_readback() { let mut b=book(vec![]);b.complete=false;assert!(matches!(evaluate(Some(&exp()),Some(&pos(4)),Some(&b),7,102,100),Coverage::Unknown(_))); }
    #[test] fn complete_missing_stop_is_a_different_state() { assert_eq!(evaluate(Some(&exp()),Some(&pos(4)),Some(&book(vec![])),7,102,100),Coverage::ConfirmedMissing{missing:4}); }
    #[test] fn old_stop_book_does_not_cover_new_fill() { let mut b=book(vec![stop()]);b.after_fill_epoch=6;assert!(matches!(evaluate(Some(&exp()),Some(&pos(4)),Some(&b),7,102,100),Coverage::Unknown(_))); }
}

