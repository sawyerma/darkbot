# DarkBot — Project 0 — Finaler Produktions-Phasenplan

Status: **ARCHITECTURE FREEZE — AUTHORITATIVE**
Stand: **2026-09-30**

Diese Datei ist die verbindliche technische Roadmap für den DarkBot Project-0-Liquidator.

Es gilt genau eine Produktionsimplementierung. Jede Phase erweitert denselben finalen Codepfad. Es gibt keinen Prototypen, der später neu geschrieben wird.

---

# 0. Primäres Produktziel

## 0.1 Zielfunktion

**Maximiere nachhaltigen realisierten Netto-Profit.**

Nicht optimiert wird auf:
- minimale lokale Latenz um jeden Preis;
- maximale Anzahl Liquidationen;
- maximale Anzahl Provider;
- maximale technische Komplexität;
- maximale Anzahl Abos;
- möglichst viele Features ohne wirtschaftlichen Nutzen.

Ökonomische Zielfunktion:

~~~text
REALIZED NET PROFIT
=
realisierter Liquidationserlös
- Repay-Kosten
- Project-0-Protokollgebühren
- Project-0-Flat-SOL-Fee
- Swap-/DEX-Gebühren
- realer Price Impact / Slippage
- Priority Fees
- Jito Tips
- Token-/Rent-Kosten
- Kosten fehlgeschlagener Versuche
- Infrastrukturkosten
- API-/SaaS-/Portal-Kosten
- vermeidbare Betriebskosten
~~~

Latenz, Capture Rate, Routing, Inclusion und Infrastruktur sind Mittel zur Profitmaximierung, nicht das Endziel.

## 0.2 Kostenregel

~~~text
kostenlos + technisch sinnvoll
→ direkt verwenden

laufende Zusatzkosten
→ nur verwenden, wenn der zusätzliche nachhaltige Netto-Profit
  die vollständigen Kosten und das Abhängigkeitsrisiko übersteigt
~~~

Kein kostenpflichtiger Provider wird allein aus Bequemlichkeit permanente Produktionsabhängigkeit.

Jeder kostenpflichtige Provider braucht:
- klaren Zweck;
- monatliche Kosten;
- Usage-Kosten;
- gemessenen technischen Nutzen;
- gemessenen zusätzlichen Profit;
- Ersatz-/Fallback-Pfad, soweit technisch möglich;
- Entfernungskriterium.

---

# 1. Harte Architekturregeln

## 1.1 Final-Implementation Rule

~~~text
zeitkritisch
→ direkt Rust

nicht zeitkritisch
→ Python erlaubt
~~~

Es gibt ausdrücklich nicht:

~~~text
Prototype
→ Rewrite
→ Production
~~~

Sondern:

~~~text
Phase 1 baut finalen Produktionscode
→ Phase 2 erweitert denselben Code
→ Phase 3 erweitert denselben Code
→ ...
~~~

## 1.2 Rust-Hotpath

Diese Komponenten werden von Anfang an in Rust implementiert:

- Solana Ingress;
- Yellowstone/Geyser Decode;
- Canonical State;
- RAM State;
- Fork/Root/Reconciliation;
- Project-0 Decoder;
- Reverse Indices;
- Premium Scheduler;
- Health Engine;
- Liquidation Detection;
- deterministische Economics;
- LIVE Opportunity Ranking;
- LIVE Routing-/Execution-Planung;
- Transaction Builder;
- v0/v1 Builder;
- Signing;
- Jito Submission;
- RPC Submission;
- Landing/Reconciliation;
- Hotpath Telemetrie.

Python ist erlaubt für:

- historische H0-Analysen;
- Account Census;
- große Offline-Auswertungen;
- Statistik;
- Reports;
- Plots;
- Shadow-Auswertung;
- p(win)-Modelltraining;
- Diagnose-/Audit-Werkzeuge.

## 1.3 Hotpath-Verbote

Im LIVE-Entscheidungspfad:

- keine Datenbank;
- kein synchroner RPC-Lookup;
- kein HTTP-Routing-Call als zwingende Voraussetzung, wenn lokal lösbar;
- kein Python;
- kein vollständiger Account-Scan;
- kein unbekannter State mit Fallback auf 0;
- keine vereinfachte Project-0-Health-Formel.

## 1.4 Fail Closed

Unbekannt bedeutet nicht 0 und nicht "wird schon stimmen".

Bei unbekanntem:
- Program-Stand;
- Layout;
- Oracle-Typ;
- Integrationstyp;
- Health-Semantik;
- Fee-Modell;
- Route;
- State-Gap;

gilt:

~~~text
HALT / SKIP
~~~

nicht:

~~~text
GUESS
~~~

## 1.5 Eine Implementierung

Kein zweiter:
- Trading-Core;
- Health-Core;
- Execution-Core;
- Economics-Core;
- Shadow-Core.

Tests, Replay, Shadow und LIVE verwenden dieselben Produktionskomponenten.

---

# 2. Finale Zielarchitektur

~~~text
                         SOLANA
                            │
                    Rust IngressSource
                            │
                 Canonical / Fork State
                            │
                     Project-0 RAM State
                            │
                      Reverse Indices
                            │
                    Premium Scheduler
                            │
                       Health Engine
                            │
                  Opportunity Detection
                            │
                    P4 Profit Engine
                            │
                      RouteProvider
                            │
               Receivership Execution Plan
                            │
                        TX Builder
                            │
                          Signer
                            │
                    SubmissionRouter
                     │             │
                    Jito          RPC
                     │             │
                     └──────┬──────┘
                            │
                         Landing
                            │
                     Reconciliation
                            │
                            ▼
                  REALIZED NET PROFIT
~~~

---

# 3. Phasenübersicht

## Track A — Finaler Native-Solana-Core

~~~text
S0  Final Native Foundation
 ↓
S1  Final Solana Ingress
 ↓
S2  Final Canonical Solana State
 ↓
S3  Final Solana Execution Core
 ↓
S4  Solana Core Parity
~~~

## Track B — Project-0-Protokoll und Evidence, parallel

~~~text
P1A  Current-Mainnet Source Freeze + Account Census
P1B  Upcoming-Release Freeze
H0   Historical Liquidation Intelligence
~~~

## Danach Zusammenführung

~~~text
P2   Final Project-0 State Engine
 ↓
P3   Final Project-0 Health Engine
 ↓
P4A  Deterministic Profit Engine
 ↓
P5   Final Receivership Execution Planner
 ↓
P6   Full Production Shadow Path
 ↓
P7   Competition + Provider Economics
 ↓
P4B  Probabilistic Profit Optimizer
 ↓
P8   Controlled LIVE
 ↓
P9   Production LIVE
 ↓
K    Continuous Production Optimization
~~~

---

# PHASE S0 — FINAL NATIVE FOUNDATION

## Ziel

Eine einzige finale Rust-Produktionsbasis schaffen, auf der die Solana-Produktionspfade aufgebaut werden.

Keine temporäre Runtime.

## Bau

Finaler Rust-Workspace mit klarer Trennung:

~~~text
common production infrastructure
solana infrastructure
pumpfun protocol
project0 protocol
bsc protocol
~~~

Gemeinsame Produktionskomponenten:

- Timing;
- Telemetrie;
- bounded channels;
- Lifecycle;
- Shutdown;
- Error Taxonomy;
- Config;
- Journal;
- Capture-Primitives;
- Replay-Primitives;
- Metrics;
- feste Ownership-Regeln.

Bestehender Gate-0-Code wird nur übernommen, wenn er wirklich Produktionscode ist oder ohne Rewrite Produktionscode werden kann.

Kein blindes Kopieren von Messharnesses in die Production Runtime.

## Timing Contract

Verbindliche Zeitpunkte:

~~~text
T_rx
T_state
T_detect
T_ready
T_sign
T_handoff
T_observed_landed
~~~

Landing Evidence umfasst zusätzlich:

~~~text
signature
landed_slot
tx_index_in_slot wo belastbar bestimmbar
confirmation status
submission path
bundle id falls vorhanden
~~~

## Messung

- Queue-Latenz;
- Timing-Overhead;
- Telemetry-Overhead;
- Allocation-Verhalten;
- Shutdown-Verhalten;
- deterministische Replay-Komponenten.

## Gate S0

PASS nur wenn:

- finaler Workspace steht;
- keine zweite Runtime entsteht;
- Timing API feststeht;
- Telemetrie API feststeht;
- bounded channels feststehen;
- Rust-Hotpath ohne Python möglich ist;
- spätere Solana-/Project-0-Phasen ohne strukturellen Rewrite darauf aufbauen können.

## Ergebnis

Finales Rust-Fundament.

---

# PHASE S1 — FINAL SOLANA INGRESS

## Ziel

Den finalen Solana-Datenpfad in Rust implementieren.

## Bau

Finales Interface:

~~~text
IngressSource
~~~

Produktionsadapter müssen austauschbar sein, ohne State, Health, Economics oder Execution zu verändern.

Unterstützte Architektur:

~~~text
IngressSource
├── Yellowstone/Geyser Provider
├── eigener Geyser / eigener Solana Node
└── weiterer qualifizierter Feed
~~~

### S1 Reference Adapter

Für S1 wird genau ein vollständig qualifizierter Reference-Adapter implementiert:

~~~text
QuickNode Yellowstone/gRPC
~~~

Grund:
- dokumentiertes Replay/fromSlot;
- konkrete v1-Message.config-Unterstützung;
- klar messbare Subscription-/Provider-Eigenschaften.

QuickNode ist **Reference-/Qualification-Adapter**, keine verpflichtende permanente Production-Abhängigkeit.

Wenn eine bereits verfügbare kostenlose Quelle dieselben Anforderungen erfüllt, kann sie direkt zusätzlich verwendet werden.

## Implementierung

Rust:

- Account Updates;
- Transaction Updates;
- Slot Updates;
- Root Updates;
- Write Version;
- Receive Timestamp;
- Stream Health;
- Reconnect;
- Gap Detection;
- Replay/fromSlot;
- ReplayInfo;
- v0/v1 Decode;
- strukturelle Erkennung von v1 Message.config;
- Subscription-Sharding-fähiges Layout;
- Provider Capability Reporting.

Historische und Streaming-Reader müssen v1 korrekt behandeln.

## Messung

Pro Ingress-Pfad:

~~~text
messages_per_second
decode_latency
T_rx
coverage
gap_count
reconnect_duration
replay_recovery
v1_message_config_fidelity
subscription_limits
monthly_cost
usage_cost
operational_cost
~~~

## Gate S1

PASS nur wenn:

- finaler Rust-Ingress stabil läuft;
- Provider nicht in Businesslogik eingebaut ist;
- v0/v1 korrekt unterschieden werden;
- Message.config bei v1 erhalten bleibt;
- Gap Detection funktioniert;
- Reconnect reproduzierbar ist;
- Replay-Recovery reproduzierbar ist;
- State vollständig aus dem Interface gespeist werden kann.

## Ergebnis

Finaler austauschbarer Rust-Solana-Ingress.

---

# PHASE S2 — FINAL CANONICAL SOLANA STATE

## Ziel

Den für Project 0 benötigten Solana-Zustand korrekt und vollständig im RAM halten.

## Bau

State umfasst mindestens:

- Accounts;
- Slot;
- Write Version;
- Parent Slot;
- Root;
- Fork State;
- Confirmation/Root Provenance;
- Blockhash Cache;
- Token Metadata;
- Mint Metadata;
- Transaction Metadata;
- LUT Metadata;
- Account-Version-Provenance.

## Bootstrap

Verbindlicher Ablauf:

~~~text
Stream starten
→ Updates puffern
→ Snapshot mit Context Slot S holen
→ Snapshot installieren
→ gepufferte canonical Updates > S anwenden
→ State bestätigen
→ Trading freigeben
~~~

## Reconnect

~~~text
letzter vertrauenswürdiger Slot
→ Replay sofern verfügbar
→ reconcile
→ state valid
~~~

Wenn Replay nicht sicher möglich ist:

~~~text
HALT
→ neuer Snapshot
→ reconcile
→ explizite Recovery
~~~

## Messung

- Snapshot-Dauer;
- Buffer-Größe;
- Reconcile-Dauer;
- Gap-Recovery;
- Fork-Wechsel;
- Root-Lag;
- RAM-Verbrauch;
- State-Update-Latenz.

## Gate S2

PASS nur wenn:

- kein Snapshot/Delta Race besteht;
- Fork/Root korrekt verarbeitet werden;
- Reconnect reproduzierbar ist;
- keine unbekannte State-Lücke weitergehandelt wird;
- unreconciled State niemals tradable ist.

## Ergebnis

Finaler canonical/fork-aware Solana RAM State.

---

# PHASE S3 — FINAL SOLANA EXECUTION CORE

## Ziel

Die finale native Solana-Ausführung implementieren.

## Bau

Rust:

- Transaction Builder;
- Legacy/v0 Builder;
- v1 Builder;
- LUT Handling;
- transaction-version-aware Resource Config;
- Compute Limits;
- Loaded Account Data Limits;
- Priority Fee;
- Serialization;
- Signing;
- Jito Adapter;
- RPC Adapter;
- Landing Reconciliation;
- Submission Result Taxonomy.

## Transaction-v1-Regeln

S3 muss die aktuell deployed Solana-v1-Semantik korrekt implementieren.

Zu qualifizieren:

- bis zu 4096 Byte v1;
- aktuelle Runtime-Account-Lock-Grenze als Cluster-Capability, nicht als permanente Konstante;
- keine LUTs in v1;
- v1 Compute Unit Limit explizit setzen;
- v1 Loaded Accounts Data Size Limit explizit setzen;
- v1 Priority Fee als totalen Lamport-Betrag behandeln;
- klassische ComputeBudget-Instructions nicht als v1-Resource-Konfiguration verwenden;
- >1232 Byte nur über einen dafür funktionierenden Submission-Pfad;
- historische Reader mit maxSupportedTransactionVersion=1 oder äquivalenter Unterstützung.

v0 und v1 werden nicht ideologisch vorentschieden.

## Jito / v1 Capability Gate

Für den exakten Produktionspfad real nachweisen:

1. RPC akzeptiert und landet v1;
2. Jito sendTransaction akzeptiert und landet v1;
3. Jito Bundle akzeptiert v1;
4. >1232-Byte-v1 überlebt den vollständigen Jito/BAM/Bundle-Pfad;
5. Message.config bleibt semantisch korrekt;
6. keine versteckte v1→v0-Degradation;
7. keine Truncation;
8. Failure/Fallback-Verhalten ist deterministisch.

Validator-Version allein gilt nicht als ausreichender Nachweis.

## Messung

Pro Execution-Klasse:

~~~text
transaction_version
serialized_bytes
account_count
compute_limit
compute_used
loaded_account_data_limit
priority_fee_model
LUT_usage
build_latency
sign_latency
submission_path
landed
rejected
expired
failure_reason
~~~

## Gate S3

PASS wenn mindestens ein vollständiger finaler Production-Pfad:

~~~text
build
→ sign
→ submit
→ observe
→ reconcile
~~~

korrekt und reproduzierbar funktioniert.

v1 darf erst Default werden, wenn sein kompletter Zielpfad qualifiziert ist.

## Ergebnis

Finaler Rust-Solana-Execution-Core.

---

# PHASE S4 — SOLANA CORE PARITY

## Ziel

Beweisen, dass der neue Rust-Solana-Core semantisch korrekt arbeitet.

Pump.fun dient hier ausschließlich als bereits vorhandene Referenz.

## Bau

Keine zweite Pump.fun-Engine.

Die bestehenden Referenzdaten werden durch denselben neuen Rust-Core verarbeitet.

Vergleich:

~~~text
bestehende Python-Referenz
vs.
neuer Rust-Production-Core
~~~

## Messung / Vergleich

- Decode;
- State Transitions;
- Bonding-Curve-Werte;
- Quotes;
- Fees;
- entscheidungsrelevante Werte;
- Timing Stages.

## Gate S4

PASS nur wenn:

- keine ungeklärte semantische Abweichung existiert;
- Differenzen entweder exakt erklärt oder behoben sind.

## Ergebnis

Verifizierter Solana-Core als Fundament für Project 0.

---

# PHASE P1A — CURRENT MAINNET SOURCE FREEZE + ACCOUNT CENSUS

P1A läuft parallel zu S1–S4.

## Ziel

Exakt einfrieren, was auf Project-0-Mainnet tatsächlich deployed ist.

Nicht GitHub-Ankündigungen, sondern der reale deployed Stand ist LIVE-Source-of-Truth.

## Bau / Freeze

Mindestens pinnen:

- Program ID;
- ProgramData / Upgrade Identity;
- deployed Release/Source;
- IDL;
- IDL Hash;
- Account Discriminators;
- Account Layouts;
- Account Sizes;
- Bank Layout;
- MarginfiAccount Layout;
- FeeState;
- Group;
- OracleSetup;
- Asset Tags;
- Integration Types;
- costly-position-Regeln;
- liquidation instructions;
- receivership rules;
- refresh allowlist;
- swap/integration allowlist;
- Flat-SOL-Liquidation-Fee;
- Premium-Aktivierungssemantik;
- Bank-Resize-/Version-Semantik.

Bei ProgramData-/Source-Änderung:

~~~text
PROGRAM_UPGRADE
→ EXECUTION HALT
→ neue Version identifizieren
→ neu qualifizieren
~~~

## Account Census

Offline/Python ist hier ausdrücklich erlaubt.

Ein vollständiger Mainnet-Census erfasst mindestens:

~~~text
total Project0 accounts
active accounts
total banks
active banks

OracleSetup distribution
asset-tag distribution
liability-bank distribution

PREMIUM_ACTIVE banks
accounts with PREMIUM_ACTIVE liabilities

accounts with:
- normal positions
- Kamino
- Drift
- Solend
- JupLend
- Staked collateral
- mixed integrations

costly-position-count distribution

fully supported accounts
unsupported/fail-closed accounts
unsupported by integration
unsupported by oracle/layout
~~~

Zusätzlich wirtschaftliche Abdeckung, soweit belastbar bestimmbar:

~~~text
supported active accounts / all active accounts
supported liability value / total liability value
~~~

## Messung

- Anzahl Accounts/Banks;
- Layout-Verteilung;
- Oracle-Verteilung;
- Integration-Verteilung;
- Premium-Aktivierung;
- adressierbarer Marktanteil;
- unbekannte/unsupported Klassen.

## Gate P1A

PASS nur wenn:

- aktuelle Mainnet-Version eindeutig identifiziert ist;
- alle für LIVE benötigten Protocol-Semantiken source-pinned sind;
- Account Census vollständig genug ist, um adressierbare und fail-closed Marktbereiche zu kennen.

## Ergebnis

Current-Mainnet Protocol Manifest + Account Census.

---

# PHASE P1B — UPCOMING RELEASE FREEZE

P1B läuft parallel zu P1A.

## Ziel

Kommende Project-0-Versionen vorbereiten, ohne sie vor Deployment als LIVE-Wahrheit zu behandeln.

## Bau

Für kommende Versionen separat pinnen:

- Source Commit/Tag;
- IDL;
- Layout-Änderungen;
- neue Oracle Types;
- neue Integrationen;
- Fee-/Premium-Änderungen;
- Receivership-Änderungen;
- Resize-Anforderungen;
- neue Tests.

## Promotion

~~~text
neue Version wirklich deployed
→ HALT
→ ProgramData bestätigen
→ Layout/Resize prüfen
→ Source/IDL bestätigen
→ Parity Suite
→ neue Version wird P1A
~~~

## Gate P1B

PASS bedeutet nur:

~~~text
upgrade-ready
~~~

nicht:

~~~text
LIVE-active
~~~

## Ergebnis

Vorbereiteter Upgrade-Manifest.

---

# TRACK H0 — HISTORICAL LIQUIDATION INTELLIGENCE

H0 läuft parallel zu S1–S4 und P1.

## Ziel

Historische reale Liquidationen und Konkurrenz verstehen, ohne einen zweiten Liquidator zu bauen.

## Bau

Offline-Extractor/Analyzer darf Python sein.

Alle historischen Solana-Reader müssen die auf Mainnet vorhandenen Transaction-Versionen korrekt verarbeiten.

## Pro Liquidation erfassen

Soweit belegbar:

- Signature;
- Slot;
- TX Index;
- Receiver/Winner;
- Liquidatee;
- Asset-/Debt-Paar;
- Seized;
- Repaid;
- aktuelle/zulässige Premium;
- Tag Age;
- Project-0-Protokollgebühren;
- Flat SOL Fee;
- Priority Fee;
- Jito Tip;
- Transaction Version;
- Serialized Bytes;
- Account Count;
- Compute Units;
- LUT Usage;
- Integration Positions;
- costly positions;
- Oracle Types;
- Refresh Instructions;
- Swap Program;
- Route;
- single TX / Bundle;
- Winner Concentration;
- wiederkehrende Receiver.

## Zweck

H0 liefert Evidenz für:

- reale TX-Komplexität;
- P5-Planbarkeit;
- Konkurrenzstruktur;
- Tip-Verhalten;
- Tagging-Verhalten;
- Account-Komposition;
- spätere p(win)-Modelle.

H0 erfindet keine Wahrscheinlichkeiten.

## Gate H0

PASS wenn genug belastbare Daten vorliegen, um:
- reale Execution-Klassen abzuleiten;
- typische Gewinnerpfade zu erkennen;
- Konkurrenzdaten für P7 vorzubereiten.

## Ergebnis

Historischer Evidence-Datensatz.

---

# PHASE P2 — FINAL PROJECT-0 STATE ENGINE

## Ziel

Project-0-State vollständig, inkrementell und effizient im Rust-RAM halten.

## Bau

State:

- Marginfi Accounts;
- Banks;
- Groups;
- FeeState;
- Oracles;
- Integration State;
- Liquidation Records;
- Token State;
- Project-0 relevante Version-/Config-Metadaten.

Reverse Indices mindestens:

~~~text
Oracle → Banks
Bank → Accounts
Account → Banks
Integration → Banks
~~~

Dadurch:

~~~text
Oracle Update
→ betroffene Banks
→ betroffene Accounts
~~~

Kein globaler Account-Scan im Hotpath.

## Premium Scheduler

Zeitgetriebene Rechecks werden nur für Accounts angelegt, deren Health durch Premium-Akkumulation tatsächlich zeitabhängig werden kann.

Scheduler-Bedingung:

~~~text
aktive Liability
+
zugehörige Bank hat PREMIUM_ACTIVE
+
Premium kann Health verändern
→ next_health_boundary berechnen
~~~

Nicht premium-aktive Liabilities erzeugen keinen sinnlosen Timer.

Bank-Flag-Änderungen müssen Scheduler-Membership aktualisieren:

~~~text
PREMIUM_ACTIVE OFF → ON
→ betroffene Borrower aufnehmen/rechnen

PREMIUM_ACTIVE ON → OFF
→ betroffene Borrower entfernen/rechnen
~~~

Der Scheduler verwendet Protocol-/Chain-Time-Semantik.

Kein periodischer Full Scan.

## Messung

- RAM-Verbrauch;
- Updates/s;
- Accounts pro Oracle Update;
- Banks pro Oracle Update;
- Health-Recheck-Queue;
- Scheduler-Größe;
- boundary accuracy;
- Update-Latenz;
- Flag-Transition-Verhalten.

## Gate P2

PASS nur wenn:

- alle relevanten Accounts warm gehalten werden;
- Updates inkrementell sind;
- Reverse Indices korrekt sind;
- Premium Scheduler nur relevante Accounts enthält;
- Flag-Änderungen korrekt propagieren;
- kein Full Scan im Hotpath stattfindet.

## Ergebnis

Finaler Project-0-State-Core.

---

# PHASE P3 — FINAL PROJECT-0 HEALTH ENGINE

## Ziel

Project-0-Health exakt wie das gepinnte offizielle Programm berechnen.

Keine vereinfachte Formel.

## Bau

Exakt pinnen und implementieren:

- RequirementType::Initial;
- RequirementType::Maintenance;
- RequirementType::Equity;
- OraclePriceType::RealTime;
- OraclePriceType::TimeWeighted;
- HealthPriceMode::Live;
- HealthPriceMode::Cached;
- PriceBias;
- Oracle Confidence;
- Oracle Freshness;
- EMode;
- Bad Debt;
- Premium Projection;
- Integration Multipliers;
- costly-position semantics;
- Pre-Liquidation;
- Post-Liquidation;
- Receivership Health Checks;
- Fixed-Point-/Rundungsregeln.

RequirementType, PriceType, PriceBias und HealthPriceMode werden nicht zu einer vereinfachten globalen "Spot/TWAP-Regel" zusammengezogen.

## Messung / Parity

Für reale und synthetische Accounts:

~~~text
Rust Project0 Health
==
gepinnte Protocol-Semantik
~~~

Verglichen werden:

- healthy/liquidatable;
- maintenance health;
- equity;
- liability projection;
- premium;
- bad debt;
- EMode;
- Integrationsfälle.

## Gate P3

Jede ungeklärte Klassifikationsabweichung:

~~~text
STOP
~~~

PASS nur bei vollständig erklärter/behobener Parity.

## Ergebnis

Finale Rust-Health-Engine.

---

# PHASE P4A — DETERMINISTIC PROFIT ENGINE

## Ziel

Liquidierbare Accounts nach echtem deterministischem Profit bewerten und ranken.

Vor P7 werden keine erfundenen Gewinnwahrscheinlichkeiten verwendet.

## Bau

P4A darf nicht erfinden:

~~~text
p(win)
p(land)
competitor response
optimal waiting time
~~~

## Realisierbarer Wert statt Oracle-Wert

Protocol-Oracle-/Equity-Werte entscheiden über:
- Eligibility;
- Premium-Cap;
- Protocol Health.

Unser Profit wird dagegen aus realisierbaren Werten berechnet.

Verbindlich:

~~~text
gross_seized_value_realizable
=
Wert, der über den ausgewählten real ausführbaren Pfad
zum aktuellen ausführbaren Marktpreis realisiert werden kann
~~~

Nicht:

~~~text
gross_seized_value = Protocol Oracle Value
~~~

Bei NO_SWAP muss separat definiert werden, wie gehaltenes Inventar für die Profitentscheidung bewertet wird.

## Deterministische Profitformel

~~~text
deterministic_net_profit
=
realizable_seized_value
- repay_cost
- Project0 protocol fees
- Project0 flat SOL fee
- executable swap/DEX fees
- real execution price impact
- priority fee
- Jito tip
- token/rent costs
- path-dependent failed-attempt cost
~~~

Zusätzlich:

~~~text
required_capital
profit_per_capital
capital_usage
~~~

## Failed-Attempt-Kosten

Keine globale Konstante.

Outcome-Matrix mindestens:

~~~text
RPC not landed
RPC landed success
RPC landed fail/revert

Jito single-TX not landed
Jito single-TX landed success
Jito single-TX landed fail/revert

Jito bundle not landed
Jito bundle landed success
Jito bundle failed/reverted
~~~

Unbekannte Kosten:

~~~text
UNKNOWN
~~~

niemals 0.

## V1 Handlungsregel vor P7

~~~text
deterministic_net_profit > 0
+ execution valid
+ state valid
+ safety valid
→ handeln
~~~

Nicht künstlich auf höhere Tagging-Premium warten.

Optimal Stopping kommt erst mit echten Konkurrenzdaten in P4B.

## Messung

- deterministic profit;
- realizable value;
- oracle-vs-realizable spread;
- required capital;
- profit/capital;
- Fee-Komponenten;
- path-dependent failed cost;
- Ranking-Stabilität.

## Gate P4A

PASS wenn jede unterstützte Opportunity deterministisch, reproduzierbar und ohne erfundene Wahrscheinlichkeiten bewertet und sortiert werden kann.

## Ergebnis

Finale deterministische LIVE-Profit-Engine.

---

# PHASE P5 — FINAL RECEIVERSHIP EXECUTION PLANNER

## Ziel

Aus einer profitablen Opportunity einen exakt protocol-gültigen ausführbaren Plan erzeugen.

## Äußere Constraint: Protocol Allowlist

Der RouteProvider steht **unter** der source-gepinnten Project-0-Allowlist.

Kein RouteProvider darf Instruktionen ausgeben, deren Top-Level-Programme innerhalb der Receivership nicht erlaubt sind.

Verbindlich aus P1A:

- Refresh Allowlist;
- Integration Allowlist;
- Swap-Program Allowlist;
- StartLiquidation-Regeln;
- EndLiquidation-Regeln;
- Flat-Fee-Semantik.

## Routing-Reihenfolge

### 0. NO_SWAP

Bevorzugter Kandidat, wenn wirtschaftlich sinnvoll:

~~~text
Repay aus eigenem Inventar
→ seized Asset behalten oder später außerhalb der Receivership verwerten
~~~

Vorteil:
- kein Router im kritischen Receivership-Pfad;
- weniger Accounts;
- weniger CU;
- weniger Failure Surface.

### 1. LOCAL_PRECOMPUTED_ALLOWED

Route/Instruktionen lokal vorberechnen.

Aber:

~~~text
lokal berechnet
!=
beliebiges Venue-Programm
~~~

Die tatsächlich ausgeführten Top-Level-Programme müssen in der gepinnten Project-0-Allowlist liegen.

### 2. FREE_PUBLIC_ALLOWED

Kostenlose Route Source darf genutzt werden, wenn:
- Output vollständig überprüft wird;
- Programme allowlisted sind;
- keine synchrone Portalabhängigkeit zwingend im Hotpath entsteht, sofern lokal vermeidbar.

### 3. PAID_ALLOWED

Nur wenn später P7 zeigt:

~~~text
zusätzlicher nachhaltiger Profit
>
vollständige Paid-Kosten
~~~

## Receivership-Sequenz

Finale Receivership-TX:

~~~text
[erlaubte Pre-Start-/Refresh-Instruktionen]
→ StartLiquidation
→ erlaubte Withdraw-/Integration-/Swap-/Repay-Instruktionen
→ EndLiquidation
~~~

## Absolute End-Regel

**EndLiquidation ist die letzte Instruction der gesamten Transaktion. Ohne Ausnahme.**

Danach darf nichts folgen:

- kein ATA Close;
- kein Transfer;
- kein Cleanup;
- keine ComputeBudget-Instruction;
- keine andere Program-Instruction.

Aufräumen:
- vorher, falls erlaubt;
- oder in einer Folge-TX.

## Bundle-Regel

Ein Bundle darf, soweit Protocol- und Submission-Semantik es erlauben:

~~~text
optionale prerequisite TXs
+
eine finale Receivership TX
~~~

Es ist verboten:

~~~text
TX1 StartLiquidation
TX2 Swap
TX3 EndLiquidation
~~~

StartLiquidation und EndLiquidation bleiben in derselben finalen Receivership-TX.

## Bau

Planner erzeugt mindestens:

- NO_SWAP Plan;
- allowlisted local/precomputed plan;
- allowlisted free route plan;
- optional paid route plan;
- erforderliche Refreshes;
- korrekte Accounts;
- korrekte Instruction-Reihenfolge;
- v0/v1-kompatiblen TX-Plan;
- optionalen prerequisite bundle plan.

## Messung

Pro Execution-Klasse:

- Route-Latenz;
- Route-Korrektheit;
- TX Bytes;
- Account Count;
- Compute Units;
- Loaded Account Data;
- Build-Latenz;
- Quote Quality;
- Price Impact;
- v0/v1 Machbarkeit;
- LUT Usage;
- single-TX/bundle feasibility;
- Allowlist Compliance;
- path-dependent Fee/Tip-Verhalten.

## Gate P5

PASS nur wenn:

- jede unterstützte Receivership-Klasse einen protocol-gültigen Plan erzeugt;
- EndLiquidation absolut letzte TX-Instruction ist;
- keine Route die gepinnte Allowlist verletzt;
- NO_SWAP als eigener Execution-Pfad unterstützt wird;
- Start/End niemals über mehrere TXs gesplittet werden.

## Ergebnis

Finaler Receivership Execution Planner.

---

# PHASE P6 — FULL PRODUCTION SHADOW PATH

## Ziel

Den finalen Produktionscode komplett gegen reale Mainnet-Ereignisse laufen lassen, bevor unqualifiziert Kapital riskiert wird.

Kein separater Shadow-Core.

## Pfad

~~~text
T_rx
→ Canonical State Update
→ Reverse Index / Premium Boundary
→ Exact Health
→ Candidate
→ P4A Deterministic Economics
→ Route Selection
→ Receivership Plan
→ Build
→ Sign
→ Submission Decision
→ Landing Observation
→ Winner Comparison
~~~

## Messung

Für jede Hotpath-Stufe:

~~~text
p50
p95
p99
max
sample_count
~~~

Zusätzlich:

- Transaction Version;
- Serialized Bytes;
- Account Count;
- CU;
- Route;
- Submission Path;
- Tip;
- landed/rejected/expired;
- Winner;
- Failure Reason;
- actual fees;
- actual tips.

Loss Taxonomy:

~~~text
INGRESS
STATE
HEALTH
ECONOMICS
ROUTING
BUILD
SUBMISSION
TIP
STATE_RACE
ECONOMIC_SKIP
UNKNOWN
~~~

## Gate P6

PASS wenn der finale Production-Code reale Liquidationsrennen vollständig:
- erkennt;
- bewertet;
- plant;
- timing-seitig misst;
- mit realen Gewinnern vergleicht.

## Ergebnis

Vollständig gemessener finaler Shadow-Pfad.

---

# PHASE P7 — COMPETITION + PROVIDER ECONOMICS

## Ziel

Mit realen historischen und Shadow-Daten feststellen, welche Entscheidungen tatsächlich den Netto-Profit maximieren.

## Für jede reale Opportunity

Beantworten:

- hätten wir sie gesehen?
- wann?
- war sie deterministisch profitabel?
- wann war unsere Execution bereit?
- wer gewann?
- welche Premium war verfügbar?
- wie alt war das Tag?
- welcher Tip wurde beobachtet/geschätzt?
- welcher Submission-Pfad?
- welche TX-Version?
- welche Route?
- warum verloren wir?
- welchen Profit hätte jeder qualifizierte Pfad erzeugt?

## Provider Economics

Für jede kostenpflichtige Option:

~~~text
incremental_net_value
=
zusätzlicher Profit durch zusätzlich gewonnene Opportunities
- Subscription
- Usage
- zusätzliche Infrastruktur
- zusätzliche Betriebs-/Abhängigkeitskosten
~~~

Regel:

~~~text
incremental_net_value > 0 und wirtschaftlich relevant
→ Provider darf bleiben

sonst
→ entfernen / nicht einführen
~~~

## p(win)

Erst hier darf aus echten Daten geschätzt werden:

~~~text
p(win |
  opportunity,
  premium,
  tag_age,
  tip,
  route,
  submission_path,
  competition)
~~~

Modelle werden versioniert und gegen Holdout/Forward-Daten geprüft.

## Messung

- Capture Rate;
- Win Rate;
- Winner Concentration;
- Tag-Age-Verteilung;
- Tip-Verteilung;
- Provider-Mehrwert;
- Competition-by-opportunity-class;
- realized/estimated opportunity value;
- Out-of-sample performance.

## Gate P7

PASS wenn:
- genügend empirische Konkurrenzdaten vorhanden sind;
- Provider wirtschaftlich bewertet sind;
- ein belastbares p(win)-Modell möglich ist oder bewusst darauf verzichtet wird.

## Ergebnis

Empirische Competition-/Provider-Evidence.

---

# PHASE P4B — PROBABILISTIC PROFIT OPTIMIZER

## Ziel

Deterministischen Profit mit empirischer Gewinnwahrscheinlichkeit verbinden.

Diese Phase startet erst nach P7.

## Formel

~~~text
expected_net_profit
=
p(win | opportunity, premium, tag_age, tip, route, path)
× deterministic_profit_if_won
- expected_failed_attempt_cost
- marginal_provider_cost
- marginal_infrastructure_cost
~~~

## Optimierung

Erst jetzt:

- Tip Size;
- Submission Path;
- Kapitalallokation;
- Opportunity Priority;
- act now vs wait;
- Tagging Premium;
- Optimal Stopping;
- parallele Opportunity-Auswahl.

Beispiel:

~~~text
jetzt bei +2 % Premium handeln
vs.
auf +5 % warten
aber steigendes Risiko, dass Konkurrenz vorher gewinnt
~~~

## Messung

- Expected vs realized profit;
- Calibration von p(win);
- Tip-Effizienz;
- Waiting-Policy;
- Opportunity Ranking;
- Kapitalauslastung;
- Out-of-sample Ergebnis.

## Gate P4B

PASS nur bei Shadow-/Forward-validierter Policy.

Keine probabilistische Policy geht allein aufgrund In-Sample-Daten LIVE.

## Ergebnis

Finaler empirischer Profit Optimizer.

---

# PHASE P8 — CONTROLLED LIVE

## Ziel

Erste echte Production-Ausführung mit begrenztem Kapital und vollständiger Reconciliation.

## Bau

Finaler Production-Code. Kein spezieller P8-Bot.

Limits:

- dedizierte Wallet;
- kleines Kapital;
- Max Opportunity Exposure;
- Max Daily Loss;
- Max Failed Attempts;
- Max Tip;
- Max Slippage;
- Max Outstanding Risk.

Hard Halts:

~~~text
feed gap
state gap
program upgrade
unknown layout
unknown oracle
unknown integration
unknown economics
reconciliation failure
~~~

## Reconciliation

Jeder Versuch:

~~~text
planned
→ submitted
→ landed / not landed
→ token deltas
→ fees
→ tips
→ capital change
→ realized PnL
~~~

## Messung

- Real Landing Rate;
- Real Win Rate;
- Real Fees;
- Real Tips;
- Real Failed Costs;
- Real Slippage;
- Realized Net PnL;
- Reconciliation Accuracy.

## Gate P8

PASS nur wenn:
- technische Reconciliation vollständig ist;
- alle realen Kosten erklärbar sind;
- keine unbekannte Exposure verbleibt;
- Safety Limits funktionieren.

## Ergebnis

Kontrolliert validierter LIVE-Pfad.

---

# PHASE P9 — PRODUCTION LIVE

## Ziel

Maximalen nachhaltigen realisierten Netto-Profit erzielen.

## Hauptmetriken

~~~text
realized_net_pnl
profit_per_day
profit_per_opportunity
profit_per_capital
capture_rate
win_rate
failed_attempt_cost
priority_fees
jito_tips
swap_cost
slippage
provider_cost
infrastructure_cost
capital_utilization
~~~

## Provider Review

Regelmäßig je kostenpflichtigem Dienst:

~~~text
incremental_profit_attributable_to_provider
vs.
full_provider_cost
~~~

Wenn nicht positiv:

~~~text
entfernen
~~~

## Gate P9

Production bleibt nur aktiv, solange:
- Protocol-Version qualifiziert ist;
- State gesund ist;
- Economics vollständig sind;
- Reconciliation stimmt;
- Safety Gates funktionieren.

## Ergebnis

Dauerhafte Production.

---

# PHASE K — CONTINUOUS PRODUCTION OPTIMIZATION

K bedeutet nicht, dass vorher Wegwerfcode gebaut wurde.

K optimiert denselben finalen Production-Code.

## CPU

Von S0 an:

~~~text
Rust
keine DB im Hotpath
keine Python-Grenze
bounded queues
wenig unnötige Allocations
kein unnötiges Locking
~~~

Wenn danach ein CPU-Bottleneck messbar ist:

~~~text
Profiling
→ Data Layout
→ Allocation Tuning
→ Cache Locality
→ CPU Affinity
→ Sharding / Parallelism
~~~

Kein Rewrite.

## Ingress

Wenn eine bessere kostenlose oder bereits vorhandene Lösung verfügbar ist:

~~~text
direkt qualifizieren und verwenden
~~~

Wenn eigener Geyser/Node zusätzliche Kosten erzeugt:

~~~text
zusätzlicher Netto-Profit
vs.
Hardware + Traffic + Storage + Betrieb + Redundanz
~~~

## Routing

Lokales/precomputed Routing ist von P5 an Teil der finalen Architektur.

Keine hart verdrahtete Paid-API, die später ersetzt werden muss.

## Submission

Kostenlose Jito-/Region-/Submission-Möglichkeiten, die technisch sinnvoll sind:

~~~text
direkt qualifizieren und verwenden
~~~

Zusätzliche kostenpflichtige Infrastruktur:

~~~text
nur wenn zusätzlicher nachhaltiger Netto-Profit > vollständige Kosten
~~~

---

# 4. Verbindliche Reihenfolge

~~~text
TRACK A
S0 → S1 → S2 → S3 → S4
                    │
TRACK B             │
P1A + P1B + H0 ─────┘
                    ↓
                   P2
                    ↓
                   P3
                    ↓
                  P4A
                    ↓
                   P5
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
~~~

P1A, P1B und H0 laufen parallel, soweit sie S0–S4 nicht blockieren.

---

# 5. Verbindliche Non-Goals

Nicht vorgesehen:

- Wegwerf-Prototypen;
- Python LIVE-Hotpath;
- Portal-zentrierte Architektur;
- bezahlte Provider ohne Profitnachweis;
- zweiter Health-Core;
- zweiter Execution-Core;
- vereinfachte Project-0-Risk-Formel;
- Full-Account-Scan pro Oracle Update;
- blindes Warten auf Tagging-Premium vor P4B;
- v1 als Default ohne End-to-End-Capability-Gate;
- beliebige DEX-Programme innerhalb Receivership;
- Instruction nach EndLiquidation;
- StartLiquidation und EndLiquidation in verschiedenen Transaktionen.

---

# 6. Source-of-Truth-Regel

Für Project 0 gilt:

1. diese Datei definiert die Phasen und Architektur;
2. P1A definiert den jeweils LIVE-fähigen deployed Protocol-Stand;
3. das Measurement Ledger dokumentiert Messungen und Gates;
4. alter Project-0-Roadmap-Text außerhalb von docs_project_0 ist nicht authoritative.

Messungen gehören in:

[MEASUREMENT_LEDGER.md](MEASUREMENT_LEDGER.md)

---

# 7. Aktueller Startstatus

Stand 2026-09-30:

~~~text
Project-0 Production Code:       NOT STARTED
Architecture:                    FROZEN BY THIS PLAN
Current Mainnet Manifest:        TO BUILD IN P1A
Account Census:                  TO BUILD IN P1A
Historical H0 Dataset:           TO BUILD
Permanent Paid Provider:         NONE APPROVED
Probabilistic Ranking:           NOT ALLOWED BEFORE P7
LIVE:                            NOT ENABLED
~~~

Die Umsetzung beginnt mit S0 sowie parallel P1A/P1B/H0 entsprechend der oben definierten Reihenfolge.
