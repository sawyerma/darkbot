# DarkBot — Project 0 — Finaler Produktions-Phasenplan

**Status:** ARCHITECTURE FREEZE — AUTHORITATIVE — FROZEN v4.2  
**Stand:** 2026-09-30  
**Geltungsbereich:** DarkBot / Project 0 Liquidator  
**Source of Truth:** Diese Datei definiert Architektur, Reihenfolge, Gates und verbindliche Entwicklungsregeln.

**Freeze-Korrekturen:** `solana-dex-superagg` ist als Remote-Aggregator-Client klassifiziert; `marginfi-v2` Apache-2.0 und `solana-dex-superagg` MIT sind lizenzseitig bestätigt; Receivership Maintenance=SPOT und Equity=EMA sowie die tatsächlichen `liquidate_end.rs`-Grenzen sind als Protocol-Semantik aufgenommen; H0 modelliert konfigurierbare Konkurrenz-Mindestprofitschwellen. **v4.2 trennt die Ingress-Rollen explizit: Race-LIVE ausschließlich über einen qualifizierten binären/native Mainnet-Source; Long-Tail-/Premium-LIVE darf kostenoptimiert über `WsRpcControlSource` plus periodische Snapshots/Reconciliation laufen. Shred/Deshred ist keine S1-Pflicht, sondern nur K-Eskalation bei gemessenem `LOSS_REASON = INGRESS`. Maßgeblich ist Quellenlatenz/Position im Datenpfad, nicht JSON-Parsingkosten.**

---

# 0. Leitbild

## 0.1 Primäres Produktziel

**Maximiere nachhaltigen realisierten Netto-Profit.**

Nicht optimiert wird auf:

- minimale lokale Latenz um jeden Preis;
- maximale Anzahl Liquidationen;
- maximale Anzahl Provider;
- maximale technische Komplexität;
- maximale Anzahl Abos;
- maximale Anzahl selbst geschriebener Komponenten.

Ökonomische Zielfunktion:

```text
REALIZED NET PROFIT
=
realisierter Liquidationserlös zum tatsächlich ausführbaren Preis
- Repay
- Project-0-Protokollgebühren
- Project-0 Flat-SOL-Liquidationsgebühr
- Swap-/DEX-Gebühren
- realer Price Impact / Slippage
- Priority Fees
- Jito Tips
- Token-/Rent-Kosten
- pfadabhängige Kosten fehlgeschlagener Versuche
- Infrastrukturkosten
- Provider-/API-/SaaS-Kosten
- Kapitalkosten
```

**Latenz, Capture Rate, Routing, Inclusion und technische Geschwindigkeit sind Mittel zur Profitmaximierung, nicht das Endziel.**

---

## 0.2 Eine Production-Implementierung

Es gibt **genau einen finalen Production-Codepfad**.

```text
kein:
Prototype → Rewrite → Production

sondern:
S0 baut final
→ jede folgende Phase erweitert denselben Code
→ Replay, Shadow und LIVE benutzen dieselben Komponenten
```

Es gibt ausdrücklich keinen separaten:

- Race-Bot;
- Long-Tail-Bot;
- Shadow-Core;
- Health-Core;
- Execution-Core;
- Economics-Core;
- zweiten Solana-Core.

---

## 0.3 Race und Long-Tail sind gleichzeitig Bestandteil desselben Systems

**Race und Long-Tail sind keine Entwicklungsstufen und keine getrennten Produkte.**

Beide werden von Anfang an in derselben finalen Architektur berücksichtigt.

Drei Opportunity-Trigger speisen dieselbe Detection-/Health-/Execution-Pipeline:

```text
(a) Oracle-/State-Update
    → sofortige Schwellenverletzung
    → Race Opportunity

(b) zeitabhängige Premium-/Debt-Grenze
    → Premium Scheduler
    → zeitgetriebene Opportunity

(c) liquidation_tagged_at
    → t_profitable Scheduler
    → Long-Tail Opportunity
```

Danach immer derselbe Pfad:

```text
Trigger
→ Exact Health
→ Deterministic Economics
→ Opportunity Ranking
→ Receivership Planner
→ TX Build
→ Sign
→ Submit
→ Landing
→ Reconciliation
```

**Es wird nicht zuerst ein billiger Long-Tail-Bot und später ein schneller Race-Bot gebaut.**

---

## 0.4 Kostenoptimierung ohne technische Verkrüppelung

Der Code wird von Anfang an:

- schnell;
- schlank;
- provider-austauschbar;
- Rust-hotpath-fähig;
- fork-/state-korrekt;
- Race-fähig;
- Long-Tail-fähig;
- v0-/v1-fähig;
- Jito-/RPC-fähig;
- NO_SWAP-/Swap-fähig

gebaut.

Die **laufende Infrastruktur** wird dagegen kostenoptimiert betrieben.

Regel:

```text
kostenlos / bereits vorhanden + technisch ausreichend
→ verwenden

laufende Zusatzkosten
→ nur wenn gemessener zusätzlicher Netto-Profit
   > vollständige Zusatzkosten + Abhängigkeitsrisiko
```

Dies gilt insbesondere für:

- Paid RPC;
- Geyser-Abos;
- QuickNode-Zusatzprodukte;
- dedizierte Server;
- eigene Solana-Nodes;
- zusätzliche Regionen;
- DoubleZero;
- kostenpflichtige Routing-/Submission-Dienste.

**Eine kostenlose Infrastruktur darf die Architektur nicht verschlechtern.**
Adapter und Interfaces werden so gebaut, dass bessere Infrastruktur später ohne Rewrite genutzt werden kann.

---

# 1. Harte Architekturregeln

## 1.1 Rust dort, wo Rust nötig ist

Nicht „Rust um jeden Preis“, sondern:

**Alles, was LIVE-Korrektheit, State, Race-Latenz, deterministische Ausführung oder TX-Erzeugung beeinflusst, wird in Rust gebaut.**

Rust ist verbindlich für:

- Solana Native/Binary Ingress;
- Protobuf-/Geyser-Decode;
- native Fast-Lane Event Mapping;
- Canonical/Fork State;
- RAM State;
- Reconnect/Reconciliation;
- Project-0 Account Decoder;
- Reverse Indices;
- Threshold Index;
- Premium Scheduler;
- Tag Scheduler / `t_profitable`;
- Health-Anbindung;
- LIVE Liquidation Detection;
- Race Detection;
- Long-Tail Detection;
- deterministische Economics;
- LIVE Opportunity Ranking;
- Receivership Planning;
- Routing-Entscheidung im LIVE-Pfad;
- Transaction Builder;
- v0/v1 Builder;
- Signing;
- RPC-/Jito-Submission;
- Landing Observation;
- Reconciliation;
- Hotpath Telemetrie.

Python ist erlaubt für:

- Source-/Referenzanalyse;
- P0-Reuse-Analyse;
- Account Census;
- historische H0-Extraktion;
- große Offline-Auswertungen;
- Statistik;
- Reports;
- Plots;
- Research;
- Modelltraining;
- Audit-/Diagnose-Werkzeuge.

**Kein Python im LIVE-Hotpath.**

---

## 1.2 REUSE BEFORE REIMPLEMENTATION

Für jede Project-0-Komponente wird **vor Eigenentwicklung** geprüft:

1. existiert die Lösung bereits in `0dotxyz/eva01`?
2. existiert die Lösung im offiziellen `marginfi-v2`-Crate?
3. existiert eine geeignete Bibliothek, die `eva01` bereits nutzt?
4. ist die Lösung technisch korrekt für unseren aktuellen Protocol-/Solana-Stand?
5. ist die Nutzung lizenzrechtlich erlaubt?
6. erfüllt sie unsere Performance- und Architekturvorgaben?

Danach genau eine der drei Entscheidungen:

```text
DIRECT_DEPENDENCY
→ vorhandenes Crate direkt pinnen und verwenden

REUSE_ADAPT
→ geeignete vorhandene Implementierung übernehmen/anpassen

REIMPLEMENT
→ nur wenn Dependency/Reuse technisch, lizenzrechtlich
   oder bezüglich Korrektheit/Performance nicht geeignet ist
```

**„Wir können es selbst neu schreiben“ ist kein ausreichender Grund für REIMPLEMENT.**

---

## 1.3 `0dotxyz/eva01` ist Referenzimplementierung

`eva01` wird systematisch analysiert, nicht nur als README-Beispiel.

Aktuell sichtbare Rust-Komponenten umfassen u. a.:

```text
src/cache.rs
src/cache_loader.rs
src/geyser.rs
src/liquidator.rs
src/execution/
src/marginfi_ixs.rs
src/kamino_ixs.rs
src/drift_ixs.rs
src/juplend_ixs.rs
src/clock_manager.rs
src/rebalancer.rs
src/config.rs
src/wrappers/
src/utils/
```

Die aktuelle `eva01`-Dependency-Struktur enthält u. a.:

- `marginfi`;
- `marginfi-type-crate`;
- `yellowstone-grpc-client`;
- `yellowstone-grpc-proto`;
- `solana-dex-superagg`;
- `pyth-solana-receiver-sdk`;
- `pythnet-sdk`;
- `rayon`;
- `crossbeam`;
- `tokio`.

**Wichtig:** Die Architektur und geeignete Implementierungen werden übernommen, aber alte Versionspins werden nicht blind kopiert. `eva01` nutzt derzeit einen älteren Solana-/Yellowstone-Stand als der aktuelle Solana-v1-Stack.

---

## 1.4 Lizenzregel

Code wird nur übernommen, wenn die Lizenzlage dies zulässt.

Für jede Reuse-Komponente dokumentiert P0:

```text
upstream
commit/tag/rev
license
allowed use
copied code? yes/no
dependency? yes/no
modifications
update strategy
```

`eva01` selbst besitzt aktuell keine ausgewiesene Root-Lizenz.

Daher:

- Source lesen und Architektur verstehen: erlaubt;
- APIs/Verhalten nachbauen: erlaubt;
- separat lizenzierte Dependencies nutzen: entsprechend deren Lizenz;
- `eva01`-Code kopieren: erst nach eindeutiger Lizenz-/Freigabeklärung.

Lizenzstatus der beiden zentralen Upstreams:

- `0dotxyz/marginfi-v2`: **Apache-2.0** — direkte Crate-Nutzung und Modifikation sind unter Einhaltung der Apache-2.0-Bedingungen erlaubt.
- `0dotxyz/solana-dex-superagg`: **MIT** — die Library ist lizenzrechtlich nutzbar; ihre Remote-API-Abhängigkeiten und Kosten werden separat in P0 qualifiziert.

---

## 1.5 Protocol-Code vor Risk-Nachbau

Für `marginfi-v2` ist die Lizenzfrage geklärt: **Apache-2.0**.

Damit ist `USE_CRATE` der bevorzugte und lizenzrechtlich zulässige Weg. Offen zu qualifizieren sind nur technische Kompatibilität, deployed Tag/API, Performance und exakte LIVE-Semantik.

**Die offizielle `marginfi`-Programmlogik / das offizielle Crate wird als Health-/Risk-Referenz und nach Möglichkeit direkt als Dependency genutzt.**

Bevorzugte Reihenfolge:

```text
offizielles Crate direkt verwenden
↓ falls nicht möglich
offizielles Crate als Parity-Referenz
↓ nur falls zwingend
eigene exakte Reimplementierung
```

Keine vereinfachte Health-Formel.

---

## 1.6 Fail Closed

Unbekannt bedeutet niemals „0“ und niemals „wird schon stimmen“.

Bei unbekanntem:

- Program-Stand;
- Layout;
- Account-Größe;
- Oracle-Typ;
- Integrationstyp;
- Health-Semantik;
- Premium-Semantik;
- Fee-Modell;
- Route;
- Allowlist;
- Transaction-Version;
- State-Gap;
- Fork-State;
- Reconciliation;

gilt:

```text
HALT / SKIP
```

nicht:

```text
GUESS
```

---

## 1.7 Hotpath-Verbote

Im latency-kritischen **Race-Production-Pfad**:

- keine Datenbank;
- kein Python;
- **kein JSON-RPC-WebSocket als normative Race-Quelle;**
- kein JSON-/Base64-Normalisierungspfad als notwendige Race-Vorstufe;
- bevorzugt binäre/native Quelle direkt aus validatornaher Datenbereitstellung;
- kein synchroner RPC-Lookup als Voraussetzung;
- kein HTTP-Routing-Call als zwingende Voraussetzung, wenn lokal/precomputed möglich;
- kein globaler Account-Full-Scan;
- kein unbekannter Wert mit Fallback auf 0;
- keine vereinfachte Health-Formel;
- keine unbounded Queue;
- keine unnötige Lock-/Allocation-Kaskade.

Der Production-Fast-Lane-Pfad ist:

```text
binary/native network event
→ minimaler Rust decode
→ fixed/native Rust event
→ bounded channel / SPSC
→ RAM state
→ detection / health / economics
→ build / sign / submit
```

WSS/RPC/JSON bleiben erlaubt für:

- P0/P1/C0/H0;
- Bootstrap/Snapshot;
- Recovery/Reconciliation;
- Control Plane;
- Evidence/Shadow-Vergleich;
- Fallback-Diagnostik;
- **Long-Tail-LIVE und Premium-Trigger-LIVE**, sofern periodische Snapshots/Reconciliation den benötigten State vollständig und fail-closed aktuell halten.

Die Trennung ist verbindlich:

```text
Race-Trigger (a):
    ausschließlich qualifizierter Binary/Mainnet-Source

Premium-/Long-Tail-Trigger (b/c):
    WsRpcControlSource + periodischer Snapshot/Reconcile ist Production-zulässig
    solange State-Freshness, Coverage und Economics die jeweilige Execution-Klasse freigeben
```

**Begründung:** Der Hauptvorteil von Geyser gegenüber Standard-WSS ist die Quellenlatenz und die Position im Datenpfad (validatornah statt hinter normalem RPC/WebSocket-Serving), nicht primär die CPU-Zeit des JSON-Parsens. JSON-Parsing ist im Vergleich typischerweise nur ein kleiner lokaler Anteil.

WSS ist **nicht** die Latency Source of Truth für Race.

---

## 1.8 Messen vor Entscheiden

Nicht ideologisch festlegen:

- v0 vs. v1;
- RPC vs. Jito;
- single TX vs. bundle;
- welcher **qualifizierte binäre/native Race-Ingress** genutzt wird (initial Geyser/Protobuf; weitere native Quellen erst nach Messung);
- Provider A vs. Provider B;
- NO_SWAP vs. Swap;
- Tip-Höhe;
- Route;
- eigener Node vs. Provider.

Entscheidung erfolgt aus:

```text
Korrektheit
+ Landing
+ Latenz
+ Failure Rate
+ realisierbarer Profit
+ vollständige Kosten
```

---

# 2. Source-of-Truth-Hierarchie

Für LIVE gilt in dieser Reihenfolge:

1. **deployed On-Chain ProgramData / tatsächlicher Mainnet-Stand**
2. **zum Deployment gehörender Source-/Release-/IDL-Stand**
3. offizielles `marginfi-v2`-Crate / Type-Crate
4. Project-0 Release Notes / Security Notes
5. qualifizierte Referenzimplementationen (`eva01`, `mary`)
6. Solana-/Yellowstone-/Jito-Primärquellen
7. diese Roadmap
8. Annahmen nur als **PRÜFEN**

GitHub-Release ≠ Mainnet-Deployment.

---

# 3. Aktueller belegter Ausgangszustand

## 3.1 Eigenes Repo

Stand 2026-09-30:

```text
bestehender Solana-Bot:
    Python
    PAPER-orientiert
    bestehende Capture-/Replay-Infrastruktur

Project-0 Production Rust:
    noch nicht aufgebaut

native/:
    wird durch S0 eingeführt

BSC:
    getrennte Linie

Pump.fun:
    getrennte Protokolllinie
```

Die existierenden Python-Komponenten werden **nicht** als LIVE-Project-0-Core weiterentwickelt.

Sie dienen nur dort als Referenz, Capture, Replay oder Offline-Analyse, wo sinnvoll.

---

## 3.2 Project 0 aktuell / kommende Version

`mrgn-0.1.12-rc1` wurde am 2026-09-29 als Pre-Release veröffentlicht.

Bis Mainnet-Deployment gilt 0.1.12 **nicht** als LIVE-Wahrheit.

Wichtige für uns relevante 0.1.12-Themen:

- Liquidation Tagging;
- `liquidation_tagged_at`;
- steigende Receivership-Premium;
- `PREMIUM_ACTIVE`;
- variable Borrow Premium in Health;
- neue Receivership-Health-Regel;
- Bank Resize 1.856 → 3.904 Bytes ohne Anchor-Discriminator;
- `ScopeKamino`;
- `ScopeJuplend`;
- `MAX_COSTLY_POSITIONS = 4`;
- neue Premium-/Balance-Felder.

Mainnet-ETA laut Release: **2026-10-12, exakte Zeit noch offen.**

---

## 3.3 Solana Transaction v1

Transaction v1 ist auf Mainnet aktiv.

Relevante Eigenschaften:

- größere Transaktionen bis 4096 Bytes;
- keine LUTs in v1;
- max. 64 Accounts nach aktueller v1-Spezifikation;
- Resource-/Fee-Konfiguration in `TransactionConfig`;
- Priority Fee als Gesamtbetrag in Lamports;
- ComputeBudget-Instructions konfigurieren v1 nicht;
- Reader müssen Version 1 unterstützen;
- Rust-Clientseite benötigt aktuelle Solana-4.x-Unterstützung;
- Yellowstone muss `Message.config` korrekt erhalten.

Deshalb dürfen ältere `eva01`-Versionspins nicht blind übernommen werden.

---

# 4. Finale Zielarchitektur

```text
                         SOLANA
                           │
               ┌───────────┴────────────┐
               │                        │
      PRIMARY PRODUCTION           CONTROL / EVIDENCE
          FAST LANE                    PLANE
               │                        │
   BinaryGeyserSource            WsRpcControlSource
   Protobuf / native             JSON-RPC / WSS
      RACE-LIVE               LONG-TAIL/PREMIUM-LIVE
               │                        │
               └──────────┬─────────────┘
                          │
                   IngressSource
                       RUST
                          │
          fixed/native internal events
          NO JSON in Race hotpath
                          │
                 Canonical/Fork State
                     RAM / RUST
                          │
                  Project-0 State
                          │
                   Reverse Indices
                          │
      ┌───────────────────┼────────────────────┐
      │                   │                    │
Oracle/State Trigger Premium Boundary     Tag Boundary
      │                   │                    │
   RACE            TIME-DEPENDENT          LONG-TAIL
      │                   │                    │
      └───────────────────┴────────────────────┘
                          │
                     Exact Health
                          │
                  Opportunity Detection
                          │
             Deterministic Profit Engine
                          │
           Probabilistic Optimizer nach P7
                          │
                 Receivership Planner
                    │             │
                 NO_SWAP         SWAP
                    └──────┬───────┘
                           │
                       TX Builder
                       v0 / v1
                           │
                         Signer
                           │
                  SubmissionRouter
                    │             │
                   RPC           Jito
                    └──────┬───────┘
                           │
                         Landing
                           │
                     Reconciliation
                           │
                  REALIZED NET PROFIT
```

**BSC-Paritätsprinzip:** Wie beim BSC-Pfad `native event → fixed Rust decode → state/decision → build/sign` darf Project 0 vor der Trade-Entscheidung keinen JSON-Konvertierungsweg erzwingen.

**Kostenregel bleibt unverändert:** Race verlangt einen qualifizierten binären/native Mainnet-Ingress, aber **keinen kostenpflichtigen Provider als Startvoraussetzung**. Fehlt eine kostenlose geeignete Binary-Quelle, bleibt Race-LIVE gesperrt. Long-Tail-/Premium-LIVE darf trotzdem über `WsRpcControlSource` plus periodischem Snapshot/Reconcile laufen und damit bereits realen Gewinn erzeugen. Ein Paid Binary Feed wird erst aktiviert, wenn sein zusätzlicher nachhaltiger Netto-Profit die vollständigen Kosten und das Abhängigkeitsrisiko übersteigt.
Nebenpfad:

```text
unhealthy + noch nicht wirtschaftlich liquidierbar
→ Tagger
→ liquidation_tagged_at
→ Tag Scheduler
→ t_profitable
→ dieselbe Detection-/Execution-Pipeline
```

---

# 5. Finaler Workspace

S0 baut:

```text
native/
  Cargo.toml

  crates/
    darkbot-common/
      timing
      telemetry
      histograms
      channels
      lifecycle
      config
      journal
      replay
      errors

    darkbot-solana/
      ingress
        binary
        geyser
        wsrpc_control
        fastlane
      canonical_state
      fork_root
      blockhash
      lut
      tx
      signer
      rpc_submission
      jito_submission
      reconciliation

    darkbot-project0/
      protocol
      decoder
      indices
      scheduler
      health
      economics
      planner
      tagger
      runner

    darkbot-pumpfun/
      eigene Protokolllinie

    darkbot-bsc/
      eigene Protokolllinie

    darkbot-capture/
      capture/replay bridge

    darkbot-runner/
      replay
      shadow
      live
```

**Keine generische Multi-Chain-Abstraktion erzwingen.**

Gemeinsam genutzt wird nur, was wirklich chain-/protokollunabhängig ist.

---

# 6. Timing Contract

Ab S0 verbindlich:

```text
T_rx
    Rohupdate im Ingress empfangen

T_state
    relevanter Canonical-/Project0-State aktualisiert

T_detect
    Opportunity als gültig + wirtschaftlich erkannt

T_ready
    finaler TX-Plan gebaut

T_sign
    Signatur fertig

T_handoff
    an Submission-Adapter übergeben

T_observed_landed
    Landing von uns beobachtet
```

Ableitungen:

```text
ingress_to_state = T_state - T_rx
state_to_detect  = T_detect - T_state
detect_to_ready  = T_ready - T_detect
ready_to_sign    = T_sign - T_ready
sign_to_handoff  = T_handoff - T_sign
```

Landing Evidence:

- signature;
- submission path;
- bundle id falls vorhanden;
- landed slot;
- tx index im Slot, wenn belastbar bestimmbar;
- confirmation status;
- failure reason;
- actual fee;
- actual tip.

**Kein erfundener exakter On-Chain-Millisekundenzeitpunkt.**

---

# 7. Gesamtphasenplan

Parallelstart:

```text
TRACK A — NATIVE CORE                 TRACK B — PROJECT-0 EVIDENCE

S0 Native Foundation                  P0  eva01/mary/reuse/licenses
S1 Solana Ingress                     P1A Current Mainnet Freeze
S2 Canonical State                    P1B Upcoming 0.1.12 Freeze
S3 Execution Core                     C0  Market / Account Census
S4 Core Parity                        H0  Historical Liquidations
          │                                      │
          └──────────────────┬───────────────────┘
                             ↓
                         MERGE GATE
                             ↓
            P2 → P3 → P4A → P5 → T
                             ↓
                            P6
                             ↓
                            P7
                             ↓
                           P4B
                             ↓
                            P8
                             ↓
                            P9
                             ↓
                             K
```

Race und Long-Tail bleiben in **allen** Project-0-Phasen gleichzeitig Bestandteil des Systems.

---

# PHASE P0 — REFERENZ-, REUSE- UND LIZENZSTUDIE

## Ziel

Vor jeder vermeidbaren Eigenentwicklung wissen:

- was `eva01` bereits löst;
- was `mary` bereits löst;
- was direkt aus `marginfi-v2` nutzbar ist;
- welche Dependencies übernommen werden können;
- was nur adaptiert werden muss;
- was wirklich neu entwickelt werden muss.

## P0.1 `eva01` vollständiger Read-through

Mindestens analysieren:

```text
cache.rs / cache/
cache_loader.rs
geyser.rs
liquidator.rs
execution/
marginfi_ixs.rs
kamino_ixs.rs
drift_ixs.rs
juplend_ixs.rs
clock_manager.rs
config.rs
wrappers/
utils/
main.rs
```

Fragen pro Bereich:

- Was macht Eva?
- Welche Source-of-Truth nutzt Eva?
- Welche State-Struktur?
- Welche Parallelisierung?
- Welche Cache-Strategie?
- Welche RPC-/Geyser-Annahmen?
- Welche Liquidationssequenz?
- Welche Refresh-Instruktionen?
- Welche Route?
- Welche Submission?
- Welche Fehlerbehandlung?
- Welche Recovery?
- Welche Teile sind für 0.1.12 noch gültig?
- Welche Teile sind für Solana v1 veraltet?

## P0.2 Dependency-Reuse-Matrix

Für jede relevante Dependency:

```text
Component
Upstream
Version/Commit/Tag
License
Purpose
Reuse decision
Required upgrade
Performance concern
LIVE critical?
```

Mindestens:

- `marginfi`;
- `marginfi-type-crate`;
- `solana-dex-superagg`;
- Pyth SDKs;
- Yellowstone;
- Solana Rust crates;
- integration-specific crates.

## P0.2A Remote-Routing-/API-Kosten-Audit

`solana-dex-superagg` ist **kein lokaler Router**. Die Library ist ein Rust-Client, der Remote-Aggregatoren wie Jupiter, Titan und DFlow anspricht und deren Ergebnisse vergleicht.

Vor einer Nutzung als Dependency muss P0 für **jeden** konfigurierbaren Aggregator dokumentieren:

```text
Aggregator
Endpoint
API key required?
kostenlos ohne Key?
kostenloser Tier mit Key?
Rate Limit
monatliche Kosten
Usage-Kosten
WebSocket/HTTP
Latenz
SLA/Abhängigkeit
Receivership-Allowlist kompatibel?
Hotpath-tauglich?
Entfernungskriterium
```

Aus der aktuellen `solana-dex-superagg`-README ist belegt:

- Jupiter: Remote Swap API; API-Key optional, für Production empfohlen.
- Titan: Remote WebSocket/API; Titan API-Key erforderlich.
- DFlow: Remote Quote API; API-Key optional, ohne Key rate-limited.

Aus `eva01/bin/env.template` ist zusätzlich belegt:

- `JUP_SWAP_API_URL=https://lite-api.jup.ag/swap/v1`;
- `JUP_SWAP_API_KEY` ist konfigurierbar.

**Kosten-/Free-Tier-Status wird nicht aus diesen Angaben abgeleitet.** Er wird unmittelbar vor Dependency-Freigabe anhand der jeweils aktuellen Anbieterbedingungen geprüft.

Klassifikation:

```text
solana-dex-superagg
→ REMOTE_AGGREGATOR_CLIENT
→ niemals LOCAL_PRECOMPUTED_ALLOWED
→ je nach aktuellem Tarif/Key:
   FREE_PUBLIC_ALLOWED oder PAID_ALLOWED
```

Wirklich lokales Routing gilt erst dann als vorhanden, wenn Quotes/Route aus lokal gehaltenem Pool-/Venue-State ohne zwingenden Remote-Routing-Call erzeugt werden.

## P0.3 Reuse Decision

Für jede Komponente exakt eine Entscheidung:

```text
DIRECT_DEPENDENCY
REUSE_ADAPT
REIMPLEMENT
DROP
```

Jede `REIMPLEMENT`-Entscheidung braucht Begründung.

## P0.4 Oracle-Modell

Klären:

- welche Preise Eva bezieht;
- welche Preisupdates Eva selbst einbindet/postet;
- Pyth Receiver / Pythnet Nutzung;
- Scope;
- Integration-spezifische Oracles;
- Freshness-/Confidence-Regeln;
- Race-Relevanz des Oracle-Pfads.

## P0.5 Staging

Klären:

- `network-staging`;
- Program IDs;
- verwendbare Testkonten;
- welche P5/P8-Tests dort real ausführbar sind.

## Gate P0

PASS nur wenn:

- Reuse-Matrix vollständig;
- Lizenzstatus je Reuse-Komponente dokumentiert;
- Remote-Routing-/API-Key-/Free-Tier-/Kosten-Audit abgeschlossen;
- Health-Strategie entschieden;
- Oracle-Modell verstanden;
- Staging-Pfad bekannt;
- kein offensichtliches Rad neu erfunden wird.

## Artefakte

```text
docs/project_0/evidence/P0_REFERENCE_STUDY.md
docs/project_0/evidence/P0_REUSE_MATRIX.md
docs/project_0/evidence/P0_LICENSE_MATRIX.md
```

---

# PHASE P1A — CURRENT MAINNET FREEZE

## Ziel

Exakt feststellen, was **jetzt tatsächlich deployed** ist.

## Einzufrieren

- Program ID;
- ProgramData;
- Upgrade Authority / Upgrade Identity;
- deployed Program Hash;
- deployed Source-/Release-Zuordnung;
- IDL + Hash;
- Discriminators;
- Account Layouts;
- Payload-Größen;
- Rohlängen inkl. Anchor-Discriminator;
- Bank;
- MarginfiAccount;
- MarginfiGroup;
- FeeState;
- OracleSetup;
- Asset Tags;
- Integrationen;
- costly-position-Regeln;
- receivership instructions;
- refresh allowlist;
- swap/integration allowlist;
- Flat-SOL-Liquidationsgebühr;
- Health-Regeln;
- Premium-Aktivierung.

## Upgrade Watch

```text
ProgramData ändert sich
→ EXECUTION HALT
→ neue Version bestimmen
→ Layout/IDL/Source neu qualifizieren
→ Parity
→ LIVE erst danach wieder freigeben
```

## Gate P1A

PASS nur wenn der LIVE-Stand eindeutig identifiziert und source-pinned ist.

## Artefakt

```text
docs/project_0/manifests/CURRENT_MAINNET.md
```

---

# PHASE P1B — UPCOMING 0.1.12 FREEZE

## Status

`mrgn-0.1.12-rc1` ist Pre-Release.

Nicht LIVE, bevor On-Chain-Deployment bestätigt ist.

## Relevante Änderungen

### Liquidation Tagging

`marginfi_account_tag_liq_record`:

- permissionless;
- keine Program-Instruktions-Signer-Anforderung;
- äußere Solana-TX benötigt weiterhin Fee Payer/Signatur;
- setzt `liquidation_tagged_at`;
- bereits getaggt + unhealthy → `AccountAlreadyTagged`;
- healthy ohne Tag → `HealthyAccount`.

Premium:

```text
0 ... 1 h:
max(FeeState.liquidation_max_fee, 5 %)

danach:
linear steigend

bei 7 Tagen:
bis 100 %
```

Kein Premium-Wachstum:

- bei Bad Debt;
- wenn `EMODE_BOOSTED`.

Nach ausreichend großer Liquidation kann Timer zurückgesetzt werden.

### Variable Borrow Premium

Health muss projizierte Premium-Schuld berücksichtigen.

`PREMIUM_ACTIVE` ist pro Bank relevant.

### Bank Resize

```text
alt: 1.856 Bytes
neu: 3.904 Bytes
jeweils ohne Anchor-Discriminator
```

Nicht-resized Bank nach Upgrade:

```text
TX berührt Bank
→ FAIL
```

### Receivership Health

Neue zwei Bedingungen:

1. Maintenance Health darf höchstens um den ungewichteten Wert `seized - repaid` fallen.
2. War der Account vor der Liquidation auf ungewichteter Equity-Basis solvent, darf er danach nicht in Bad Debt geraten.

### Weitere relevante Änderungen

- `ScopeKamino = 27`;
- `ScopeJuplend = 28`;
- `MAX_COSTLY_POSITIONS = 4`;
- `Balance.premium_rate_snapshot`;
- `Balance.premium_outstanding`;
- `MarginfiAccount.liquidation_tagged_at`;
- neue Premium-Felder in Bank/Group/FeeState.

## Promotion

Nach tatsächlichem Mainnet-Deployment:

```text
HALT
→ ProgramData prüfen
→ Program Hash prüfen
→ Bank Resize prüfen
→ IDL/Layout prüfen
→ Parity
→ P1B wird neuer P1A
```

## Gate P1B

PASS bedeutet:

```text
upgrade-ready
```

nicht:

```text
already-live
```

## Artefakt

```text
docs/project_0/manifests/UPCOMING_0_1_12.md
```

---

# PHASE C0 — MARKET- UND ACCOUNT-CENSUS

## Ziel

Vor LIVE und vor unnötigem Aufwand quantitativ kennen:

- wie groß der adressierbare Markt ist;
- welche Account-Klassen existieren;
- wie viel davon Race-Potential besitzt;
- wie viel davon Long-Tail-Potential besitzt;
- welche Integrationen fail-closed bleiben müssen.

## Erfassung

Für alle relevanten Project-0-Accounts:

```text
healthy

unhealthy + sofort wirtschaftlich
    → Race / sofortige Liquidation

unhealthy + aktuell nicht wirtschaftlich
    → Tag-Kandidat / Long-Tail

zeitabhängig durch Borrow Premium
    → zukünftiger Health-Trigger

unsupported / fail-closed
```

Zusätzlich:

- Account-Wert;
- Liability-Wert;
- Banks;
- Oracle-Typen;
- Integrationen;
- costly positions;
- PREMIUM_ACTIVE;
- e-mode;
- Bad Debt;
- supported vs. unsupported.

## Long-Tail-Modell

Für jeden Tag-Kandidaten offline:

```text
premium(t)
premium_cap(t)
realizable_value(t)
deterministic_net_profit(t)
t_profitable
```

## Race-Modell

Offline erfassen:

- Abstand zur Liquidationsschwelle;
- betroffene Oracle/Banks;
- erwartete TX-Komplexität;
- relevante Integrationen;
- mögliche v0/v1-Klassen.

## Gate C0

Kein willkürliches „Long-Tail muss allein reichen“.

GO für Project-0-spezifischen Ausbau, wenn belastbare Daten zeigen, dass mindestens eine oder mehrere Opportunity-Klassen wirtschaftlich relevant sind:

```text
Race
oder
Long-Tail
oder
Premium-driven
```

## Artefakt

```text
docs/project_0/evidence/C0_MARKET_CENSUS.md
```

---

# TRACK H0 — HISTORICAL LIQUIDATION INTELLIGENCE

## Ziel

Reale historische Liquidationen und Konkurrenz verstehen.

## Reader

Alle historischen Reader müssen aktuelle Transaction-Versionen unterstützen.

Mindestens:

```text
maxSupportedTransactionVersion = 1
```

oder äquivalente aktuelle Client-Unterstützung.

## Pro Liquidation erfassen

Soweit belegbar:

- signature;
- slot;
- tx index;
- receiver/winner;
- liquidatee;
- seized asset;
- repaid asset;
- seized;
- repaid;
- premium;
- tag age, sobald 0.1.12 live ist;
- protocol fees;
- flat SOL fee;
- priority fee;
- Jito tip;
- transaction version;
- serialized bytes;
- account count;
- compute;
- LUT;
- integration positions;
- costly positions;
- oracle types;
- refresh instructions;
- swap program;
- route;
- single TX / bundle;
- wiederkehrende Winner;
- erkennbare Eva-/Standard-Bot-Muster.

## Nach 0.1.12 zusätzlich

Für Tags:

```text
tagged_at
wann liquidiert
bei welcher Premium
wer gewann
Tag gelöscht?
Timer reset?
```

## Konkurrenz-Mindestprofit

`eva01` besitzt eine explizite Konfigurationsvariable:

```text
MIN_PROFIT
```

Der aktuelle Template-Wert ist `1` USD; `liquidator.rs` verwirft Opportunities, wenn:

```text
liquidator_profit <= min_profit
```

Damit ist belegt, dass mindestens die Eva-Referenz eine konfigurierbare Mindestprofit-Schwelle verwendet.

H0 nutzt Tag-Zeitpunkt, Premium-Kurve, Asset-/Debt-Werte, beobachteten Take-Zeitpunkt und rekonstruierte Execution-Kosten, um für wiederkehrende Konkurrenten **implizite Mindestprofit-Bänder** zu schätzen:

```text
observed_take_time
+ premium(t)
+ reconstructable fees/route
→ implied competitor min-profit range
```

Das ist eine **Inference**, kein exaktes Auslesen fremder Configs: unbekannte Tips, Inventar, Route, Providerkosten und interne Policies erzeugen Unsicherheit. Ergebnisse werden daher als Band/Verteilung mit Unsicherheitskennzeichnung geführt, nicht als behaupteter exakter Wert.

Zielvergleich:

```text
unsere t_profitable / Mindestprofitschwelle
vs.
geschätzte Konkurrenzschwelle
```

## Zeitraum

Start:

```text
30 Tage
```

Bei kleiner Stichprobe:

```text
90 Tage
```

## Gate H0

Genug Daten, um:

- typische TX-Klassen;
- Competition;
- Winner-Konzentration;
- Tip-Verhalten;
- Long-Tail-Wartekurven;
- implizite Konkurrenz-Mindestprofit-Bänder;
- Execution-Klassen

zu quantifizieren.

## Artefakt

```text
docs/project_0/evidence/H0_LIQUIDATIONS.md
```

---

# PHASE S0 — FINAL NATIVE FOUNDATION

## Ziel

Finales Rust-Fundament für Solana/Project 0 schaffen.

## Bau

`darkbot-common`:

- monotonic timing;
- Histogramme p50/p95/p99/max/count;
- bounded channels;
- Lifecycle;
- Shutdown;
- Config;
- Error Taxonomy;
- Journal;
- Replay-Primitives;
- Capture-Primitives;
- deterministische IDs;
- feste Ownership-Regeln.

## Performance-Regeln

Von Anfang an:

- keine DB im Hotpath;
- keine unnötigen Allocations;
- kein unnötiges Cloning;
- keine unbounded Queues;
- keine globalen Locks, wenn vermeidbar;
- keine String-/JSON-Konvertierung im inneren Decision-Loop;
- Messung jeder Queue-Grenze.

## Bestehenden Code übernehmen

Gate-0-/BSC-/Solana-Code nur dann übernehmen, wenn:

- er wirklich allgemeiner Production-Code ist;
- kein Rewrite nötig ist;
- Semantik eindeutig ist.

Benchmark-Harness bleibt Harness.

## Gate S0

PASS nur wenn:

- Workspace steht;
- Timing API fest;
- Telemetry API fest;
- Channels/Lifecycle fest;
- Rust-Hotpath ohne Python möglich;
- spätere Komponenten ohne strukturellen Rewrite integrierbar.

---

# PHASE S1 — FINAL SOLANA INGRESS

## Ziel

Eine provider-austauschbare Rust-Ingress-Schicht bauen, die zwei klar getrennte Production-Rollen unterstützt:

```text
RACE-LIVE
→ BinaryGeyserSource
→ validatornahe binäre/Protobuf-Daten

LONG-TAIL / PREMIUM-LIVE
→ WsRpcControlSource
→ JSON-RPC/WebSocket + periodischer Snapshot/Reconcile
```

Beide speisen **denselben** Canonical-/Project-0-State und denselben Health/Economics/Execution-Core. Es gibt keinen zweiten Bot.

## S1.1 `IngressSource` / native Event-Modell

Businesslogik kennt weder Provider noch Transport.

Ein einheitliches Rust Event-Modell enthält mindestens:

- Accounts;
- Transactions;
- Slots;
- Roots;
- write_version / gleichwertige Provenance;
- receive timestamp;
- source health;
- gap events;
- reconnect/recovery events.

Nach dem Source-Adapter arbeiten die latency-kritischen Rust-Komponenten nur mit nativen/fixen Events.

## S1.2 RACE PRIMARY — `BinaryGeyserSource`

Yellowstone-/Geyser-kompatibler binärer Race-Production-Adapter.

```text
validatornahe Source
→ gRPC / Protobuf
→ generated Rust structs
→ minimal mapping
→ native DarkBot event
→ Canonical State
```

Mindestens:

- account updates;
- transactions;
- slots;
- roots;
- write version;
- reconnect;
- gap detection;
- replay/fromSlot soweit Source;
- replay information soweit Source;
- aktuelle v0/v1-Felder einschließlich `Message.config`.

### Race Source Gate

Race-LIVE ist nur erlaubt, wenn ein **Mainnet**-Binary-Source qualifiziert ist:

- vollständige benötigte Daten;
- korrekte Provenance/Fork-/Slot-Semantik;
- v0/v1 fidelity;
- Recovery/Gaps beherrscht;
- reale Latenz gemessen;
- Kosten dokumentiert.

Fehlt ein kostenloser qualifizierter Mainnet-Binary-Source:

```text
Race-LIVE = BLOCKED
Long-Tail/Premium-LIVE = DARF WEITERLAUFEN
```

Ein Paid Geyser wird **nicht** allein deshalb gekauft. Er kommt erst nach positivem wirtschaftlichem Nachweis.

## S1.3 LONG-TAIL/PREMIUM PRODUCTION — `WsRpcControlSource`

`WsRpcControlSource` ist für folgende Rollen Production-zulässig:

- Long-Tail-Trigger über `liquidation_tagged_at` / `t_profitable`;
- Premium-/zeitgetriebene Trigger;
- Account-/Bank-/Oracle-State für diese Klassen;
- periodische Snapshots;
- Reconnect/Recovery/Reconciliation;
- Bootstrap;
- C0/P1/H0;
- Shadow-/Evidence-Vergleich.

Production-Vertrag:

```text
WSS incremental updates
+ periodischer Snapshot
+ gap detection
+ reconcile
+ fail closed bei unbekannter/staler Coverage
→ Long-Tail/Premium State gültig
```

Diese Freigabe ist bewusst wirtschaftlich: Die relevante Zeitskala für Tag-/Premium-Opportunities ist typischerweise wesentlich größer als der Race-Pfad. Daher darf eine kostenlose WSS/RPC-Quelle diese Execution-Klassen tragen, solange State-Freshness und Coverage gemessen ausreichen.

**WSS darf niemals stillschweigend Race-LIVE speisen.**

## S1.4 Warum Geyser für Race

Der Architekturgrund ist **Quellenlatenz**, nicht die Behauptung, JSON-Parsing allein sei langsam.

```text
Standard WSS:
validator / RPC-internal state
→ RPC serving layer
→ JSON-RPC/WebSocket framing
→ client

Geyser:
validatornahe account/tx/slot events
→ gRPC/Protobuf
→ client
```

Der relevante Vorteil ist, dass Geyser näher an der Quelle sitzt und weniger Serving-/Polling-/RPC-Pfad vor dem Client hat. Lokale JSON-Parsingkosten werden gemessen, aber nicht als Hauptbegründung verwendet.

## S1.5 Interne Fast Lane

Für Race nach dem Netzwerk-Decode:

```text
BinaryGeyserSource
→ fixed/native Rust event
→ bounded/preallocated queue
→ state owner
→ threshold/health/economics
```

Regeln analog BSC:

- bounded;
- sequence/provenance;
- kein common-path lock wenn vermeidbar;
- kein stiller overwrite;
- overflow/gap explizit;
- keine DB/File-I/O;
- kein synchroner External Lookup;
- keine unnötige interne JSON-Serialisierung.

SPSC wird dort bevorzugt, wo Ownership tatsächlich 1 Producer / 1 Consumer erlaubt; andernfalls eine gemessene bounded Rust-Struktur mit gleicher Evidenzdisziplin.

## S1.6 Kosten-/Provider-Regel

Open-Source-Code für Yellowstone/gRPC/Protobuf erzeugt keine notwendige monatliche Gebühr.

Providerentscheidung:

```text
kostenloser/bereits verfügbarer Mainnet Binary-Source
→ zuerst qualifizieren

kein kostenloser geeigneter Binary-Source
→ Race-LIVE bleibt gesperrt
→ Long-Tail/Premium-LIVE nutzt WSS/RPC

Paid Binary Feed
→ erst wenn incremental_net_profit > full_cost + dependency_risk
```

Damit kann der kostenlose Long-Tail-/Premium-Betrieb den wirtschaftlichen Nachweis bzw. das Kapital für eine spätere Race-Infrastruktur liefern, ohne eine zweite Codebasis zu erzeugen.

## S1.7 v1

Bei aktuellem Yellowstone-/Proto-Stand muss v1 verlustfrei erhalten bleiben.

Mindestens prüfen:

- `Message.config`;
- version;
- transaction bytes;
- account list;
- resource config;
- priority fee fields;
- keine v1→v0-Degradation.

`versioned = true` allein reicht nicht zur sicheren v0/v1-Unterscheidung.

## S1.8 Recovery

Binary Source mit Replay:

```text
trusted slot
→ replay
→ canonical reconcile
```

WSS/RPC Production-State für Long-Tail/Premium:

```text
gap / reconnect / uncertainty
→ Execution-Klasse HALT
→ Snapshot
→ reconcile
→ state valid
→ Long-Tail/Premium wieder freigeben
```

Unreconciled State ist nie tradable.

## S1.9 Messung

Für `BinaryGeyserSource`:

- source→`T_rx`;
- decode/mapping latency;
- p50/p95/p99/max;
- events/s;
- bytes/s;
- gaps/reconnect;
- v1 fidelity;
- CPU/Memory;
- Kosten.

Für `WsRpcControlSource`:

- update age / state freshness;
- subscription coverage;
- bytes/credits/requests;
- gap/reconnect;
- snapshot duration;
- reconcile duration;
- Long-Tail/Premium missed-opportunity rate;
- Kosten.

## Gate S1

S1 hat zwei getrennte Freigaben:

```text
S1-LT PASS:
    WsRpcControlSource + Snapshot/Reconcile ist für Long-Tail/Premium korrekt

S1-RACE PASS:
    qualifizierter Mainnet BinaryGeyserSource vorhanden
```

`S1-LT PASS` darf vor `S1-RACE PASS` erreicht werden. Das ändert **nicht** die Codearchitektur, sondern nur welche Execution-Klasse LIVE freigegeben ist.

Tickets:
`S1-01 IngressSource/native event model` ·
`S1-02 BinaryGeyserSource` ·
`S1-03 WsRpcControlSource` ·
`S1-04 Long-Tail/Premium snapshot+reconcile` ·
`S1-05 binary fast lane/queues` ·
`S1-06 v1 fidelity` ·
`S1-07 reconnect/gap/reconcile` ·
`S1-08 Mainnet binary source + cost qualification`

---
# PHASE S2 — FINAL CANONICAL SOLANA STATE

## Ziel

Korrekter fork-aware Solana-State im RAM.

## State

Mindestens:

- account data;
- slot;
- write_version;
- parent slot;
- root;
- fork/provenance;
- confirmation;
- blockhash cache;
- LUT cache;
- mint metadata;
- token metadata;
- transaction metadata.

## Bootstrap

Verbindlich:

```text
Stream starten
→ Updates puffern
→ Snapshot mit Context Slot S
→ Snapshot installieren
→ gepufferte canonical Updates > S anwenden
→ reconcile
→ state valid
→ Trading freigeben
```

## Reconnect

Geyser mit Replay:

```text
trusted slot
→ replay
→ fork-aware reconcile
```

WS ohne Replay:

```text
HALT
→ Snapshot
→ buffered deltas
→ reconcile
→ state valid
```

## Fork-Regel

`write_version` ordnet Writes.

`write_version` bestimmt **nicht allein** den canonical Fork.

## Gate S2

PASS nur wenn:

- kein Snapshot/Delta-Race;
- Fork/Root korrekt;
- Recovery deterministisch;
- unreconciled State niemals tradable.

---

# PHASE S3 — FINAL SOLANA EXECUTION CORE

## Ziel

Finaler nativer Build-/Sign-/Submit-/Reconcile-Pfad.

## Bau

- legacy/v0 Builder;
- v1 Builder;
- LUT für v0;
- v1 TransactionConfig;
- Ed25519 Signing;
- Serialization;
- RPC Adapter;
- Jito Adapter;
- Bundle Adapter;
- Submission Result Taxonomy;
- Landing Reconciliation.

## v1

Unterstützen:

- bis 4096 Byte;
- max. 64 Accounts entsprechend aktuellem Format/Runtime-Capability;
- keine LUTs;
- explizites CU-Limit;
- explizites Loaded-Account-Data-Limit;
- Priority Fee in totalen Lamports;
- ComputeBudget-Instructions nicht als v1-Konfiguration;
- Reader Version 1;
- base64-/Submission-Kompatibilität für große TX.

## v0/v1 Auswahl

Kein permanentes Default-Dogma.

Pro Execution-Klasse messen:

```text
v0:
bytes
accounts
LUT
CU
landing
latency
failure
cost

v1:
bytes
accounts
resource config
CU
landing
latency
failure
cost
```

Dann auswählen.

## Jito Capability Gate

Vor LIVE-Nutzung eines konkreten v1-/Bundle-Pfads:

- sendTransaction akzeptiert;
- Bundle akzeptiert;
- >1232 Byte überlebt vollständigen Pfad;
- keine Truncation;
- config bleibt korrekt;
- Failure-Verhalten bekannt;
- Fallback deterministisch.

## Gate S3

Mindestens ein vollständiger Pfad:

```text
build
→ sign
→ submit
→ observe
→ reconcile
```

reproduzierbar korrekt.

---

# PHASE S4 — CORE PARITY + PERFORMANCE BASELINE

## Ziel

Semantische Korrektheit des neuen Rust-Cores beweisen.

## Referenzen

- vorhandene Captures;
- bekannte Solana-TXs;
- Eva-Verhalten, wo geeignet;
- offizielle Crates;
- Staging.

## Vergleich

- Decode;
- State Transitions;
- Account Layout;
- TX Build;
- Serialization;
- Signatures;
- relevante Fees;
- Timing.

Pump.fun-spezifische Curve-/Quote-Parity nur, wenn für die Pump.fun-Linie benötigt.

## Performance-Baseline

Für jede Kernstufe:

```text
p50
p95
p99
max
count
allocations
queue_wait
```

## Gate S4

Keine ungeklärte semantische Abweichung.

---

# MERGE GATE — PROJECT-0 BUILD FREIGABE

P2 startet, wenn mindestens:

- P0 Reuse-/Lizenzentscheidung ausreichend;
- P1A aktueller LIVE-Stand eingefroren;
- P1B Upgrade-Pfad vorbereitet;
- C0 Marktklassen verstanden;
- H0 reale Execution-/Competition-Daten ausreichend;
- S0–S4 stabil genug als gemeinsames Fundament.

Es gibt **kein Long-Tail-only Gate** und **kein Race-only Gate**.

---

# PHASE P2 — FINAL PROJECT-0 STATE ENGINE

## Ziel

Alle Project-0-relevanten Daten vollständig und inkrementell im Rust-RAM.

## State

- MarginfiAccount;
- Bank;
- Group;
- FeeState;
- Oracles;
- Integration State;
- Liquidation Records;
- Token State;
- Version Metadata.

## Reverse Indices

Mindestens:

```text
Oracle → Banks
Bank → Accounts
Account → Banks
Integration → Banks
```

## Threshold Index

Pro relevanter Bank/Preisdimension:

- Kandidaten nach Liquidationsschwelle indexieren;
- Oracle Update → betroffene Kandidaten;
- exakte Health nur dort, wo nötig.

Kein globaler Full Scan.

## Premium Scheduler

Nur Accounts, deren Health durch laufende Premium-Akkumulation zeitabhängig wird.

```text
PREMIUM_ACTIVE liability
→ next health boundary
```

Bank-Flag-Transitions aktualisieren Membership.

## Tag Scheduler

Für getaggte Accounts:

```text
liquidation_tagged_at
+ Premium-Kurve
+ Economics
→ t_profitable
```

Reagiert auf:

- Tag gelöscht;
- Timer reset;
- Health-Änderung;
- Asset-/Debt-Preis;
- Fee-/Route-Änderung.

## TX Template Cache

Für relevante Near-Threshold-/Near-`t_profitable`-Kandidaten:

- Accounts;
- Refresh-Skelett;
- Route-Skelett;
- Integration Accounts;
- Instruction-Skelett.

Bei Trigger nur dynamische Teile patchen.

## Gate P2

PASS nur wenn:

- kein Full Scan;
- alle drei Trigger denselben State speisen;
- Indices korrekt;
- Scheduler korrekt;
- keine stale/unreconciled Opportunity handelbar.

---

# PHASE P3 — FINAL PROJECT-0 HEALTH ENGINE

## Ziel

Health exakt wie deployed Project 0.

## Bevorzugter Weg

```text
USE_CRATE
```

`marginfi-v2` ist Apache-2.0 lizenziert. Die Lizenz ist daher **kein offenes Gate mehr**.

`USE_CRATE` ist der Default, sofern der deployed Tag technisch eingebunden werden kann:

- deployed Tag pinnen;
- Client-/No-entrypoint-Features qualifizieren;
- State aus unserem RAM zuführen;
- keine eigene Risk-Formel bauen.

## Belegte Spot-/EMA-Semantik der Receivership

Der offizielle `LiquidationCache` dokumentiert:

```text
asset_value_maint      → Maintenance + Confidence Adjustments → SPOT
liability_value_maint  → Maintenance + Confidence Adjustments → SPOT

asset_value_equity     → unweighted Equity + Confidence Adjustments → EMA
liability_value_equity → unweighted Equity + Confidence Adjustments → EMA
```

**Wichtig:** Aus dem Kommentar im `LiquidationCache` wird **keine** vereinfachte Formel
`liability_value_equity - asset_value_equity` als Seize-Grenze abgeleitet.

Die tatsächliche 0.1.12-Receivership-Regel wird aus
`programs/marginfi/src/instructions/marginfi_account/liquidate_end.rs`
des deployed/qualifizierten Tags gepinnt.

Für `mrgn-0.1.12-rc1` ist im Source belegt:

```text
pre_health
=
pre_asset_value_maint
- pre_liability_value_maint

seized
=
pre_asset_value_equity
- post_asset_value_equity

repaid
=
pre_liability_value_equity
- post_liability_value_equity

extracted
=
max(0, seized - repaid)
```

Bei einer normalen Receivership-Liquidation gelten zusätzlich getrennte Protocol-Grenzen:

```text
1. Premium-Cap:
   seized <= repaid * (1 + allowed_premium)

2. Health-Loss-Cap:
   pre_health - post_health <= extracted

3. Solvency-Preservation:
   wenn pre_asset_value_equity >= pre_liability_value_equity,
   dann muss auch gelten:
   post_asset_value_equity >= post_liability_value_equity
```

Damit ist **BELEGT**:

- Maintenance-Snapshots verwenden SPOT;
- Equity-Snapshots verwenden EMA;
- `seized` und `repaid` werden aus den jeweiligen Pre/Post-Equity-Werten bestimmt;
- der erlaubte Premium wird separat auf `seized / repaid` begrenzt;
- ein durch die Liquidation verursachter Health-Verlust darf den extrahierten Wert `max(0, seized-repaid)` nicht übersteigen;
- ein zuvor solventer Account darf durch die Receivership nicht in Bad Debt gedrückt werden.

Diese Regeln werden weder durch eine globale „Spot/TWAP“-Faustregel noch durch eine vereinfachte Equity-Differenz ersetzt.

## Fallback

Nur falls direkte Nutzung nicht möglich:

```text
REIMPLEMENT
```

Dann exakte Matrix:

- Initial;
- Maintenance;
- Equity;
- RealTime;
- TimeWeighted;
- Live;
- Cached;
- PriceBias;
- Confidence;
- Freshness;
- EMode;
- Premium Projection;
- Bad Debt;
- Integrationsmultiplikatoren;
- costly positions;
- pre-/post-Receivership;
- Fixed-Point/Rundung.

Parity gegen offizielles Crate.

## 0.1.12

Zusätzlich exakt:

- projizierte Borrow Premium;
- `PREMIUM_ACTIVE`;
- neue Receivership-Health-Regel;
- `EMODE_BOOSTED`;
- Bad-Debt-Grenze.

## Gate P3

Jede ungeklärte Healthy/Liquidatable-Abweichung:

```text
STOP
```

---

# PHASE P4A — DETERMINISTIC PROFIT ENGINE

## Ziel

Jede Opportunity nach real ausführbarem Netto-Profit bewerten.

## Realisierbarer Wert

Nicht:

```text
Protocol Oracle Value = unser Profit
```

Sondern:

```text
gross_seized_value_realizable
=
Wert des Assets über den tatsächlich ausführbaren Pfad
zum aktuell realisierbaren Preis
```

Protocol-Oracle dient u. a. für:

- Eligibility;
- Health;
- Premium-Cap.

## Protocol-Seizure-/Premium-Cap: SPOT vs. EMA

Für Receivership wird die Protocol-Grenze exakt aus dem gepinnten `liquidate_end.rs` übernommen.

```text
Maintenance-Snapshots:
    SPOT

Equity-Snapshots:
    EMA

seized:
    pre_asset_value_equity - post_asset_value_equity

repaid:
    pre_liability_value_equity - post_liability_value_equity

extracted:
    max(0, seized - repaid)
```

Für `mrgn-0.1.12-rc1` muss der Planner/Economics-Pfad **alle** relevanten Grenzen gemeinsam erfüllen:

```text
seized <= repaid * (1 + allowed_premium)

pre_health - post_health <= extracted

wenn pre_equity solvent:
    post_equity muss solvent bleiben
```

Es gibt daher **keine einzelne naive Equity-Differenz-Formel**, die allein den zulässigen Seizure bestimmt.

Der **realisierbare Marktwert** bleibt davon getrennt: Protocol-Werte und Protocol-Checks bestimmen, was erlaubt ist; ausführbare Marktpreise bestimmen, was es uns wirtschaftlich wert ist.

## Deterministische Formel

```text
deterministic_net_profit
=
realizable_seized_value
- repay_cost
- protocol_fees
- flat_SOL_fee
- executable_swap_fees
- price_impact
- priority_fee
- jito_tip
- rent/token_cost
- path_dependent_failed_attempt_cost
```

Zusätzlich:

- required capital;
- profit/capital;
- capital usage.

## Long-Tail-Zeitdimension

```text
premium(t)
premium_cap(t)
deterministic_net_profit(t)

t_profitable
=
frühester Zeitpunkt,
an dem net_profit > konfigurierter Mindestprofit
und alle Safety-/Execution-Regeln erfüllt sind
```

`t_profitable` wird laufend neu bewertet, wenn Markt-/State-/Fee-Daten sich ändern.

## Race

Race Opportunity:

```text
State/Oracle Update
→ Threshold Candidate
→ Exact Health
→ Economics
→ sofortige Entscheidung
```

## Failed-Attempt-Matrix

Pfadabhängig:

```text
RPC:
not landed
landed success
landed fail

Jito single:
not landed
landed success
landed fail

Jito bundle:
not landed
landed success
bundle failed
```

Unbekannt = `UNKNOWN`, niemals 0.

## Gate P4A

Jede unterstützte Opportunity deterministisch und reproduzierbar bewertet.

---

# PHASE P5 — FINAL RECEIVERSHIP EXECUTION PLANNER

## Ziel

Aus einer wirtschaftlichen Opportunity einen exakt protocol-gültigen TX-Plan erzeugen.

## Allowlist

P1A ist authoritative für:

- Refresh Allowlist;
- Integration Allowlist;
- Swap Allowlist;
- Start-/End-Regeln.

Kein RouteProvider darf nicht erlaubte Programme einschleusen.

## Sequenz

Schematisch:

```text
[erlaubte Pre-Start Refreshes]
[Preisupdates falls erforderlich]
→ StartLiquidation
→ Withdraw / Integration Withdraw
→ Swap oder Repay aus Inventar
→ Repay
→ EndLiquidation
```

Die exakte Reihenfolge wird aus dem deployed Source gepinnt.

## End-Regel

Wenn deployed Source die bekannte Introspection-Regel bestätigt:

**EndLiquidation muss letzte Instruction der Receivership-TX sein.**

Danach kein:

- Cleanup;
- Transfer;
- ATA Close;
- ComputeBudget;
- sonstiger Program Call.

## Routing

Alle Pfade von Anfang an architektonisch vorhanden:

### `NO_SWAP`

```text
Repay aus eigenem Inventar
→ seized Asset behalten
→ spätere Verwertung außerhalb Receivership
```

Nur aktiv, wenn Inventar + Economics passen.

### `LOCAL_PRECOMPUTED_ALLOWED`

Nur für **wirklich lokale** Route-/Quote-Erzeugung:

```text
lokal gehaltener Pool-/Venue-State
→ lokale Quote
→ lokale Route/Instructions
→ kein zwingender Remote-Routing-Call
```

Aktuell ist in den geprüften Referenzen **kein solcher lokaler Router belegt**.

`solana-dex-superagg` gehört ausdrücklich **nicht** in diese Klasse.

### `FREE_PUBLIC_ALLOWED`

Remote Route Source ist hier erlaubt, wenn:

- aktuell keine laufenden Kosten entstehen;
- Rate Limits für unseren Pfad reichen;
- Output vollständig validiert wird;
- Programme allowlisted sind;
- Abhängigkeit und Latenz gemessen werden.

### `PAID_ALLOWED`

Remote Route Source / API / Key mit laufenden oder nutzungsabhängigen Kosten.

Nur wenn gemessener zusätzlicher Netto-Profit die vollständigen Kosten rechtfertigt.

## `solana-dex-superagg`

Klassifikation:

```text
REMOTE_AGGREGATOR_CLIENT
```

Die MIT-lizenzierte Rust-Library spricht Remote-Aggregatoren an und vergleicht deren Quotes:

- Jupiter;
- Titan;
- DFlow.

Sie ist **kein lokaler Router**.

Die konkrete Nutzung wird nach dem zugrunde liegenden Aggregator klassifiziert:

```text
aktuell kostenlos + Rate Limits ausreichend
→ FREE_PUBLIC_ALLOWED

laufende/Usage-Kosten
→ PAID_ALLOWED
```

P0 muss API-Key-Pflicht, aktuelle Free Tiers, Rate Limits, Kosten und Hotpath-Latenz **vor** Dependency-Freigabe prüfen.

Der einzige derzeit referenzseitig belegte router-freie Pfad ist:

```text
NO_SWAP
→ Repay aus Inventar
```

## Bundle

Prerequisite-TXs dürfen separat im Bundle liegen, wenn semantisch erlaubt.

`StartLiquidation` und `EndLiquidation` werden nicht über mehrere TXs gesplittet.

## Gate P5

Jede unterstützte Klasse:

- protocol-valid;
- allowlist-valid;
- simulierbar;
- v0/v1-planbar;
- Economics bekannt.

---

# PHASE T — TAGGER

## Ziel

Unhealthy, noch nicht wirtschaftliche Accounts früh markieren und denselben Account später automatisch bei `t_profitable` verarbeiten.

## Logik

```text
unhealthy
+ receivership-supported
+ aktuell nicht ausreichend profitabel
+ Tagging wirtschaftlich sinnvoll
→ marginfi_account_tag_liq_record
```

## Signaturpräzisierung

Die Project-0-Instruktion benötigt laut 0.1.12 keinen Protokoll-Signer.

Die Solana-Transaktion benötigt selbstverständlich einen Fee Payer / gültige Signatur.

## State Tracking

Speichern:

- Account;
- tag tx;
- tag slot;
- observed `liquidation_tagged_at`;
- `t_profitable`;
- Tag-Clear;
- Timer-Reset;
- Grund.

## Economics

Tagging-Kosten gehören in das Opportunity-Modell.

Kein kostenloser Fantasy-Wert.

## Gate T

- Tag korrekt gesetzt;
- Chain-State bestätigt;
- Scheduler aktualisiert;
- Fehlerfälle deterministisch;
- kein Doppelspam.

---

# PHASE P6 — FULL PRODUCTION SHADOW

## Ziel

Der **finale** Production-Code läuft vollständig gegen Mainnet, nur Submission mit Kapitalrisiko bleibt deaktiviert/abgesichert.

Kein Shadow-Sondercode.

## Pfad

```text
T_rx
→ Canonical State
→ Project0 State
→ Trigger
→ Exact Health
→ Candidate
→ P4A
→ Route
→ Plan
→ Build
→ Sign
→ Submission Decision
→ Landing Observation
→ Winner Comparison
```

Race und Long-Tail beide aktiv im Shadow.

## Messung

Pro Stufe:

- p50;
- p95;
- p99;
- max;
- count.

Pro Opportunity:

- type: RACE / TAG / PREMIUM_TIME;
- detect time;
- ready time;
- tx version;
- bytes;
- accounts;
- CU;
- route;
- path;
- tip;
- winner;
- landed/rejected/expired;
- estimated/actual fees;
- failure reason.

## Long-Tail-spezifisch

```text
unser t_profitable
vs.
Zeitpunkt, an dem Konkurrenz liquidierte
vs.
Premium der Konkurrenz
```

## Race-spezifisch

```text
Oracle/State event
→ unser detect/ready
→ Winner Slot/Index
→ Verlustgrund
```

## Loss Taxonomy

```text
INGRESS
STATE
HEALTH
ECONOMICS
ROUTING
BUILD
SIGN
SUBMISSION
TIP
STATE_RACE
THRESHOLD_TOO_HIGH
ECONOMIC_SKIP
UNSUPPORTED
UNKNOWN
```

## Gate P6

Finaler Code erkennt, bewertet, plant und misst beide Opportunity-Klassen vollständig.

---

# PHASE P7 — COMPETITION + PROVIDER ECONOMICS

## Ziel

Aus echten Daten bestimmen, was zusätzlichen Profit erzeugt.

## Pro Opportunity

- gesehen?
- wann?
- welcher Trigger?
- profitabel?
- wann?
- `t_profitable`?
- wann ready?
- wer gewann?
- Premium?
- Tag Age?
- Tip?
- Route?
- Submission Path?
- TX Version?
- warum verloren?
- welcher alternative Pfad wäre besser gewesen?

## Provider Economics

Für jede optionale bezahlte Komponente:

```text
incremental_net_value
=
zusätzlicher realisierter/realistisch attribuierbarer Profit
- Subscription
- Usage
- Hardware
- Traffic
- Storage
- Operations
- Abhängigkeitskosten
```

Nur bei positivem, relevantem Wert einsetzen.

## p(win)

Erst jetzt aus echten Daten:

```text
p(win |
  opportunity_class,
  premium,
  tag_age,
  tip,
  route,
  submission_path,
  tx_version,
  competition,
  timing)
```

Versioniert und Holdout-/Forward-validiert.

## Gate P7

- Competition ausreichend gemessen;
- Provider wirtschaftlich bewertet;
- p(win) belastbar oder bewusst nicht verwendet.

---

# PHASE P4B — PROBABILISTIC PROFIT OPTIMIZER

## Ziel

Deterministischen Profit mit empirischer Gewinnwahrscheinlichkeit verbinden.

## Formel

```text
expected_net_profit
=
p(win) * deterministic_profit_if_won
- expected_failed_attempt_cost
- marginal_provider_cost
- marginal_infrastructure_cost
```

## Erst jetzt optimieren

- Tip;
- Submission Path;
- v0/v1 Policy;
- Route;
- Kapitalallokation;
- Opportunity-Priorität;
- parallele Opportunities;
- act now vs. wait;
- Tag-Premium-Optimal-Stopping.

## Race + Long-Tail

Beide Klassen werden zusammen nach Expected Net Profit gerankt.

Kein separater Optimizer.

## Gate P4B

Nur Forward-/Shadow-validierte Policy darf LIVE.

---

# PHASE P8 — CONTROLLED LIVE

## Ziel

Echter finaler Code mit begrenztem Risiko.

## Kein P8-Sonderbot

Es läuft derselbe Production-Core.

## Konfigurierbare Limits

Keine Architekturkonstante wie „100 EUR“.

Runtime Config:

- max capital;
- max opportunity exposure;
- max daily loss;
- max failed attempts;
- max tip;
- max slippage;
- max outstanding inventory;
- max concurrent positions;
- allowlisted assets;
- allowlisted execution classes.

## Hard Halts

- feed gap;
- state gap;
- fork/reconcile failure;
- program upgrade;
- unknown layout;
- unknown oracle;
- unknown integration;
- unknown fee/economics;
- route violation;
- reconciliation failure.

## Reconciliation

Jeder Versuch:

```text
planned
→ signed
→ submitted
→ landed / not landed
→ instruction result
→ token deltas
→ fees
→ tips
→ inventory delta
→ capital delta
→ realized PnL
```

## Race und Long-Tail

Beide dürfen Controlled LIVE sein, sobald ihre jeweilige Execution-Klasse alle Gates erfüllt.

Kein künstliches sechsmonatiges Warten.

## Gate P8

- Reconciliation vollständig;
- Kosten erklärbar;
- Safety funktioniert;
- keine unbekannte Exposure.

---

# PHASE P9 — PRODUCTION LIVE

## Ziel

Dauerhaft maximalen nachhaltigen realisierten Netto-Profit erzielen.

## Hauptmetriken

```text
realized_net_pnl
profit_per_day
profit_per_opportunity
profit_per_capital

race_opportunities
race_capture_rate
race_win_rate

tagged_accounts
long_tail_opportunities
long_tail_capture_rate
t_profitable_error

failed_attempt_cost
priority_fees
jito_tips
swap_cost
price_impact
provider_cost
infrastructure_cost
capital_utilization
inventory_risk
```

## Provider Review

Regelmäßig:

```text
incremental_profit_attributable
vs.
full_cost
```

Negativ:

```text
entfernen / downgraden
```

## LIVE bleibt nur aktiv solange

- Protocol-Version qualifiziert;
- State gesund;
- Health parity gültig;
- Economics vollständig;
- Reconciliation stimmt;
- Safety Gates funktionieren.

---

# PHASE K — MEASURED CONTINUOUS OPTIMIZATION

K bedeutet **kein Rewrite**.

K optimiert denselben finalen Code.

## Ingress

Wenn `LOSS_REASON = INGRESS`:

prüfen, in dieser Reihenfolge:

- besserer kostenloser Mainnet-Binary-Feed;
- vorhandener Geyser-Zugang;
- Paid Geyser;
- eigener Geyser / validatornaher Aufbau;
- eigener Node;
- Region;
- **erst danach Shred/Deshred/native Shred Delivery**, wenn Geyser nachweislich nicht reicht.

Shred/Deshred ist **keine S1-Pflicht** und kein prophylaktischer Build. Es wird nur aktiviert, wenn reale Race-Verluste dem Ingress zugerechnet werden und der erwartete zusätzliche Netto-Profit die vollständigen Shred-/Validator-/Netzwerkkosten rechtfertigt.

Nur bei messbarem Netto-Mehrwert.

## CPU

Wenn `LOSS_REASON = CPU/BUILD`:

```text
Profiling
→ Data Layout
→ Allocation Tuning
→ Cache Locality
→ Lock Reduction
→ CPU Affinity
→ Sharding / Parallelism
```

## Routing

Wenn `LOSS_REASON = ROUTING`:

1. zuerst router-freien Pfad prüfen:
   - Inventar / `NO_SWAP`;

2. Remote-Routing optimieren:
   - Route Cache;
   - Quote Cache nur soweit semantisch sicher;
   - `solana-dex-superagg` oder direkte Jupiter/Titan/DFlow-Anbindung nur als **Remote-API-Pfad**;
   - Provider-/API-Kosten gegen zusätzlichen Profit messen;

3. wirklich lokales Routing nur bei nachgewiesenem Bedarf:
   - lokale Pool-/Venue-Daten;
   - lokale Quote;
   - lokale Route/Instructions;
   - nur für Project-0-allowlisted Programme.

Aktuell ist in den geprüften Referenzen **kein lokaler Router belegt**. Ein solcher wird nicht prophylaktisch neu gebaut; erst ein gemessener Routing-Verlustgrund rechtfertigt die zusätzliche Komplexität.

## Submission

Wenn `LOSS_REASON = SUBMISSION/TIP`:

- Jito Region;
- Tip Policy;
- Submission Redundancy;
- Bundle Policy;
- RPC vs. Jito.

## Transaction Version

v0/v1 dauerhaft anhand realer Daten vergleichen.

Keine ideologische Bindung.

---

# 8. Verbindliche Ticket-Reihenfolge

## Sofort parallel

```text
P0-01 eva01 full read-through
P0-02 mary read-through
P0-03 dependency/reuse matrix
P0-04 license matrix
P0-05 remote routing/API key/free-tier/cost audit
P0-06 oracle model
P0-07 staging model

P1A-01 current ProgramData
P1A-02 current source/IDL manifest
P1A-03 current allowlists
P1A-04 upgrade watcher

P1B-01 0.1.12 manifest
P1B-02 dual-size bank decoder tests
P1B-03 promotion procedure

C0-01 account census
C0-02 health classification
C0-03 race opportunity map
C0-04 long-tail/t_profitable simulation

H0-01 historical extractor
H0-02 winner matcher
H0-03 execution classifier
H0-04 tag timeline after 0.1.12
H0-05 competitor min-profit inference
H0-06 report
```

## Native Core

```text
S0-01 workspace
S0-02 timing
S0-03 telemetry
S0-04 bounded channels/lifecycle
S0-05 journal/replay

S1-01 IngressSource/native event model
S1-02 BinaryGeyserSource
S1-03 WsRpcControlSource
S1-04 Long-Tail/Premium snapshot+reconcile
S1-05 binary fast lane/queues
S1-06 v1 fidelity
S1-07 reconnect/gap/reconcile
S1-08 Mainnet binary source + cost qualification

S2-01 canonical state
S2-02 fork/root
S2-03 snapshot bootstrap
S2-04 recovery
S2-05 blockhash/LUT/token metadata

S3-01 v0 builder
S3-02 v1 builder
S3-03 signer
S3-04 RPC submission
S3-05 Jito submission
S3-06 bundle
S3-07 landing reconciliation

S4-01 parity corpus
S4-02 decode/state parity
S4-03 tx parity
S4-04 performance baseline
```

## Project 0

```text
P2-01 Project0 RAM model
P2-02 reverse indices
P2-03 threshold index
P2-04 premium scheduler
P2-05 tag scheduler
P2-06 tx template cache

P3-01 crate integration / exact fallback
P3-02 premium health
P3-03 receivership rule
P3-04 integration parity
P3-05 parity corpus

P4A-01 realizable value
P4A-02 fee model
P4A-03 failed-attempt matrix
P4A-04 t_profitable
P4A-05 capital model
P4A-06 ranking

P5-01 allowlist enforcement
P5-02 refresh planner
P5-03 NO_SWAP
P5-04 local/precomputed route
P5-05 free route
P5-06 receivership builder
P5-07 staging fixtures

T-01 tag transaction
T-02 tag state tracking
T-03 tag economics

P6-01 E2E telemetry
P6-02 race shadow
P6-03 long-tail shadow
P6-04 winner comparison
P6-05 loss classifier

P7-01 competition replay
P7-02 provider economics
P7-03 v0/v1 analysis
P7-04 race curve
P7-05 long-tail competition curve
P7-06 p(win)

P4B-01 expected profit
P4B-02 tip/path policy
P4B-03 waiting policy
P4B-04 capital allocation
P4B-05 forward validation

P8-01 safety gates
P8-02 wallet/capital limits
P8-03 staging live
P8-04 controlled mainnet
P8-05 reconciliation

P9-01 production metrics
P9-02 provider review
P9-03 inventory review
P9-04 protocol-upgrade watch
```

---

# 9. Non-Goals

Nicht vorgesehen:

- Wegwerf-Prototyp;
- zweiter Race-Core;
- zweiter Long-Tail-Core;
- Python im LIVE-Hotpath;
- JSON/WSS als normativer Production-Race-Hotpath; Long-Tail/Premium-LIVE über qualifiziertes WSS/RPC bleibt ausdrücklich erlaubt;
- interne JSON-Serialisierung zwischen latency-kritischen Rust-Stufen;
- Nachbau vorhandener funktionierender Komponenten ohne Grund;
- Nachbau der Risk Engine bei nutzbarem offiziellen Crate;
- vereinfachte Health-Formel;
- Full Scan pro Oracle Update;
- DB im Hotpath;
- Provider-zentrierte Businesslogik;
- Paid Provider ohne Profitnachweis;
- eigener Node aus Prestige-/Latenzgründen ohne Messung;
- feste „6 Monate warten“-Regel;
- feste Kapitalzahl als Architektur;
- v0 oder v1 als Dogma;
- nicht-allowlisted Programme im Receivership;
- unbekannte State-/Fee-Werte als 0;
- Codeübernahme ohne Lizenzprüfung.

---

# 10. Measurement Ledger

Alle Gates und empirischen Entscheidungen werden separat dokumentiert:

```text
docs/project_0/MEASUREMENT_LEDGER.md
```

Pflicht pro Eintrag:

```text
date
commit
phase/ticket
environment
provider
config hash
dataset/run id
measurement
sample count
p50/p95/p99/max
result
gate decision
evidence links
```

Architecture Claims gehören nicht als „gemessen“ ins Ledger.

---

# 11. Empfohlene Project-0-Dokumentstruktur

```text
docs/project_0/
  README.md
  PHASENPLAN.md
  MEASUREMENT_LEDGER.md

  manifests/
    CURRENT_MAINNET.md
    UPCOMING_0_1_12.md

  evidence/
    P0_REFERENCE_STUDY.md
    P0_REUSE_MATRIX.md
    P0_LICENSE_MATRIX.md
    C0_MARKET_CENSUS.md
    H0_LIQUIDATIONS.md

gates/project_0/
  P0.md
  P1A.md
  P1B.md
  C0.md
  H0.md
  S0.md
  S1.md
  S2.md
  S3.md
  S4.md
  P2.md
  P3.md
  P4A.md
  P5.md
  T.md
  P6.md
  P7.md
  P4B.md
  P8.md
  P9.md
```

---

# 12. Startstatus 2026-09-30

```text
ARCHITECTURE:
    FREEZE — AUTHORITATIVE v4.2

ONE PRODUCTION CORE:
    REQUIRED

RACE:
    INCLUDED FROM INITIAL DESIGN

LONG-TAIL:
    INCLUDED FROM INITIAL DESIGN

RUST HOTPATH:
    REQUIRED

PRODUCTION RACE INGRESS:
    QUALIFIED MAINNET BINARY / GEYSER REQUIRED

JSON / WSS RACE HOTPATH:
    FORBIDDEN

WS/RPC ROLE:
    CONTROL / BOOTSTRAP / RECOVERY / EVIDENCE
    + LONG-TAIL / PREMIUM LIVE ALLOWED WITH SNAPSHOT+RECONCILE

SHRED / DESHRED:
    NOT S1
    K-ONLY AFTER MEASURED INGRESS BOTTLENECK

PYTHON LIVE HOTPATH:
    FORBIDDEN

EVA01 REUSE:
    MANDATORY TO EVALUATE BEFORE REIMPLEMENTATION

EVA01 CODE COPYING:
    BLOCKED UNTIL LICENSE/FREIGABE IS CLEAR

MARGINFI CRATE REUSE:
    LICENSE CLEARED: APACHE-2.0
    USE_CRATE IS DEFAULT
    TECHNICAL TAG/API/PARITY QUALIFICATION REMAINS

SOLANA-DEX-SUPERAGG:
    LICENSE: MIT
    CLASS: REMOTE_AGGREGATOR_CLIENT
    NOT LOCAL ROUTING
    API-KEY/FREE-TIER/COST AUDIT REQUIRED IN P0

CURRENT MAINNET:
    VERIFY IN P1A

0.1.12:
    PRE-RELEASE
    UPGRADE MANIFEST TO BUILD
    MAINNET ETA 2026-10-12, EXACT TIME PENDING

SOLANA TX V1:
    ACTIVE
    MUST BE SUPPORTED

PAID PROVIDER:
    NONE REQUIRED BY ARCHITECTURE
    ONLY AFTER MEASURED POSITIVE INCREMENTAL NET VALUE

OWN NODE:
    NOT REQUIRED
    ONLY AFTER MEASURED ECONOMIC CASE

LIVE:
    NOT ENABLED
```

---

# 13. Verbindlicher Start

Ab jetzt parallel:

```text
P0  → eva01/mary vollständig analysieren, Reuse-/Lizenzmatrix und Remote-API-Kosten-Audit erstellen
P1A → aktuellen Mainnet-Stand on-chain einfrieren
P1B → 0.1.12 vollständig vorbereiten
C0  → Race + Long-Tail Markt quantifizieren
H0  → reale historische Liquidationen analysieren

S0  → finalen Rust-Workspace aufbauen
```

Danach:

```text
S1 → S2 → S3 → S4
```

und nach Merge Gate:

```text
P2 → P3 → P4A → P5 → T → P6 → P7 → P4B → P8 → P9 → K
```

**Keine zweite Entwicklung. Kein späterer Race-Rewrite. Kein späterer Rust-Rewrite.  
Von Anfang an ein schneller, schlanker, kostenoptimierter finaler Production-Core.**
