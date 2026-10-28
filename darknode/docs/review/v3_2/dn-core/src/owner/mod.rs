//! V3.2 review-only serial Owner decision core (B01, B08, B24, B35-B44).
//! Exchange I/O is deliberately absent. All planned actions go through
//! DurableStore::commit *before* any external dispatcher receives them.
//! Production MUST bind this trait to the real SQLite journal, including
//! atomic intent/reservation/outbox updates, full durability and crash tests.

use std::collections::BTreeMap;
use crate::model::{PositionKey, PositionProof, StopExpectation, StopsProof, is_fresh};
use crate::protection::coverage::{self, Coverage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode { Reconciling, Halted, Active, EmergencyRecovery }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState { Prepared, Dispatching, Unknown, AckAwaitingReadback, VerifiedDone }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    ReadPositions,
    ReadStops,
    LookupOrder { intent_id: String },
    ReadFills { intent_id: String },
    PlaceStop { stop_intent_id: String, lots: i64 },
    EmergencyClose { close_intent_id: String, lots: i64 },
}
impl ActionKind { pub fn is_write(&self) -> bool { matches!(self, Self::PlaceStop {..} | Self::EmergencyClose {..}) } }

#[derive(Debug, Clone)]
pub struct ActionRecord {
    pub id: u64,
    pub key: String,
    pub kind: ActionKind,
    pub state: ActionState,
    pub created_mono_ms: u64,
}
#[derive(Debug, Clone)]
pub struct AccountBlock {
    pub id: String,
    pub reason: String,
    pub resolved_with_evidence: Option<String>,
}
#[derive(Debug, Clone, Default)]
pub struct Persistent {
    pub next_action_id: u64,
    pub actions: BTreeMap<u64, ActionRecord>,
    pub blocks: BTreeMap<String, AccountBlock>,
    pub expected_stop_from_journal: Option<StopExpectation>,
    pub latest_fill_epoch: u64,
    /// Within one runtime epoch only. On recovery restart the grace timer.
    pub confirmed_missing_since_mono_ms: Option<u64>,
    pub last_emergency_intent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerError {
    Storage,
    InvalidEvidence(&'static str),
    NotReady(&'static str),
    NotFound,
    NotPermitted(&'static str),
}

/// A production implementation must commit the *whole* snapshot atomically,
/// with durable journal semantics. This small draft does not implement SQL.
pub trait DurableStore {
    fn load(&self) -> Result<Persistent, OwnerError>;
    fn commit(&mut self, next: &Persistent) -> Result<(), OwnerError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ActivationEvidence {
    pub feed_fresh: bool,
    pub clock_qualified: bool,
    pub one_way_isolated_and_leverage_readback: bool,
    pub daily_pnl_baseline_fresh: bool,
    pub wallet_equity_fresh: bool,
    pub instrument_version_match: bool,
    pub fees_and_risk_policy_version_match: bool,
    pub private_ws_recovered: bool,
    pub all_order_identities_reconciled: bool,
}
impl ActivationEvidence {
    fn all(self) -> bool {
        self.feed_fresh && self.clock_qualified
            && self.one_way_isolated_and_leverage_readback
            && self.daily_pnl_baseline_fresh && self.wallet_equity_fresh
            && self.instrument_version_match && self.fees_and_risk_policy_version_match
            && self.private_ws_recovered && self.all_order_identities_reconciled
    }
}

/// Only this serial owner mutates Persistent. No Bitget keys or signer inside.
pub struct Owner<S: DurableStore> {
    store: S,
    db: Persistent,
    pub mode: Mode,
    pub key: PositionKey,
    pub position: Option<PositionProof>,
    pub stops: Option<StopsProof>,
    pub max_readback_age_ms: u64,
    pub missing_stop_grace_ms: u64,
    pub evidence: ActivationEvidence,
    /// Fail closed until UTA V3 strategy-order capability is qualified.
    pub stop_repair_capability_qualified: bool,
    /// Fail closed until explicit emergency reduce-only capability is qualified.
    pub emergency_capability_qualified: bool,
    pub last_coverage: Coverage,
}

impl<S: DurableStore> Owner<S> {
    pub fn new(store: S, key: PositionKey) -> Result<Self, OwnerError> {
        let db = store.load()?;
        Ok(Self { store, db, mode: Mode::Halted, key, position: None,
            stops: None, max_readback_age_ms: 5_000, missing_stop_grace_ms: 10_000,
            evidence: ActivationEvidence::default(),
            stop_repair_capability_qualified: false,
            emergency_capability_qualified: false,
            last_coverage: Coverage::Unknown("NOT_RECONCILED") })
    }

    /// Prevent a state change if durability fails.
    fn persist(&mut self, next: Persistent) -> Result<(), OwnerError> {
        self.store.commit(&next)?;
        self.db = next;
        Ok(())
    }

    pub fn blocks_open(&self) -> bool {
        self.db.blocks.values().any(|b| b.resolved_with_evidence.is_none())
    }

    pub fn open_block(&mut self, id: &str, reason: &str) -> Result<(), OwnerError> {
        if id.is_empty() || reason.is_empty() { return Err(OwnerError::InvalidEvidence("BAD_BLOCK")); }
        let mut next = self.db.clone();
        next.blocks.entry(id.into()).or_insert(AccountBlock {
            id: id.into(), reason: reason.into(), resolved_with_evidence: None,
        });
        self.persist(next)?;
        self.mode = Mode::Reconciling;
        Ok(())
    }
    /// B36: no clear_blocks()/clear_all(). A specific evidence must close a
    /// specific block. The external operator/reconciler validates evidence first.
    pub fn resolve_block(&mut self, id: &str, checked_evidence_id: &str) -> Result<(), OwnerError> {
        if checked_evidence_id.is_empty() { return Err(OwnerError::NotPermitted("NO_RESOLUTION_EVIDENCE")); }
        let mut next = self.db.clone();
        let block = next.blocks.get_mut(id).ok_or(OwnerError::NotFound)?;
        if block.resolved_with_evidence.is_some() { return Err(OwnerError::NotPermitted("ALREADY_RESOLVED")); }
        block.resolved_with_evidence = Some(checked_evidence_id.into());
        self.persist(next)
    }

    fn add_action(next: &mut Persistent, key: String, kind: ActionKind, now_ms: u64)
        -> Result<u64, OwnerError>
    {
        if let Some(a) = next.actions.values().find(|a| a.key == key) {return Ok(a.id);}
        next.next_action_id = next.next_action_id.checked_add(1)
            .ok_or(OwnerError::InvalidEvidence("ACTION_ID_OVERFLOW"))?;
        let id=next.next_action_id;
        next.actions.insert(id, ActionRecord {id,key,kind,state:ActionState::Prepared,
            created_mono_ms:now_ms});
        Ok(id)
    }

    /// Idempotent outbox creation: same key never allocates a second stop.
    /// A lost dispatcher response remains UNKNOWN until explicit readback.
    pub fn plan_action(&mut self, key: String, kind: ActionKind, now_ms: u64)
        -> Result<u64, OwnerError>
    {
        if let Some(a) = self.db.actions.values().find(|a| a.key == key) { return Ok(a.id); }
        let mut next = self.db.clone();
        let id = Self::add_action(&mut next,key,kind,now_ms)?;
        self.persist(next)?;
        Ok(id)
    }

    /// Call before handing a command to the one authorized I/O executor.
    /// On crash during dispatch, state is UNKNOWN and NEVER auto-resubmitted.
    pub fn begin_dispatch(&mut self, id: u64) -> Result<ActionKind, OwnerError> {
        let mut next = self.db.clone();
        let a = next.actions.get_mut(&id).ok_or(OwnerError::NotFound)?;
        if a.state != ActionState::Prepared { return Err(OwnerError::NotPermitted("ACTION_NOT_PREPARED")); }
        a.state = ActionState::Dispatching;
        let command = a.kind.clone();
        self.persist(next)?;
        Ok(command)
    }
    /// B38: only the correlated action changes; an entry ACK cannot clear a stop.
    pub fn on_io_ack(&mut self, id: u64, action_is_correlated: bool) -> Result<(), OwnerError> {
        if !action_is_correlated { return Err(OwnerError::InvalidEvidence("MISMATCHED_IO_ACTION")); }
        let mut next = self.db.clone();
        let a = next.actions.get_mut(&id).ok_or(OwnerError::NotFound)?;
        if a.state != ActionState::Dispatching && a.state != ActionState::Unknown {
            return Err(OwnerError::NotPermitted("UNEXPECTED_IO_ACK"));
        }
        a.state = ActionState::AckAwaitingReadback;
        self.persist(next)
    }
    pub fn finish_from_correlated_readback(&mut self, id: u64, proof_ref: &str) -> Result<(), OwnerError> {
        if proof_ref.is_empty() { return Err(OwnerError::InvalidEvidence("READBACK_WITHOUT_PROOF")); }
        let mut next = self.db.clone();
        let a = next.actions.get_mut(&id).ok_or(OwnerError::NotFound)?;
        if a.state == ActionState::Prepared { return Err(OwnerError::NotPermitted("NO_DISPATCH_EVIDENCE")); }
        if a.kind.is_write() {
            return Err(OwnerError::NotPermitted("WRITE_REQUIRES_TYPED_EXCHANGE_READBACK"));
        }
        a.state = ActionState::VerifiedDone;
        self.persist(next)
    }

    /// Complete a stop write only if an exchange-confirmed strategy stop is
    /// correlated to that *same* journaled intent, and full protection is proven.
    pub fn confirm_stop_from_readback(&mut self, id: u64, proof_ref: &str, now_ms: u64)
        -> Result<(), OwnerError>
    {
        if proof_ref.is_empty() || self.coverage(now_ms) != Coverage::ConfirmedFull {
            return Err(OwnerError::InvalidEvidence("STOP_NOT_FULLY_CONFIRMED"));
        }
        let mut next = self.db.clone();
        let a = next.actions.get_mut(&id).ok_or(OwnerError::NotFound)?;
        let ActionKind::PlaceStop { stop_intent_id, .. } = &a.kind else {
            return Err(OwnerError::NotPermitted("NOT_STOP_ACTION"));
        };
        if a.state != ActionState::AckAwaitingReadback && a.state != ActionState::Unknown {
            return Err(OwnerError::NotPermitted("STOP_ACTION_NOT_AWAITING_READBACK"));
        }
        let linked = self.stops.as_ref().is_some_and(|s|s.rows.iter().any(|row|{
            row.exchange_active && row.binding_verified && row.bound_stop_intent.as_ref() == Some(stop_intent_id)
        }));
        if !linked { return Err(OwnerError::InvalidEvidence("STOP_INTENT_NOT_ON_EXCHANGE")); }
        a.state = ActionState::VerifiedDone;
        self.persist(next)
    }

    /// B39: an ACK alone never proves the emergency exit executed. The caller
    /// must prove the exit OrderIntent terminal state, complete fills, and a
    /// full flat position snapshot taken afterwards.
    pub fn confirm_emergency_from_readback(&mut self, id: u64,
        exchange_exit_terminal_and_fills_reconciled: bool,
        proof_ref: &str, now_ms: u64) -> Result<(), OwnerError>
    {
        if !exchange_exit_terminal_and_fills_reconciled || proof_ref.is_empty()
            || self.coverage(now_ms) != Coverage::ConfirmedFlat
        { return Err(OwnerError::InvalidEvidence("EMERGENCY_NOT_RECONCILED")); }
        let mut next=self.db.clone();
        let a=next.actions.get_mut(&id).ok_or(OwnerError::NotFound)?;
        if !matches!(&a.kind,ActionKind::EmergencyClose{..})
            || (a.state!=ActionState::AckAwaitingReadback && a.state!=ActionState::Unknown)
        { return Err(OwnerError::NotPermitted("EMERGENCY_ACTION_NOT_AWAITING_READBACK")); }
        a.state=ActionState::VerifiedDone;
        self.persist(next)?;
        self.maybe_finish_emergency(now_ms)
    }

    /// B42: executor queue overflow/disconnect must be visible to the Owner.
    /// No write is ever blindly retried. Evidence-bearing read requests can
    /// be re-planned under a new request generation after this state is recorded.
    pub fn on_dispatch_queue_failure(&mut self,id:u64) -> Result<(),OwnerError> {
        let mut next=self.db.clone();
        let a=next.actions.get_mut(&id).ok_or(OwnerError::NotFound)?;
        if a.state!=ActionState::Dispatching {return Err(OwnerError::NotPermitted("NOT_IN_DISPATCH"));}
        a.state=ActionState::Unknown;
        self.persist(next)?;
        self.mode=Mode::Reconciling;
        Ok(())
    }

    /// B35: restore only from durable journal, never an in-memory value alone.
    /// Intent states DISPATCHING stay UNKNOWN: no auto order sends after restart.
    pub fn recover(&mut self) -> Result<(), OwnerError> {
        let mut next = self.store.load()?;
        for a in next.actions.values_mut() {
            if a.kind.is_write() && a.state != ActionState::VerifiedDone {
                a.state = ActionState::Unknown; // even PREPARED: reconcile before any resend
            } else if a.state == ActionState::Dispatching {
                a.state = ActionState::Unknown;
            }
        }
        // A stored monotonic timestamp is not portable across process restarts.
        next.confirmed_missing_since_mono_ms = None;
        self.persist(next)?;
        self.position = None;
        self.stops = None;
        self.evidence = ActivationEvidence::default();
        self.stop_repair_capability_qualified = false;
        self.emergency_capability_qualified = false;
        self.last_coverage = Coverage::Unknown("RESTART_REQUIRES_READBACKS");
        self.mode = Mode::Reconciling;
        Ok(())
    }

    /// B37: positive evidence, never `last_position_poll_ms >= last_fill_ms`
    /// where both timestamps can be zero. Ticket/correlation validation is the
    /// job of the private adapter before constructing PositionProof.
    pub fn apply_position_readback(&mut self, p: PositionProof, now_ms: u64)
        -> Result<(), OwnerError>
    {
        if p.key != self.key || !p.complete || !p.scope_verified
            || p.applied_fill_epoch < self.db.latest_fill_epoch
            || !is_fresh(now_ms, p.observed_mono_ms, self.max_readback_age_ms)
            || self.position.as_ref().is_some_and(|old| p.generation <= old.generation)
        {
            return Err(OwnerError::InvalidEvidence("POSITION_POLL_NOT_APPLIED"));
        }
        self.position = Some(p);
        self.stops = None; // a stop readback from the earlier position is not reusable
        Ok(())
    }
    pub fn apply_stop_readback(&mut self, s: StopsProof, now_ms: u64)
        -> Result<(), OwnerError>
    {
        let Some(pos) = &self.position else { return Err(OwnerError::InvalidEvidence("NO_POSITION_GENERATION")); };
        if s.key != pos.key || !s.complete || !s.scope_verified
            || s.after_position_generation < pos.generation
            || s.after_fill_epoch < self.db.latest_fill_epoch
            || !is_fresh(now_ms, s.observed_mono_ms, self.max_readback_age_ms)
            || self.stops.as_ref().is_some_and(|old| s.generation <= old.generation)
        { return Err(OwnerError::InvalidEvidence("STOP_POLL_NOT_APPLIED")); }
        self.stops = Some(s);
        Ok(())
    }

    /// B41: journal receives the committed fill *first*, then the owner
    /// schedules an Order lookup + complete positions + strategy stops readback.
    /// Full local fill sum is NOT evidence of exchange FILLED or reservation release.
    pub fn on_fill_committed(&mut self, entry_intent_id: &str, fully_booked: bool,
        expected_stop_journal_proof: Option<StopExpectation>, now_ms: u64)
        -> Result<(), OwnerError>
    {
        if entry_intent_id.is_empty() { return Err(OwnerError::InvalidEvidence("EMPTY_INTENT")); }
        let mut next = self.db.clone();
        next.latest_fill_epoch = next.latest_fill_epoch.checked_add(1)
            .ok_or(OwnerError::InvalidEvidence("FILL_EPOCH_OVERFLOW"))?;
        let e = next.latest_fill_epoch;
        if let Some(exp) = expected_stop_journal_proof {
            if exp.entry_intent_id != entry_intent_id
                || exp.established_after_fill_epoch == 0
                || exp.established_after_fill_epoch > e {
                return Err(OwnerError::InvalidEvidence("FOREIGN_EXPECTED_STOP"));
            }
            next.expected_stop_from_journal = Some(exp);
        }
        // One atomic transaction: fill-epoch + all required reconciliation actions.
        Self::add_action(&mut next, format!("fill:{e}:positions"), ActionKind::ReadPositions, now_ms)?;
        Self::add_action(&mut next, format!("fill:{e}:stops"), ActionKind::ReadStops, now_ms)?;
        if fully_booked {
            Self::add_action(&mut next, format!("fill:{e}:lookup:{entry_intent_id}"),
                ActionKind::LookupOrder { intent_id: entry_intent_id.into() }, now_ms)?;
            Self::add_action(&mut next, format!("fill:{e}:fills:{entry_intent_id}"),
                ActionKind::ReadFills { intent_id: entry_intent_id.into() }, now_ms)?;
        }
        self.persist(next)?;
        self.position = None;
        self.stops = None;
        self.mode = Mode::Reconciling;
        Ok(())
    }

    fn coverage(&self, now_ms: u64) -> Coverage {
        coverage::evaluate(self.db.expected_stop_from_journal.as_ref(),
            self.position.as_ref(), self.stops.as_ref(),
            self.db.latest_fill_epoch, now_ms, self.max_readback_age_ms)
    }

    /// B35: automatic emergency only with *fresh, complete, affirmative* evidence
    /// of missing stop cover, never from absent/unknown/expired data.
    pub fn tick(&mut self, now_ms: u64) -> Result<Coverage, OwnerError> {
        let coverage = self.coverage(now_ms);
        self.last_coverage = coverage.clone();
        match &coverage {
            Coverage::Unknown(_) => {
                // Actions are read-only; no place/cancel/market-close on uncertainty.
                let refresh_bucket = now_ms / self.max_readback_age_ms.max(1);
                if self.position.as_ref().map_or(true,|p| !is_fresh(now_ms,p.observed_mono_ms,self.max_readback_age_ms)) {
                    self.plan_action(format!("epoch:{}:position:bucket{}",self.db.latest_fill_epoch,refresh_bucket),ActionKind::ReadPositions,now_ms)?;
                }
                self.plan_action(format!("epoch:{}:stops:bucket{}",self.db.latest_fill_epoch,refresh_bucket),ActionKind::ReadStops,now_ms)?;
            }
            Coverage::ConfirmedFull | Coverage::ConfirmedFlat => {
                if self.db.confirmed_missing_since_mono_ms.is_some() {
                    let mut n=self.db.clone();n.confirmed_missing_since_mono_ms=None;self.persist(n)?;
                }
            }
            Coverage::ConfirmedMissing { missing } | Coverage::ConfirmedPartial { missing, .. } => {
                if self.db.confirmed_missing_since_mono_ms.is_none() {
                    let mut n=self.db.clone();n.confirmed_missing_since_mono_ms=Some(now_ms);self.persist(n)?;
                }
                // B06/B38: only one durable repair intent may exist before
                // a verified Exchange readback resolves it. No duplicate stop.
                let unresolved_stop = self.db.actions.values().any(|a|
                    matches!(&a.kind, ActionKind::PlaceStop{..}) && a.state!=ActionState::VerifiedDone);
                let unresolved_emergency = self.db.actions.values().any(|a|
                    matches!(&a.kind, ActionKind::EmergencyClose{..}) && a.state!=ActionState::VerifiedDone);
                if self.mode!=Mode::EmergencyRecovery && self.stop_repair_capability_qualified
                    && !unresolved_stop && !unresolved_emergency && !self.blocks_open() {
                    if let Some(exp) = &self.db.expected_stop_from_journal {
                        let repair_id=format!("repair:{}:fill{}",exp.entry_intent_id,self.db.latest_fill_epoch);
                        let mut next=self.db.clone();
                        let id=Self::add_action(&mut next,repair_id.clone(),
                            ActionKind::PlaceStop{stop_intent_id:repair_id.clone(),lots:*missing},now_ms)?;
                        if let Some(e)=next.expected_stop_from_journal.as_mut() {
                            if !e.bound_stop_intents.contains(&repair_id) { e.bound_stop_intents.push(repair_id); }
                        }
                        // One journal transaction for action + identity binding.
                        self.persist(next)?;
                        debug_assert!(self.db.actions.contains_key(&id));
                    }
                }
                let since = self.db.confirmed_missing_since_mono_ms.unwrap_or(now_ms);
                if self.emergency_capability_qualified && now_ms.saturating_sub(since)>=self.missing_stop_grace_ms {
                    // Must be a known, confirmed signed position. No action when unproven.
                    let Some(p)=&self.position else { return Ok(coverage); };
                    let qty = p.signed_lots.checked_abs().ok_or(OwnerError::InvalidEvidence("I64_MIN_LOTS"))?;
                    if qty>0 && *missing>0 && !unresolved_emergency && !self.blocks_open() {
                        // Intent ID is a stable journal correlation key, not a new random ID each tick.
                        let key=format!("emergency:{}:fill{}",self.key.symbol,self.db.latest_fill_epoch);
                        let id= self.plan_action(key.clone(),ActionKind::EmergencyClose {close_intent_id:key.clone(),lots:qty},now_ms)?;
                        if self.db.actions.get(&id).is_some_and(|a|a.state!=ActionState::VerifiedDone) {
                            self.mode=Mode::EmergencyRecovery;
                        }
                    }
                }
            }
            Coverage::Conflict(_) => {
                self.open_block("STOP_COVERAGE_CONFLICT","STOP_COVERAGE_CONFLICT")?;
            }
        }
        Ok(coverage)
    }

    /// B39: emergency workflow terminates only after the close intent is
    /// reconciled, positions FLAT is proven, and all blocking identities resolved.
    pub fn maybe_finish_emergency(&mut self, now_ms: u64) -> Result<(), OwnerError> {
        if self.mode!=Mode::EmergencyRecovery {return Ok(());}
        if self.coverage(now_ms)!=Coverage::ConfirmedFlat {return Ok(());}
        if self.blocks_open() {return Ok(());}
        if self.db.actions.values().any(|a|a.kind.is_write() && a.state!=ActionState::VerifiedDone) {return Ok(());}
        self.mode=Mode::Reconciling;
        self.mode=Mode::Halted; // never auto-activate
        Ok(())
    }

    /// A recovered account becomes HALTED, never automatically ACTIVE.
    /// Leaving RECONCILING is prohibited while any scoped block or uncertain
    /// write action remains. Caller must have completed all account/fee/gate
    /// evidence checks (independent of UI connection status).
    pub fn maybe_finish_reconciling(&mut self, now_ms: u64) -> Result<(), OwnerError> {
        if self.mode != Mode::Reconciling { return Ok(()); }
        if self.blocks_open() || !self.evidence.all() { return Ok(()); }
        if self.db.actions.values().any(|a|a.kind.is_write() && a.state!=ActionState::VerifiedDone) {
            return Ok(());
        }
        if matches!(self.coverage(now_ms),Coverage::ConfirmedFull | Coverage::ConfirmedFlat) {
            self.mode=Mode::Halted;
        }
        Ok(())
    }

    /// B44: a complete account state is needed, not a blank/zero timestamp.
    /// Graph/risk controls and independent human arming are outside this module.
    pub fn activate(&mut self, now_ms: u64) -> Result<(), OwnerError> {
        if self.mode!=Mode::Halted {return Err(OwnerError::NotReady("NOT_HALTED"));}
        if !self.evidence.all() {return Err(OwnerError::NotReady("MISSING_QUALIFICATION"));}
        if self.blocks_open() {return Err(OwnerError::NotReady("ACCOUNT_BLOCK_OPEN"));}
        if self.db.actions.values().any(|a|a.kind.is_write() && a.state!=ActionState::VerifiedDone) {
            return Err(OwnerError::NotReady("UNCERTAIN_WRITE_ACTION"));
        }
        match self.coverage(now_ms) {
            Coverage::ConfirmedFlat | Coverage::ConfirmedFull => {self.mode=Mode::Active;Ok(())}
            _ => Err(OwnerError::NotReady("PROTECTION_UNQUALIFIED")),
        }
    }

    pub fn store_ref(&self) -> &S { &self.store }
}

#[derive(Clone, Default)]
pub struct MemoryStore { pub snapshot: Persistent, pub fail_next_commit: bool }
impl DurableStore for MemoryStore {
    fn load(&self) -> Result<Persistent,OwnerError> { Ok(self.snapshot.clone()) }
    fn commit(&mut self, next:&Persistent)->Result<(),OwnerError>{
        if self.fail_next_commit { self.fail_next_commit=false;return Err(OwnerError::Storage); }
        self.snapshot=next.clone();Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Side,TriggerBy,StopKind,StopReadback};
    fn key()->PositionKey {PositionKey{account_uid:"uid".into(),category:"USDT-FUTURES".into(),symbol:"BTCUSDT".into(),pos_side:"one_way_mode".into()}}
    fn owner()->Owner<MemoryStore>{Owner::new(MemoryStore::default(),key()).unwrap()}
    fn position(lots:i64)->PositionProof{PositionProof{key:key(),signed_lots:lots,generation:9,applied_fill_epoch:1,observed_mono_ms:100,complete:true,scope_verified:true}}
    fn expectation()->StopExpectation{StopExpectation{position_key:key(),entry_intent_id:"e1".into(),bound_stop_intents:vec!["s1".into()],trigger_ticks:90,trigger_by:TriggerBy::Mark,required_kind:StopKind::Market,established_after_fill_epoch:1}}
    fn stops(rows:Vec<StopReadback>)->StopsProof{StopsProof{key:key(),generation:10,after_position_generation:9,after_fill_epoch:1,observed_mono_ms:101,complete:true,scope_verified:true,rows}}
    #[test] fn restart_does_not_emergency_close_unknown_stop() {
        let mut o=owner();o.on_fill_committed("e1",true,Some(expectation()),100).unwrap();o.recover().unwrap();
        for t in [200,12_000,80_000] {assert!(matches!(o.tick(t).unwrap(),Coverage::Unknown(_)));}
        assert!(!o.store_ref().snapshot.actions.values().any(|a|matches!(&a.kind,ActionKind::EmergencyClose{..})));
    }
    #[test] fn proven_missing_still_requires_grace_and_is_idempotent() {
        let mut o=owner();o.on_fill_committed("e1",true,Some(expectation()),100).unwrap();
        o.apply_position_readback(position(4),101).unwrap();o.apply_stop_readback(stops(vec![]),102).unwrap();
        assert!(matches!(o.tick(103).unwrap(),Coverage::ConfirmedMissing{..}));
        o.tick(10_103).unwrap(); // stale data => cannot auto-close on old proof
        assert!(!o.store_ref().snapshot.actions.values().any(|a|matches!(&a.kind,ActionKind::EmergencyClose{..})));
        // New fresh proofs with greater generations must be applied to reconfirm missing protection.
    }
    #[test] fn confirmed_missing_after_fresh_recheck_plans_one_emergency() {
        let mut o=owner();o.emergency_capability_qualified=true;
        o.on_fill_committed("e1",true,Some(expectation()),100).unwrap();
        o.apply_position_readback(position(4),101).unwrap();o.apply_stop_readback(stops(vec![]),102).unwrap();
        o.tick(103).unwrap();
        let mut p=position(4);p.generation=10;p.observed_mono_ms=10_103;
        o.apply_position_readback(p,10_104).unwrap();
        let mut s=stops(vec![]);s.generation=11;s.after_position_generation=10;s.observed_mono_ms=10_105;
        o.apply_stop_readback(s,10_106).unwrap();
        o.tick(10_107).unwrap();o.tick(10_108).unwrap();
        assert_eq!(o.store_ref().snapshot.actions.values().filter(|a|matches!(&a.kind,ActionKind::EmergencyClose{..})).count(),1);
    }
    #[test] fn certified_stop_repair_dedupes_action_and_binds_identity() {
        let mut o=owner();o.stop_repair_capability_qualified=true;
        let mut x=expectation();x.bound_stop_intents.clear();
        o.on_fill_committed("e1",true,Some(x),100).unwrap();
        o.apply_position_readback(position(4),101).unwrap();o.apply_stop_readback(stops(vec![]),102).unwrap();
        o.tick(103).unwrap();o.tick(104).unwrap();
        let db=&o.store_ref().snapshot;
        assert_eq!(db.actions.values().filter(|a|matches!(&a.kind,ActionKind::PlaceStop{..})).count(),1);
        assert_eq!(db.expected_stop_from_journal.as_ref().unwrap().bound_stop_intents.len(),1);
    }
    #[test] fn block_is_never_cleared_by_fresh_readback() {
        let mut o=owner();o.open_block("ORPHAN:exec01","ORPHAN").unwrap();
        o.apply_position_readback(position(0),101).unwrap();o.apply_stop_readback(stops(vec![]),102).unwrap();
        assert!(o.blocks_open());assert_eq!(o.activate(103),Err(OwnerError::NotReady("MISSING_QUALIFICATION")));
        o.resolve_block("ORPHAN:exec01","verified-link:case-123").unwrap();assert!(!o.blocks_open());
    }
    #[test] fn unrelated_ack_does_not_remove_stop_action() {
        let mut o=owner();let stop=o.plan_action("stop:s1".into(),ActionKind::PlaceStop{stop_intent_id:"s1".into(),lots:4},0).unwrap();
        let entry=o.plan_action("lookup:e1".into(),ActionKind::LookupOrder{intent_id:"e1".into()},0).unwrap();
        o.begin_dispatch(stop).unwrap();o.begin_dispatch(entry).unwrap();o.on_io_ack(entry,true).unwrap();
        assert_eq!(o.store_ref().snapshot.actions.get(&stop).unwrap().state,ActionState::Dispatching);
    }
    #[test] fn recovered_dispatch_is_unknown_and_not_auto_retry() {
        let mut o=owner();let id=o.plan_action("stop:s1".into(),ActionKind::PlaceStop{stop_intent_id:"s1".into(),lots:4},0).unwrap();
        o.begin_dispatch(id).unwrap();o.recover().unwrap();
        assert_eq!(o.store_ref().snapshot.actions.get(&id).unwrap().state,ActionState::Unknown);
        assert_eq!(o.begin_dispatch(id),Err(OwnerError::NotPermitted("ACTION_NOT_PREPARED")));
    }
    #[test] fn full_booked_fill_queues_reconciliation() {
        let mut o=owner();o.on_fill_committed("e1",true,Some(expectation()),100).unwrap();
        let kinds=o.store_ref().snapshot.actions.values().map(|a|&a.kind).collect::<Vec<_>>();
        assert!(kinds.iter().any(|x|matches!(x,ActionKind::LookupOrder{..})));
        assert!(kinds.iter().any(|x|matches!(x,ActionKind::ReadPositions)));
        assert!(kinds.iter().any(|x|matches!(x,ActionKind::ReadStops)));
    }
    #[test] fn failed_persist_does_not_dispatch() {
        let mut o=owner();o.store.fail_next_commit=true;
        assert_eq!(o.plan_action("x".into(),ActionKind::ReadPositions,2),Err(OwnerError::Storage));
        assert!(o.store_ref().snapshot.actions.is_empty());
    }
    #[test] fn repeated_partial_fills_can_reuse_same_stop_policy_proof() {
        let mut o=owner();
        o.on_fill_committed("e1",false,Some(expectation()),100).unwrap();
        // ExpectedStop was established on the first actual fill, not re-invented.
        o.on_fill_committed("e1",true,Some(expectation()),102).unwrap();
        assert_eq!(o.store_ref().snapshot.latest_fill_epoch,2);
        assert!(o.store_ref().snapshot.actions.values().any(|a|matches!(&a.kind,ActionKind::LookupOrder{..})));
    }
    #[test] fn cannot_activate_with_missing_evidence() {
        let mut o=owner();o.apply_position_readback(position(0),101).unwrap();o.apply_stop_readback(stops(vec![]),102).unwrap();
        assert_eq!(o.activate(103),Err(OwnerError::NotReady("MISSING_QUALIFICATION")));
    }
}

