# BSC – Phasenplan REV.2.0
## Dynamische Anbieter-/Deployment-Qualifikation und BSC-WATCH

**Stand:** 31.10.2026  
**Status:** GEPLANT – nächste BSC-Iteration; nicht implementiert, nicht abgenommen  
**Verbindliche technische Grundlage:** BSC-WATCH V0.4 (vollständig und unverändert als Anhang A)  
**Projektumfang:** ausschließlich BSC; Project_0 bleibt unabhängig.  
**Ablageziel im Repository:** `bsc/docs/1_PLAN/Phasenplan Rev2.0.md`

### 1. Einordnung und Reihenfolge

REV.2.0 ist ein **Folgeplan**. Die aktuell laufende BSC-REV.1-Fertigstellung und deren eigene Abnahmen behalten ihre Priorität. Durch das Anlegen dieses Plans beginnt weder die Implementierung von REV.2.0 noch eine Mainnet-Handlung. Der bestehende verbindliche BSC-Phasenplan wird nicht stillschweigend ersetzt.

**REV.2.0-Implementierungsziel:** Der Bot überprüft dauerhaft und nachweisbar, ob die für einen konkret freigegebenen Handelsweg qualifizierten Abhängigkeiten weiterhin gelten, erkennt Änderungen an Contracts, Launchpads, Signalquellen, RPC/WSS- und Builder-Schnittstellen und trifft daraufhin eine korrekte, dauerhaft dokumentierte BUY-/Exit-Entscheidung. Die vollständige Anzeige und die vorgesehenen Betreiberaktionen reichen bis ins Frontend.

Die Reihenfolge **für diese Iteration** lautet: Freeze und Beweise → WATCH-Kern → Chain-/Source-/Adapter-Integration → Requalifikation und Exit-Regeln → komplette Frontend-Durchleitung → Test-, Latenz- und Gesamtabnahme. REV.2.0 erteilt keine automatische Freigabe für BUY, SELL, SIGN, ARM, SEND, Builder-Probes oder kostenpflichtige Anbieter. Der gesondert vereinbarte reale BUY-/SELL-Testablauf von REV.1 wird durch dieses Zukunftsdokument nicht eigenmächtig verändert.

### 2. Verbindlicher Funktionsumfang

| Bereich | REV.2.0-Soll |
|---|---|
| Deployment / Contracts | Flap, Four Classic und OpenFour: vollständiger versionierter Abhängigkeitsgraph, Proxy-/Implementation-/Admin-/Beacon- und ökonomisch relevante Slots, Code-Hashes, Selektoren, Clone-Implementierungen |
| Upgrade-Früherkennung | Upgrade-Events, relevante Pending-Admin-Aktionen, Safe-/Timelock-Pfade, unbekannte Selektoren als Anomalie, Parameteränderungen; Pending allein beweist kein Upgrade |
| Chain-Nachweise | Aktueller kanonischer Blockhash, zwei unabhängige kostenlose Leser, Widerspruch → UNKNOWN, Reorg-Invalidierung |
| Provider | Je Source und Builder: Verfügbarkeit, Datenqualität, Latenz, Fehlerquote und Schemaänderungen; nicht verwendete defekte Alternativen blockieren keine qualifizierte andere Route |
| Native Zulassung | Ein wirtschaftlicher Owner; typisierte BuyRoute/GateCell; Prüfung vor Executor, im Owner und unmittelbar vor tatsächlichem Egress, ohne RPC-/Datei-I/O im BUY-Hotpath |
| Drift | Reproduzierbare, neustartfeste Sperre bei bestätigter semantischer Abweichung; Freigabe nur durch nachweislich qualifizierten neuen Freeze und Betreiberbestätigung |
| Offene Positionen | EXIT_AFTER_DRIFT ausschließlich mit separat gebundener Exit-Authority, identischer signierter Transaktion, lokalem EVM-Nachweis, Token-/BNB-Deltas und Slippage-Grenze |
| Frontend / CLI | Native Status- und Ereignisrecords → Protobuf → Reporter → Gateway → Store → globales Banner, BSC-Monitoring, LIVE-Reiter, Warnungen, Verlauf und expliziter Freigabe-Dialog |
| Leistung | Vergleich desselben vollständigen BUY-Pfads mit/ohne WATCH; keine unbelegte Übernahme des BSC3-076-1-ms-Nachweises |
| Evidenz | Binäre, dauerhafte Records; Git-SHA, Freeze-SHA, Blocknummer/-hash, Quellenidentität, Ereignisse, Messungen und Tests; kein internes JSON |

### 3. Umsetzungspakete W0–W7

| Paket | Geplante Umsetzung | Abnahme |
|---|---|---|
| **W0** | Versionierter Freeze-Abhängigkeitsgraph, Provider-Basislinien, zwei Leser | Beleggebundene, vollständige Abhängigkeiten |
| **W1** | `watch.rs`, typisierte Gates/Routen, drei native Kontrollstellen, erster Status-Push | Native Tests und Frontend-Verbindung |
| **W2** | Chain-Probes, Proxy-/Admin-/Timelock-Signale, Reorg, dauerhaftes Latch und Ereignisjournal | Drift-, Restart-, Konflikt- und Speicherfehler-Tests |
| **W3** | Source-/Adapter-/Execution-Monitoring, Anomalien, Terms-Drift | Qualität, Ausfall, Latenz, Fenster-/Erholungstests |
| **W4** | Freeze-Verifier, aktivierter neuer Freeze, CAS-Clear, Betreiber-Dialog | Requalifikation nur mit gültiger Evidenz |
| **W5** | Qualifizierter EXIT_AFTER_DRIFT auf der tatsächlichen Position | Signed-Byte-, Delta-, Slippage- und Negativtests |
| **W6** | Vollständige GUI-/CLI-Projektion, Alarmierung, Verlauf und Quittierung | Frontend FT-01–FT-08, inklusive Browser-E2E |
| **W7** | Mutationstests, Hotpath-Benchmark, unabhängiges Review, Gesamtregression | Alle bindenden Gates am finalen SHA bestanden |

### 4. Erfolgsbedingungen / Definition of Done

1. Jede relevante Änderung wird auf den **tatsächlich verwendeten** Source×Venue×Adapter- und Grant-Scope zurückgeführt. Unbestätigte Anzeichen (`SUSPECT`) werden nicht als nachgewiesene semantische Drift ausgegeben.
2. Ein BUY wird ausschließlich mit frischem, für **denselben kanonischen Head** gültigem Gate erlaubt; aktuelle Zulassung und Release-Barriere lesen dieselbe autoritative WATCH-Zustandsfolge. Vor dem ersten Byte wird die tatsächlich genutzte Route erneut geprüft.
3. Ein bestätigter Drift sperrt neue BUYs auch nach Crash/Restart. Ein Speicher-/Journalfehler lässt die WATCH nicht fälschlich freigeben.
4. Eine Requalifikation erfordert neuen Freeze-Record, unabhängige Leser, EVM-Parität, Versions-/Blockbindung, gültige Betreiberbestätigung und eine neue erfolgreiche Probe.
5. Bereits vorhandene Positionen und Exit-Autorisierung werden getrennt vom BUY behandelt. EXIT_AFTER_DRIFT darf nicht allein aufgrund einer Simulation als real erfolgreicher SELL bewertet werden.
6. Die vollständige Kette **Rust → Protobuf → Reporter → Gateway → Frontend** ist nachweislich angeschlossen, einschließlich Status, Alarmen, Verlauf, Warnbanner und Quittierungs-/Freigabefunktionen. Jede Anzeige beruht auf dem nativen Zustand.
7. Kein WATCH-bedingter Ereignis- oder Alarmverlust ohne dokumentierte Lücke; Crash-/Restart-/Reorg-/CAS-Races sind testabgedeckt.
8. Gate- und Egress-Latenz (p50/p99/max), Head→Verdikt, BUY-Blockierungsrate, zusätzliche Allokationen und Lock-Konflikte werden mit identischem Korpus vor/nach WATCH gemessen. Die numerische Freigabeschwelle wird aus Messungen verbindlich festgelegt, nicht als bereits bestanden behauptet.
9. Je Paket: gezielte Rot→Grün-Regression, Mutationsnachweise, unabhängiges Review, vollständiges CI und versionierte Evidence; keine ungeprüften Behauptungen über Mainnet-Qualifikation.
10. **REV.2.0 PASS** erst, wenn alle W0–W7-Abnahmen sowie die Frontend-E2E- und Performance-Gates auf demselben finalen Release-Stand belegt sind. Code fertig ≠ wirtschaftlich freigegeben.

### 5. Verbindliche Prüfpunkte bei der konkreten Integration durch Claude

Der folgende Anhang ist ein **Code-Entwurf**, noch kein kompilierter oder im Repository integrierter Produktionscode. Claude muss die dortigen Typ- und Pfadangaben gegen den aktuellen `main`-Stand prüfen und bestehende Owner-/Transport-/Record-/Gateway-Strukturen wiederverwenden, ohne eine zweite Autorität oder parallele Architektur zu errichten.

Insbesondere nachzuweisen: vollständige Admin-/Safe-/Timelock-Erfassung auch über berechtigte Relayer; generation- und Grant-gebundene Exit-Freigaben; atomare Ereignispersistenz vor Entfernung aus der Queue; Release-Barriere mit neuestem Gate; Performance-Einfluss der Synchronisation; Protobuf-Topic-Registry samt generiertem Client, `bscAdapter`, `bscStore`, Monitoring und LIVE-Frontend. Dies sind **Prüfaufträge**, keine ungetesteten Behauptungen, dass V0.4 sie bereits vollständig erfüllt.

### 6. Ausführungsregeln

Die Implementierung erfolgt ausschließlich in **BSC** und in den für seine Wire-/Frontend-Anbindung notwendigen gemeinsamen Komponenten. Änderungen an Project_0-Fachlogik sind ausgeschlossen. FREE_ONLY, binäre interne Formate, keine Mock-Pfade als produktive Beweise, keine hartkodierten Qualifikationen und bestehende Sicherheits-/Freigabegrenzen bleiben verbindlich. Das Vorhaben erhält eine eigene dokumentierte REV.2.0-Abnahme; ältere REV.1-Belege dürfen nicht ohne identische Git-, Binary- und Config-Identität als aktuelle PASSs gezählt werden.

---

# Anhang A – BSC-WATCH V0.4 (vollständige Implementierungsgrundlage)

Der folgende Text ist als technische Ausgangsversion **wortgetreu** aus der bereitgestellten V0.4 übernommen; fachliche Ausführungsreihenfolge und spätere Entscheidungen stehen im Hauptteil dieses REV.2.0-Plans. Die Rust-Snippets sind Implementierungsentwürfe, keine Build-Nachweise.

# BSC-WATCH V0.4 – verbindliche Implementierungsgrundlage

**Stand:** 31.10.2026
**Basis:** V0.2 (Spezifikation), V0.3 (Code-Kern), Review 2 (6 Befunde), Frontend-Durchleitung
**Status:** Implementierungsgrundlage. Noch nicht integriert, nicht abgenommen.
**Verhältnis:** V0.4 ersetzt V0.3. Wo V0.2 genauer ist, gilt V0.2 zusätzlich.

---

## 0. Änderungen gegenüber V0.3

| Befund (Review 2) | Korrektur in V0.4 |
|---|---|
| HOCH: `allows_buy()` nimmt eine beliebige Subjektliste | Ein BUY geht nur über eine typisierte `BuyRoute` (Venue, Quelle, ≥ 1 Adapter, keine Duplikate). Entscheidungen fallen nur über `GateCell` |
| HOCH: `clear()` ohne Generation-CAS | CAS auf die Generation des Subjekts, die gesehene Sperre und einen `QualifiedFreeze`. Der neue Freeze wird mit dem Cleared-Eintrag aktiv und bleibt dauerhaft gebunden; Probes zählen nur gegen den aktiven Freeze |
| HOCH: `is_admin_action()` prüft nur den Absender | `classify_admin_action()`: Admin-EOA laut Freeze **und** Ziel im Abhängigkeitsgraph; Safe-Ausführung (auch Delegatecall) und Timelock schedule/execute werden aufgelöst |
| HOCH: EXIT_AFTER_DRIFT ohne Slippage, `eth_call`-Output | Lauf der exakt signierten Bytes im bestehenden lokalen EVM-Simulator mit gemessenen Token- und BNB-Deltas. Mindestmenge und Slippage-Grenze aus der `ExitAuthority` prüft der Code selbst |
| MITTEL: `execution()` ohne Stichprobe OK | Ohne Messung kein neuer Nachweis, das Verdikt verfällt zu `Unknown`. Ohne Basislinie `Unqualified` |
| MITTEL: Gate kopierbar | `GateCell` ist die einzige Entscheidungsschnittstelle. Die Release-Barriere liest den neuesten Stand und prüft die tatsächlich genutzte Route |
| Neu | §7: Durchleitung Native → Protobuf → Gateway → Frontend, Banner, Verlauf, Freigabe-Dialog, System- und Browsermeldungen |
| Neu | §10: vollständiger Sendepfad mit und ohne WATCH (BSC3-076) |

## 1. Zweck und Grenzen

- Die Watch hält je Subjekt fest, ob die qualifizierten Voraussetzungen **jetzt** gelten. Gelten sie nicht, sperrt sie neue BUYs sofort.
- Der native Owner (`sources.rs`) bleibt die einzige wirtschaftliche Autorität. Die Watch gibt nie frei, sendet nie und requalifiziert nie automatisch.
- Fail-closed: nie geprüft, veraltet, fehlgeschlagen, widersprüchlich, durch Reorg entwertet oder nicht speicherbar ist nie OK.
- Kein I/O im BUY-Pfad. Chain-Abfragen und Simulationen laufen im Probe-Worker; der BUY-Pfad liest nur `GateCell`.
- Jede sichtbare Zustandsänderung ist ein `WatchEvent`: im Journal gespeichert und im Frontend angezeigt.
- Die Watch läuft immer, auch ohne Grant und ohne LIVE. Eine abgeschaltete Watch gibt es nur im Benchmark (§10).

## 2. Subjekte, Zustände, Klassen

| Subjekt | Bedeutung |
|---|---|
| `Chain` | kanonische Sicht: Head, Hash, Übereinstimmung der Leser |
| `Watch` | die Watch selbst: am Head ausgewertet, Sperren speicherbar |
| `Execution` | eigener Pfad: T_ingress→T_send gegen die Basislinie, verlorene BUYs je Fenster |
| `Source(id)` | jede Signalquelle |
| `Venue(id)` | Flap, Four Classic, OpenFour |
| `Adapter(id)` | jeder Sendeadapter (48Club, BlockRazor) |

| Zustand | Wirkung |
|---|---|
| `Ok` | erlaubt, sofern frisch am aktuellen Head |
| `Degraded` | BUYs gesperrt; Exits je nach Subjekt erlaubt |
| `Suspect` | BUYs gesperrt, bis eine Prüfung ab dem Verdachtsblock entscheidet |
| `Blocked` | BUYs gesperrt, gelatcht, Ende nur mit `QualifiedFreeze` |
| `Unknown` | BUYs gesperrt, bis wieder frisch geprüft |
| `Unqualified` | nicht qualifiziert, kein aktiver Freeze oder keine Basislinie |

| Klasse | Ende |
|---|---|
| `Semantic` | nur durch Requalifikation (§9); übersteht Neustarts |
| `Suspect` | Prüfung gegen den aktiven Freeze ab dem Verdachtsblock |
| `Infrastructure` | nächste erfolgreiche, frische Prüfung |
| `Performance` | `BSC_WATCH_RECOVER_EVALUATIONS` saubere Auswertungen mit Messdaten |

## 3. Signale

**Venues**
- **V1 Upgrade-Logs:** `Upgraded`, `AdminChanged`, `BeaconUpgraded` aller Proxys im Graph → `Suspect`, die Probe entscheidet.
- **V2 Probe je Block, zwei unabhängige Leser:**
  - Implementierungs-, Admin- und Beacon-Slot sowie alle gebundenen wirtschaftlichen Parameter-Slots am Blockhash.
  - Alle `BSC_WATCH_SNAPSHOT_BLOCKS` Blöcke zusätzlich Code-Hashes, Selektoren und storage-gewählte Clone-Implementierungen.
  - Eine Probe meldet, gegen welchen Freeze sie geprüft hat; nur der aktive Freeze zählt.
  - Abweichung → `Blocked`; Widerspruch der Leser → `Unknown`; Prüfung gegen einen alten Freeze → `Unknown`.
- **V3 Admin-Aktion pending (`classify_admin_action`):** Signer ist ein Admin-EOA laut Freeze **und** das Ziel liegt im Graph, oder es ist eine Safe-Ausführung in den Graph bzw. per Delegatecall, oder ein Timelock-schedule/execute in den Graph → `Suspect`.
  - Entschieden ist der Verdacht, wenn die Chain die Nonce des Signers verbraucht und eine Probe ab diesem Block passt.
  - Sachfremde Transaktionen eines Admins und alle Transaktionen Fremder lösen nichts aus.
- **V4 Timelock `CallScheduled`:** Vorwarnung als Ereignis, keine Sperre.
- **V5 Unbekannte Selektoren:** über `BSC_WATCH_UNKNOWN_SELECTOR_MAX` im Fenster → Alarm und sofortige Probe.
- **V6 TERMS_DRIFT** systematisch über `BSC_WATCH_TERMS_DRIFT_MAX` → `Blocked`.

**Quellen**
- **S1:** `ingress::health()` → `Degraded`.
- **S2:** Ablehnungsquote mit Mindeststichprobe → `Degraded`.
- **S3:** relativer Lag-p99 gegen Basislinie × Faktor → `Degraded`.
- **S4:** Kopf-Rückstand → `Degraded`.

**Sendeadapter**
- **A1:** nicht einordenbare Antwort → `Suspect`.
- **A2:** Verbindung weg oder Fehlerquote → `Degraded`.
- **A3:** RTT-p99 → `Degraded`.

**Eigener Pfad**
- **E1:** p99 von T_ingress→T_send, aus LIVE und dem nichtsendenden Shadow-Pfad jedes Launches → `Degraded`. Ohne Messung kein Nachweis; ohne Basislinie `Unqualified`.
- **E2:** verlorene BUYs je Fenster → BUYs pausiert, Abkühlphase.

**Watch:** Herzschlag je Head (`evaluated`). Eine nicht speicherbare Sperre lässt die Watch bis zum Neustart `Unknown`.

## 4. Wirkung

**BUY, nur über eine `BuyRoute` und die `GateCell`:**
1. **Executor:** `BuyRoute::new(venue, auslösende Quelle, OK-Adapter ∩ Grant-Adapter)`, dann `gate_cell.allows_buy(head, &route)`. Das Ergebnis ist eine `BuyClearance`. Ohne Route oder bei Ablehnung: `Refusal::Watch`, nichts wird verbraucht.
2. **Owner (`admit_live`):** `allows_buy` erneut am neuesten Stand.
3. **Release-Barriere unmittelbar vor dem ersten Byte:** `confirm_before_egress(head, &clearance, &tatsächliche Route)`. Der neueste Stand muss am selben Head die tatsächlich genutzten Adapter zulassen. Sonst greift der bestehende WITHDRAWN-Pfad. Zusätzlich gilt die bestehende STALE_HEAD-Regel.

Ein BUY braucht immer `Chain`, `Watch`, `Execution` und jedes Subjekt der Route OK.

**Exit:** `gate_cell.allows_exit(head, venue, adapters)`.
- `Chain` und `Watch` müssen OK sein; das Venue darf weder `Blocked` noch `Suspect` sein; mindestens ein Adapter muss OK sein.
- `Execution` wird nicht gebraucht.
- Ein gedriftetes Venue lässt sich nur über EXIT_AFTER_DRIFT (§8) verlassen.

**Unverändert:** Not-Aus, Grant-Ablauf und Neustart (BSC3-058).

## 5. Code-Kern `bsc/engine/crates/execution/src/watch.rs`

```rust
//! BSC-WATCH (spec V0.4): the dynamic qualification of the trading chain.
//!
//! Per subject (the chain view, the watch itself, the own execution path,
//! every source, venue and send adapter) this module holds whether the
//! qualified prerequisites hold *now*. Decisions go only through a
//! [`GateCell`] (the newest published state, bound to one head) and a typed
//! [`BuyRoute`]; the native owner (`sources.rs`) stays the one economic
//! authority. This module admits and sends nothing.
//!
//! No I/O on the BUY hot path: the follower, the ingress, the adapters and
//! the probe worker hand typed facts in. A confirmed semantic drift is
//! latched through [`LatchStore`] (append-only, the record system),
//! survives a restart and ends only with a [`QualifiedFreeze`], which also
//! becomes the venue's active freeze. Every visible change is a
//! [`WatchEvent`] for the journal and the frontend. Fail-closed throughout.

use crate::ingress::Health;
use crate::sources::{LiveAdapter, SourceId, VenueId};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const MAX_SUBJECTS: usize = 16;
pub const MAX_ADAPTERS: usize = 4;
/// Events held until the runtime drains them into the journal.
pub const MAX_PENDING_EVENTS: usize = 1024;

/// What the watch judges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Chain,
    Watch,
    Execution,
    Source(SourceId),
    Venue(VenueId),
    Adapter(LiveAdapter),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Ok,
    Degraded,
    Suspect,
    Blocked,
    Unknown,
    Unqualified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Semantic,
    Suspect,
    Infrastructure,
    Performance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockRef {
    pub number: u64,
    pub hash: [u8; 32],
}

/// What a pending admin transaction would do ([`classify_admin_action`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminAction {
    /// Any call of an admin EOA to a contract of the venue's graph.
    Direct { to: [u8; 20], selector: [u8; 4] },
    /// A Safe execution into the graph or a timelock of it, or any
    /// delegatecall (MultiSend and the like: conservative).
    Safe { safe: [u8; 20], inner_to: [u8; 20], delegate: bool },
    /// A timelock schedule or execute into the graph; batches conservatively.
    Timelock { timelock: [u8; 20], selector: [u8; 4] },
}

/// A typed finding; its code is what the owner, the ledger and the
/// frontend show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    NotProbed,
    NotQualified,
    FreezeMismatch,
    StorageDrift { address: [u8; 20], slot: [u8; 32], observed: [u8; 32] },
    CodeDrift { address: [u8; 20], observed: [u8; 32] },
    TermsDrift,
    UpgradeEvent { proxy: [u8; 20], topic: [u8; 32] },
    PendingAdminAction { tx_hash: [u8; 32], signer: [u8; 20], action: AdminAction },
    /// An announcement only (V4): an event, never a state.
    TimelockScheduled { timelock: [u8; 20], ready_unix_s: u64 },
    ProbeFailed,
    ProbesDisagree,
    Reorged,
    LatchUnwritable,
    SourceHealth(&'static str),
    SourceRejects { rejected: u64, checked: u64 },
    SourceLag { p99_ns: u64, limit_ns: u64 },
    AdapterSchema,
    AdapterErrors { errors: u64, requests: u64 },
    AdapterRtt { p99_ns: u64, limit_ns: u64 },
    OwnLatency { p99_ns: u64, limit_ns: u64 },
    LostBuys { lost: u32, limit: u32 },
}

impl Cause {
    pub fn code(&self) -> &'static str {
        match self {
            Cause::NotProbed => "WATCH_NOT_PROBED",
            Cause::NotQualified => "WATCH_NOT_QUALIFIED",
            Cause::FreezeMismatch => "WATCH_FREEZE_MISMATCH",
            Cause::StorageDrift { .. } => "WATCH_STORAGE_DRIFT",
            Cause::CodeDrift { .. } => "WATCH_CODE_DRIFT",
            Cause::TermsDrift => "WATCH_TERMS_DRIFT",
            Cause::UpgradeEvent { .. } => "WATCH_UPGRADE_EVENT",
            Cause::PendingAdminAction { .. } => "WATCH_PENDING_ADMIN_ACTION",
            Cause::TimelockScheduled { .. } => "WATCH_TIMELOCK_SCHEDULED",
            Cause::ProbeFailed => "WATCH_PROBE_FAILED",
            Cause::ProbesDisagree => "WATCH_PROBES_DISAGREE",
            Cause::Reorged => "WATCH_REORGED",
            Cause::LatchUnwritable => "WATCH_LATCH_UNWRITABLE",
            Cause::SourceHealth(_) => "WATCH_SOURCE_HEALTH",
            Cause::SourceRejects { .. } => "WATCH_SOURCE_REJECTS",
            Cause::SourceLag { .. } => "WATCH_SOURCE_LAG",
            Cause::AdapterSchema => "WATCH_ADAPTER_SCHEMA",
            Cause::AdapterErrors { .. } => "WATCH_ADAPTER_ERRORS",
            Cause::AdapterRtt { .. } => "WATCH_ADAPTER_RTT",
            Cause::OwnLatency { .. } => "WATCH_OWN_LATENCY",
            Cause::LostBuys { .. } => "WATCH_LOST_BUYS",
        }
    }

    pub fn class(&self) -> Class {
        match self {
            Cause::StorageDrift { .. } | Cause::CodeDrift { .. } | Cause::TermsDrift => {
                Class::Semantic
            }
            Cause::UpgradeEvent { .. }
            | Cause::PendingAdminAction { .. }
            | Cause::TimelockScheduled { .. }
            | Cause::AdapterSchema => Class::Suspect,
            Cause::NotProbed
            | Cause::NotQualified
            | Cause::FreezeMismatch
            | Cause::ProbeFailed
            | Cause::ProbesDisagree
            | Cause::Reorged
            | Cause::LatchUnwritable => Class::Infrastructure,
            Cause::SourceHealth(_)
            | Cause::SourceRejects { .. }
            | Cause::SourceLag { .. }
            | Cause::AdapterErrors { .. }
            | Cause::AdapterRtt { .. }
            | Cause::OwnLatency { .. }
            | Cause::LostBuys { .. } => Class::Performance,
        }
    }

    fn state(&self) -> State {
        if *self == Cause::NotQualified {
            return State::Unqualified;
        }
        match self.class() {
            Class::Semantic => State::Blocked,
            Class::Suspect => State::Suspect,
            Class::Infrastructure => State::Unknown,
            Class::Performance => State::Degraded,
        }
    }

    fn recovers_by_streak(&self) -> bool {
        self.class() == Class::Performance || *self == Cause::AdapterSchema
    }
}

/// What a probe of the frozen dependency graph found at a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    /// Every bound slot and code hash as in the freeze `freeze_sha256`.
    Match { freeze_sha256: [u8; 32] },
    /// A difference to the freeze (StorageDrift, CodeDrift, TermsDrift).
    Drift(Cause),
    Failed,
    Disagree,
}

/// What the two independent readers say about the head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainCheck {
    Agree,
    Disagree,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub subject: Subject,
    pub state: State,
    pub cause: Option<Cause>,
    pub at: Option<BlockRef>,
    pub latched: bool,
}

impl Verdict {
    fn unprobed(subject: Subject) -> Self {
        Verdict { subject, state: State::Unknown, cause: Some(Cause::NotProbed), at: None, latched: false }
    }
}

/// One visible change (or announcement, `from == to`) for the journal and
/// the frontend; the runtime stamps sequence and wall time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchEvent {
    pub subject: Subject,
    pub from: State,
    pub to: State,
    pub cause: Option<Cause>,
    pub at: Option<BlockRef>,
    pub latched: bool,
    pub subject_generation: u64,
}

/// One append-only latch record in the owner state (the record system).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatchEvent {
    Set { subject: Subject, cause: Cause, at: BlockRef, evidence_sha256: [u8; 32] },
    Cleared {
        subject: Subject,
        subject_generation: u64,
        latch_evidence: [u8; 32],
        freeze_sha256: [u8; 32],
        freeze_block: u64,
    },
}

pub trait LatchStore {
    fn load(&self) -> Result<Vec<LatchEvent>, String>;
    fn append(&mut self, event: &LatchEvent) -> Result<(), String>;
}

/// Proof that the freeze verifier accepted a new freeze of `subject`
/// (spec §9). Private fields; the only call of [`QualifiedFreeze::verified`]
/// is in that verifier, pinned by a CI source scan. A hash alone never
/// clears a latch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedFreeze {
    subject: Subject,
    freeze_sha256: [u8; 32],
    block: BlockRef,
}

impl QualifiedFreeze {
    pub(crate) fn verified(subject: Subject, freeze_sha256: [u8; 32], block: BlockRef) -> Self {
        QualifiedFreeze { subject, freeze_sha256, block }
    }
    pub fn subject(&self) -> Subject {
        self.subject
    }
    pub fn freeze_sha256(&self) -> [u8; 32] {
        self.freeze_sha256
    }
    pub fn block(&self) -> BlockRef {
        self.block
    }
}

/// Validated `BSC_WATCH_*` values (typed config, ranges, unknown keys stop).
#[derive(Clone, Copy, Debug)]
pub struct WatchConfig {
    pub max_stale_blocks: u64,
    pub perf_max_stale_blocks: u64,
    pub recover_evaluations: u32,
    pub min_samples: u64,
    pub max_reject_ppm: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SourceWindow {
    pub checked: u64,
    pub rejected: u64,
    pub lag_ns: Vec<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct AdapterWindow {
    pub connected: bool,
    pub requests: u64,
    pub errors: u64,
    pub uninterpretable: u64,
    pub rtt_ns: Vec<u64>,
}

/// The own path's window: T_ingress→T_send (LIVE and the non-sending
/// shadow path of every launch) and the BUYs lost in it.
#[derive(Clone, Debug, Default)]
pub struct ExecWindow {
    pub own_ns: Vec<u64>,
    pub lost_buys: u32,
}

pub struct Watch {
    config: WatchConfig,
    generation: u64,
    head: Option<BlockRef>,
    verdicts: Vec<Verdict>,
    subject_generation: Vec<u64>,
    streak: Vec<u32>,
    suspect_from: Vec<Option<u64>>,
    latch_evidence: Vec<Option<[u8; 32]>>,
    active_freeze: Vec<Option<[u8; 32]>>,
    pending: Vec<([u8; 32], Subject)>,
    events: VecDeque<WatchEvent>,
    events_dropped: u64,
    store: Box<dyn LatchStore>,
    unwritable: bool,
}

impl Watch {
    /// Everything starts UNKNOWN; `freezes` are the venues' frozen
    /// deployments, replaced by any durable `Cleared` record; persisted
    /// latches are BLOCKED at once. An unreadable latch store leaves the
    /// watch itself UNKNOWN for the life of the process.
    pub fn new(
        config: WatchConfig,
        subjects: &[Subject],
        freezes: &[(VenueId, [u8; 32])],
        store: Box<dyn LatchStore>,
    ) -> Result<Self, String> {
        let mut all = vec![Subject::Chain, Subject::Watch, Subject::Execution];
        for s in subjects {
            if !all.contains(s) {
                all.push(*s);
            }
        }
        if all.len() > MAX_SUBJECTS {
            return Err(format!("{} watch subjects, at most {MAX_SUBJECTS}", all.len()));
        }
        let n = all.len();
        let mut w = Watch {
            config,
            generation: 1,
            head: None,
            verdicts: all.into_iter().map(Verdict::unprobed).collect(),
            subject_generation: vec![0; n],
            streak: vec![0; n],
            suspect_from: vec![None; n],
            latch_evidence: vec![None; n],
            active_freeze: vec![None; n],
            pending: Vec::new(),
            events: VecDeque::new(),
            events_dropped: 0,
            store,
            unwritable: false,
        };
        for (venue, sha) in freezes {
            if let Some(i) = w.index(Subject::Venue(*venue)) {
                w.active_freeze[i] = Some(*sha);
            }
        }
        match w.store.load() {
            Ok(events) => events.into_iter().for_each(|e| w.replay(e)),
            Err(_) => w.fail_store(),
        }
        Ok(w)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn verdicts(&self) -> &[Verdict] {
        &self.verdicts
    }

    pub fn subject_generation(&self, s: Subject) -> Option<u64> {
        self.index(s).map(|i| self.subject_generation[i])
    }

    pub fn latch_evidence(&self, s: Subject) -> Option<[u8; 32]> {
        self.index(s).and_then(|i| self.latch_evidence[i])
    }

    /// The freeze the executor's deployment of `venue` must match.
    pub fn active_freeze(&self, venue: VenueId) -> Option<[u8; 32]> {
        self.index(Subject::Venue(venue)).and_then(|i| self.active_freeze[i])
    }

    /// The visible changes since the last call, for the event journal.
    pub fn take_events(&mut self) -> Vec<WatchEvent> {
        self.events.drain(..).collect()
    }

    pub fn events_dropped(&self) -> u64 {
        self.events_dropped
    }

    fn index(&self, s: Subject) -> Option<usize> {
        self.verdicts.iter().position(|v| v.subject == s)
    }

    fn replay(&mut self, e: LatchEvent) {
        match e {
            LatchEvent::Set { subject, cause, at, evidence_sha256 } => {
                if let Some(i) = self.index(subject) {
                    self.verdicts[i] = Verdict {
                        subject,
                        state: State::Blocked,
                        cause: Some(cause),
                        at: Some(at),
                        latched: true,
                    };
                    self.latch_evidence[i] = Some(evidence_sha256);
                }
            }
            LatchEvent::Cleared { subject, freeze_sha256, .. } => {
                if let Some(i) = self.index(subject) {
                    self.verdicts[i] = Verdict::unprobed(subject);
                    self.latch_evidence[i] = None;
                    self.active_freeze[i] = Some(freeze_sha256);
                }
            }
        }
    }

    fn push_event(&mut self, e: WatchEvent) {
        if self.events.len() == MAX_PENDING_EVENTS {
            self.events.pop_front();
            self.events_dropped += 1;
        }
        self.events.push_back(e);
    }

    /// Every verdict change goes through here: generations and the event.
    fn change(&mut self, i: usize, next: Verdict) {
        let prev = self.verdicts[i];
        if next == prev {
            return;
        }
        self.verdicts[i] = next;
        if next.state != prev.state || next.cause != prev.cause || next.latched != prev.latched {
            self.generation += 1;
            self.subject_generation[i] += 1;
            self.push_event(WatchEvent {
                subject: next.subject,
                from: prev.state,
                to: next.state,
                cause: next.cause,
                at: next.at,
                latched: next.latched,
                subject_generation: self.subject_generation[i],
            });
        }
    }

    fn fail_store(&mut self) {
        self.unwritable = true;
        if let Some(i) = self.index(Subject::Watch) {
            let next = Verdict { cause: Some(Cause::LatchUnwritable), ..Verdict::unprobed(Subject::Watch) };
            self.change(i, next);
        }
    }

    fn set(&mut self, i: usize, cause: Option<Cause>, at: Option<BlockRef>) {
        let v = self.verdicts[i];
        if v.latched || (v.subject == Subject::Watch && self.unwritable) {
            return;
        }
        let state = cause.map_or(State::Ok, |c| c.state());
        self.change(i, Verdict { state, cause, at, ..v });
    }

    fn latch(&mut self, i: usize, cause: Cause, at: BlockRef, evidence_sha256: [u8; 32]) {
        let subject = self.verdicts[i].subject;
        let event = LatchEvent::Set { subject, cause, at, evidence_sha256 };
        if self.store.append(&event).is_err() {
            // Held in memory for this process; a restart may not know it.
            self.fail_store();
        }
        self.latch_evidence[i] = Some(evidence_sha256);
        self.change(i, Verdict { subject, state: State::Blocked, cause: Some(cause), at: Some(at), latched: true });
    }

    fn recoverable(&mut self, i: usize, finding: Option<Cause>, at: BlockRef) {
        if let Some(c) = finding {
            self.streak[i] = 0;
            return self.set(i, Some(c), Some(at));
        }
        if self.verdicts[i].cause.is_some_and(|c| c.recovers_by_streak()) {
            self.streak[i] += 1;
            if self.streak[i] < self.config.recover_evaluations {
                return;
            }
        }
        self.streak[i] = 0;
        self.set(i, None, Some(at));
    }

    pub fn on_head(&mut self, head: BlockRef) {
        self.head = Some(head);
    }

    /// Every fact of `head` is applied: the watch is evaluated at it.
    pub fn evaluated(&mut self, head: BlockRef) {
        if self.head == Some(head)
            && let Some(i) = self.index(Subject::Watch)
        {
            self.set(i, None, Some(head));
        }
    }

    /// Blocks from `from` left the chain: no OK verdict checked there is
    /// evidence any more. Findings and latches stay.
    pub fn reorg(&mut self, from: u64) {
        for i in 0..self.verdicts.len() {
            let v = self.verdicts[i];
            if v.state == State::Ok && v.at.is_some_and(|b| b.number >= from) {
                self.set(i, Some(Cause::Reorged), None);
            }
        }
        if self.head.is_some_and(|h| h.number >= from) {
            self.head = None;
        }
    }

    pub fn chain(&mut self, at: BlockRef, check: ChainCheck) {
        let Some(i) = self.index(Subject::Chain) else { return };
        let cause = match check {
            ChainCheck::Agree => None,
            ChainCheck::Disagree => Some(Cause::ProbesDisagree),
            ChainCheck::Failed => Some(Cause::ProbeFailed),
        };
        self.set(i, cause, Some(at));
    }

    pub fn venue_probe(&mut self, venue: VenueId, at: BlockRef, probe: Probe, evidence_sha256: [u8; 32]) {
        let subject = Subject::Venue(venue);
        let Some(i) = self.index(subject) else { return };
        match probe {
            Probe::Drift(c) if c.class() == Class::Semantic => self.latch(i, c, at, evidence_sha256),
            Probe::Drift(_) | Probe::Failed => self.set(i, Some(Cause::ProbeFailed), Some(at)),
            Probe::Disagree => self.set(i, Some(Cause::ProbesDisagree), Some(at)),
            Probe::Match { freeze_sha256 } => match self.active_freeze[i] {
                None => self.set(i, Some(Cause::NotQualified), Some(at)),
                Some(active) if active != freeze_sha256 => {
                    self.set(i, Some(Cause::FreezeMismatch), Some(at))
                }
                Some(_) => {
                    let undecided = self.pending.iter().any(|(_, s)| *s == subject)
                        || self.suspect_from[i].is_some_and(|from| at.number < from);
                    if !undecided {
                        self.suspect_from[i] = None;
                        self.set(i, None, Some(at));
                    }
                }
            },
        }
    }

    pub fn upgrade_event(&mut self, venue: VenueId, at: BlockRef, proxy: [u8; 20], topic: [u8; 32]) {
        let Some(i) = self.index(Subject::Venue(venue)) else { return };
        self.suspect_from[i] = Some(self.suspect_from[i].map_or(at.number, |f| f.max(at.number)));
        self.set(i, Some(Cause::UpgradeEvent { proxy, topic }), Some(at));
    }

    /// A classified admin action ([`classify_admin_action`]) pending:
    /// suspect until the chain consumes the signer's nonce
    /// ([`Watch::pending_decided`]) and a probe at or after that block passes.
    pub fn pending_admin_action(&mut self, venue: VenueId, tx_hash: [u8; 32], signer: [u8; 20], action: AdminAction) {
        let subject = Subject::Venue(venue);
        let Some(i) = self.index(subject) else { return };
        if !self.pending.iter().any(|(h, _)| *h == tx_hash) {
            self.pending.push((tx_hash, subject));
        }
        let at = self.verdicts[i].at;
        self.set(i, Some(Cause::PendingAdminAction { tx_hash, signer, action }), at);
    }

    /// The chain consumed the signer's nonce of `tx_hash` at `at`.
    pub fn pending_decided(&mut self, tx_hash: [u8; 32], at: BlockRef) {
        let Some(pos) = self.pending.iter().position(|(h, _)| *h == tx_hash) else { return };
        let (_, subject) = self.pending.remove(pos);
        if let Some(i) = self.index(subject) {
            self.suspect_from[i] = Some(self.suspect_from[i].map_or(at.number, |f| f.max(at.number)));
        }
    }

    /// A timelock announced a call into the graph (V4): an event for the
    /// operator, no state; its execution is watched by V1/V2.
    pub fn announced(&mut self, venue: VenueId, at: BlockRef, timelock: [u8; 20], ready_unix_s: u64) {
        let Some(i) = self.index(Subject::Venue(venue)) else { return };
        let v = self.verdicts[i];
        self.push_event(WatchEvent {
            subject: v.subject,
            from: v.state,
            to: v.state,
            cause: Some(Cause::TimelockScheduled { timelock, ready_unix_s }),
            at: Some(at),
            latched: v.latched,
            subject_generation: self.subject_generation[i],
        });
    }

    pub fn source(&mut self, source: SourceId, at: BlockRef, health: Health, w: &SourceWindow, lag_limit_ns: Option<u64>) {
        let Some(i) = self.index(Subject::Source(source)) else { return };
        let c = self.config;
        let finding = match health {
            Health::Healthy => None,
            Health::Off | Health::Unqualified => Some(Cause::NotQualified),
            h => Some(Cause::SourceHealth(h.name())),
        }
        .or_else(|| {
            (w.checked >= c.min_samples
                && w.rejected.saturating_mul(1_000_000) > w.checked.saturating_mul(c.max_reject_ppm))
            .then_some(Cause::SourceRejects { rejected: w.rejected, checked: w.checked })
        })
        .or_else(|| {
            let limit = lag_limit_ns?;
            let p99 = percentile(&w.lag_ns, 99, c.min_samples)?;
            (p99 > limit).then_some(Cause::SourceLag { p99_ns: p99, limit_ns: limit })
        });
        self.recoverable(i, finding, at);
    }

    pub fn adapter(&mut self, adapter: LiveAdapter, at: BlockRef, w: &AdapterWindow, rtt_limit_ns: Option<u64>) {
        let Some(i) = self.index(Subject::Adapter(adapter)) else { return };
        let c = self.config;
        let finding = if !w.connected {
            Some(Cause::AdapterErrors { errors: w.errors.max(1), requests: w.requests })
        } else if w.uninterpretable > 0 {
            Some(Cause::AdapterSchema)
        } else if w.requests >= c.min_samples
            && w.errors.saturating_mul(1_000_000) > w.requests.saturating_mul(c.max_reject_ppm)
        {
            Some(Cause::AdapterErrors { errors: w.errors, requests: w.requests })
        } else {
            rtt_limit_ns.and_then(|limit| {
                let p99 = percentile(&w.rtt_ns, 99, c.min_samples)?;
                (p99 > limit).then_some(Cause::AdapterRtt { p99_ns: p99, limit_ns: limit })
            })
        };
        self.recoverable(i, finding, at);
    }

    /// E1/E2. Without a baseline the path is not qualified; a window
    /// without enough samples is no evidence (the verdict is not renewed
    /// and turns UNKNOWN once stale).
    pub fn execution(&mut self, at: BlockRef, w: &ExecWindow, own_limit_ns: Option<u64>, max_lost: u32) {
        let Some(i) = self.index(Subject::Execution) else { return };
        if max_lost > 0 && w.lost_buys >= max_lost {
            let lost = Cause::LostBuys { lost: w.lost_buys, limit: max_lost };
            return self.recoverable(i, Some(lost), at);
        }
        let Some(limit) = own_limit_ns else {
            return self.set(i, Some(Cause::NotQualified), Some(at));
        };
        let Some(p99) = percentile(&w.own_ns, 99, self.config.min_samples) else {
            return;
        };
        let finding = (p99 > limit).then_some(Cause::OwnLatency { p99_ns: p99, limit_ns: limit });
        self.recoverable(i, finding, at);
    }

    /// The operator's requalification of exactly the latch they saw: CAS on
    /// the subject's generation, its evidence hash, and a freeze the
    /// verifier accepted at or after the drift. The new freeze is active
    /// exactly when its record is durable; the subject is UNKNOWN until a
    /// probe against it passes.
    pub fn clear(
        &mut self,
        subject: Subject,
        expected_subject_generation: u64,
        latch_evidence: [u8; 32],
        freeze: &QualifiedFreeze,
    ) -> Result<(), &'static str> {
        let i = self.index(subject).ok_or("WATCH_UNKNOWN_SUBJECT")?;
        if self.subject_generation[i] != expected_subject_generation {
            return Err("WATCH_GENERATION_CHANGED");
        }
        let v = self.verdicts[i];
        if !v.latched {
            return Err("WATCH_NOT_LATCHED");
        }
        if self.latch_evidence[i] != Some(latch_evidence) {
            return Err("WATCH_LATCH_CHANGED");
        }
        if freeze.subject != subject {
            return Err("WATCH_FREEZE_OTHER_SUBJECT");
        }
        if v.at.is_some_and(|at| freeze.block.number < at.number) {
            return Err("WATCH_FREEZE_BEFORE_DRIFT");
        }
        if self.unwritable {
            return Err("WATCH_LATCH_UNWRITABLE");
        }
        let event = LatchEvent::Cleared {
            subject,
            subject_generation: expected_subject_generation,
            latch_evidence,
            freeze_sha256: freeze.freeze_sha256,
            freeze_block: freeze.block.number,
        };
        if self.store.append(&event).is_err() {
            self.fail_store();
            return Err("WATCH_LATCH_UNWRITABLE");
        }
        self.active_freeze[i] = Some(freeze.freeze_sha256);
        self.latch_evidence[i] = None;
        self.suspect_from[i] = None;
        self.change(i, Verdict::unprobed(subject));
        Ok(())
    }

    fn effective(&self, v: &Verdict) -> State {
        if v.latched {
            return State::Blocked;
        }
        if v.state != State::Ok {
            return v.state;
        }
        let (Some(head), Some(at)) = (self.head, v.at) else {
            return State::Unknown;
        };
        let max = match v.subject {
            Subject::Source(_) | Subject::Adapter(_) | Subject::Execution => {
                self.config.perf_max_stale_blocks
            }
            _ => self.config.max_stale_blocks,
        };
        let fresh = at.number <= head.number
            && head.number - at.number <= max
            && (at.number != head.number || at.hash == head.hash);
        if fresh { State::Ok } else { State::Unknown }
    }

    pub(crate) fn gate(&self) -> WatchGate {
        let mut gate = WatchGate { generation: self.generation, head: self.head, states: [None; MAX_SUBJECTS] };
        for (slot, v) in gate.states.iter_mut().zip(&self.verdicts) {
            *slot = Some((v.subject, self.effective(v)));
        }
        gate
    }

    /// Publishes the newest state; called after every batch of facts.
    pub fn publish(&self, cell: &GateCell) {
        cell.publish(self.gate());
    }
}

/// The complete, typed route of one BUY: its venue, the source whose
/// sighting triggered it and every adapter it is sent through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyRoute {
    venue: VenueId,
    source: SourceId,
    adapters: [Option<LiveAdapter>; MAX_ADAPTERS],
}

impl BuyRoute {
    /// None without an adapter, above MAX_ADAPTERS or with a duplicate.
    pub fn new(venue: VenueId, source: SourceId, adapters: &[LiveAdapter]) -> Option<Self> {
        if adapters.is_empty() || adapters.len() > MAX_ADAPTERS {
            return None;
        }
        let mut slots = [None; MAX_ADAPTERS];
        for (k, a) in adapters.iter().enumerate() {
            if adapters[..k].contains(a) {
                return None;
            }
            slots[k] = Some(*a);
        }
        Some(BuyRoute { venue, source, adapters: slots })
    }

    fn subjects(&self) -> impl Iterator<Item = Subject> + '_ {
        [Subject::Venue(self.venue), Subject::Source(self.source)]
            .into_iter()
            .chain(self.adapters.iter().flatten().map(|a| Subject::Adapter(*a)))
    }

    /// Same venue and source, and a subset of this route's adapters.
    fn covers(&self, other: &BuyRoute) -> bool {
        self.venue == other.venue
            && self.source == other.source
            && other.adapters.iter().flatten().all(|a| self.adapters.contains(&Some(*a)))
    }
}

/// What admission hands to the release barrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyClearance {
    pub generation: u64,
    pub head: BlockRef,
    route: BuyRoute,
}

/// One published state at one head (internal snapshot; decisions only
/// through [`GateCell`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchGate {
    pub generation: u64,
    pub head: Option<BlockRef>,
    states: [Option<(Subject, State)>; MAX_SUBJECTS],
}

impl WatchGate {
    pub const CLOSED: WatchGate = WatchGate { generation: 0, head: None, states: [None; MAX_SUBJECTS] };

    /// Never judged: UNKNOWN.
    pub fn state(&self, s: Subject) -> State {
        self.states.iter().flatten().find(|(x, _)| *x == s).map_or(State::Unknown, |(_, st)| *st)
    }

    fn allows_buy(&self, head: BlockRef, route: &BuyRoute) -> Result<BuyClearance, Subject> {
        if self.head != Some(head) {
            return Err(Subject::Watch);
        }
        let always = [Subject::Chain, Subject::Watch, Subject::Execution];
        if let Some(s) = always.into_iter().chain(route.subjects()).find(|s| self.state(*s) != State::Ok) {
            return Err(s);
        }
        Ok(BuyClearance { generation: self.generation, head, route: *route })
    }

    fn allows_exit(&self, head: BlockRef, venue: VenueId, adapters: &[LiveAdapter]) -> Result<(), Subject> {
        if self.head != Some(head) {
            return Err(Subject::Watch);
        }
        for s in [Subject::Chain, Subject::Watch] {
            if self.state(s) != State::Ok {
                return Err(s);
            }
        }
        let v = Subject::Venue(venue);
        if !matches!(self.state(v), State::Ok | State::Degraded) {
            return Err(v);
        }
        match adapters.iter().find(|a| self.state(Subject::Adapter(**a)) == State::Ok) {
            Some(_) => Ok(()),
            None => Err(adapters.first().map_or(Subject::Watch, |a| Subject::Adapter(*a))),
        }
    }
}

/// The one decision interface: always the newest published state. One
/// writer (the reconcile), uncontended readers; a poisoned lock admits
/// nothing.
#[derive(Clone)]
pub struct GateCell(Arc<Mutex<WatchGate>>);

impl Default for GateCell {
    fn default() -> Self {
        GateCell(Arc::new(Mutex::new(WatchGate::CLOSED)))
    }
}

impl GateCell {
    fn current(&self) -> WatchGate {
        self.0.lock().map(|g| *g).unwrap_or(WatchGate::CLOSED)
    }

    fn publish(&self, gate: WatchGate) {
        if let Ok(mut g) = self.0.lock() {
            *g = gate;
        }
    }

    /// Check points 1 and 2.
    pub fn allows_buy(&self, head: BlockRef, route: &BuyRoute) -> Result<BuyClearance, Subject> {
        self.current().allows_buy(head, route)
    }

    /// Check point 3, immediately before the first byte: the newest state at
    /// the same head admits the route actually used, which the clearance
    /// covers. An older copy can never decide.
    pub fn confirm_before_egress(&self, head: BlockRef, clearance: &BuyClearance, actual: &BuyRoute) -> Result<(), Subject> {
        if clearance.head != head || !clearance.route.covers(actual) {
            return Err(Subject::Watch);
        }
        self.current().allows_buy(head, actual).map(|_| ())
    }

    pub fn allows_exit(&self, head: BlockRef, venue: VenueId, adapters: &[LiveAdapter]) -> Result<(), Subject> {
        self.current().allows_exit(head, venue, adapters)
    }

    /// For the status projection only, never for a decision.
    pub fn snapshot(&self) -> WatchGate {
        self.current()
    }
}

/// The `p`-th percentile (nearest rank) of `samples`; none below `min`.
pub fn percentile(samples: &[u64], p: u64, min: u64) -> Option<u64> {
    if samples.is_empty() || (samples.len() as u64) < min.max(1) {
        return None;
    }
    let mut s = samples.to_vec();
    s.sort_unstable();
    let rank = (p * s.len() as u64).div_ceil(100).max(1);
    Some(s[(rank - 1) as usize])
}

/// A venue's admin graph as its freeze proves it (two readers): every
/// contract of the dependency graph, the Safes and timelocks that control
/// any of them, and the EOAs with authority over those (ProxyAdmin owner,
/// Safe owners, timelock proposers and executors).
pub struct AdminGraph<'a> {
    pub contracts: &'a [[u8; 20]],
    pub safes: &'a [[u8; 20]],
    pub timelocks: &'a [[u8; 20]],
    pub admin_eoas: &'a [[u8; 20]],
}

/// A verified pending transaction is an admin action of the venue only when
/// its signer is an admin EOA **and** it reaches the graph: directly, through
/// a Safe execution (into the graph or a timelock of it, or any delegatecall)
/// or a timelock schedule/execute (batches conservatively). Anything else of
/// an admin, and everything of an outsider, is none.
pub fn classify_admin_action(signer: [u8; 20], to: Option<[u8; 20]>, data: &[u8], g: &AdminGraph<'_>) -> Option<AdminAction> {
    if !g.admin_eoas.contains(&signer) {
        return None;
    }
    let to = to?;
    let selector: [u8; 4] = data.get(..4).and_then(|s| s.try_into().ok()).unwrap_or([0; 4]);
    if g.contracts.contains(&to) {
        return Some(AdminAction::Direct { to, selector });
    }
    if g.safes.contains(&to) && selector == admin_selectors::SAFE_EXEC_TRANSACTION {
        let inner_to = abi_address(data, 0)?;
        let delegate = abi_word(data, 3)?[31] == 1;
        let into_graph = g.contracts.contains(&inner_to) || g.timelocks.contains(&inner_to);
        return (delegate || into_graph).then_some(AdminAction::Safe { safe: to, inner_to, delegate });
    }
    if g.timelocks.contains(&to) {
        if selector == admin_selectors::TIMELOCK_SCHEDULE || selector == admin_selectors::TIMELOCK_EXECUTE {
            let target = abi_address(data, 0)?;
            return g.contracts.contains(&target).then_some(AdminAction::Timelock { timelock: to, selector });
        }
        if selector == admin_selectors::TIMELOCK_SCHEDULE_BATCH
            || selector == admin_selectors::TIMELOCK_EXECUTE_BATCH
        {
            return Some(AdminAction::Timelock { timelock: to, selector });
        }
    }
    None
}

fn abi_word(data: &[u8], arg: usize) -> Option<&[u8]> {
    data.get(4 + 32 * arg..4 + 32 * (arg + 1))
}

fn abi_address(data: &[u8], arg: usize) -> Option<[u8; 20]> {
    let w = abi_word(data, arg)?;
    if w[..12].iter().any(|b| *b != 0) {
        return None;
    }
    w[12..].try_into().ok()
}

/// EIP-1967 topics and slots; upgrade selectors for display. Every value
/// is checked against keccak in a test.
pub mod eip1967 {
    use alloy_primitives::hex;
    pub const TOPIC_UPGRADED: [u8; 32] =
        hex!("bc7cd75a20ee27fd9adebab32041f755214dbc6bffa90cc0225b39da2e5c2d3b");
    pub const TOPIC_ADMIN_CHANGED: [u8; 32] =
        hex!("7e644d79422f17c01e4894b5f4f588d331ebfa28653d42ae832dc59e38c9798f");
    pub const TOPIC_BEACON_UPGRADED: [u8; 32] =
        hex!("1cf3b03a6cf19fa2baba4df148e9dcabedea7f8a5c07840e207e5c089be95d3e");
    pub const SLOT_IMPLEMENTATION: [u8; 32] =
        hex!("360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc");
    pub const SLOT_ADMIN: [u8; 32] =
        hex!("b53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103");
    pub const SLOT_BEACON: [u8; 32] =
        hex!("a3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50");
    pub const UPGRADE_SELECTORS: [[u8; 4]; 5] = [
        hex!("3659cfe6"), // upgradeTo(address)
        hex!("4f1ef286"), // upgradeToAndCall(address,bytes)
        hex!("8f283970"), // changeAdmin(address)
        hex!("99a88ec4"), // ProxyAdmin.upgrade(address,address)
        hex!("9623609d"), // ProxyAdmin.upgradeAndCall(address,address,bytes)
    ];
}

/// Safe and timelock selectors (checked against keccak in a test).
pub mod admin_selectors {
    use alloy_primitives::hex;
    pub const SAFE_EXEC_TRANSACTION: [u8; 4] = hex!("6a761202");
    pub const TIMELOCK_SCHEDULE: [u8; 4] = hex!("01d5062a");
    pub const TIMELOCK_EXECUTE: [u8; 4] = hex!("134008d3");
    pub const TIMELOCK_SCHEDULE_BATCH: [u8; 4] = hex!("8f2a0bb0");
    pub const TIMELOCK_EXECUTE_BATCH: [u8; 4] = hex!("e38335e5");
}

/// The owner's authority for one exit after a drift: the grant allows the
/// policy and confirms its exit slippage bound. Only the owner's exit gate
/// constructs it (pinned by the CI source scan, like QualifiedFreeze).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitAuthority {
    grant_generation: u64,
    max_exit_slippage_bps: u16,
}

impl ExitAuthority {
    pub(crate) fn granted(grant_generation: u64, max_exit_slippage_bps: u16) -> Option<Self> {
        (max_exit_slippage_bps <= 10_000).then_some(ExitAuthority { grant_generation, max_exit_slippage_bps })
    }
    pub fn grant_generation(&self) -> u64 {
        self.grant_generation
    }
}

/// The signed exit as the executor built it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignedExit {
    pub tx_hash: [u8; 32],
    pub raw_sha256: [u8; 32],
    pub token: [u8; 20],
    pub tokens_sold: u128,
    /// The signed minimum output (calldata).
    pub min_output_wei: u128,
    /// The frozen math's quote of this SELL on the head's curve state.
    pub quote_wei: u128,
}

/// The existing local-EVM simulator's run of the exact signed bytes
/// (signature, nonce, gas included) on the state of `at`, with the
/// wallet's measured balance deltas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitSimulation {
    pub tx_hash: [u8; 32],
    pub raw_sha256: [u8; 32],
    pub at: BlockRef,
    pub success: bool,
    pub token: [u8; 20],
    /// Token balance after minus before (a SELL: negative).
    pub token_delta: i128,
    /// BNB balance after minus before, gas paid.
    pub bnb_delta_wei: i128,
    pub gas_cost_wei: u128,
}

/// EXIT_AFTER_DRIFT (spec §8): the run of the exact signed bytes on the
/// head's state sold exactly the signed tokens and received at least the
/// signed minimum and the quote less the granted exit slippage. A pass is
/// evidence, never proof of the later inclusion.
pub fn exit_after_drift_ok(
    sim: &ExitSimulation,
    exit: &SignedExit,
    head: BlockRef,
    authority: &ExitAuthority,
    exit_selectors_unchanged: bool,
) -> Result<(), &'static str> {
    if !exit_selectors_unchanged {
        return Err("EXIT_AFTER_DRIFT_SELECTORS_CHANGED");
    }
    if sim.tx_hash != exit.tx_hash || sim.raw_sha256 != exit.raw_sha256 {
        return Err("EXIT_SIM_OTHER_TX");
    }
    if sim.at != head {
        return Err("EXIT_SIM_STALE");
    }
    if !sim.success {
        return Err("EXIT_SIM_REVERTED");
    }
    let sold = i128::try_from(exit.tokens_sold).map_err(|_| "EXIT_SIM_RANGE")?;
    if sim.token != exit.token || sim.token_delta != -sold {
        return Err("EXIT_SIM_TOKEN_DELTA");
    }
    let gas = i128::try_from(sim.gas_cost_wei).map_err(|_| "EXIT_SIM_RANGE")?;
    let proceeds = sim.bnb_delta_wei.checked_add(gas).ok_or("EXIT_SIM_RANGE")?;
    let min = i128::try_from(exit.min_output_wei).map_err(|_| "EXIT_SIM_RANGE")?;
    if proceeds < min {
        return Err("EXIT_SIM_BELOW_MIN");
    }
    let keep = u128::from(10_000 - authority.max_exit_slippage_bps);
    let floor = exit.quote_wei.checked_mul(keep).ok_or("EXIT_SIM_RANGE")? / 10_000;
    let floor = i128::try_from(floor).map_err(|_| "EXIT_SIM_RANGE")?;
    if proceeds < floor {
        return Err("EXIT_SIM_SLIPPAGE");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Mem {
        events: Rc<RefCell<Vec<LatchEvent>>>,
        fail: Rc<RefCell<bool>>,
    }

    impl LatchStore for Mem {
        fn load(&self) -> Result<Vec<LatchEvent>, String> {
            Ok(self.events.borrow().clone())
        }
        fn append(&mut self, e: &LatchEvent) -> Result<(), String> {
            if *self.fail.borrow() {
                return Err("disk".into());
            }
            self.events.borrow_mut().push(*e);
            Ok(())
        }
    }

    const CFG: WatchConfig = WatchConfig {
        max_stale_blocks: 0,
        perf_max_stale_blocks: 30,
        recover_evaluations: 3,
        min_samples: 20,
        max_reject_ppm: 50_000,
    };
    const FREEZE: [u8; 32] = [0xf0; 32];
    const OWN_LIMIT: Option<u64> = Some(1_000);
    const FLAP: Subject = Subject::Venue(VenueId::Flap);
    const PN: Subject = Subject::Source(SourceId::PublicNode);
    const DRPC: Subject = Subject::Source(SourceId::Drpc);
    const C48: Subject = Subject::Adapter(LiveAdapter::Club48BuilderV1);
    const DRIFT: Cause = Cause::CodeDrift { address: [1; 20], observed: [2; 32] };

    fn b(n: u64) -> BlockRef {
        BlockRef { number: n, hash: [n as u8; 32] }
    }

    fn watch(store: &Mem) -> Watch {
        Watch::new(CFG, &[PN, DRPC, FLAP, C48], &[(VenueId::Flap, FREEZE)], Box::new(store.clone())).unwrap()
    }

    fn fast() -> ExecWindow {
        ExecWindow { own_ns: vec![100; 20], lost_buys: 0 }
    }

    fn checked(w: &mut Watch, at: BlockRef, exec: &ExecWindow) {
        w.on_head(at);
        w.chain(at, ChainCheck::Agree);
        let freeze_sha256 = w.active_freeze(VenueId::Flap).unwrap();
        w.venue_probe(VenueId::Flap, at, Probe::Match { freeze_sha256 }, [0; 32]);
        w.source(SourceId::PublicNode, at, Health::Healthy, &SourceWindow::default(), None);
        w.adapter(
            LiveAdapter::Club48BuilderV1,
            at,
            &AdapterWindow { connected: true, ..Default::default() },
            None,
        );
        w.execution(at, exec, OWN_LIMIT, 3);
        w.evaluated(at);
    }

    fn all_ok_at(w: &mut Watch, at: BlockRef) {
        checked(w, at, &fast());
    }

    fn all_ok(w: &mut Watch, n: u64) {
        all_ok_at(w, b(n));
    }

    fn route() -> BuyRoute {
        BuyRoute::new(VenueId::Flap, SourceId::PublicNode, &[LiveAdapter::Club48BuilderV1]).unwrap()
    }

    fn cell(w: &Watch) -> GateCell {
        let c = GateCell::default();
        w.publish(&c);
        c
    }

    fn buy(w: &Watch, n: u64) -> Result<(), Subject> {
        cell(w).allows_buy(b(n), &route()).map(|_| ())
    }

    fn exit(w: &Watch, n: u64) -> Result<(), Subject> {
        cell(w).allows_exit(b(n), VenueId::Flap, &[LiveAdapter::Club48BuilderV1])
    }

    #[test]
    fn nothing_probed_admits_nothing() {
        let w = watch(&Mem::default());
        assert_eq!(buy(&w, 1), Err(Subject::Watch));
        assert_eq!(GateCell::default().allows_buy(b(1), &route()).map(|_| ()), Err(Subject::Watch));
    }

    #[test]
    fn a_fully_checked_head_admits() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        assert_eq!(buy(&w, 10), Ok(()));
    }

    #[test]
    fn a_new_head_not_yet_checked_admits_nothing() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        w.on_head(b(11));
        assert_eq!(buy(&w, 11), Err(Subject::Chain));
    }

    #[test]
    fn a_route_needs_an_adapter_and_no_duplicate() {
        let a = LiveAdapter::Club48BuilderV1;
        assert_eq!(BuyRoute::new(VenueId::Flap, SourceId::PublicNode, &[]), None);
        assert_eq!(BuyRoute::new(VenueId::Flap, SourceId::PublicNode, &[a, a]), None);
    }

    #[test]
    fn the_barrier_reads_the_newest_state_and_the_route_used() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        let c = cell(&w);
        let clearance = c.allows_buy(b(10), &route()).unwrap();
        assert_eq!(c.confirm_before_egress(b(10), &clearance, &route()), Ok(()));
        let other = BuyRoute::new(VenueId::Flap, SourceId::Drpc, &[LiveAdapter::Club48BuilderV1]).unwrap();
        assert_eq!(c.confirm_before_egress(b(10), &clearance, &other), Err(Subject::Watch));
        assert_eq!(c.confirm_before_egress(b(11), &clearance, &route()), Err(Subject::Watch));
        w.source(SourceId::PublicNode, b(10), Health::Disconnected, &SourceWindow::default(), None);
        w.publish(&c);
        assert_eq!(c.confirm_before_egress(b(10), &clearance, &route()), Err(PN));
    }

    #[test]
    fn a_drift_without_event_latches_and_survives_a_restart() {
        let store = Mem::default();
        let mut w = watch(&store);
        all_ok(&mut w, 10);
        w.venue_probe(VenueId::Flap, b(10), Probe::Drift(DRIFT), [9; 32]);
        assert_eq!(buy(&w, 10), Err(FLAP));
        let mut w = watch(&store);
        all_ok(&mut w, 11);
        assert_eq!(buy(&w, 11), Err(FLAP));
        assert_eq!(w.gate().state(FLAP), State::Blocked);
    }

    #[test]
    fn clear_needs_cas_the_seen_latch_and_a_qualified_freeze_and_activates_it() {
        let store = Mem::default();
        let mut w = watch(&store);
        all_ok(&mut w, 10);
        w.venue_probe(VenueId::Flap, b(10), Probe::Drift(DRIFT), [9; 32]);
        let g = w.subject_generation(FLAP).unwrap();
        let proof = |subject, n| QualifiedFreeze::verified(subject, [7; 32], b(n));
        assert_eq!(w.clear(FLAP, g + 1, [9; 32], &proof(FLAP, 10)), Err("WATCH_GENERATION_CHANGED"));
        assert_eq!(w.clear(FLAP, g, [1; 32], &proof(FLAP, 10)), Err("WATCH_LATCH_CHANGED"));
        assert_eq!(w.clear(FLAP, g, [9; 32], &proof(PN, 10)), Err("WATCH_FREEZE_OTHER_SUBJECT"));
        assert_eq!(w.clear(FLAP, g, [9; 32], &proof(FLAP, 9)), Err("WATCH_FREEZE_BEFORE_DRIFT"));
        let pn = w.subject_generation(PN).unwrap();
        assert_eq!(w.clear(PN, pn, [9; 32], &proof(PN, 10)), Err("WATCH_NOT_LATCHED"));
        assert_eq!(w.clear(FLAP, g, [9; 32], &proof(FLAP, 10)), Ok(()));
        assert_eq!(w.active_freeze(VenueId::Flap), Some([7; 32]));
        w.on_head(b(11));
        w.venue_probe(VenueId::Flap, b(11), Probe::Match { freeze_sha256: FREEZE }, [0; 32]);
        assert_eq!(w.gate().state(FLAP), State::Unknown, "a probe against the old freeze");
        all_ok(&mut w, 11);
        assert_eq!(buy(&w, 11), Ok(()));
        let w = watch(&store);
        assert_eq!(w.active_freeze(VenueId::Flap), Some([7; 32]), "durable");
    }

    #[test]
    fn an_admin_action_is_suspect_until_the_nonce_and_a_probe_decide() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        let action = AdminAction::Direct { to: [0xc0; 20], selector: eip1967::UPGRADE_SELECTORS[1] };
        w.pending_admin_action(VenueId::Flap, [5; 32], [0xad; 20], action);
        assert_eq!(buy(&w, 10), Err(FLAP));
        all_ok(&mut w, 11);
        assert_eq!(buy(&w, 11), Err(FLAP), "undecided");
        w.pending_decided([5; 32], b(12));
        all_ok(&mut w, 11);
        assert_eq!(w.gate().state(FLAP), State::Suspect, "probe before the decision");
        all_ok(&mut w, 12);
        assert_eq!(buy(&w, 12), Ok(()));
    }

    fn word(addr: [u8; 20]) -> [u8; 32] {
        let mut w = [0u8; 32];
        w[12..].copy_from_slice(&addr);
        w
    }

    fn call(selector: [u8; 4], words: &[[u8; 32]]) -> Vec<u8> {
        let mut d = selector.to_vec();
        for w in words {
            d.extend_from_slice(w);
        }
        d
    }

    #[test]
    fn admin_actions_need_an_admin_signer_and_a_graph_target() {
        let (admin, outsider) = ([0xad; 20], [0x01; 20]);
        let (contract, safe, timelock, elsewhere) = ([0xc0; 20], [0x5a; 20], [0x71; 20], [0xee; 20]);
        let g = AdminGraph { contracts: &[contract], safes: &[safe], timelocks: &[timelock], admin_eoas: &[admin] };
        let setter = call([1, 2, 3, 4], &[[0; 32]]);
        assert!(matches!(classify_admin_action(admin, Some(contract), &setter, &g), Some(AdminAction::Direct { .. })));
        assert_eq!(classify_admin_action(outsider, Some(contract), &setter, &g), None);
        assert_eq!(classify_admin_action(admin, Some(elsewhere), &setter, &g), None);
        let exec = |to: [u8; 20], op: u8| {
            let mut operation = [0u8; 32];
            operation[31] = op;
            call(admin_selectors::SAFE_EXEC_TRANSACTION, &[word(to), [0; 32], [0; 32], operation])
        };
        assert!(matches!(
            classify_admin_action(admin, Some(safe), &exec(contract, 0), &g),
            Some(AdminAction::Safe { delegate: false, .. })
        ));
        assert_eq!(classify_admin_action(admin, Some(safe), &exec(elsewhere, 0), &g), None);
        assert!(matches!(
            classify_admin_action(admin, Some(safe), &exec(elsewhere, 1), &g),
            Some(AdminAction::Safe { delegate: true, .. })
        ));
        let schedule = |target: [u8; 20]| call(admin_selectors::TIMELOCK_SCHEDULE, &[word(target)]);
        assert!(matches!(
            classify_admin_action(admin, Some(timelock), &schedule(contract), &g),
            Some(AdminAction::Timelock { .. })
        ));
        assert_eq!(classify_admin_action(admin, Some(timelock), &schedule(elsewhere), &g), None);
        let batch = call(admin_selectors::TIMELOCK_EXECUTE_BATCH, &[[0; 32]]);
        assert!(matches!(classify_admin_action(admin, Some(timelock), &batch, &g), Some(AdminAction::Timelock { .. })));
    }

    #[test]
    fn an_upgrade_event_then_a_drift_latches_and_without_drift_returns() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        w.upgrade_event(VenueId::Flap, b(11), [3; 20], eip1967::TOPIC_UPGRADED);
        all_ok(&mut w, 11);
        assert_eq!(buy(&w, 11), Ok(()), "same code after the event");
        w.upgrade_event(VenueId::Flap, b(12), [3; 20], eip1967::TOPIC_UPGRADED);
        w.on_head(b(12));
        assert_eq!(buy(&w, 12), Err(Subject::Chain));
        let drift = Cause::StorageDrift { address: [3; 20], slot: eip1967::SLOT_IMPLEMENTATION, observed: [4; 32] };
        w.venue_probe(VenueId::Flap, b(12), Probe::Drift(drift), [8; 32]);
        all_ok(&mut w, 12);
        assert_eq!(buy(&w, 12), Err(FLAP));
    }

    #[test]
    fn a_timelock_announcement_is_an_event_not_a_state() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        let _ = w.take_events();
        w.announced(VenueId::Flap, b(10), [0x71; 20], 1_800_000_000);
        let events = w.take_events();
        assert_eq!(events.len(), 1);
        assert_eq!((events[0].from, events[0].to), (State::Ok, State::Ok));
        assert_eq!(buy(&w, 10), Ok(()));
    }

    #[test]
    fn every_visible_change_is_one_event() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        let _ = w.take_events();
        let g = w.generation();
        all_ok(&mut w, 11);
        assert!(w.take_events().is_empty(), "only the block changed");
        assert_eq!(w.generation(), g);
        w.venue_probe(VenueId::Flap, b(11), Probe::Drift(DRIFT), [9; 32]);
        let events = w.take_events();
        assert_eq!(events.len(), 1);
        let e = events[0];
        assert_eq!((e.subject, e.from, e.to, e.latched), (FLAP, State::Ok, State::Blocked, true));
        assert_eq!(w.generation(), g + 1);
    }

    #[test]
    fn a_reorg_voids_the_evidence_of_its_blocks() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        w.reorg(10);
        assert_eq!(buy(&w, 10), Err(Subject::Watch));
        let new = BlockRef { number: 10, hash: [0xaa; 32] };
        all_ok_at(&mut w, new);
        assert_eq!(cell(&w).allows_buy(new, &route()).map(|_| ()), Ok(()));
    }

    #[test]
    fn a_reorg_launders_no_finding() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        w.source(SourceId::PublicNode, b(10), Health::Disconnected, &SourceWindow::default(), None);
        w.reorg(10);
        let new = BlockRef { number: 10, hash: [0xaa; 32] };
        all_ok_at(&mut w, new);
        assert_eq!(cell(&w).allows_buy(new, &route()).map(|_| ()), Err(PN));
    }

    #[test]
    fn an_unwritable_latch_closes_the_watch_for_good() {
        let store = Mem::default();
        let mut w = watch(&store);
        all_ok(&mut w, 10);
        *store.fail.borrow_mut() = true;
        w.venue_probe(VenueId::Flap, b(10), Probe::Drift(DRIFT), [9; 32]);
        *store.fail.borrow_mut() = false;
        all_ok(&mut w, 11);
        assert_eq!(buy(&w, 11), Err(Subject::Watch));
    }

    #[test]
    fn disagreeing_readers_are_unknown() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        w.chain(b(10), ChainCheck::Disagree);
        assert_eq!(buy(&w, 10), Err(Subject::Chain));
        w.venue_probe(VenueId::Flap, b(10), Probe::Disagree, [0; 32]);
        w.chain(b(10), ChainCheck::Agree);
        assert_eq!(w.gate().state(FLAP), State::Unknown);
    }

    #[test]
    fn performance_recovers_only_after_stable_evaluations() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        w.source(SourceId::PublicNode, b(11), Health::Disconnected, &SourceWindow::default(), None);
        for n in 11..13 {
            all_ok(&mut w, n);
            assert_eq!(buy(&w, n), Err(PN), "block {n}");
        }
        all_ok(&mut w, 13);
        assert_eq!(buy(&w, 13), Ok(()));
    }

    #[test]
    fn rejects_are_judged_on_a_minimum_sample() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        let few = SourceWindow { checked: 10, rejected: 10, ..Default::default() };
        w.source(SourceId::PublicNode, b(10), Health::Healthy, &few, None);
        assert_eq!(buy(&w, 10), Ok(()));
        let many = SourceWindow { checked: 100, rejected: 10, ..Default::default() };
        w.source(SourceId::PublicNode, b(10), Health::Healthy, &many, None);
        assert_eq!(buy(&w, 10), Err(PN));
    }

    #[test]
    fn a_source_lag_above_its_baseline_degrades_it() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        let mut lag = vec![1_000u64; 99];
        lag.push(50_000);
        let ok = SourceWindow { checked: 100, lag_ns: lag, ..Default::default() };
        w.source(SourceId::PublicNode, b(10), Health::Healthy, &ok, Some(2_000));
        assert_eq!(buy(&w, 10), Ok(()));
        let mut lag = vec![1_000u64; 95];
        lag.extend([50_000; 5]);
        let slow = SourceWindow { checked: 100, lag_ns: lag, ..Default::default() };
        w.source(SourceId::PublicNode, b(10), Health::Healthy, &slow, Some(2_000));
        assert_eq!(buy(&w, 10), Err(PN));
    }

    #[test]
    fn an_unused_degraded_source_blocks_no_buy() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        w.source(SourceId::Drpc, b(10), Health::Disconnected, &SourceWindow::default(), None);
        assert_eq!(buy(&w, 10), Ok(()));
        let via_drpc = BuyRoute::new(VenueId::Flap, SourceId::Drpc, &[LiveAdapter::Club48BuilderV1]).unwrap();
        assert_eq!(cell(&w).allows_buy(b(10), &via_drpc).map(|_| ()), Err(DRPC));
    }

    #[test]
    fn an_uninterpretable_adapter_response_blocks_it_until_clean_again() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        let odd = AdapterWindow { connected: true, requests: 1, uninterpretable: 1, ..Default::default() };
        w.adapter(LiveAdapter::Club48BuilderV1, b(10), &odd, None);
        assert_eq!(buy(&w, 10), Err(C48));
        for n in 11..14 {
            all_ok(&mut w, n);
        }
        assert_eq!(buy(&w, 13), Ok(()));
    }

    #[test]
    fn execution_needs_a_measurement_and_a_baseline() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        let n = 10 + CFG.perf_max_stale_blocks + 1;
        checked(&mut w, b(n), &ExecWindow::default());
        assert_eq!(buy(&w, n), Err(Subject::Execution), "no measurement for too long");
        w.execution(b(n), &fast(), None, 3);
        assert_eq!(w.gate().state(Subject::Execution), State::Unqualified, "no baseline");
    }

    #[test]
    fn a_loss_streak_pauses_buys_not_exits_and_cools_down() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        w.execution(b(10), &ExecWindow { own_ns: vec![100; 20], lost_buys: 3 }, OWN_LIMIT, 3);
        assert_eq!(buy(&w, 10), Err(Subject::Execution));
        assert_eq!(exit(&w, 10), Ok(()));
        for n in 11..14 {
            all_ok(&mut w, n);
        }
        assert_eq!(buy(&w, 13), Ok(()));
    }

    #[test]
    fn exits_survive_source_findings_not_a_drift() {
        let mut w = watch(&Mem::default());
        all_ok(&mut w, 10);
        w.source(SourceId::PublicNode, b(10), Health::Disconnected, &SourceWindow::default(), None);
        assert_eq!(exit(&w, 10), Ok(()));
        w.venue_probe(VenueId::Flap, b(10), Probe::Drift(DRIFT), [9; 32]);
        assert_eq!(exit(&w, 10), Err(FLAP));
        assert_eq!(exit(&w, 11), Err(Subject::Watch));
    }

    fn signed_exit() -> SignedExit {
        SignedExit {
            tx_hash: [3; 32],
            raw_sha256: [4; 32],
            token: [5; 20],
            tokens_sold: 1_000_000,
            min_output_wei: 900,
            quote_wei: 1_000,
        }
    }

    fn run(at: BlockRef) -> ExitSimulation {
        ExitSimulation {
            tx_hash: [3; 32],
            raw_sha256: [4; 32],
            at,
            success: true,
            token: [5; 20],
            token_delta: -1_000_000,
            bnb_delta_wei: 920,
            gas_cost_wei: 30,
        }
    }

    #[test]
    fn an_exit_after_drift_needs_the_signed_bytes_their_deltas_and_the_bounds() {
        let wide = ExitAuthority::granted(7, 1_000).unwrap();
        let check = |s: &ExitSimulation, a: &ExitAuthority, unchanged| {
            exit_after_drift_ok(s, &signed_exit(), b(10), a, unchanged)
        };
        assert_eq!(check(&run(b(10)), &wide, true), Ok(()));
        assert_eq!(check(&run(b(10)), &wide, false), Err("EXIT_AFTER_DRIFT_SELECTORS_CHANGED"));
        assert_eq!(check(&ExitSimulation { tx_hash: [9; 32], ..run(b(10)) }, &wide, true), Err("EXIT_SIM_OTHER_TX"));
        assert_eq!(check(&ExitSimulation { raw_sha256: [9; 32], ..run(b(10)) }, &wide, true), Err("EXIT_SIM_OTHER_TX"));
        assert_eq!(check(&run(b(9)), &wide, true), Err("EXIT_SIM_STALE"));
        assert_eq!(check(&ExitSimulation { success: false, ..run(b(10)) }, &wide, true), Err("EXIT_SIM_REVERTED"));
        assert_eq!(check(&ExitSimulation { token_delta: -999_999, ..run(b(10)) }, &wide, true), Err("EXIT_SIM_TOKEN_DELTA"));
        assert_eq!(check(&ExitSimulation { token: [6; 20], ..run(b(10)) }, &wide, true), Err("EXIT_SIM_TOKEN_DELTA"));
        assert_eq!(check(&ExitSimulation { bnb_delta_wei: 820, ..run(b(10)) }, &wide, true), Err("EXIT_SIM_BELOW_MIN"));
        let tight = ExitAuthority::granted(7, 200).unwrap();
        assert_eq!(check(&run(b(10)), &tight, true), Err("EXIT_SIM_SLIPPAGE"));
        assert_eq!(ExitAuthority::granted(7, 10_001), None);
    }

    #[test]
    fn the_constants_are_their_keccak() {
        use alloy_primitives::{U256, keccak256};
        assert_eq!(keccak256("Upgraded(address)").0, eip1967::TOPIC_UPGRADED);
        assert_eq!(keccak256("AdminChanged(address,address)").0, eip1967::TOPIC_ADMIN_CHANGED);
        assert_eq!(keccak256("BeaconUpgraded(address)").0, eip1967::TOPIC_BEACON_UPGRADED);
        let slot = |label: &str| (U256::from_be_bytes(keccak256(label).0) - U256::from(1u8)).to_be_bytes::<32>();
        assert_eq!(slot("eip1967.proxy.implementation"), eip1967::SLOT_IMPLEMENTATION);
        assert_eq!(slot("eip1967.proxy.admin"), eip1967::SLOT_ADMIN);
        assert_eq!(slot("eip1967.proxy.beacon"), eip1967::SLOT_BEACON);
        let selectors = [
            ("upgradeTo(address)", eip1967::UPGRADE_SELECTORS[0]),
            ("upgradeToAndCall(address,bytes)", eip1967::UPGRADE_SELECTORS[1]),
            ("changeAdmin(address)", eip1967::UPGRADE_SELECTORS[2]),
            ("upgrade(address,address)", eip1967::UPGRADE_SELECTORS[3]),
            ("upgradeAndCall(address,address,bytes)", eip1967::UPGRADE_SELECTORS[4]),
            (
                "execTransaction(address,uint256,bytes,uint8,uint256,uint256,uint256,address,address,bytes)",
                admin_selectors::SAFE_EXEC_TRANSACTION,
            ),
            ("schedule(address,uint256,bytes,bytes32,bytes32,uint256)", admin_selectors::TIMELOCK_SCHEDULE),
            ("execute(address,uint256,bytes,bytes32,bytes32)", admin_selectors::TIMELOCK_EXECUTE),
            (
                "scheduleBatch(address[],uint256[],bytes[],bytes32,bytes32,uint256)",
                admin_selectors::TIMELOCK_SCHEDULE_BATCH,
            ),
            ("executeBatch(address[],uint256[],bytes[],bytes32,bytes32)", admin_selectors::TIMELOCK_EXECUTE_BATCH),
        ];
        for (sig, sel) in selectors {
            assert_eq!(&keccak256(sig)[..4], &sel[..], "{sig}");
        }
    }
}
```

**Annahmen im Code:**
- `SourceId`, `VenueId` und `LiveAdapter` sind `Copy + Eq + Debug`. Falls nicht, Derives ergänzen.
- `QualifiedFreeze::verified` wird nur im Freeze-Verifier aufgerufen, `ExitAuthority::granted` nur im Exit-Gate des Owners. Ein CI-Quelltext-Scan pinnt beide Aufrufstellen; Ausnahme sind die Tests in `watch.rs`.

## 6. Integration (native)

1. **`lib.rs`:** `pub mod watch;`
2. **Freeze-Abhängigkeitsgraph (W0):** versionierter Record je Venue, mit zwei Lesern geprüft, nie im Code. Er bindet:
   - alle Contracts (Proxys, ProxyAdmins, Implementierungen, Clone-Implementierungen),
   - Safes, Timelocks und Admin-EOAs (`AdminGraph`),
   - die Slots: Implementierung, Admin, Beacon und die wirtschaftlichen Parameter,
   - Code-Hashes, Selektoren und die Exit-Selektoren.

   Sein SHA-256 ist der aktive Freeze.
3. **`runtime.rs`, Reconcile:**
   - `Watch::new(config, subjects, freezes, store)` mit dem `LatchStore` im Owner-Zustand (append-only, Temp + fsync + Hardlink wie das Ledger).
   - Fakten anwenden, danach `evaluated(head)`, dann `watch.publish(&gate_cell)` nach jedem Fakten-Durchlauf.
   - `take_events()` ins Ereignisjournal (§7.1), `verdicts()` in den Status-Record.
4. **Probe-Worker (außerhalb des Owner-Threads, bestehender Transport):**
   - Auf jeden neuen Head gebündelt `eth_getStorageAt` am Blockhash für alle gebundenen Slots mit zwei Lesern parallel; Ergebnis `Probe::Match { freeze_sha256 }` mit dem tatsächlich verwendeten Freeze.
   - Code-Hashes alle `BSC_WATCH_SNAPSHOT_BLOCKS` Blöcke.
   - Nonces der Admin-EOAs, solange ein Verdacht offen ist.
   - Exit-Simulationen über den bestehenden lokalen EVM-Simulator am Blockhash.
5. **`chain_follower`:** Upgrade-Topics und Timelock-Ereignisse der gebundenen Contracts in den Log-Filter aufnehmen. `CallScheduled` → `announced()`, Reorg → `reorg()`.
6. **`ingress.rs`:**
   - `classify_admin_action(signer, to, data, graph)` → `pending_admin_action()`.
   - Nach dem 063-Fix gelten Transaktionen von Admin-EOAs als relevant für die Signer-Wiederherstellung.
   - V5-Zähler im bestehenden `classify`-Pfad.
7. **`runtime_live.rs`, Prüfstelle 1:** `BuyRoute::new(venue, Quelle, OK-Adapter ∩ Grant-Adapter)` → `gate_cell.allows_buy()`. Ohne Route: `Refusal::Watch`. Die `BuyClearance` wird bis zur Barriere mitgegeben.
8. **`sources.rs`, Prüfstelle 2:**
   - `admit_live` prüft mit `gate_cell.allows_buy()` am neuesten Stand.
   - Neue `LIVE_CONDITIONS`-Zeile „WATCH“.
   - Die Deployment-Auswahl je Venue muss `watch.active_freeze(venue)` entsprechen, sonst ist das Venue nicht handelbar.
9. **`live_end.rs`, Prüfstelle 3:**
   - Unmittelbar vor dem ersten Byte `gate_cell.confirm_before_egress(head, &clearance, &Route der tatsächlich beschriebenen Adapter)` plus STALE_HEAD.
   - Schlägt sie fehl, greift der bestehende WITHDRAWN-Pfad.
10. **Exit-Gate (Owner):** `ExitAuthority::granted(…)` nur bei Grant-Flag `exit_after_drift` und bestätigter Exit-Slippage-Grenze. Vor dem Dispatch eines Exits auf einem gedrifteten Venue prüft `runtime_live` mit `exit_after_drift_ok()`.
11. **Operator-Kette:** `bsc_runtime watch clear` bzw. Gateway `bsc.watch.clear` (§7.3) → Freeze-Verifier (§9) → `QualifiedFreeze` → `clear()`.

## 7. Frontend und Meldungen (Ende-zu-Ende)

**Grundsatz:** Jeder angezeigte Wert stammt aus dem nativen Zustand. Das Frontend leitet nichts selbst ab und gibt nichts frei. „Quittieren“ ist keine Freigabe.

### 7.1 Native: Status und Ereignisjournal

- **Status-Record `bsc_runtime::watch`:**
  - je Subjekt: id, state, cause_code, cause_text, class, block_number, block_hash, checked_unix_ms, evidence_sha256, latched, subject_generation, active_freeze_sha256 (Venues);
  - Gate: generation, head, fresh;
  - Zähler: events_total, events_dropped.
- **Ereignisjournal `<owner-state>/watch-events/`:**
  - append-only, ein Record je `WatchEvent`, mit seq (lückenlos), unix_ms, severity und evidence_sha256;
  - Aufbewahrung `BSC_WATCH_EVENT_RETENTION_DAYS`; übersteht Neustarts.
- **Schweregrad:**
  - **CRITICAL:** Wechsel nach `Blocked` oder `Unknown` von `Chain`, `Watch` oder einem Venue, sowie jeder Wechsel eines Subjekts, das eine scharfe LIVE-Route braucht, weg von `Ok`.
  - **WARNING:** `Degraded` oder `Suspect`.
  - **INFO:** Erholung, Timelock-Ankündigung, Freigabe (Cleared).

### 7.2 Protobuf (additiv, `darkbot/bsc/v1/status.proto`)

- **`BscWatchState`:**
  - `repeated WatchSubject subjects`,
  - `WatchGateView gate` (generation, head_number, head_hash, buys_allowed, buy_block_reason, exits_allowed),
  - `repeated WatchEvent recent_events` (die letzten N),
  - `repeated WatchAlert open_alerts`,
  - `uint64 events_total`, `uint64 events_dropped`.
- **`WatchSubject`:** id, kind, state, cause_code, cause_text, class, block_number, block_hash, checked_unix_ms, evidence_sha256, latched, subject_generation, active_freeze_sha256, requalification_report (Pfad + SHA, falls vorhanden).
- **`WatchEvent`:** seq, unix_ms, subject_id, from_state, to_state, cause_code, cause_text, severity, block_number, evidence_sha256, latched, subject_generation.
- **`WatchAlert`:** event_seq, severity, acknowledged, ack_unix_ms, ack_request_id.
- **Kommandos:**
  - `BscWatchClearCommand` (subject_id, expected_subject_generation, latch_evidence_sha256, freeze_record_path, freeze_sha256),
  - `BscWatchAckCommand` (event_seq).

### 7.3 Reporter und Gateway

- Der Reporter bildet den nativen Status und das Journal auf `BscWatchState` ab. Dazu gehören ein Golden-Test und F0-VAR-Einträge für jedes Feld.
- Gateway-Topic `bsc.watch.state` wird per Push bei jeder Änderung gesendet, ohne Polling.
- **`bsc.watch.clear`:**
  - Session/CSRF, dauerhafte Quittung, CAS auf `expected_subject_generation`.
  - Der native Owner verifiziert den Freeze (§9). Erst danach folgt `clear()`.
  - Jede Ablehnung erscheint mit ihrem Code im Frontend.
- **`bsc.watch.ack`:** markiert einen Alarm als gesehen; der Zustand bleibt unverändert.
- **macOS-Meldung (osascript) bei jedem neuen CRITICAL- oder WARNING-Ereignis:**
  - höchstens eine je Subjekt pro `BSC_WATCH_NOTIFY_MIN_INTERVAL_MS`;
  - Text: Subjekt, Zustand, deutscher Ursachentext, Block.

### 7.4 Frontend

- **Globales Banner in der App-Hülle, auf allen Reitern, auch bei Project_0:**
  - rot bei offenen CRITICAL-Alarmen, gelb bei WARNING;
  - zeigt Subjekt, Ursache (deutscher Text und Code), Block, Uhrzeit, einen Link zum Watch-Abschnitt und „Quittieren“.
  - Eine eigene rote Zeile „LIVE-Käufe gesperrt: <Grund>“ lässt sich nicht wegklicken, solange `buys_allowed = false` bei aktivem Grant.
- **BSC → Monitoring → Abschnitt „Watch“:**
  - Subjekttabelle mit farbigen Zustandsplaketten, Ursache, Block, Prüfzeit, Evidenz-Hash, Sperre ja/nein, aktivem Freeze;
  - Ereignis-Zeitleiste, filterbar nach Schweregrad und Subjekt; seq-Lücken zeigen „Ereignisse fehlen“;
  - Sperrkarte mit Requalifikationsbericht und Freigabe-Dialog.
- **Freigabe-Dialog:**
  - zeigt die Sperre (Ursache, Block, Evidenz-Hash) und den gewählten Freeze-Record (SHA, Block, Prüfergebnis des Verifiers);
  - verlangt eine getippte Bestätigung und sendet `bsc.watch.clear` mit der angezeigten Subjekt-Generation.
- **LIVE-Reiter:** Gate-Zusammenfassung (BUYs erlaubt oder gesperrt mit Grund, Exits erlaubt oder gesperrt) und EXIT_AFTER_DRIFT-Status je offener Position.
- **Browser-Meldung (Notification API),** wenn der Tab im Hintergrund ist. Ein Ton ist optional über eine Einstellung.
- **Texte:** Jeder `cause_code` hat einen deutschen Klartext. Ein fehlender Text lässt den Test FT-08 scheitern.

### 7.5 Anforderungen

- Erkennung → Banner ≤ 1 s, gemessen im Browser-E2E.
- Kein Ereignisverlust ohne Anzeige: seq lückenlos, `events_dropped` sichtbar.
- Nach einem Neustart sind Verlauf, offene Alarme und Sperren unverändert sichtbar.

## 8. EXIT_AFTER_DRIFT

Nach bestätigter Venue-Drift ist ein Exit nur erlaubt, wenn **alle** Bedingungen gelten:

1. Die `ExitAuthority` des Owners liegt vor: Grant-Flag `exit_after_drift` (Teil des Scope-Hashes) und eine bestätigte Exit-Slippage-Grenze.
2. SELL- und Approve-Selektor sowie das Trade-Event sind im neuen Code unverändert (laut Requalifikationsbericht).
3. Der bestehende lokale EVM-Simulator hat die **exakt signierten Bytes** am Blockhash des aktuellen Heads ausgeführt, mit Signatur, Nonce und Gas.
4. `exit_after_drift_ok()` passt:
   - gleicher Hash und gleicher SHA-256 der Rohbytes;
   - Token-Delta = −verkaufte Menge, gleiches Token;
   - Erlös (BNB-Delta + Gas) ≥ signierte Mindestmenge **und** ≥ Quote × (1 − Exit-Slippage).

Fehlt eine Bedingung: halten, Alarm, nichts senden. Die Simulation ist ein Befund, kein Inklusionsbeweis. Weicht das echte Ergebnis ab: Alarm, und EXIT_AFTER_DRIFT ist für dieses Venue gesperrt, bis du es neu freigibst.

## 9. Requalifikation (nie automatisch)

1. Bericht (read-only): neue Implementierung, Code-Hash, Selektor-Diff, Parameter-Slots, Admin-Kette, Quote-Gegencheck in der lokalen EVM (Quote = EVM-Ergebnis).
2. Ein neuer Freeze-Record entsteht aus dem Bericht.
3. Der Freeze-Verifier prüft den SHA-256 des Records, beide Leser übereinstimmend am Freeze-Block, den EVM-Gegencheck und Freeze-Block ≥ Drift-Block. Erst dann erzeugt er `QualifiedFreeze`.
4. Du bestätigst im Dialog genau die gesehene Sperre: Subjekt-Generation, Evidenz-Hash, Freeze.
5. Mit dem dauerhaften Cleared-Eintrag ist der neue Freeze aktiv. Das Venue bleibt `Unknown`, bis eine Probe gegen den neuen Freeze passt. Die Deployment-Auswahl des Executors folgt dem aktiven Freeze.

## 10. Leistung: Sendepfad mit und ohne WATCH (BSC3-076)

- **Harness:** Ein aufgezeichneter Launch-Korpus aus der G0.4-Kampagne (≥ 10 000 Launches) läuft durch den echten Pfad: Ingress → C2 → Executor → Owner-Zulassung → Release-Barriere → Loopback-Adapter. Einmal mit WATCH, einmal ohne. Das Abschalten gibt es nur im Benchmark-Build, nie produktiv.
- **Messgrößen:**
  - T_ingress→T_send p50/p99/max,
  - Kosten der drei GateCell-Prüfungen einzeln,
  - zusätzliche Allokationen im Hot Path (Soll: keine),
  - Lock-Konflikte der `GateCell`,
  - Head→Verdikt p50/p99,
  - Anzahl und Dauer der BUY-Ablehnungen wegen noch nicht geprüftem Head,
  - dadurch entgangene Launches.
- **Abnahme:** Die Grenze legst du nach der ersten Messung fest. Vorschlag: p99-Zuwachs ≤ max(1 %, 2 µs).
- **Frische:** Start mit `BSC_WATCH_MAX_STALE_BLOCKS=0`. Gelockert wird nur auf Basis der Messung, als deine Entscheidung, nie stillschweigend.

## 11. Konfiguration (`ingress.public.env`, typisiert, mit Bereich)

| Schlüssel | Bedeutung |
|---|---|
| `BSC_WATCH_MAX_STALE_BLOCKS` | Frische Chain/Watch/Venue (Start 0) |
| `BSC_WATCH_PERF_MAX_STALE_BLOCKS` | Frische Quellen/Adapter/Execution |
| `BSC_WATCH_RECOVER_EVALUATIONS` | saubere Auswertungen bis zur Erholung |
| `BSC_WATCH_MIN_SAMPLES` | Mindeststichprobe |
| `BSC_WATCH_MAX_REJECT_PPM` | Ablehnungs- und Fehlerquote |
| `BSC_WATCH_SOURCE_EVAL_MS` | Takt der Fensterauswertung |
| `BSC_WATCH_SNAPSHOT_BLOCKS` | Takt des vollen Snapshots |
| `BSC_WATCH_LAG_FACTOR_MAX` | Faktor auf gemessene Basislinien |
| `BSC_WATCH_HEAD_LAG_BLOCKS` | Kopf-Rückstand einer Quelle |
| `BSC_WATCH_LOST_BUYS_MAX` | verlorene BUYs je Fenster |
| `BSC_WATCH_UNKNOWN_SELECTOR_MAX` | Anomalie-Schwelle (V5) |
| `BSC_WATCH_TERMS_DRIFT_MAX` | systematischer Parameterdrift (V6) |
| `BSC_WATCH_EVENT_RETENTION_DAYS` | Aufbewahrung des Ereignisjournals |
| `BSC_WATCH_NOTIFY_MIN_INTERVAL_MS` | Mindestabstand der Systemmeldungen je Subjekt |

Basislinien kommen nur aus gemessenen Qualifikationsbelegen, nie aus dem Code.

## 12. Tests

**Unit:** alle Tests in §5.

**Integration (rot → grün, nichtsendend, mit den bestehenden Rigs):**

| ID | Fall | Erwartung |
|---|---|---|
| WT-01 | nichts geprüft | keine BUYs |
| WT-02 | Slot- oder Code-Drift ohne Event | Latch, übersteht Neustart |
| WT-03 | Upgrade-Event | Suspect, Probe entscheidet |
| WT-04 | Admin-Aktion in den Graph (direkt, Safe, Timelock) | Suspect bis Nonce verbraucht und Probe passt |
| WT-05 | sachfremde Admin-Transaktion oder Fremder | nichts |
| WT-06 | Timelock `CallScheduled` | Ereignis, keine Sperre |
| WT-07 | Parameter-Slot geändert | Latch |
| WT-08 | Reorg | Nachweise verfallen, Befunde bleiben |
| WT-09 | Leser widersprechen sich | Unknown |
| WT-10 | Probe-Worker fällt aus oder Head ohne Prüfung | keine BUYs |
| WT-11 | Route ohne Adapter; Barriere mit fremder Route oder altem Stand | abgelehnt |
| WT-12 | Quelle getrennt, still, Müll oder langsam | nur diese Quelle gesperrt |
| WT-13 | Adapter mit unbekanntem Format, Fehlern oder hoher RTT | Adapter gesperrt, anderer weiter |
| WT-14 | eigener Pfad langsam, ohne Messung, ohne Basislinie, Verlustserie | BUYs gesperrt, Exits weiter |
| WT-15 | Sperre nicht speicherbar | Watch Unknown bis Neustart |
| WT-16 | Clear mit falscher Generation, Sperre, Subjekt, Freeze vor Drift oder ohne Verifier | abgelehnt |
| WT-17 | Probe gegen alten Freeze nach Clear | Unknown; Deployment folgt aktivem Freeze |
| WT-18 | Zustandswechsel zwischen Zulassung und erstem Byte | WITHDRAWN |
| WT-19 | offene Position bei Drift | ohne Authority halten; mit Authority nur bei exakter Simulation, Deltas und Slippage |
| WT-20 | Not-Aus, Grant-Ablauf, Neustart | unverändert wirksam |

**Frontend:**

| ID | Fall | Erwartung |
|---|---|---|
| FT-01 | Protobuf-Golden, F0-VAR | alle Felder inventarisiert |
| FT-02 | Gateway-Push | jede Änderung ohne Polling |
| FT-03 | `bsc.watch.clear` (CAS ok/falsch, falsche Sperre, ohne Verifier), `bsc.watch.ack` | korrekte Codes, dauerhafte Quittung |
| FT-04 | simulierte Flap-Drift | rotes Banner auf allen Reitern ≤ 1 s |
| FT-05 | Quittieren | Alarm aus, Zustand und LIVE-Sperrzeile bleiben |
| FT-06 | Neustart | Verlauf und Alarme vollständig, seq-Lücke sichtbar |
| FT-07 | macOS- und Browser-Meldung | einmal je Subjekt und Intervall (Meldungs-Stub nur im Test) |
| FT-08 | Texte | jeder `cause_code` hat einen deutschen Klartext |

**Mutationsnachweise.** Jede Mutation muss mindestens einen Test rot machen:
- Latch wird nicht gespeichert.
- Barriere nutzt eine alte Kopie.
- Route ohne Adapter wird zugelassen.
- Reorg löscht Befunde.
- Clear ohne CAS oder Proof.
- Probe gegen alten Freeze zählt.
- Sachfremde Admin-Transaktion löst Verdacht aus.
- Exit ohne Deltas oder Slippage-Prüfung.
- Execution ohne Messung gilt als OK.
- Nicht speicherbare Watch erholt sich.
- Ereignis wird nicht erzeugt.

**Leistung:** §10, berichtet mit Commit-SHA.

## 13. Arbeitspakete

| Paket | Inhalt | Abnahme |
|---|---|---|
| W0 | Freeze-Abhängigkeitsgraph je Venue, Basislinien aus Belegen | Record versioniert, zwei Leser |
| W1 | `watch.rs` + Unit-Tests; `GateCell`/`BuyRoute` an drei Prüfstellen; Status → Protobuf → Gateway → Banner Ende-zu-Ende mit S1 | WT-01, 10, 11, 18, 20; FT-01, 02, 04 |
| W2 | Probe-Worker, V1–V4, Admin-Klassifikation, Reorg, LatchStore, Ereignisjournal, RPC-Last | WT-02 bis 09, 15 |
| W3 | Quellen S2–S4, Adapter A1–A3, Execution E1–E2, V5/V6 | WT-12 bis 14 |
| W4 | Requalifikation: Bericht, Freeze-Verifier, aktiver Freeze, `bsc.watch.clear`, Dialog | WT-16, 17; FT-03 |
| W5 | EXIT_AFTER_DRIFT: lokale EVM, Deltas, ExitAuthority | WT-19 |
| W6 | Frontend komplett: Zeitleiste, Quittieren, Meldungen, Texte | FT-05 bis 08 |
| W7 | Leistung mit/ohne WATCH, Mutationen, Restrisiko-Doku, Gesamtabnahme | §10, alle Mutationen rot |

Je Paket: Rot → Grün, unabhängiges Review, CI auf dem finalen SHA, Belege in STATUS.md.

## 14. Abdeckung und Restrisiken

| Risiko | Erkannt durch | Reaktion | Restrisiko |
|---|---|---|---|
| Flap- oder Four-Upgrade | Event, Slots je Block, Code-Hashes | BUYs gesperrt, Alarm, Requalifikation | BUY, der im Upgrade-Block schon unterwegs ist |
| Vorbereitete Admin-Aktion | Pending-Tx eines Admins in den Graph, Timelock | BUYs gesperrt bis zur Entscheidung, Vorwarnung | private Admin-Tx: erst im Block erkannt |
| Parameteränderung | Parameter-Slots, TERMS_DRIFT | wie Upgrade | nur gebundene Slots |
| Neue Launch-Funktion | unbekannte Selektoren | Alarm, sofortige Probe | Anomalie, kein Beweis |
| Quelle gestört | Health, Quote, Lag, Kopf | Quelle gesperrt, andere weiter | gleichmäßig langsame Quellen: nur über E1/E2 |
| Builder gestört | Format, Fehler, RTT | Adapter gesperrt, anderer weiter | keins über diese Prüfungen hinaus |
| Vorsprung weg | eigene Latenz, Verlustserie | BUYs pausiert | keins über diese Prüfungen hinaus |
| Watch gestört | Herzschlag, Blockbindung, Speicher | alles gesperrt, Alarm | Exits dann auch gesperrt (bewusst) |
| Reorg, Leser-Widerspruch | Invalidierung, zwei Leser | Unknown | keins |

**Bewusst nicht Aufgabe der Watch:**
- Risiken einzelner Token: prüft die bestehende Zulassung pro Launch.
- Verluste durch die Strategie: dafür sind Verluststopp und Grant-Grenzen da.
- Ein gestohlener Schlüssel: BSC3-001.

## 15. Auftrag an Claude

```text
Setze BSC-WATCH V0.4 (dieses Dokument) um. Es ersetzt V0.3; wo V0.2 genauer ist, gilt V0.2 zusätzlich.
1. Code-Kern §5 übernehmen, Typnamen an die tatsächlichen anpassen (SourceId, VenueId, LiveAdapter: Copy + Eq + Debug), alle Tests behalten.
2. QualifiedFreeze::verified nur im Freeze-Verifier, ExitAuthority::granted nur im Exit-Gate des Owners; ein CI-Quelltext-Scan pinnt beide Aufrufstellen.
3. Integration exakt nach §6 und Frontend-Durchleitung exakt nach §7, Pakete W0–W7 nach §13. Je Paket Rot → Grün, Mutationsnachweise §12, unabhängiges Review, CI auf dem finalen SHA, Belege in STATUS.md.
4. Leistung nach §10: vollständiger Sendepfad mit und ohne WATCH auf demselben Korpus, Zahlen mit Commit-SHA. Die Watch darf den durch BSC3-076 optimierten Hot Path nicht messbar über die Abnahmegrenze verlangsamen.
5. Frische nach §10: Start mit 0, messen und berichten, nicht stillschweigend lockern.
6. Keine Echtgeldaktion, kein SIGN, ARM oder SEND, keine Builder-Probe ohne meine Freigabe.
```
