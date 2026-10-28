# DarkNode – CODE-ÜBERNAHMEPLAN V3.2 · korrigierte Owner-/Stop-Coverage-Vorlage

**Stand:** 28.10.2026 · **Status:** ENTWURFSMUSTER / NICHT KOMPILIERT / NICHT GETESTET / KEINE BITGET-FREIGABE · **Git-Push:** NEIN (vorherige Nutzeranweisung: zuerst im Chat prüfen).  
**Referenz:** V3.1 (4.805 Zeilen) plus Benutzerbefund „Eingefügter Text(9).txt“ B35–B44. **Kein Architekturwechsel, kein zweiter Bot, kein Frontend-Neubau.**

## 0. Verbindlicher Geltungsbereich

**Dies ist ein gezielter Review-/Implementierungsentwurf für genau zwei P5-Module:** `dn-core/src/owner/mod.rs` und `dn-core/src/protection/coverage.rs`, inklusive minimaler Typ-Verträge. Die Quelldateien im beigefügten Review-Paket sind als isolierte, std-only Rust-Library angelegt. Sie enthalten bewusst **keinen Bitget-Signer, keine REST-/WS-Verbindung, keinen produktiven SQLite-Journaladapter, keine Gateway- oder Chart-Komponente**. Der produktive Code muss beim späteren Einbau an die bestehenden `dn-types`, den `dn-store` und die P5-S0–S7-Verträge angeschlossen werden.

**Vorrang:** Die Beispiele der V3.1-Abschnitte `TEIL I §10 coverage.rs` und `§12 owner/mod.rs` gelten nach dieser Revision als **HISTORISCH / NICHT DIREKT IMPLEMENTIEREN**. Für diese zwei Module ist die hier folgende **einzige aktuelle V3.2-Vorlage** maßgeblich. Alle anderen V3.1-Abschnitte sind nach wie vor Entwürfe mit offenen Reviewpunkten; sie werden nicht indirekt für produktionsreif erklärt.

**Repo-Iststand geprüft:** `darknode/docs/PHASENPLAN_P5_V1.md` enthält **P5 V1.6**, P1 unter `PHASENPLAN_P1_V1.md` ist inhaltlich **V1.3**; `darknode/native` existiert im geprüften Branch noch nicht. `TESTPROTOKOLL.md` enthält **49 eindeutige zusätzliche Solltest-IDs** (6 WS + 11 AGG + 8 Chart + **20** P5-UP einschließlich UP-001–004 und UP-016–031 + 4 P4); **alle NOT_RUN**. Die zuvor genannten 45 zählen die vier bereits registrierten UP-001–004 nicht mit. Bei `UP-017` und `UP-018` stehen in alten Codebeispielen mehrere Subcases unter derselben ID: die Fallvarianten zusammenführen, *keine* ID für eine andere fachliche Prüfung umdeuten. P5-A01–A28 und C00–C16 behalten Vorrang.

## 1. Fachliche Prüfung des neuen Nutzerbefunds

| Befund | Urteil anhand V3.1-Code | Konsequenz in der aktuellen V3.2-Vorlage |
|---|---|---|
| **B35 CRITICAL** | Bestätigt: `expected_stop=None` nach Crash + laufender Emergency-Timer im ursprünglichen Owner | `expected_stop_from_journal`; nach Boot kein Positions- oder Stop-Vertrauen ohne vollständige neue Readbacks; unbekannte Schutzdeckung **niemals** als bestätigtes Fehlen behandeln. Automatische Emergency nur bei frischer qualifizierter fehlender Deckung und qualifizierter Venue-Capability. |
| **B36 CRITICAL** | Bestätigt: `clear_blocks` entfernt fachfremde Sperren | Persistente `AccountBlock`-Einträge werden **einzeln mit überprüftem Nachweis** geschlossen; keine blanket-clear-Methode. |
| **B37 HIGH** | Bestätigt: initiale Zeitstempel beide 0 → fälschlich reconciled | Generation/Scope/Completed-Status/`applied_fill_epoch` einer echten Positionantwort verlangen. |
| **B38 HIGH** | Bestätigt: jedes `Placed`-ACK löscht `stop_in_flight` | Persistente Stop-Aktions-ID; nur typgetreues korreliertes Resultat kann diese eine Aktion verändern. ACK ≠ Exchange-Schutz. |
| **B39 HIGH** | Bestätigt: `emergency_in_flight` ohne abgeschlossenen Wiederherstellungspfad | Emergency-Aktion bleibt offen bis *Terminal-Order/Fills + Position FLAT* nachweisbar; Übergang über RECONCILING nach HALTED, niemals automatisches ACTIVE. |
| **B40 HIGH** | Offene Bitget-Capability: erzeugt ein Strategie-Stop eine Child-Order mit anderer ID? | Journal-Mapping **StrategyOrder → triggered_order_id → Fill/execId** ergänzen und Demo-Readback prüfen. In dieser isolierten Vorlage **noch nicht implementiert**, daher P5-Gate OPEN. |
| **B41 HIGH** | Bestätigt: `FillBooked` voll, Status bleibt PARTIAL ohne garantiertes Lookup | Nach **gebuchtem** Full-Fill sofort atomar Order-Lookup, Fills-, Positions- und Stop-Readback als dauerhafte Sollaktionen einplanen; Status/Reservation bleiben bis echter Reconcile offen. |
| **B42 MEDIUM** | Bestätigt: Queue-Fehler kann Readback verlieren | Outbox-Aktion vor Dispatch dauerhaft speichern, Queue-Fehler als UNKNOWN + RECONCILING protokollieren. Read-only Retry-/Alarmplan und tatsächlicher Executor fehlen noch. Keine blinde Wiederholung von Write-Aktionen. |
| **B43 MEDIUM** | Bestätigt: `expected_stop` wird schon vor erfolgreichem Entry geschrieben | Erwarteter Stop wird aus validiertem Intent-Journal **erst nach gebuchtem Fill** in die aktive Protection-Projection überführt. |
| **B44 MEDIUM** | Bestätigt: ACTIVATE prüft zu wenige echte Account-Gates | Aktivieren nur aus HALTED mit frischem, qualifiziertem Konto-/Positions-/Stop-/Order-/DailyPnL-/Gebühren-/Instrument-/Leverage-/Privat-WS-Nachweis und ohne fachliche Sperren oder offene Write-Aktionen. |
| **B01** | Präzisierung bestätigt: `UNIQUE(client_oid)` führt bei Restart-Kollision normalerweise zu `HALTED`, **nicht** zur automatischen Doppelorder | Separates P5-Journal-Gate: persistenter, accountgescopter `clientOid`-Generator und Collision-Test. **Diese isolierte Vorlage implementiert noch keine clientOid-Erzeugung.** |
| **B17** | Präzisierung bestätigt: Gleichzeitiger Start zweier 1s-Bars ist nicht der Fehler; Verlust der Coverage-Qualität über fehlende Sekunden ist es | Mit B31 in P1-Aggregationsmodul lösen; hier keine Owner-Änderung. |
| **B19** | Bestätigt: `closed_gaps` meint abgeschlossene **Intervallgrenzen**, nicht reparierte Daten | In P1 `gap_intervals` umbenennen und Daten-Coverage getrennt von Transport-Liveness halten. |
| **B23** | Im beschriebenen V3.1-Sendpfad war `INTENT_DURABLE` vor `SUBMIT_PENDING` beweisbar nicht übergeben; somit kein pauschaler neuer Codeblocker | Crash-C03/C04 zum **Beleg der Zustandsreihenfolge** beibehalten. Unklare Send-Übergänge bleiben UNKNOWN. |

### Kein falscher Sicherheitsabschluss durch den neuen Beispielcode

Die **neuen Beispielfunktionen** sind keine Freigabe von B35–B44. Sie bilden die Sicherheitsanforderung ab, doch mehrere **Produktionsbindungen** fehlen noch: echte WAL-/FULLFSYNC-Transaktionen, durable Boot-Projektion aller benötigten Fremd- und Journal-Orders, exakte Bitget-UTA-V3-Datentypen und Stop-Endpunktvarianten, vollständiges triggered-child-Order-Mapping (B40), echte Ready-/Command-/ACL-Kette, read-only Retry-/Timeout-/Alarm, und alle E2E-/Fault-/Crash-Tests. Jedes unbewiesene Feld führt zu BLOCK/NO_ENTRY, nicht zu einem PASS.

## 2. Eine einzige aktuelle Rust-Vorlage für Owner-/Protection-Grenzen

**Nutzung:** Das Reviewpaket enthält die echten Dateien `src/model.rs`, `src/protection/coverage.rs`, `src/owner/mod.rs` und `Cargo.toml`. Die drei Codeabschnitte unten sind wortidentisch mit diesen Quelldateien. Die Types sind **Review-Verträge**; nicht blind in `dn-types` kopieren, sondern an die vorhandenen P1/P5-Typen anbinden. Keine direkte Signierfähigkeit.

### 2.1 Minimaler gemeinsamer Fachvertrag (`src/model.rs`)

```rust
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

```

### 2.2 Read-only-Stopdeckung (`src/protection/coverage.rs`)

```rust
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

```

### 2.3 Serieller Owner mit persistierter Aktions-Outbox (`src/owner/mod.rs`)

```rust
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

```

## 3. Verbindliche Integration in vorhandene P5-Modulgrenzen

| Zieldatei im V3.1-Plan | Was hier konkret vorbereitet ist | Produktions-Integration / noch OPEN |
|---|---|---|
| `dn-store/src/order_journal.rs` | `Persistent`, `DurableStore`, Aktions-IDs, konkrete Blocks, Stop-Erwartung und Fill-Epoch; **keine tatsächliche SQL-Implementierung** | SQLite WAL+FULL+`fullfsync`, atomare Write-Intent-/Action-/Reservation-/Fill-Transitions, clientOid-Unique, Startup-Projektion aus vorhandener Datenbank, Migration und Crash-Goldens. |
| `dn-core/src/protection/coverage.rs` | Vollständiges Key-/Seiten-/Status-/Trigger-/Stoptyp-/Qty-/ReduceOnly-/Intentlink-Checking; `Unknown` strikt getrennt von `ConfirmedMissing` | Bitget-UTA-V3-Readback muss tatsächlich `binding_verified`/Scopes/Status belegen. Keine Fremd-Stop-Autorisierung oder stilles Fremd-Stop-Cancel. Alle Sorten (Preset/Strategy/TriggeredChild) qualifizieren. |
| `dn-core/src/owner/mod.rs` | Nur ein serialer Entscheidungskern; sichere Recovery-HALT; keine flüchtigen `stop_in_flight`-Bool-Gates; keine pauschalen Block-Löschungen | An echte priorisierte Event-Queues und den bereits vorgesehenen EINEN I/O-Signer anbinden. Keine synchronen HTTP-Calls im Owner. Zeitüberschreitungen/IO-Queue-Ausfall mit eigener Alarm-Outbox. |
| `dn-types/src/order_fsm.rs` | Fill-Epoch löst Lookup/Fills/Positions/Stops aus, aber **kein automatisches FILLED** | Im tatsächlichen Journal/FSM `FILL_COMPLETE_PENDING_RECONCILE` und `Reconciled(...)` mit echten Börsenpositionen/Fills implementieren. Reservations erst nach beweisbarem Endzustand freigeben. |
| `dn-core/account/positions.rs` | `PositionProof` enthält erfolgreiche Generation, Umfang, Last-Fill-Epoch und Freshness | Ticket, Generation, Source/Cursor, paginierte REST-Complete/Latest-WS-Merge kausal überprüfen; niemals leere Liste als FLAT ohne Vollständigkeit. |
| `dn-bitget/private/*` | Ausführungsports fehlen bewusst | UTA V3 Payload/Auth/Strategy-Readback/triggeredOrderId/reduceOnly und One-way+Isolated in DEMO belegen, inklusive Quoten/Rate-Limits. |

**Besondere Engineering-Entscheidung:** Ein `ActionState::AckAwaitingReadback` wird erst nach passender **Exchange-Readback-Identität** abgeschlossen. `ActionState::Unknown` wird nach Neustart niemals automatisch als „nicht gesendet“ oder „wieder senden“ behandelt. Bei fehlender Stop-Sollidentität bleibt die Deckung UNKNOWN; fehlende eigene Daten lösen nicht automatisch Market-Closing aus. Die bewusst konservativen Limitierungen können im Fehlerfall Entries blockieren oder ein menschliches Incident-Handling verlangen – das ist kein Nachweis eines vollständig automatisierten Schutzsystems.

## 4. Pflichtregressionen für diese zwei Module (neu zu referenzieren, nicht blind neu nummerieren)

| Fallname, noch keine eigene registrierte ID | B-ID | Erwartung | PASS nur bei |
|---|---|---|---|
| `restart_protected_position_no_auto_close` | B24/B35 | Nach Boot mit unbestätigten Stops nur Readbacks, **kein** Emergency-OrderIntent | C00–C16-Boot-Capture und echte Readback-Sequenz |
| `orphan_block_survives_unrelated_reconcile` | B36 | ORPHAN/IDENTITY_CONFLICT nicht durch andere erfolgreiche Polls gelöscht | Persistiertes Journal nach Restart |
| `zero_timestamps_are_not_successful_position_poll` | B37 | Ohne angewandte vollständige Positionsgeneration kein FLAT/ACTIVE | P5-A08/A13 Negativtest |
| `entry_ack_does_not_clear_stop_action` | B38 | Nur korrelierte Stopaktions-ID ändert Status | ACK-/WS-Order-Reordering |
| `emergency_finishes_only_after_flat_and_order_truth` | B39 | Übergang nach HALTED erst nach bestätigtem FLAT + Fill-/Orderjournal | C07–C16 einschließlich ACK-Verlust |
| `strategy_stop_triggers_child_order` | B40 | Strategy→ChildOrder→Fill nachgewiesen oder eindeutig blockiert | Bitget Demo-Capture, reale triggeredOrderId |
| `full_fill_must_schedule_lookup_and_position` | B08/B41 | Full booked ≠ FILLED; Lookup/Full-Fills/Positions/Stops atomar pending | Ausfall unmittelbar nach Fill-Journal |
| `dispatch_queue_failure_is_persisted` | B42 | Keine verlorene Readback-/Stop-Anfrage, Write UNKNOWN ohne automatischen Retry | QueueOverflow-/ExecutorRestart-Tests |
| `rejected_entry_does_not_create_protected_position` | B43 | erwarteter Stop erst nach erfolgreichem Fill | Exchange REJECT/TIMEOUT mit neuer Order-ID |
| `activate_requires_account_generation_and_daily_pnl` | B44 | Fehlende Positionsgeneration, Gebühren/Instrument, Verlust-Basis ⇒ NO_ENTRY | vollständiger Read-only-Konto-Test |
| `same_action_key_is_idempotent_across_restart` | B06/B38 | Ein persistenter Stop-Intent pro Korrelationsschlüssel | Stop-ACK→Crash→Readback und C04 |
| `client_oid_unique_after_restart` | B01 | Kein erneuter `dn1...`-Konflikt bei neuem Entry nach Boot | SQLite Identity-Sequence/Length-Goldens |

**Aktueller Status:** Die std-only Rust-Skizze definiert dazu `#[test]`-Fälle, ist aber **nicht mit rustc/cargo kompiliert**. In dieser Umgebung ist die Rust-Toolchain nicht installiert; die oben genannten Tests wurden **nicht** ausgeführt. Alle DarkNode-Testregisterfälle bleiben `NOT_RUN`. Vor Bitget-Demo-Write sind vollständige P5-A01–A28, Crash C00–C16 und die erforderlichen Stop-/Recovery-/Journal-Cases Pflicht; vor LIVE zusätzlich sämtliche P9-Freigaben.

## 5. P0-/Phasenplan-Deltas – keine Neuarchitektur

1. **P0 / CODE-00:** Vor dem Einbau den tatsächlichen `dn-store`-/`dn-core`-Workspace, Git-Quellpin und die einzigartige Konto-/Action-ID-Struktur festlegen; kein zweiter Owner, keine neuen Server. `B01` und UTA `B40` offen lassen, bis echte Belege vorliegen.
2. **P5 / CODE-07:** `owner/mod.rs` und `coverage.rs` aus V3.1 durch *diese* Vorlagen ersetzen, aber erst nach P5-Types-/Journal-Bindings; `MemoryStore` nur für unit-tests. Unbekannte Orders erhalten kein automatisches Resend.
3. **P5 / Wallet:** Kausaler REST-/WS-Snapshot mit vollständigem Endpoint-/Positions-Scope und Order-/Fill-/Stop-Generation. Kein `clear_blocks`.
4. **P5 / S3 Schutz:** Stop-Identität inklusive Konto/Symbol/Position/Ordertyp/trigger/Qty/Reduce-only/Strategy-Intent; Stopp-Reparatur und Emergency nach expliziter Policy und echter Readback-Capability, nicht durch ein flüchtiges `expected_stop=None`.
5. **TESTPROTOKOLL.md:** Die bereits registrierten **49** IDs behalten ihre Bedeutung und `NOT_RUN`. B35–B44 als neue Review-Befunde mit den obigen Fallnamen zu den vorhandenen P5-Gates cross-referenzieren; erst nach Nummerninventur eventuell neue Einzeltest-IDs vergeben. Kein Doppelzähl-PASS.

## 6. Weitere ausdrücklich nicht erledigte Befunde

Die V3.2-Korrektur betrifft **nur** Owner und Stopdeckung. Andere V3.1-Befunde – insbesondere Journal-/SQLite-Identitätskonflikte, doppelte `exec_id`-/Payload-Zuordnungen, Overfill-Verbuchung, Stop-API-Payloads, P1-TF-Coverage, Mac-`F_FULLFSYNC` und realistische Backtest-Fills – bleiben bestehen. **B40 (Strategy→TriggeredChildOrder) bleibt BLOCKED/PENDING DEMO**, keine simulierte Lösung. Ein Quellcodebeispiel, das `ConfirmedFull` produziert, ist nicht gleich ein Exchange-Capability-PASS.

---

## HISTORISCHER ANHANG: V3.1 vollständig erhalten (nicht die aktuellen Owner-/Coverage-Vorlagen)

**Achtung:** Die nachfolgenden ca. 4.805 Zeilen dienen ausschließlich Provenienz und Quellrecherche. **TEIL I §10 und §12 NICHT direkt implementieren; oben stehen die einzigen aktuellen Vorlagen.** Andere Codeabschnitte nur nach ihrem eigenen offenen Reviewstatus. Diese Kennzeichnung verhindert, dass der alte fehlerhafte Owner irrtümlich erneut eingebaut wird.

