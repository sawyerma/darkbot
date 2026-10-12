> **AKTUELLER C0-E1 ASTRA-MASTERAUFTRAG (2026-10-11):** Siehe den neuen verbindlichen Abschnitt „C0-E1 — ASTRA MASTER IMPLEMENTATION CONTRACT 2026-10-11“ am Dokumentende. Der fehlgeschlagene 5h-Run (527 GAPs, langanhaltende 429, 15.960s fehlende Slot-Evidence, 0 kanonische Blocks) ist analysiert. Implementierungs- und Mainnet-Requalifikation sind OFFEN; LIVE/Submission gesperrt. Ältere C0-E1-Handoffs sind historische Evidence, kein erneuter Run-Befehl.

> 2026-10-09: canonical full-slot scanner and typed four-path decoder implemented locally; master remains PARTIAL. Current resume/evidence and exact remaining steps are at the top of MEASUREMENT_LEDGER.md. No 4h qualification or LIVE release.

# DarkBot — Project 0 — Produktions-Phasenplan v4.5

### C0-E1 current gate update — 2026-10-10 19:57 Europe/Berlin (newer than earlier historical entries)

The current user-observed work is on `project0/c0e1-startup-wip-20261010`, base commit `c2ba995e647296d1c3d3b0a134d2f30ff8db1d64`; `main` remained on `3fdf30b` at this checkpoint. Startup fixes: `base64+zstd` snapshots, actual Shadow READY gate before the market measurement clock, in-window evaluation required for technical PASS, and cancellable fail-closed recovery. Two test-only local corrections compile and pass. Solana State 19/19; Runner both `crate_health` and `crate_health txv1` exit0; release build exit0; Clippy exit0 with reported warnings. These are **local gates**, not certification of a full Mainnet campaign.

Run `c0e1_gap_smoke_20261010T195016Z` (60-second capture, longer walltime) has demonstrated actual Shadow snapshot A/B + root catch-up `Ready` *before* market observation; first RPC GPA snapshot attempt did fail body decoding, retry succeeded. Later market WebSocket connection-reset and reconnect `2/2` subscription ACKs and StreamReady were recorded. **No verified `GAP_RECOVERED`, full-slot completed market reconciliation, zero unresolved slots or final `CONTROLLED_GAP_SMOKE_PASS` at last supplied evidence.** Proxy forces market WSS disconnect, not Shadow WSS disconnect.

**NEXT BLOCKING GATE:** capture actual final controlled-gap run verdict, inspect complete archived logs and market/full-slot reconciliation. On any incomplete interval fail closed; investigate RPC response decode failures and backfill limits. Separately qualify Shadow WSS disconnect/recovery. Preserve evidence, no financial claims from health sweep counts. User alone may start subsequent 4h after complete short smoke qualification, sufficient disk and explicit decision. Production LIVE/submission remain hard disabled. Detailed evidence and commands: top of `MEASUREMENT_LEDGER.md`.

---


**Status:** ARCHITECTURE FREEZE — AUTHORITATIVE — FROZEN v4.5
**Promoted:** 2026-10-07; v4.5 übernimmt v4.4 vollständig und ergänzt das verbindliche P6-MT-v2-Runtime-Subgate nach Review von FIX-139-Code, Mainnet-Messungen, Provider-Grenzen und Fables P6-MT-v2-Fassung.  
**Stand:** 2026-10-07  
**Geltungsbereich:** DarkBot / Project 0 Liquidator  
**Source of Truth:** Diese Datei. Ältere Fassungen (v4.2, v4.3, v4.4) sind historisch, nicht autoritativ.

## Aktueller Umsetzungsstand — 2026-10-07

Dieser Block ändert **nicht** die eingefrorene Architektur v4.4; er dokumentiert nur den realen Implementierungs-/Evidence-Stand.

```text
G1  Compiler / Workspace-Tests                    PASS
G2  C0 Mainnet Census / Zielgruppe                GO für Shadow
G3  Exact Health + Protocol Sizing                 IMPLEMENTIERT + build/test grün
G4  Jupiter V2 RouteProvider + P0 Builder          IMPLEMENTIERT + build/test grün
G5  Mainnet Bootstrap / Recovery / READY           NÄCHSTER GATE — noch nicht real belegt

P6-LT technische Shadow-Kette                      IMPLEMENTIERT, Mainnet-Evidence offen
P6-MT v2 Runtime-Hardening                          IMPLEMENTIERUNG LÄUFT; Tranche 1 (F1/F8/P1-C) lokal qualifiziert
P8-LT Landing / Token-Deltas / realized PnL        OFFEN
LIVE                                                HART GESPERRT
Produktionsprofitabilität                           NICHT BEWIESEN
```

**Project-0 Runtime-Baseline:** FIX-139-Inhalt auf `main`. Die experimentellen FIX-140..143-Parallelisierungsimplementierungen wurden vollständig verworfen; ausschließlich ihre Mess-Evidence bleibt im `MEASUREMENT_LEDGER.md`. P6-MT wird neu und ticketweise nach diesem Phasenplan implementiert.

Mainnet-C0 für Group `4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8`: `165123` Accounts, davon `82680` mit Liability, `61` beim letzten Pulse liquidierbar, `5114` historisch bereits liquidiert.

Runtime-Hardening bis `[FIX-80]`: Unknown-Economics Technical Shadow, ZERO_PREFUND Shadow-Probe getrennt von LIVE-Qualifikation, kompakter C0, Group-/Liability-Filter für Bootstrap/Recovery/WSS, Helius-Free-RPC/WSS Launcher.

**Unmittelbar nächster Schritt:** realer G5-Mainnet-Lauf:
`StreamReady → filtered Snapshot → Reconcile → READY → Health → Sizing → Route → Build → Sign → Simulate`.

Details/Evidence: `project_0/docs/MEASUREMENT_LEDGER.md`.


**v4.4 gegenüber v4.2:** korrigiert die Reihenfolge der v4.3-Speed-Hebel, damit der kostenlose Long-Tail-/Premium-Pfad zuerst bis LIVE kommt und Race-Komplexität ihn nicht blockiert.

- **FIRST EURO PATH wird Hauptpfad:** `S1-LT → S3-CORE → P2 → P3 → P4A → P5 → T → P6-LT → P8-LT`;
- **S3-CORE bleibt schlank:** Builder, Signer, `RpcSubmitter`, Jito-Core, Reconciliation;
- **Jito gRPC / Race-Prewarm / Race-Benchmark:** nur nach `S1-RACE PASS`;
- **Direct TPU QUIC:** kein S3-Bau; nur K-Kandidat nach `S1-RACE PASS` und nach SWQoS-/Stake-Qualifikation;
- **Prepared Candidate / TX Prebuild:** bleibt in P2/P5;
- **SubscribeDeshred / Raw Shred:** bleibt K-only;
- keine zweite Codebasis, keine spätere Race-Neuentwicklung.

**BELEGT — Solana SWQoS:** Solana dokumentiert stake-weighted QoS für TPU-Traffic. Ein standalone Direct-TPU-Sender besitzt diesen Vorteil nicht automatisch; ein vertrauenswürdiger/gestakter oder virtuell gestakter Pfad muss separat qualifiziert werden.

**Konsequenz:** Direct TPU QUIC bleibt technisch interessant, ist ohne qualifizierte SWQoS-/Stake-Beziehung aber **nicht Race-Primary** und wird nicht vorab gebaut.

**BELEGT — Yellowstone:** `SubscribeDeshred` liefert pre-execution Transactions aus eingehenden Shreds, ist aktuell aber nur auf Triton-Extension-Servern verfügbar; der Open-Source-`yellowstone-grpc-geyser`-Server liefert dafür `UNIMPLEMENTED`.

**BELEGT — Agave:** `tpu-client-next` existiert und kann QUIC-Verbindungen zu kommenden Leadern vorhalten. Das beweist technische Verfügbarkeit, nicht wirtschaftliche Priorität für einen unstaked/ungepaarten Bot.

**PRÜFEN durch Fable:** aktuelle SWQoS-/TPU-Regeln, Jito-gRPC-Zugang/TxV1, exakte Long-Tail-LIVE-Gates, sichere Prebuild-Grenzen und ob irgendeine Race-Erweiterung noch weiter nach K gehört.

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

**Es wird keine zweite Entwicklung gebaut.** Architektur und Interfaces berücksichtigen Race von Anfang an; **LIVE-Freigabe darf aber bewusst Long-Tail/Premium zuerst erfolgen**, während Race auf `S1-RACE PASS` und seine eigenen wirtschaftlichen Gates wartet.

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

## 0.5 Speed-Regel v4.3

Race wird nicht nur auf Ingress optimiert. Die kontrollierbare End-to-End-Kette lautet:

```text
source proximity
→ decode/state
→ candidate narrowing
→ exact health/economics
→ prepared execution patch
→ sign
→ leader/block-engine handoff
```

Optimiert wird nur dort, wo die Messung realen Lead/Landing-/Profitgewinn zeigt.

Priorität:

1. keine unnötige Arbeit nach Trigger;
2. keine Remote-Lookups im Race-Pfad;
3. Connection/Leader/Block-Engine-Verbindungen vorwärmen;
4. statische TX-/Account-/Route-Teile vorbauen;
5. Submission-Pfade gegeneinander messen;
6. teure pre-execution/shred/validatornahe Eskalation erst nach belegtem Bottleneck.

---

## 0.6 FIRST EURO PATH — höchste Umsetzungspriorität

Der erste wirtschaftliche Produktionspfad ist **Long-Tail/Premium**, nicht Race.

```text
P0 minimal required reuse/protocol decisions
+ P1A current mainnet freeze
+ C0 long-tail market evidence

S0
→ S1-LT PASS
→ S2
→ S3-CORE PASS
→ P2
→ P3
→ P4A
→ P5
→ T
→ P6-LT shadow
→ S4 parity before value-bearing LIVE
→ P8-LT controlled LIVE
→ first realized net profit
```

Parallel, aber **nicht blockierend für P2/Long-Tail-Bau**:

- H0 vertiefen;
- P1B vorbereiten und bei Deployment promoten;
- S1-RACE Binary-Source suchen/qualifizieren;
- Race-spezifische Submission-Forschung;
- P7/P4B Competition-/Provider-Optimierung.

Regel:

```text
kein kostenloser Mainnet-Geyser
≠ Projekt blockiert

sondern:
Long-Tail/Premium weiterbauen und LIVE qualifizieren
Race bleibt deaktiviert
```

0.1.12-spezifisches Tagging/T-Pfad darf erst LIVE, wenn der entsprechende Program-Stand tatsächlich on-chain deployed und über P1B→P1A promoted ist.

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
- max. 64 Accounts (Runtime-Limit, gilt für legacy/v0/v1; v1 macht es zum Formatlimit);
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

Optional für spätere K-PreExecution-Sensoren:

```text
T_pre
    speculative/pre-execution Signal empfangen; niemals allein authoritative State
```

Verbindlicher authoritative/Production-Pfad:

```text
T_rx
    authoritative bzw. für die Execution-Klasse qualifiziertes Rohupdate im Ingress empfangen

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

P6-MT ergänzt ausschließlich **diagnostische** Grenzen; die normativen Zeitpunkte oben bleiben unverändert:

```text
T_rx_first      ältester noch unbedienter Auslöser einer coalesced Candidate-Revision
T_admit         Candidate erhält Owner-Credit / wird einer Ausführungsstufe übergeben
T_prep_start    Start von prepare_candidate
T_prep_done     PreparedCandidate fertig
T_route_start   Route-Remotegrenze beginnt
T_route_done    Route-Remotegrenze beendet
T_build_done    lokaler Build der betrachteten Planvariante fertig
T_owner_final   finale Owner-Prüfung vor Reservation/Sign
T_sim_start     Simulation desselben SignedIntent beginnt
T_sim_done      Simulation beendet
T_result_owner  asynchrones Resultat beim Canonical Owner angekommen
```

Für jede P6-MT-Queue werden `wait_ns`, `service_ns`, Tiefe und Maximaltiefe als count/p50/p95/p99/max nach Lane und Trigger-Klasse gemessen. Queue-Alter und State-Freshness bleiben getrennte Größen.

Ableitungen:

```text
ingress_to_state = T_state - T_rx
state_to_detect  = T_detect - T_state
detect_to_ready  = T_ready - T_detect
ready_to_sign    = T_sign - T_ready
sign_to_handoff  = T_handoff - T_sign

optional_precompute_lead = T_rx - T_pre   # nur wenn T_pre vorhanden; diagnostisch, nicht State-Wahrheit
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
project_0/docs/evidence/P0_REFERENCE_STUDY.md
project_0/docs/evidence/P0_REUSE_MATRIX.md
project_0/docs/evidence/P0_LICENSE_MATRIX.md
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
project_0/docs/manifests/CURRENT_MAINNET.md
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
project_0/docs/manifests/UPCOMING_0_1_12.md
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
project_0/docs/evidence/C0_MARKET_CENSUS.md
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
project_0/docs/evidence/H0_LIQUIDATIONS.md
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

Den **kleinsten finalen Production-Execution-Core** bauen, der Long-Tail/Premium bis LIVE tragen kann und später Race-Adapter ohne Rewrite aufnehmen kann.

## S3.1 CORE — Build / Sign

- legacy/v0 Builder;
- v1 Builder;
- LUT für v0;
- v1 TransactionConfig;
- Ed25519 Signing;
- Serialization;
- Submission Result Taxonomy;
- Landing Reconciliation.

## S3.2 CORE — Submission

### `RpcSubmitter`

Pflicht für Baseline, Long-Tail/Premium, Recovery/Fallback und Vergleich.

### `JitoSubmitter`

Gemeinsames Interface für Jito Single-TX/Bundle-Semantik.

Für den **First-Euro-Pfad** reicht zunächst der aktuell verfügbare, korrekt qualifizierte Jito-Pfad; JSON-RPC ist hier zulässig, weil Long-Tail/Premium nicht auf den Race-Latenzpfad angewiesen ist.

Pflicht:
- atomare Bundle-Semantik soweit benötigt;
- definierte Failed-Cost-/Revert-Semantik;
- Tip-/Fee-Erfassung;
- v0/v1/TxV1-Kompatibilität;
- Landing/Reconciliation.

## S3.3 CORE Gate

`S3-CORE PASS` wenn mindestens ein vollständiger Long-Tail/Premium-Pfad reproduzierbar korrekt ist:

```text
build
→ sign
→ RpcSubmitter oder JitoSubmitter
→ observe
→ reconcile
```

`S3-CORE PASS` ist die Voraussetzung für P2 im First-Euro-Pfad.

## S3.R — RACE EXTENSIONS — nur nach `S1-RACE PASS`

Diese Tickets blockieren P2/P3/P4A/P5/T/P8-LT nicht.

### `JitoGrpcSubmitter`

Erst bauen/messen, wenn ein qualifizierter Race-Ingress existiert.

Qualifikation:
- Access/Auth;
- Regionen;
- Single TX / Bundle;
- TxV1;
- >1232 Byte;
- Tip-/Revert-Semantik;
- Landing/Execution Position;
- `T_sign→T_handoff`;
- Kosten.

### Race Prewarm Manager

Erst nach `S1-RACE PASS`:
- Jito gRPC connections;
- block-engine region state;
- Race submission buffers/queues;
- keine Handshakes/DNS nach Trigger.

### Race Submission Benchmark

Vergleicht zunächst:

```text
Jito gRPC
vs.
RPC baseline
```

Direct TPU gehört **nicht** zu S3.R; es bleibt K.

## S3.4 v1

Unterstützen:
- bis 4096 Byte;
- max. 64 Accounts entsprechend aktuellem Format/Runtime-Capability;
- keine LUTs in v1;
- explizites CU-Limit;
- explizites Loaded-Account-Data-Limit;
- Priority Fee in totalen Lamports;
- ComputeBudget-Instructions nicht als v1-Konfiguration;
- Reader Version 1;
- Adapter-Kompatibilität.

## S3.5 v0/v1 Auswahl

Kein Dogma; pro Execution-Klasse messen:

```text
version
bytes
accounts
LUT/resource config
CU
submission path
landing
latency
failure
cost
```

## Gate S3

```text
S3-CORE PASS:
    Long-Tail/Premium final build/sign/submit/reconcile works

S3-RACE EXTENSIONS PASS:
    requires S1-RACE PASS
    Jito gRPC/prewarm/race benchmark qualified
```

`S3-RACE EXTENSIONS PASS` ist **keine Voraussetzung** für P2 oder Long-Tail-LIVE.

Tickets CORE:
`S3-01 v0 builder` ·
`S3-02 v1 builder` ·
`S3-03 signer/serialization` ·
`S3-04 RpcSubmitter` ·
`S3-05 JitoSubmitter core` ·
`S3-06 landing reconciliation`

Tickets RACE — `requires: S1-RACE PASS`:
`S3-R01 JitoGrpcSubmitter` ·
`S3-R02 race prewarm manager` ·
`S3-R03 submission race benchmark`

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

# PROJECT-0 IMPLEMENTATION UNLOCKS

Es gibt **keinen einzelnen großen Merge Gate**, der Long-Tail auf Race-Evidence warten lässt.

## P2 BUILD UNLOCK — First-Euro-Pfad

P2 darf beginnen, sobald mindestens:
- P0: relevante Reuse-/Protocol-Entscheidungen für P2/P3 geklärt;
- P1A: aktueller LIVE-Stand ausreichend eingefroren;
- C0: Long-Tail/Premium-Marktklasse ausreichend verstanden;
- S0 PASS;
- S1-LT PASS;
- S2 State-Basis funktionsfähig;
- S3-CORE PASS.

Parallel weiterlaufend und **nicht P2-blockierend**:
- P1B bis zum tatsächlichen Deployment;
- H0;
- S1-RACE;
- S3-RACE Extensions;
- vertiefte Competition-/Provider-Forschung.

## P8-LT LIVE GATE

Vor value-bearing Long-Tail/Premium-LIVE zusätzlich Pflicht:
- tatsächlicher deployed Program-Stand qualifiziert; bei 0.1.12: P1B→P1A Promotion abgeschlossen;
- S4 Core-Parity PASS;
- P3 exact Health PASS;
- P4A deterministic Economics PASS;
- P5 execution plan PASS;
- T PASS soweit Tagging verwendet wird;
- P6-LT Shadow ausreichend;
- Reconciliation/Safety/Halts PASS.

## RACE LIVE GATE

Zusätzlich zum gemeinsamen Core:
- S1-RACE PASS;
- S3-RACE Extensions PASS oder anderer gemessener Race-Submission-Pfad;
- Race-spezifisches P6 Shadow/Loss Evidence;
- wirtschaftlicher Vorteil gegenüber Kosten belegt.

Race wird später auf demselben Core freigeschaltet; kein zweiter Health-/Economics-/Planner-/TX-Core.

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

## Prepared Candidate / TX Template Cache

Für relevante Near-Threshold-/Near-`t_profitable`-Kandidaten wird **vor** dem Trigger so viel wie semantisch sicher vorbereitet.

Precomputable/resident:

- betroffene Account-Liste;
- Bank-/Oracle-/Integration-Dependencies;
- Refresh-Skelett;
- Receivership-Instruction-Skelett;
- AccountMeta-Reihenfolge;
- Route-Klasse / `NO_SWAP`-Kandidat;
- v0/v1 Execution-Klasse;
- LUT-/Address-Plan für v0;
- v1 Resource-Config-Skelett;
- Compute-/Fee-Profil;
- statische Serialization-Buffers;
- Submission-Klasse;
- invalidation keys/version.

Nicht blind cachen:

- stale Health;
- stale Oracle-Werte;
- stale Quote/Route;
- blockhash/lifetime;
- dynamischer seize/repay amount;
- dynamischer Tip/Fee;
- Signatur.

Race-Ziel:

```text
trigger
→ candidate lookup O(log n)/O(1)-nah
→ exact current health/economics
→ nur dynamische Felder patchen
→ finalize
→ sign
→ submit
```

Jeder Cache-Eintrag besitzt harte Invalidation auf relevante Account-/Program-/Oracle-/Route-/Version-Änderungen.

**PRÜFEN:** Welche Instruction-/Message-Bytes bei v0/v1 tatsächlich sicher vorserialisiert werden können, ohne Signatur-/Lifetime-/Account-Ordering-Semantik zu verletzen.

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

## Prepared Execution Plan

P5 konsumiert den P2 Prepared-Candidate-Cache und hält für Near-Threshold-Kandidaten einen **unsigned prepared execution plan** bereit.

Zielzustand vor Trigger:

```text
static accounts known
+ protocol instruction skeleton known
+ route class known
+ tx version class known
+ resource profile known
+ submission class known
```

Beim Trigger verbleiben nur:

```text
exact current health/economics
→ dynamic seize/repay/route values
→ blockhash/lifetime + fee/tip
→ final serialization
→ sign
→ handoff
```

Keine Vorab-Signatur, solange irgendein signature-bound Feld noch dynamisch oder invalidierbar ist.

Remote-Routing darf nicht als synchroner Pflicht-Call in diesen Race-Pfad zurückkehren. Wenn eine Route nicht ausreichend frisch/prepared verfügbar ist, greift die definierte alternative Execution-Klasse (`NO_SWAP`, vorbereitete erlaubte Route oder SKIP).

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


## C0-MO — UNFILTERED LIVE MARKET ECONOMICS (Evidence-Subgate)

**Status:** IMPLEMENTED (Code), MAINNET-LAUF AUSSTEHEND. Kein wirtschaftlicher PASS ohne realen Lauf im Measurement Ledger.

**Zweck.** C0-MO beantwortet vor weiterer größerer Runtime-Optimierung: Wie viele reale Receivership-Liquidationen entstehen? Wie viel Gross Edge wird real abgeschöpft? Wie verteilt sich die Edge? Welche Receiver gewinnen? Wie konzentriert ist der Markt?

**Architekturvertrag.**

```text
S1 / WsRpcControlSource (P0_MODE=market, früher Abzweig vor jedem Trading-Objekt)
→ Marginfi transactionSubscribe   (source=transactions, Helius Developer+, ohne failed-Filter)
  oder logsSubscribe mentions      (source=logs, alle Pläne; Enrichment erst NACH der Capture,
                                    gezählt und begrenzt per getTransaction)
  + WebSocket-Heartbeat (Ping, 30 s), slotSubscribe nur als Telemetrie; Commitment finalized (Default)
→ Cutoff = t_rx(StreamReady) + Dauer; Ingress stoppen, Queue drainen, nur t_rx ≤ Cutoff zählt
→ Raw Transaction Evidence         (transactions.jsonl, jede eindeutige Marginfi-TX inkl. failed)
→ erfolgreicher LiquidationReceiverEvent-Decode (programm-attribuiert, fail closed)
→ Market Gross-Edge Evidence       (liquidations.jsonl, coverage.jsonl, summary.json, final_report.txt)
```

Gross Edge = `seized_equity_usd − repaid_equity_usd` aus dem Event. Einheiten bleiben getrennt (USD, Lamports, native Token-Units); keine erzwungene SOL/USD-Umrechnung. `GAP`, `INGRESS_OVERFLOW` oder `FATAL` im Beobachtungsfenster → `coverage_complete=false`; fehlende Logs, Decoder-Fehler oder **nicht qualifizierte** abgeschnittene Logs in erfolgreichen TXs → `decode_complete=false` (abgeschnittene Logs werden nach Capture/Drain gegen die vollständigen Instruktionen mit derselben F8-`end_liquidation`-Wahrheit qualifiziert: keine end_liquidation oder exakte target/receiver-Multiset-Parität = qualifiziert; Abweichung = `TRUNCATED_LIQUIDATION_MISMATCH`; nicht prüfbar = `LOGS_TRUNCATED_UNQUALIFIED`); unvollständiger Drain oder vorzeitiges Ende → jeweils `MARKET_DATA_INCOMPLETE`. Keine Quelle gilt derzeit als coverage-verifiziert (LUT-only-Referenzen auf das Programm unbelegt); jeder Lauf endet daher höchstens als `MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED`. `MARKET_DATA_VALID` erst nach autoritativer Provider-Garantie oder reproduzierbarem LUT-only-Zustellungstest. Ein Fehler des Enrichment-Clients verhindert nie Summary und Report. Enrichment-Vollständigkeit wird getrennt ausgewiesen (`enrichment_complete`, `fee_totals_complete`).

**Non-Claims.** C0-MO ist kein zweiter Bot. C0-MO verändert FIX-139 nicht. C0-MO ersetzt weder P1-C noch F1/F8, P6-MT, die Competition-/Capture-Messung oder P8 realized PnL. Keine Tages-, Monats- oder Jahreshochrechnung aus einem einzelnen Lauf.

**Wirtschaftliche Messkette (dürfen nicht vermischt werden).**

```text
A) C0-MO — MARKET EDGE
   Was wurde im beobachteten Markt real liquidiert, welche Gross Edge real abgeschöpft?
        ↓
B) P1-C / F1 / F8 — EXECUTABLE EDGE
   Welcher Teil dieser Markt-Edge wäre mit unseren Production-Regeln ausführbar?
        ↓
C) COMPETITION / CAPTURE
   Welchen Teil der ausführbaren Edge können wir gegen andere Liquidatoren gewinnen?
        ↓
D) P8 — REALIZED NET PNL
   Was landet tatsächlich als realisierter Netto-Profit bei DarkBot?
```

**Priorität.** Wirtschaftliches Stop/Continue-Gate vor dem großen Runtime-Ausbau MT6–MT10; verbindlich eingeordnet in P6-MT.16 nach MT5. Blockiert keine notwendigen Correctness-Arbeiten, verhindert aber, dass umfangreiche Performance-Arbeit allein auf Basis interner Shadow-Candidates priorisiert wird.

**Qualifikation.** Erst Smoke-Test (`P0_MARKET_DURATION_SECS=600`), dann 4-Stunden-Lauf (`14400`). Eintrag im Measurement Ledger mit source commit, run_id, provider class, source, start/end, actual duration, first/last slot, coverage_complete, gap/overflow, unique/liquidation transactions, events, gross edge total/hour/median/p90/p99/max, unique receivers, top/top-3 share und den Non-Claims.

## P6-MT — BOUNDED PARALLEL RUNTIME / APPLE-M-SERIES

### Status / Zweck

P6-MT ist ein **verbindliches Subgate von P6-LT**. Es wurde durch FIX-138/139-Messungen ausgelöst und ersetzt keinen bestehenden Health-/Economics-/Planner-/TX-Core.

```text
Baseline: Project-0 FIX-139 content
LIVE:     HART GESPERRT

Ziel:
minimale T_rx_first → T_sign-Latenz
für wirtschaftlich relevante Kandidaten

unter:
korrektem Canonical State
deterministischer Recovery
bounded Remote-Budget
null verlorenen Kandidaten
```

Messbasis:

```text
FIX-138 serial:
  Health Ø      ≈ 3.39 µs
  Pipeline Ø    ≈ 1.57 ms
  Pipeline max  ≈ 2.654 s
  pending       ≈ 6,239

FIX-141 experiment:
  ≈ 659/s vs. ≈389/s serial
  ≈ +69.5 % throughput
  aber Queue-/Retry-/Ownership-Probleme

FIX-142 experiment:
  exec_job_overflow 1,500,739 → 0
  exec_queue_full   128,813 → 0
  603 freie Exec-Slots
  pending ≈ 6,217
```

Konsequenz: Nicht „mehr Threads“ optimieren, sondern Owner-Entlastung, wertvolle Kandidaten zuerst, Remote-Budget, vermiedene Remote-Arbeit und erst danach gemessene Cross-Account-Parallelität.

Der verworfene FIX-140..143-Code darf **nicht** wiederverwendet oder cherry-gepickt werden.

### P6-MT.1 Canonical Owner / Ownership

Genau ein Owner verändert:

- `StateCore`;
- Fork/Root/Recovery;
- ProgramData-/Upgrade-Latch;
- Reverse Indices;
- Candidate Cache / Epochen;
- per-Account Ownership;
- Safety / Reservations;
- Ledger / Winner / Submission Commit.

Verboten:

```text
Arc<Mutex<StateCore>>
Arc<RwLock<StateCore>>
&mut StateCore in Workern
Worker-seitige Safety-/Ledger-/State-Mutation
unbounded spawn
Thread pro Candidate
Thread pro Jupiter-Quote
```

Per Account:

```text
IDLE
→ EXECUTING(candidate_id)
→ SIGNED
→ SUBMITTED(signature,last_valid_block_height)
→ RELEASED

max 1 EXECUTING|SIGNED|SUBMITTED
+
max 1 newest pending revision
```

Neue Revisionen coalescen auf genau die neueste Pending-Version.

### P6-MT.2 Generationen / Readset / Recovery

Der Owner führt eine monotone `state_epoch`; jeder relevante Record erhält `last_changed_epoch`.

`prepare_candidate` liest über einen automatischen `ReadSetRecorder`:

```text
(pubkey, observed_epoch)
```

fehlende Keys als `ABSENT`.

Candidate-Identität bindet mindestens:

```text
candidate_id
account
trigger_class/lane
recovery_generation
program_generation
policy_generation
config_generation
candidate_revision
t_rx_first / t_rx_latest
trigger_slot
evaluation_slot / time
valid_until
read_set
prepared
blockhash identity / validity, falls bereits gebunden
```

Result-Aktionen ausschließlich zentral:

```text
ACCEPT
DROP_OLD_GENERATION
DROP_FOREIGN_INFLIGHT
RETRY_NEWEST_PENDING
RECHECK_CURRENT
DROP
```

Ein altes Resultat darf niemals einen neueren Inflight-Eintrag löschen.

Recovery erhöht `recovery_generation` und invalidiert alle älteren Candidate-, Pending-, Ownership-, Route-, Fee-, Simulation-, Projection-, Prepared- und Intent-Arbeiten. Alte Resultate erzeugen keine Folgearbeit.

### P6-MT.3 Eine Preparation-Semantik

Es existiert exakt eine pure/read-only Semantik:

```text
prepare_candidate(...)
```

Sie umfasst:

- Exact Health;
- Receivership Eligibility;
- Integration-Klassifikation;
- Sizing-Inputs;
- ATA-/Inventar-Prüfung;
- lokale Fee-Inputs;
- automatisches Readset;
- lokale Profit-Obergrenze zunächst nur als Priorisierungssignal;
- immutable PreparedCandidate.

Heutige Baseline:

```text
Oracle/Bank-Sweep-Pfad:
Health bis zu 3× pro Candidate-Revision

andere Trigger:
typischerweise weniger
```

Ziel:

```text
Health genau 1× pro Candidate-Revision
und weitergereicht
```

Gate:

```text
serial prepare == parallel prepare
100 % feldweise
```

### P6-MT.4 Owner-Diät / Owner-Inbox

Owner-Duty-Cycle pro 1-s-Fenster messen:

```text
ingress_apply
sweep
admission
commit
sign
evidence_enqueue
other
```

Startziel p99 <= 60 %, MT0 darf es evidenzbasiert korrigieren.

Vom Owner weg, sofern MT0 bestätigt:

- JSON/Base58/File-I/O;
- Mehrfach-Health;
- Planning/Route;
- Fee-RPC;
- Simulation;
- Remote-Retry-Schlaf.

Evidence läuft über bounded compact records zu einem Writer. `evidence_overflow > 0` ist FAIL.

FAST-/Remote-/Control-Resultate wecken den Owner ohne 1-/250-ms-Pollingverzögerung. Kein Busy-Spin.

### P6-MT.5 Prioritäts-Lanes

Zweistufig:

**Trigger-Priorität vor Preparation**

```text
L0: Transition / Forced Rearm / fällige Premium- oder t_profitable-Grenze
L1: materielle Revision bereits relevanter Candidates
L2: periodisch / Error / Maintenance
```

**Prepared-Priorität nach Preparation**

lokale Economics / konservative UB dürfen Remote-Reihenfolge weiter priorisieren.

Innerhalb der Lanes:

- L0: früheste Deadline, dann höchste konservative wirtschaftliche Priorität;
- L1: ältestes `t_rx_first`;
- L2: ältester `last_forwarded_slot` / FIX-135-Fairness.

Startgewicht WDRR 8:3:1; erst nach MT0 festschreiben.

Coalescing bewahrt `t_rx_first`, `t_rx_latest`, newest revision.

Eine neuere canonical Revision darf promoten, **demoten oder canceln**. Keine bloße altersbedingte Herabstufung.

### P6-MT.6 Freshness ist eigenes P1-Gate

FIX-21-Freshness wird nicht still im Lane-Ticket geändert.

Zu prüfen:

```text
heute:
Opportunity-Alter ab Trigger/Detect

Alternative:
Freshness des neu evaluierten States
+
Queue-Alter nur diagnostisch
```

Änderung nur nach eigenem `FRESHNESS-REVIEW PASS`.

### P6-MT.7 Remote-Budget

Startannahmen, vor Umsetzung aktuell verifizieren:

```text
Helius Free:
general RPC = 10 RPS
plus methodenspezifische niedrigere Caps

Jupiter Free:
API-Key = 1 RPS
keyless  = 0.5 RPS
```

Concurrency pro Klasse:

```text
inflight_max
=
ceil(rate_budget × measured_latency_p95 × safety_factor)
+ kleine Reserve
```

Token-Bucket je Provider/Klasse mit Rate, Burst, Inflight-Semaphore, Priorität und Deadline.

Recovery/Snapshot präemptiert Candidate-Budget.

Ohne Token:

```text
prepared Candidate bleibt pending
neuere Revision ersetzt ihn
kein Reprepare
kein Spin
kein Rückwurf in Detection
```

HTTP 429 → typisiertes `RateLimited`; kein Worker schläft auf Retry. `remote_retry_sleep_ns = 0` ist Gate.

### P6-MT.8 Remote-Arbeit vermeiden

Eine lokale Profit-Obergrenze darf zunächst **nur priorisieren**.

Hard-Drop erst nach:

1. P1-C / Executable-Floor-Economics PASS;
2. mathematischem/protokollseitigem Obergrenzenbeweis;
3. einheitlichen Units;
4. Property-/Boundary-/Replay-Tests;
5. `dominance_violations = 0`.

Bis dahin:

```text
niedrige UB
→ niedrige Priorität
NICHT
→ Opportunity verwerfen
```

Lokales Fee-Modell ersetzt `getFeeForMessage` erst nach 100 % Parität über >=10.000 qualifizierte v0/v1-Messages mit/ohne Priority und Bundle-Bestandteile. Mismatch → RPC-Fallback + Gate-FAIL.

Route-Cap-Cache ist nur Hint, nie Quote-Wahrheit:

```text
input mint
output mint
RouteFootprintClass
```

Der echte kompilierte TX beweist Fit.

### P6-MT.9 Remote Runtime / Transport

Eine gemeinsame async Runtime ist zulässig; eine gemeinsame unstrukturierte Queue nicht.

Getrennte bounded Klassen mindestens:

```text
Recovery/Snapshot
Jupiter
Fee RPC solange nötig
Simulation
Integration Projection
Oracle Projection
Winner
Reconcile
Blockhash
```

Jede Klasse besitzt eigene Queue, Semaphore, Timeout-/429-Taxonomie, Request-/Generation-ID und Telemetrie. Recovery höchste Priorität.

Kein blockierender Solana-`RpcClient` auf async Executor-Threads; final nonblocking RPC oder eigener async `RpcSender`.

Pro Hostklasse ein persistenter HTTP-Client/Pool:

- kein Clientbau pro Candidate;
- DNS/TLS nicht bewusst im Hotpath;
- Keepalive;
- getrennte Connect-/Request-Timeouts;
- `tcp_nodelay` verifiziert;
- HTTP/2 als Capability-/Messvariante, HTTP/1.1-Fallback;
- warm / idle 1s / 10s / 60s messen.

Fehlerpfade `RateLimited|Timeout|BudgetDeferred|RemoteUnavailable` erzeugen keine automatische Zusatzsimulation, Technical Probe, Reprepare oder Retry-Spin.

### P6-MT.10 Lokaler Fast Path / Snapshot

Lokale kleine Schritte run-to-completion:

```text
OWNER admission
→ FAST prepare/local plan/build/local fee/economics
→ OWNER final current-state check/reservation/sign
→ REMOTE simulation
→ OWNER deterministic commit
```

Route-Pfad:

```text
FAST S1
→ REMOTE Jupiter
→ FAST S2 Build/Economics
→ OWNER final/sign
```

Worker lesen nie live in mutable `StateCore`.

Snapshot-Varianten **ohne Vorentscheidung**:

1. versionierter persistenter State / Arc-Root;
2. Arc<Record> + vollständiger Readset;
3. owned Kopie des minimalen Readsets.

Wahl nach `snapshot_build_ns`, `owner_apply_overhead_ns`, Bytes, Retention, Misses und Recovery-Parität.

### P6-MT.11 SignedIntent / Ressourcen

Nach finaler Owner-Prüfung genau ein immutable `SignedIntent`:

```text
candidate_id
Generationen
Readset-/Policy-Versionen
message hash
signierte Bytes
Economics
Exposure
Reservationen
Blockhash-Lifetime
```

Simulation und spätere Submission benutzen denselben Intent. Kein Rebuild, keine zweite Signatur.

Owner reserviert vor Sign mindestens:

- Liability-Inventar;
- Fee-Payer-Lamports;
- Rent;
- Safety/Exposure.

**Shadow erzwingt dieselbe virtuelle Arbitration wie LIVE.**

Konkurrierende Candidates dürfen nicht beide als executable gezählt werden, wenn sie dieselbe exklusive Ressource benötigen.

### P6-MT.12 Apple-M4 Runtime

Keine feste Kernzahl.

Startup-Evidence:

```text
hw.nperflevels
hw.perflevel0.logicalcpu
hw.perflevel1.logicalcpu
hw.perflevelN.cpusperl2
hw.perflevelN.l2cachesize
hw.logicalcpu
hw.cachelinesize
active CPU count
arm64/native
macOS / Power / Thermal
```

Qualifizierende M4-Läufe nicht unter Rosetta.

Startmodell:

```text
Owner            1
Ingress WSS      bestehend, getrennt
Remote async     1–2 Runtime-Threads
FAST CPU         Start 2, nur nach Messung erhöhen
Evidence Writer  1
BULK Pool        nur bei gemessenem Bedarf
```

QoS:

```text
Owner/Ingress/FAST/Remote = USER_INITIATED
Evidence                  = UTILITY
USER_INTERACTIVE          = nur Messvariante
```

Keine behauptete harte P-/E-Core-Affinität auf macOS.

FAST wächst nur, wenn Queue-Wartezeit p99 verletzt **und** FAST-Auslastung >70 %.

GPU/Neural Engine/Metal/CoreML bleiben aus dem Online-Hotpath ohne profilierten homogenen Batch-Kernel.

### P6-MT.13 Pflichttelemetrie

Mindestens:

```text
owner_busy_ns{category}
owner_duty_cycle
owner_wakeup_ns
lane_wait_ns{L0,L1,L2}
lane_promotions/demotions/cancellations
staterace_by_queue_age
prepare_calls / prepare_health_calls / prepare_ns
snapshot_build_ns / snapshot_bytes / snapshot_miss
remote_requests{provider,class}
remote_429{provider,class}
remote_retry_sleep_ns
budget_denied{class,lane}
budget_denied_ub_sum
remote_inflight{class}
remote_queue_depth{class}
route_cap_cache_hits/misses
jupiter_requests_per_routed_revision
local_fee_checks/mismatches
dominance_samples/violations
reservation_conflicts
intents_live
evidence_overflow / evidence_queue_depth
thermal_state / active_cpu / fast_utilization
```

Histogramme: count/p50/p95/p99/max.

### P6-MT.14 Pflicht-Tests

Vor Gesamt-Shadow mindestens:

1. verschiedene Accounts laufen parallel;
2. ein Account nie doppelt in Execution;
3. newest pending gewinnt;
4. ältere pending revisions erzeugen keine volle Remote-Arbeit;
5. altes Resultat löscht keine neue Ownership;
6. Recovery invalidiert alte lokale/remote Jobs;
7. alte Projection-Generation kann nicht committen;
8. geändertes Readset kann nicht committen;
9. ABSENT wird bei Anlage invalidiert;
10. Queue-full verliert keinen Candidate;
11. Backpressure erzeugt keinen Retry-/Reprepare-Spin;
12. serial prepare == parallel prepare;
13. Completion Order ändert Business-Commit nicht;
14. Shared HTTP Client ändert Route-Mapping nicht;
15. Worker/Remote-Fehler fail closed;
16. Shadow kann nicht submitten;
17. derselbe SignedIntent wird simuliert;
18. FIX-134/135/137 Regressionen PASS;
19. Recovery/StreamReady/Gap PASS;
20. Workspace + Clippy `-D warnings` PASS;
21. L0 überholt großen L2-Backlog;
22. L2 bleibt starvation-frei;
23. Coalescing behält `t_rx_first`;
24. Resultat weckt Owner ohne Ingress;
25. 429 erzeugt keinen Retry-Schlaf;
26. Budgetmangel erzeugt keinen Spin;
27. Inflight-Grenzen nie überschritten;
28. UB verletzt bewiesene Obergrenze nie;
29. Schwellenkreuzung kann hochpriorisieren;
30. lokale Fee-Parität PASS;
31. Route-Hint wird durch echten Build validiert;
32. ReadSetRecorder vollständig;
33. `valid_until` invalidiert korrekt;
34. SUBMITTED verhindert Doppel-Submission;
35. exklusive Ressourcen nur einmal reserviert;
36. Evidence Writer blockiert Owner nie;
37. M4-Qualifikation nativ arm64;
38. Snapshot konsistent unter Owner-Writes;
39. neuere Revision darf Lane demoten/canceln;
40. Policy/Config-Generation invalidiert alten Candidate;
41. Shadow-Reservation verhindert Doppelzählung unmöglicher Parallelität.

### P6-MT.15 PASS-Gate

Startwerte; MT0 darf nur evidenzbasiert präzisieren:

```text
ingress_overflow                 = 0
sink_overflow                    = 0
evidence_overflow                = 0
remote_retry_sleep_ns            = 0
stale commits                    = 0
lost candidates                  = 0
serial/parallel prepare parity   = 100 %
LIVE                             = gesperrt

owner_duty_cycle p99             <= 60 %
owner_wakeup p99                 <= 1 ms
L0 T_rx_first → T_admit p99      <= 50 ms
L0 max wait                      <= 500 ms
L0 queue-age drops               = 0
NO_SWAP T_state → T_sign p99     <= 2 ms
pending over 10 min              drainfähig
```

Aktivierte Dominance-/Fee-Gates verlangen jeweils 0 Violations/Mismatches.

### P6-MT.16 Umsetzungstickets

Verbindliche Reihenfolge:

```text
MT0  Instrumentierung / H1–H6
MT1  Remote-Client-Hygiene
P1-C Executable-Floor-Economics
     → bereits durch Tranche 1 r3 lokal qualifiziert; nicht neu implementieren
MT2  Owner-Diät + eine prepare_candidate
MT3  Prioritäts-Lanes + t_rx_first
     Freshness NICHT ohne eigenes Review
MT4  Remote-Arbeit vermeiden; UB nur so hart wie bewiesen
MT5  Generationen + Readset + Ownership-Lifecycle

C0-MO
     → 10-min Smoke
     → danach 4-h Market Observation
     → MARKET EDGE / Marktgröße / Gross Edge bewerten
     → vor großem MT6–MT10-Ausbau
     (kein Ersatz für MT0–MT5, kein LIVE-Gate; wirtschaftliches Stop/Continue-Gate)

MT6  Snapshot-Modell messen und wählen
MT7  FAST-Pool / Cross-Account-Admission
MT8  async Remote-Runtime / getrennte bounded Klassen
MT9  SignedIntent + virtuelle/echte Reservations + Commit
MT10 M4-Feintuning, je eine Messvariante
MT11 Mainnet Shadow 3 min
MT12 Mainnet Shadow 10 min
MT13 Freeze
```

MT0 entscheidet mindestens:

- 429-/Retry-Anteil am Tail;
- Latenz pro Remote-Klasse;
- Queue-Wartezeit pro Trigger-Klasse;
- Owner-Duty-Cycle;
- Health-Mehrfacharbeit pro Triggerpfad;
- vermeidbare Remote-Arbeit;
- Fee-RPC-Paritätspotential.

Nach MT4: Wenn die Gates schon mit weniger Parallelität erfüllt sind, wird MT6–MT8 auf das gemessen Nötige reduziert. Parallelität ist Mittel, kein Liefergegenstand.

### P6-MT.17 Implementierungs-Ownership — Fable-Regel

P6-MT ist groß genug, dass die **nichttrivialen Runtime-Tickets von Fable implementiert werden**.

Fable schreibt ein Ticket vollständig, sobald es mindestens einen dieser Bereiche berührt:

- Canonical State / Recovery;
- async Remote Runtime / eigener RpcSender;
- Budget Scheduler;
- Lanes / Fairness / Coalescing;
- Readset / Epochen / Ownership;
- Snapshot-Repräsentation;
- Cross-Account-Concurrency;
- SignedIntent / Reservation / Safety;
- Economics / UB / Fee-Semantik;
- mehrere Produktionsdateien mit gemeinsamem Verhalten;
- grob >50 nichttriviale Runtime-Codezeilen.

Wir geben Fable pro Ticket:

```text
aktuellen main SHA
relevante Source-Dateien
exaktes Ticketziel
Invarianten
verbotene Änderungen
Pflicht-Tests
Pflicht-Telemetrie
PASS/STOP-Gate
```

Fable liefert:

```text
vollständige Production-Implementierung
keine Demo/Minimalversion
betroffene Dateien
Tests
Rollback-/Migrationshinweise
keine Branch-Annahme
```

Wir übernehmen Review, Compiler/Clippy, Regressionen, Shadow-Evidence und Integration.

**Kleine Eigenänderungen** sind nur zulässig, wenn alle Bedingungen erfüllt sind:

```text
grob <= 50 LOC
klar mechanisch/begrenzt
keine State-/Recovery-/Economics-/Concurrency-/Async-/Safety-Semantik
keine neue Architektur
Tests eindeutig
```

Bei Zweifel: Fable implementiert.

### P6-MT.18 Git-Disziplin

Project 0 arbeitet auf `main`.

```text
keine Project-0 Featurebranches
keine Rescue-/Hardening-/Experimentbranches
keine ad-hoc Patch-Kaskaden
```

Die globale Repo-main-SHA darf durch BSC weiterlaufen. Project-0-Baselines sind daher **path-/content-spezifisch**.

Vor jedem Project-0-Write:

1. aktuellen `origin/main`-SHA lesen;
2. Project-0-Zieldateien gegen erwartete Baseline prüfen;
3. keine BSC-Datei verändern;
4. genau ein freigegebenes Ticket integrieren;
5. Ref-Update nur gegen erwartete Main-SHA;
6. bei Abweichung STOP und Diff;
7. niemals Force;
8. Ledger aktualisieren.

### P6-MT.19 Verhältnis zu Audit / anderen Phasen

P6-MT ersetzt weder P4A noch P5 noch P7/P8.

```text
P1-C Executable Floor
→ Voraussetzung für harte ökonomische Remote-Unterdrückung

P4A
→ normative Profit-Wahrheit

P5
→ normative Planner-/Route-/TX-Semantik

P6-MT
→ Runtime-/Scheduling-/Remote-Budget-Hardening

P7
→ Competition-/Provider-Economics

P8
→ einziges value-bearing LIVE-Gate
```

Winner Parser, Drift, Historical Prestate, Replay und PnL dürfen zwischen MT-Tickets weiterlaufen, sofern sie nicht von Runtime-Änderungen abhängen.

Kompetitive Mainnet-Shadow-Evidence erst nach MT12.

### P6-MT.20 Stop Conditions / Gate

Sofort STOP bei:

- mutable Canonical State im Worker;
- mehrdeutiger Ownership;
- unklarer Recovery-Generation;
- serial/parallel-Divergenz;
- unvollständigem Snapshot ohne fail-closed;
- internem Remote-Retry-Schlaf;
- Remote-Call am Budget vorbei;
- Retry-Spin;
- unbewiesenem Dominance-Hardfilter;
- Fee-Mismatch;
- systematisch blockiertem L0;
- Owner >90 % bei leerem Downstream;
- Shadow-Doppelzählung exklusiver Ressourcen;
- BSC-Dateiänderung;
- erreichbarem LIVE.

P6-MT PASS erst nach:

```text
MT0–MT10 Ticket-Gates PASS
MT11 3-min Shadow PASS
MT12 10-min Stability PASS
alle Pflicht-Tests PASS
keine unbounded Queue / Spawn
Owner wartet nie auf Candidate-Remote-I/O
persistente Remote-Clients
keine Retry-Sleeps
Priorität + Fairness PASS
virtuelle Shadow-Reservation PASS
M4-Konfiguration gemessen gewählt
LIVE gesperrt
```

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

1. Filter/Region/Provider des bestehenden Mainnet-Geyser optimieren;
2. besserer kostenloser/bereits verfügbarer Mainnet-Binary-Feed;
3. Paid Geyser nur bei positivem incremental net value;
4. eigener Geyser / validatornaher Aufbau;
5. eigener Node / Region;
6. **erst danach pre-execution / Shred / Deshred**, wenn Geyser nachweislich nicht reicht.

### K-INGRESS-PRE — `SubscribeDeshred` Sensor

**BELEGT:** Yellowstone beschreibt `SubscribeDeshred` als separaten Stream für pre-execution Transactions. Der Server rekonstruiert Entries aus eingehenden Shreds und streamt decodierte Transactions, bevor Replay sie ausgeführt hat.

**BELEGT / Einschränkung:** Die aktuelle Yellowstone-README sagt zugleich:

```text
SubscribeDeshred is only available on Triton extension servers.
Open-source yellowstone-grpc-geyser currently returns UNIMPLEMENTED.
```

Daher ist `SubscribeDeshred` **kein Bestandteil von S1**, keine kostenlose Annahme und keine Voraussetzung für Race-LIVE.

Wenn wirtschaftlich verfügbar, ist seine Rolle ausschließlich:

```text
T_pre speculative pre-execution transaction
→ Project0/Oracle relevance filter
→ affected banks/accounts lookup
→ speculative/prepared health/economics work
→ prepared TX/route/serialization warming

authoritative Geyser/state evidence
→ validate/recompute dynamic truth
→ sign/send
```

Der Deshred-Stream darf **niemals allein**:

- canonical State setzen;
- Health final bestätigen;
- eine Liquidation blind absenden;
- Fork/Execution-Erfolg behaupten.

Messgröße:

```text
precompute_lead = T_rx_authoritative - T_pre
```

und entscheidend:

```text
incremental race wins / realized net profit
- access/provider/shred cost
```

Nur bei messbarem Netto-Mehrwert.

### K-INGRESS-RAW

Raw Shred Delivery / eigener validatornaher Deshred-Pfad bleibt noch eine Stufe später. Erst wenn Geyser **und** ein verfügbarer pre-execution Sensor nachweislich Ingress-Limit sind, wird eigener Shred-/Validator-Stack erwogen.

## CPU

Die grundlegende Runtime-/Parallelarchitektur ist **kein K-Thema mehr**; sie muss bereits über **P6-MT PASS** qualifiziert sein.

Wenn danach `LOSS_REASON = CPU/BUILD` verbleibt, ist K ausschließlich Feintuning desselben Production-Cores:

```text
Profiling
→ Data Layout
→ Allocation Tuning
→ Cache Locality
→ nachgewiesene ARM64/SIMD-Hebel
→ PGO
→ simd-json / Decode-Tuning bei gemessenem Bedarf
→ weitere ISA-/Scheduler-Feinarbeit
```

Kein zweiter Parallel-Core und keine nachträgliche Runtime-Neuentwicklung in K.

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

1. Jito gRPC / Region / Tip / Bundle Policy optimieren;
2. Jito vs. RPC unter gleichen Bedingungen messen;
3. erst danach Direct TPU QUIC prüfen.

### K-SUB-TPU — `DirectTpuQuicSubmitter`

**BELEGT:** Agave `tpu-client-next` kann QUIC-Verbindungen zu kommenden Leader-TPUs cachen/vorwärmen.

**BELEGT — SWQoS:** TPU-Kapazität kann stake-gewichtet priorisiert werden. Ein standalone Sender ohne gestakten/virtuell gestakten vertrauenswürdigen Pfad hat diesen Vorteil nicht automatisch.

Vor Implementierung zwingend dokumentieren:
- verwendete Identity/QUIC-Identity;
- eigener gestakter Validatorpfad ja/nein;
- vertrauenswürdige SWQoS-/virtual-stake-Beziehung ja/nein;
- reale unstaked/staked TPU-Kapazität;
- Landing unter Last gegenüber Jito/Staked-RPC.

Hard Rule:

```text
kein qualifizierter SWQoS-/Stake-Pfad
→ Direct TPU darf experimentell gemessen werden
→ aber NICHT Race-Primary werden
→ kein großer Leader/Prewarm/Worker-Bau ohne Messbeleg
```

Erst bei belastbarem Vorteil bauen/optimieren:
- LeaderUpdater;
- TPU address service;
- QUIC worker pool;
- leader lookahead;
- connection prewarm;
- fanout/retry policy.

Entscheidend:

```text
incremental realized net profit
vs.
Jito gRPC / staked provider path / full operating cost
```

### Weitere K-Submission-Hebel

- Submission Redundancy derselben signierten TX nur bei sicherer Semantik;
- Block-Engine-/Region-Nähe;
- dedizierter Host erst nach Messung;
- Kernel/NIC-Spezialisierung erst als späteste Stufe.
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
S3-03 signer/serialization
S3-04 RpcSubmitter
S3-05 JitoSubmitter core
S3-06 landing reconciliation

S3-R01 JitoGrpcSubmitter                  requires S1-RACE PASS
S3-R02 race prewarm manager              requires S1-RACE PASS
S3-R03 submission race benchmark         requires S1-RACE PASS

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
P2-06 prepared candidate / tx template cache
P2-07 cache invalidation + prebuild benchmark

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
P5-07 prepared execution plan
P5-08 staging fixtures

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

K-01 ingress loss attribution
K-02 SubscribeDeshred economic/access qualification
K-03 raw-shred/validator escalation only if justified
K-04 Jito/region optimization
K-05 DirectTPU SWQoS/stake qualification
K-06 DirectTPU build only if qualification passes
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
project_0/docs/MEASUREMENT_LEDGER.md
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
project_0/docs/
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
    REVIEW CANDIDATE v4.4
    v4.2 REMAINS AUTHORITATIVE UNTIL PROMOTION

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
    SubscribeDeshred = OPTIONAL K PRE-EXECUTION SENSOR ONLY
    RAW SHRED = LATER K-ONLY AFTER MEASURED INGRESS BOTTLENECK

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

FIRST EURO SUBMISSION:
    RPC + JITO CORE

RACE SUBMISSION:
    JITO gRPC ONLY AFTER S1-RACE PASS
    DIRECT TPU = K-ONLY + SWQoS/STAKE QUALIFICATION
    UNSTAKED/UNPAIRED DIRECT TPU = NOT RACE-PRIMARY

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

# 12A. Fable Review Checklist für v4.4

Fable soll diese Review-Fassung **nicht nur sprachlich**, sondern technisch gegen aktuelle Primärquellen prüfen.

## A — Ingress

- Ist v4.2-S1 weiterhin korrekt: Race=BinaryGeyser, Long-Tail/Premium=WSS+Snapshot/Reconcile?
- Ist `SubscribeDeshred` korrekt als Triton-spezifischer optionaler K-Sensor eingeordnet?
- Ist `SubscribeDeshred` tatsächlich pre-execution und nicht authoritative/canonical?
- Gibt es aktuell bessere/kostenlose Mainnet-Binary-Quellen, die S1-RACE ohne Paid-Abo ermöglichen?

## B — Submission

- Ist es korrekt, Direct TPU vollständig nach K zu verschieben?
- SWQoS: welche Kapazität ist für unstaked/unpaired Direct-TPU-Verbindungen aktuell real verfügbar?
- Welche trusted-validator/virtual-stake-Konstellation wäre nötig, damit Direct TPU überhaupt Race-Primary werden kann?
- Jito gRPC: aktueller Access, Auth, Regionen, TxV1, Bundle-/Single-TX-Semantik, Kosten/Tips?
- Reicht für First-Euro-Live `RpcSubmitter` + Jito-Core ohne gRPC-Race-Ausbau?
- Ist parallele/redundante Submission derselben TX sicher und wirtschaftlich sinnvoll?

## C — Prebuild

- Welche Teile des Project-0 Receivership-TX lassen sich wirklich vor dem Trigger kompilieren/serialisieren?
- Welche Felder müssen wegen v0/v1, Lifetime, Fees, route quote, account ordering oder Signatur zwingend spät gepatcht werden?
- Welche Invalidation-Events fehlen im Prepared-Candidate-Cache?

## D — Economics / Scope

- Entsteht irgendwo eine versteckte Paid-Provider-Pflicht?
- Ist irgendeine Speed-Optimierung nur theoretisch und sollte nach K verschoben werden?
- Bleibt ein einziger Production-Core erhalten?
- Bleiben Race und Long-Tail gleichzeitig funktional, ohne Zweitentwicklung?

Erst nach dieser Prüfung darf v4.4 die autoritative v4.2 ersetzen.

---

# 13. Verbindlicher Start

## Hauptpfad — First Euro

Parallel vorbereiten:

```text
P0  → nur die für Current Mainnet / Health / Reuse unmittelbar nötigen Entscheidungen zuerst
P1A → aktuellen Mainnet-Stand einfrieren
P1B → 0.1.12 vorbereiten; bei Deployment sofort Promotion/Requalification
C0  → Long-Tail/Premium-Markt quantifizieren
S0  → finalen Rust-Workspace
```

Dann höchste Priorität:

```text
S1-LT PASS
→ S2
→ S3-CORE PASS
→ P2
→ P3
→ P4A
→ P5
→ T
→ P6-LT
→ P6-MT PASS
→ S4 PASS vor value-bearing LIVE
→ P8-LT
→ erster realisierter Netto-Profit
```

## Parallel / danach — Race

```text
S1-RACE qualification
→ S3-R01 Jito gRPC
→ S3-R02 prewarm
→ S3-R03 race benchmark
→ P6-RACE evidence
→ Race LIVE gate
```

Nur bei gemessenem Bottleneck/Mehrwert:

```text
K SubscribeDeshred
K Raw Shred
K Direct TPU + SWQoS/stake path
K own validator/node/region/kernel/NIC
```

H0, P7 und P4B laufen als Evidence/Optimization weiter, dürfen aber den First-Euro-Build nicht künstlich verzögern, solange die für sichere Controlled-LIVE-Freigabe notwendigen Gates erfüllt sind.

**Keine zweite Entwicklung. Kein späterer Race-Rewrite. Kein späterer Rust-Rewrite.  
Ein finaler Core; Long-Tail/Premium wird zuerst wirtschaftlich aktiviert, Race wird danach auf demselben Core freigeschaltet.  
Diese v4.5 ist mit P6-MT-v2-Review promoted und eingefroren.**

---

## C0-E1 — 4H ECONOMIC / CAPITAL / COMPETITION / RACE ATTRIBUTION

**Status (2026-10-08): C0-E1 LOCAL INTEGRATION / BOTH BUILDS AND WORKSPACE TEST SUITES PASS; DIFFERENTIAL CLIPPY AND MAINNET SMOKE OPEN.**

### Ausgangsstand

C0-MO r6 ist lokal qualifiziert, committed und auf `main` gepusht:

`57bef334e60764f614712785438a95edb7638352`

Der 600-s-Mainnet-`logs`-Smoke auf diesem Stand ist **PASS**:

- `verdict=MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED`
- `actual_observation_seconds=600.0`
- `transactions_received=63`
- `unique_transactions=63`
- `successful_transactions=26`
- `failed_transactions=37`
- `liquidation_transactions=0`
- `liquidation_events=0`
- `gap_count=0`
- `reconnect_count=0`
- `ingress_overflow_count=0`
- `decoder_error_count=0`
- `logs_truncated_total=2`
- `logs_truncated_qualified=2`
- `logs_truncated_unqualified=0`
- `logs_truncated_liquidation_mismatch=0`
- `rpc_fallback_count=2`
- `decode_complete=true`
- `market_capture_complete=true`
- `source_coverage_verified=false`
- `incomplete_reasons=[]`
- `FINAL_REPORT_KEYS=PASS`
- `R6_600S_QUALIFICATION=PASS`

Builds, beide Workspace-Test-Sets, rustfmt und differential Clippy (`candidate-only=0` für beide Feature-Sets) sind PASS.

### Warum C0-E1 vor dem 4-h-Lauf eingeführt wird

Der ursprünglich geplante C0-MO-4-h-Lauf beantwortet Marktaktivität und Market Gross Edge, aber nicht vollständig:

- welcher Teil mit dem **aktuellen Project_0** ausführbar wäre;
- wie viel Working Capital benötigt wird;
- welche Kapitalstufe welchen Teil der bekannten Net Edge abdeckt;
- welche Opportunities vor / im gleichen / nach dem realen Winner-Slot fertig wären;
- wie groß die lokale Race-Margin in ms ist;
- wie viel gemessener lokaler Speedup bei verlorenen Races nötig gewesen wäre;
- in welcher Stufe Rx→Detect, Detect→Ready oder Ready→Sign die Zeit verloren geht;
- welcher konkrete Bottleneck den größten bekannten wirtschaftlichen Wert blockiert.

Darum wird der 4-h-Lauf vor dem Start zu **C0-E1** erweitert. C0-E1 kombiniert zwei strikt getrennte Wahrheiten:

```text
A) C0-MO Market Truth
   reale Marginfi-TX / reale Liquidation / reale Market Gross Edge

B) Project_0 Shadow Truth
   Health → Sizing → Funding → Economics → Build → Sign → Simulation
   → Winner/Race Evidence
   → KEINE Submission
```

C0-E1 darf weder eine vereinfachte zweite Economics-Engine noch einen `repaid <= capital`-Pseudo-Executable-Filter einführen.

### Verbindliche C0-E1-Ausgabe

Der qualifizierte Produktionslauf bleibt:

- Dauer: `14400 s`
- Snapshot: `3600 s`
- vier automatische Stunden-Snapshots:
  - `hour_01.json`
  - `hour_02.json`
  - `hour_03.json`
  - `hour_04.json`
- ein Abschluss:
  - `economics_final.json`

Jede Stunden-JSON enthält **Stundendelta + kumulierten Stand**.

Die Reports müssen ohne Upload der Rohdaten mindestens enthalten:

1. technische Stabilität / Capture-Vollständigkeit;
2. Market Gross Edge und Verteilung;
3. aktuelle bekannte Project_0 Executable Net Edge;
4. Kapital-/Funding-Evidence;
5. Standardszenarien `$200 / $500 / $1,000 / $2,500 / $5,000 / $10,000`;
6. Winner-/Competition-Korrelation;
7. Slot-Race:
   - `PRE_WINNER_SLOT_PROVEN`
   - `SAME_SLOT_UNKNOWN`
   - `POST_WINNER_SLOT_PROVEN_LATE`
   - unresolved/fail-closed Klassen;
8. gleiche monotone Uhr für Market + Shadow;
9. lokale Winner-Race-Margins in ns/ms;
10. `required_speedup` für verlorene, qualifizierte Opportunities;
11. Speedup-Sensitivity bei 1/5/10/25/50/100/200/500/1000 ms;
12. Stage-Latenzen Rx→Detect / Detect→Ready / Ready→Sign / Rx→Sign;
13. Bottleneck-Ranking nach bekannt blockierter Net Edge;
14. alle UNKNOWN-Zähler und Non-Claims.

Die **wahre gegnerische Submission-Deadline bleibt UNKNOWN**. Lokale Provider-/Host-Beobachtungszeit darf nicht als exakter Send-Zeitpunkt des Konkurrenten ausgegeben werden.

### Fable-Handoff

Fable-Basis bleibt absichtlich der bereits qualifizierte Code-Stand:

`57bef334e60764f614712785438a95edb7638352`

Das separate nachgelagerte Dokumentations-Commit dieses Statusblocks verändert **nicht** Fables Implementierungsbasis.

Erzeugtes Baseline-Paket:

`DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33.zip`

SHA-256:

`e829116253cd14da0236e31b0b10dd469becb52dc5d309406a2758600634b850`

Größe:

`1,660,074 bytes`

Marginfi-Pin:

`33c67987a661f59c6683a65485ebb4f0ef030b77` (`mrgn-0.1.11-rc1`)

Packaging-Gates:

- exact `HEAD == origin/main == 57bef334...`: PASS
- `project_0/.env` geschützt / nicht paketiert: PASS
- `git archive` source export: PASS
- exact Marginfi pin context: PASS
- kein `.git`, kein `target/`, keine Secrets im Paket: PASS
- Cargo metadata: PASS
- build `crate_health`: PASS
- build `crate_health txv1`: PASS
- tests `crate_health`: PASS
- tests `crate_health txv1`: PASS
- Clippy beide Feature-Sets: PASS auf Baseline
- Release-`p0-longtail`: PASS
- interne SHA-256-Prüfung: PASS
- Paketgröße <=80 MiB: PASS

Toolchain der Baseline:

- macOS 15.6.1 / arm64
- rustc 1.93.1
- cargo 1.93.1

### Was als Nächstes verbindlich passiert

```text
C0-MO r6 code + 600-s smoke PASS
        ↓
C0-E1 Fable handoff READY          (abgeschlossen)
        ↓
Fable candidate zurück
        ↓
Base/Scope/Diff/Secrets/SHA Review
        ↓
git apply --check
        ↓
lokale Builds + Workspace Tests
        ↓
rustfmt + differential Clippy
        ↓
C0-E1 600-s combined smoke
  duration=600
  snapshot=150
  → 4 Snapshots + Final
        ↓
nur bei PASS:
C0-E1 real 4-h run
  duration=14400
  snapshot=3600
        ↓
hour_01..04.json + economics_final.json
        ↓
Economic GO / CONDITIONAL / STOP
        ↓
MT6–MT10 nur nach gemessenem Bottleneck priorisieren
```

### C0-E1 Non-Claims

C0-E1 kann Market Edge, bekannte aktuelle Shadow-Executable-Edge, Kapitalbedarf, Race-Slot-Evidence, lokale beobachtete Race-Margins, Speedup-Sensitivity und Bottlenecks messen.

C0-E1 beweist **nicht**:

- dass eine Shadow-ready TX bei echter Submission garantiert gewonnen hätte;
- den echten Send-Zeitpunkt des Konkurrenten;
- realisierten PnL;
- zukünftigen Tages-/Monatsgewinn aus einem einzelnen 4-h-Fenster;
- Provider Source Coverage;
- LIVE-Readiness.

**LIVE bleibt blockiert.**


## C0-E1 — Economic / Capital / Competition / Race measurement candidate

Implementation extends C0-MO plus the existing Shadow pipeline under `P0_MODE=econ4h`.
One monotonic Clock, bounded typed analytics channel, no submission, unchanged pinned Health /
Sizing / Funding / Economics / Build / Sign / Simulation and authoritative F8 winner semantics.
Four automatic interval/cumulative reports and one reconciled final report replace manual
four-hour raw-data analysis. Capital scenarios default to 200/500/1000/2500/5000/10000 USD.

This is a measurement candidate, not a new economics engine or MT6–MT10 optimization.
It preserves independent market truth, UNKNOWN costs, ambiguous same-slot identity, and
unverified source coverage. Chain-slot evidence, local-observation speedup bounds and the
unobservable true competitor send deadline remain distinct. Shadow evidence is not realized
PnL or guaranteed capture. LIVE remains blocked.

Qualification order: external patch review and apply-check against its exact export base →
local build/tests → candidate formatting/differential Clippy → 600/150 combined smoke →
14400/3600 observation. No Mainnet or P8 PASS is implied by this implementation note.
Operational entry, output schema and limitations: `project_0/README.md`, section 18.

## C0-E1 RETENTION-SAFE 600S GATE — PASS

Status 2026-10-08:

1. C0-E1 implementation completed.
2. Initial 600-second Mainnet smoke exposed retention overflow:
   `293482`.
3. Analytics retention was converted from per-revision RAM retention to
   complete disk streaming plus bounded semantic state.
4. One-million-evaluation stress test passed:
   - received 1000000
   - streamed 1000000
   - coalesced 999984
   - semantic max 16
   - overflow 0
   - evictions 0
5. Both builds passed.
6. Both full workspace test configurations passed.
7. Differential Clippy against `57bef334...` passed candidate-only 0/0.
8. Mainnet retest `c0e1_retention_smoke_20261008T215650Z` passed:
   - 600 seconds
   - four snapshots
   - technical PASS
   - market capture complete
   - analytics queue drops 0
   - retention overflow 0
   - received 840518
   - streamed 840518
   - coalesced 816112
   - semantic max 24406
   - semantic evictions 0
   - submissions 0
   - live_ready false

C0-E1 is technically qualified for the four-hour measurement gate.

This does not yet establish profitability, representative sample size,
complete market-source coverage, exact competitor timing or LIVE readiness.

Next:

**Run the four-hour C0-E1 qualification from the exact committed/pushed
revision.**

## C0-E1 AUTHORITATIVE PRE-4H CHECKPOINT — 2026-10-08

Qualified C0-E1 code source:

`b7f5ed3850e0b1e412d39c673d89a8c8ffc3644a`

The retention-safe 600-second gate is complete and PASS. This exact code revision passed the full local build/test/static qualification and the real Mainnet retention retest.

Current phase boundary:

**C0-E1 implementation + 600s qualification complete → next gate: 4-hour observation.**

4-hour run requirements:

- duration 14,400 seconds;
- four snapshots at 3,600-second intervals;
- C0-MO Market Truth plus the real Shadow production path;
- capital bands 200 / 500 / 1000 / 2500 / 5000 / 10000 USD;
- Economics, Capital, Competition and Race Attribution;
- raw Shadow evaluations streamed to disk;
- analytics queue drops 0;
- retention overflow 0;
- semantic evictions 0;
- LIVE disabled;
- submissions 0;
- tested source identity recorded as `b7f5ed3850e0b1e412d39c673d89a8c8ffc3644a`.

The 4-hour run may still produce an insufficient economic sample; that must not be converted into a profitability claim.

Still open unless separately proven:

- complete market-source coverage;
- exact competitor submission timing;
- same-slot ordering/capture probability;
- LIVE readiness.

## C0-E1 4H ATTEMPT 1 — BLOCKED BY MARKET GAP RECOVERY

Run `c0e1_4h_20261008T222326Z` completed the full 14,400-second duration on qualified code
`b7f5ed3850e0b1e412d39c673d89a8c8ffc3644a`, but returned technical `INCOMPLETE`.

The retention architecture itself passed the four-hour load:

- 20,056,722 Shadow evaluations received;
- 20,056,722 streamed;
- retention overflow 0;
- semantic evictions 0;
- analytics queue drops 0;
- four snapshots and final reconciliation complete;
- clean automatic shutdown;
- submissions 0;
- LIVE false.

The blocking condition was one C0-MO WebSocket continuity gap:

- last safe slot: 454701191;
- gap at 2026-10-09T00:10:54Z;
- reconnect/StreamReady at 2026-10-09T00:10:55Z;
- observed transport interruption approximately 1.125408459 s;
- current gap event has `to_slot=None`;
- `gap_count=1`;
- `coverage_complete=false`;
- `market_capture_complete=false`.

All 81 truncated-log transactions were qualified; decoder, drain and enrichment completed without
their own qualification failure.

Phase decision:

**Do not rerun four hours unchanged.**

Next gate is a fail-closed C0-MO gap-recovery/backfill correction that derives the post-reconnect
slot boundary, backfills the missing slot range outside the capture hot path, deduplicates recovered
transactions, and restores coverage only when the entire missing range is proven complete.

After that correction:

1. build/tests/static qualification;
2. deterministic gap/backfill regression tests;
3. short Mainnet smoke with reconnect/gap recovery evidence;
4. only then repeat the 14,400-second C0-E1 economic observation.

The zero observed liquidations in this failed 4h attempt are not a qualified market-zero claim.
# C0-E1 master-contract override — 2026-10-09

The current normative C0-E1 objective supersedes prior gap-only and receivership-only
qualification: **capture all real Marginfi liquidation paths, reconcile the entire finalized
slot window, reconstruct evidenced economics, then join/filter against our Shadow pipeline**.
The existing locally edited gap-recovery code is a partial foundation. Classic, forced
deleverage/settlement classification, complete slot reconciliation and expanded per-type
economics/join are still OPEN. No new four-hour run until the expanded controlled smoke PASS.
Current resume point and exact open work: `MEASUREMENT_LEDGER.md`, top RESUME block.


### C0-E1 continuation 2026-10-10

Canonical join, hourly all-path economics, capital/loss-dollar attribution and pinned
source scope are implemented; final qualification is tracked at the top of
MEASUREMENT_LEDGER.md. Final controlled smoke: 600s, actual transport interruption,
full finalized reconciliation. No MASTER PASS until the final matrix and combined
smoke pass. The user will start the four-hour observation; do not launch it automatically.
LIVE/submission remain disabled.


### C0-E1 transport qualification checkpoint — 2026-10-10

Current follow-up uses checkpoint2c03b03. Implementation first, then targeted
checks and remaining qualification within the user's credit priority. Proxy
readiness is upstream ACK+slot proven; isolated short reconnect test PASS.
Full-slot retry now isolates the failing block, retaining pacing, bounded retry
and fail-closed completeness. Economics remains the production-cost-based
canonical join. No new trading features or hotpath RPC dependencies.

Authoritative current verdict and exact commands are in MEASUREMENT_LEDGER.md
and docs/evidence/C0E1_TRANSPORT_FINISH_20261010/. A short probe or green Rust
suite alone does not qualify4h. User exclusively starts the4h run after combined
600s completeness/clean-shutdown/report checks pass. LIVE/submission stay blocked.


Final user-directed handoff2026-10-10: no more600s agent testing. Implemented
transport corrections and local302/304-test matrix are green; differentialClippy
0/0. Short upstream/proxy probePASS.60s combined smoke remainedINCOMPLETE because
Shadow bootstrap/root catch-up had not produced evaluations; prolonged waiting
was stopped to preserve credits. All evidence retained, no new commit/push,
no4h launch. MEASUREMENT_LEDGER.md contains the exact remaining diagnosis and
resume commands. Code completion is not an end-to-end profitability/stabilityPASS.

---

## C0-E1 — ASTRA MASTER IMPLEMENTATION CONTRACT 2026-10-11

**Status: VERBINDLICHE UMSETZUNGSSPEZIFIKATION / P1/P3 LOCAL TECHNICAL PASS / P2,P4–P6 OPEN / NO NEW LONG RUN AUTHORIZED.** Dieser neue Abschnitt ist der maßgebliche aktuelle Auftrag für C0-E1-Teststeuerung, Transport- und Provider-Robustheit sowie den unabhängigen historischen Marginfi-Marktvergleich. Er **ersetzt** ältere Handlungsanweisungen zum unveränderten 4h/5h-Neustart und zur Pflicht, vor jeder Daueränderung Shell/Rust-Parameter manuell zu setzen. Er **ersetzt nicht** den strengeren bestehenden C0-E1-Vertrag zur vollständigen finalisierten Markt-/Slot-Qualifikation, die Trading-Sicherheitsregeln oder das v4.5-Produktziel. Ältere Evidence bleibt historisch unverändert.

**Repo:** sawyerma/darkbot, aktuelles origin/main beim Arbeitsbeginn selbst verifizieren; keine SHA blind übernehmen. **Scope:** ausschließlich project_0/**; bsc/** niemals bearbeiten. **Auftraggeber-Freigabe:** Astra darf den spezifizierten Code implementieren, reviewen, testen und kleine projektbezogene Commits auf main pushen. **Keine Freigabe** für neue 5h-/24h-Mainnet-Captures, LIVE, Submission, Providerkäufe oder das Löschen bisheriger Evidence. Fehler ehrlich als OPEN dokumentieren; keinen vollständigen PASS durch abweichende Definitionen erzwingen.

### A. Bereits abgeschlossene Fehleranalyse – als Fakten übernehmen, nicht als neue Forschungsphase

Quelle: Anwenderausgaben, market/summary.json, market/coverage.jsonl und runtime.log des tatsächlich durchgeführten Runs **market5h_20261011T010758Z**. Rohdateien liegen auf dem Mac des Anwenders und nicht notwendigerweise in Git; keine Verfügbarkeit im Remote erfinden.

| Befund | Gemessen | Schlussfolgerung / Evidenzgrenze |
| --- | --- | --- |
| Configured/elapsed | 18.000 s; 2026-10-11T01:08:38Z bis 06:08:38Z | Die Messuhr lief 5h; das ist kein Beweis für 5h Market-Coverage |
| Live Capture | 181 Transaktionen, davon 100 success / 81 failed | Teilbeobachtung, kein vollständiger Markt-Census |
| Slotdaten | 9.369 Notifications; Slot 455464097 (01:08:38Z) bis 455473465 (01:42:38Z) | 15.960 s vor Capture-Ende ohne später nachgewiesene Slot-Grenze |
| Transport | 527 GAP, 0 erfolgreich gezählte Reconnects, 0 ingress overflows | Einmalige StreamReady-Barriere; später anhaltende Verbindungsabbrüche und 429, keine erfolgreich dokumentierte Wiederaufnahme |
| WSS-Log | anfänglich close frame, dann vielfach HTTP 429; Backoff erreicht 30s und 429 wiederholt sich | Provider verweigert Verbindungen; genaue Kontingentursache und beteiligte Client-Last noch nicht belegt |
| Gap-State | 32 GAP_UNRECOVERED und GAP_STATE_CAPACITY | default max_gaps=32; ein langanhaltender Ausfall verbrauchte die Zustandskapazität |
| Canonical | slots_expected=9369, produced=0, fetched=0, skipped=0, unresolved=9369; error=RPC_RATE_LIMIT | Vor dem ersten erfolgreich verarbeiteten Block gescheitert; ob getSlot oder getBlocks den ersten endgültigen 429 lieferte, aus Summary allein nicht bestimmbar |
| Deployment | DEPLOYMENT_RPC_RATE_LIMIT | weiterer Provider-429; keine Bytecode-Kontinuitätsqualifikation |
| Decoder | LOGS_TRUNCATED_UNQUALIFIED=2 | zwei Truncation-Fälle ohne nachgewiesene Auflösung |
| Endstatus | MARKET_DATA_INCOMPLETE, canonical_complete=false, TEST_EXIT=2 | NULL gemeldete kanonische Liquidationen ist **UNKNOWN**, keine bestätigte Marktnull |

**Getrennte frühere Evidenz:** 10-Minuten-Run market10m_20261011T003528Z: externer finalized Marginfi-Adressindex 32 Signaturen, Bot-Live-Zähler 32, persistent gespeichert 31, eine im Inneren des Fensters fehlende Signatur, keine vom unabhängigen Prüfskript erkannten Liquidationsinstruktionen/-Events. Danach wurde pro Live-Transaktion Flush ergänzt; Rust-Release-Build und Decoder-Selbsttest PASS. **Nicht** daraus ableiten: vollständige Blockabdeckung, hundertprozentige Event-Decoder-Korrektheit oder erfolgreiche 5h-Wiederholung.

**Unmittelbar aus Git belegte Gründe:** econ.rs erzwingt duration_secs == 4 * snapshot_secs; run_econ4h.sh wechselt bei short/auto (<3600s) auf harte gesamte Walltime, die Bootstrap und Cleanup einschließt; market_observer.rs startet seine Uhr nach StreamReady, aktualisiert Markt-Slotgrenzen aber nur bei empfangenen Slot-Notifications; ws_rpc.rs emittiert Gap bei Sessionfehler und Reconnected erst nach allen Subscription-ACKs; market_gap.rs führt maximal 32 offene GAP-Datensätze standardmäßig; market_reconcile.rs hat 100.000 Slots Gesamtgrenze, 14.400s Phasenlimit und verarbeitet ganze finalized Blöcke; der bestehende Python-Auditor liest beim Full-Run den verkürzten observerseitigen Slotbereich. Diese Tatsachen bilden die zu behebenden Codepfade.

**Wichtige Grenze:** 43 GiB des älteren Vierstundenlaufs sind gespeicherte Run-Evidence, NICHT nachgewiesene 43 GiB heruntergeladene Blöcke. Ein indexbasierter Vergleich reduziert voraussichtlich Netzwerk-/Block-I/O, aber nicht automatisch das Shadow-Journaling. Speicherverbrauch nach Dateityp empirisch messen, nicht die Ursachen vermischen.

### B. Lieferziel und Bedienungsvertrag (ein Parameter statt manueller Nachjustierung)

Verbindliche Soll-CLI; Namenswahl test.sh und verify.sh nur ändern, wenn eine bessere bestehende konsistente Schnittstelle nachgewiesen wird und gleichwertige Beispiele dokumentiert sind:

~~~bash
./project_0/scripts/test.sh --duration 10m
./project_0/scripts/test.sh --duration 1h
./project_0/scripts/test.sh --duration 5h
./project_0/scripts/test.sh --duration 24h
./project_0/scripts/test.sh --duration 5h --verify index
./project_0/scripts/verify.sh --run market5h_20261011T010758Z --mode index
./project_0/scripts/verify.sh --run market5h_20261011T010758Z --mode strict
~~~

Die CLI darf weder Snapshots noch Slots noch Timeout-Höhen manuell voraussetzen. Dauer wird genau einmal validiert (z.B. 10m/1h/5h/24h; gültige ganzzahlige Grenzen dokumentieren) und als typisierter MeasurementPlan/Run-Manifest durch Launcher, Rust-Capture, Analytics und Nachverifikation geführt. Optionen dürfen Overrides für Fachleute anbieten, müssen auf Konflikte prüfen und sich selbst dokumentieren. **Keine stille Umschaltung auf kürzere Capture-Walltime**: 10m bedeutet 600s tatsächlich gewünschte Beobachtung ab StreamReady, sofern kein offen ausgewiesener Fail-closed-Abbruch eintritt. Bootstrap, Capture, Drain, Historienprüfung und optional strict reconciliation besitzen getrennte Deadlines. Ablauf einer Uhr ≠ Proof von Source-Coverage.

Standardmäßig vier deterministisch bei 1/4, 2/4, 3/4, 4/4 der gewählten Gesamtdauer liegende Snapshots (auch bei nicht durch vier teilbaren Sekunden ohne Drift/Doppelzählung). Beispielabstände 10m→150s, 1h→900s, 5h→4500s, 24h→21600s. Entferne sämtliche impliziten *4-/min(4)-Annahmen in Capture, econ.rs-Report/Attribution, Snapshot-Fenstern und Tests oder setze sie über exakt eine gemeinsame Planquelle um. Snapshot-Beweiskette und Report-Metadaten müssen in 10m-, 1h-, 5h- und 24h-Konfiguration denselben Cutoff bezeichnen. 24h nicht durch starres 100.000-Slot-Gesamtlimit blockieren: strict slot-scans in dauerunabhängige, versionierte, bounded, überlappungsfrei/parent-proven Batches aufteilen, nicht nur max_slots hochsetzen.

Run-Manifest vor Start persistent erzeugen: run_id, UTC+monotonic Zeitpunkt/Definition, duration, Planversion, Schema, Snapshot-Grenzen, tested commit/source/binary hashes, provider-class **ohne URLs/Keys**, capture/transport/verification budgets, risk limits, permitted modes. Spätere Zustände append-only/journaled; niemals Roh-Evidence überschreiben. Durations und Grenzen **nicht** nachträglich aus dem letzten empfangenen Slot stillschweigend abkürzen.

### C. Transport-/Provider-Fix – dokumentierte 429-Kaskade, keine bloßen Limit-Erhöhungen

1. Inventar aller realen RPC-/WSS-Verbraucher des kombinierten Prozesses (Shadow bootstrap, health, simulation/winner, deployment, Market WSS, kanonischer Scanner, External Indexer) samt vorhandenem globalem/kundenseitigem Providerbudget und tatsächlicher paralleler Last erstellen. Für noch nicht belegte Details kurze gezielte, sichere read-only Diagnostik und Methoden-/Status-Logging ergänzen; **keine neue allgemeine Ursachenanalysephase**, kein mehrstündiger Lauf. Rotierte/private API-Keys niemals ausgeben.
2. Gemeinsame Provider-Awareness und zulässige Rate-/Concurrency-Limits pro Endpunkt und Workload herstellen; Token-Bucket/Leaky-Bucket oder gleichwertige deterministische Budgetregel mit Fairness. Historien-Verification **erst nach Capture/Shadow-Quiescence** beziehungsweise auf separatem qualifizierten Provider, nie im Hot Path. Für HTTP 429 Retry-After respektieren, exponentielles begrenztes Backoff mit Jitter, Circuit Breaker/Quota-exhaustion, endliches Recoverybudget und eindeutige Errors. WSS-Verbindungsversuche nicht endlos 429-Spam produzieren lassen. Providerkontingent nicht durch eine algorithmische Zahl „wegkonfigurieren“.
3. WSS-State-Machine anhand bestätigter Subscription-ACKs UND verwertbarer Anwendungsframes/Slotfortschritte: STARTING, READY, CAPTURING, DEGRADED, RECOVERING, RECOVERED, INCOMPLETE_ABORT/FINISHED. Erkenne mangelhafte Anwendungsliveness, nicht nur Ping/Pong. Dokumentiere Start/Ende jedes wirklichen Ausfalls (monotonic und UTC), letzte gültige Slot- und Payload-Evidence, erfolglose Reconnects, tatsächliche Wiederherstellung. Ein einziges anfängliches StreamReady beweist keine fünfstündige Abdeckung.
4. Bei anhaltender Transportstörung Fail-closed nach begrenztem, fachlich festgelegtem Recoverybudget **frühzeitig**: markiere Live-Coverage INCOMPLETE und stoppe teure Shadow-/Capture-Arbeit geordnet (sofern sichere Shutdown-Kette), statt Stunden „CAPTURE“ vorzutäuschen. Keine automatische Erhöhung von Budgets proportional zur Messdauer; ein 24h-Lauf braucht gleichwertige Fehlersemantik.
5. GAPs als zusammenhängende/überlappende zeit- und slotbezogene Ausfallintervalle behandeln, wiederholte 429-Reconnect-Versuche ohne echte Erholung nicht als hunderte autonome Gap-Recovery-Fälle in RAM halten. Trotzdem einzelne Roh-GAP-Meldungen und Ursache historisch journalisieren, Grenzen und Unsicherheiten kennzeichnen; Recovery erst mit nachgewiesenen finalized Grenzen/Events als vollständig markieren. Niemals unbekannte Anfangs-/Endslots erfinden oder Recovery-PASS trotz Lücke.
6. Deployment-Continuity separat mit providerverträglichem Retry-/Quotamanagement ausführen; ausgefallenes Deployment-RPC wird separat INCOMPLETE, nicht stillschweigend als validiert gewertet.

**Externe technische Voraussetzung:** Falls der Anbieter trotz budgetgerechter Clientlast alle notwendigen RPC/WSS/Archivzugriffe über Stunden sperrt, kann kein Code einen vollständigen 5h-Beweis garantieren. Verfügbarkeit/Quota/Archivhorizont müssen vor einem Langlauf durch kurze definierte Preflight-Probes nachgewiesen sein. Providerwechsel oder kostenpflichtiger Upgrade bedarf expliziter Benutzerentscheidung; niemals automatisch kaufen oder neue Zugangsdaten voraussetzen.

### D. Automatischer effizienter historischer Marginfi-Transaktionsvergleich – primärer Normalpfad

**Ziel:** Alle indexierten Program-Transaktionen für das **tatsächliche geplante UTC-Capture-Fenster** automatisch ermitteln, ohne Slots manuell zu setzen und ohne volle Solana-Blöcke der gesamten Kette pauschal herunterzuladen. Programm: **MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA**.

**Default-Workflow nach Capture und nachträglich für alte Runs:**

1. UTC start/end aus Run-Manifest bzw. vorhandenen nachweisbaren Reportdaten bestimmen. Bei abgebrochenen Captures gewünschtes Intervall und tatsächlich erreichte Beobachtungszeit getrennt halten. Für alten 5h-Run explizit 2026-10-11T01:08:38Z bis 06:08:38Z und 15.960s fehlende Slot-Evidence, jedoch **keine hardcodierten Werte im Programm**.
2. Unabhängigen historischen Provider über Solana-JSON-RPC **getSignaturesForAddress** (oder gleichwertige spezifizierte Index-API) abfragen, opaque Pagination-Cursor bis zuverlässig **vor** Anfangsgrenze zurückführen, blockTime==null behandeln. Beginne am aktuellen Historienende; sehr alte Daten/Providerpruning/Seitenlimits müssen als INCOMPLETE enden. UTC-/Blocktime ist nicht identisch mit T_rx; Grenzslots und zensierte/fehlende Zeiten explizit qualifizieren. Keine Annahmen über genau einen TX je Slot oder eine feste Signatur pro Programminstruktion.
3. Sämtliche indexierten Signaturen für den definierten Zeitbereich persistent und dedupliziert erfassen, einschließlich fehlgeschlagener; erfolgreiche TX für alle relevanten Instruktionen/Eventdiscriminators **vollständig prüfen**, nicht durch unbewiesene „nur potenzielle“ Vorfilter Liquidationen verlieren. Per **getTransaction** vollständige relevante Roh-Transaktionen abrufen, äußere und innere CPI, Legacy/v0/v1 soweit RPC unterstützt, loaded addresses/LUTs, Logs, gesonderte start/end/settlement-Semantik, Success, fee payer, Lamport-/Token-Deltas und optional historisch korrekt bewertete USD-Daten qualifizieren. Nicht unterstützte Transaktionsversionen/Pruning/fehlende Payload: UNKNOWN, nicht „kein Event“.
4. Für jede gefundene Liquidation Signatur, Slot, TX-Index (sofern beweisbar), Instructions-/Event-Index, Typ, betroffene Konten, Actor/Liquidator, Gebühren-/Mengen-Provenienz, Solscan-Link und die Beweisquelle erfassen. Vier Kategorien CLASSIC, RECEIVERSHIP, FORCED_DELEVERAGE, BANKRUPTCY_SETTLEMENT; start_liquidation und end_liquidation dürfen nicht zu doppelten realisierten Liquidationen werden. Keine Kauf-/Trade-Aktionen.
5. Innerhalb des Prüfers **eigenständig** dekodieren; nicht die Bot-Eventzählung als Input für externe Wahrheit benutzen. Belegte Protokoll-Event-Discriminators und Pinned-Source/Semantik als Autorität verwenden; bekannte reale Mainnet-Positivfixtures aus Repo verifizieren bzw. aus öffentlich verifizierbaren Signaturen ergänzen (nicht behaupten, sechs Fixtures seien vorhanden, ehe sie lokal nachgewiesen sind). Negative failed-TX, foreign-program frame, log truncation, CPI-/LUT-only, versionierte TX testen.
6. **Auto-Join** Signatur + Slot + nachgewiesener Instruktions-/Event-Position zu live transactions, canonical events, Shadow evaluations, Winner/Capital/Economics; je Klasse MATCH, MISSING_LIVE, MISSING_LIQUIDATION, TYPE_MISMATCH, AMOUNT_MISMATCH, SHADOW_MATCH, SHADOW_NO_EVIDENCE, HISTORICAL_UNKNOWN. Live-Zählung bleibt getrennt von Canonical-/Index-Zählung; eindeutige Grenzslot-Semantik und unsichere Zuordnungen als UNKNOWN. Keine Profit-Zuschreibung allein aus Marktaktivität; Market Gross Edge ≠ executable Edge ≠ realized PnL. Fehlende Tokenpreise, Gas/priority/rent/DEX-Kosten niemals als 0 imputieren.
7. Optional zweiter tatsächlich **unabhängiger** Indexprovider (Konfiguration/Abdeckung/Quellprovenienz dokumentieren). Solscan vor allem für Explorer-Links; pro-API nur optional, kein bezahlter Dienst als notwendige Voraussetzung, kein Browser-Scraping. **Adressindex-Completeness nicht unterstellen:** LUT-only/Indexer-Coverage, CPI, providerseitige Lücken sind als eigene Risiken zu qualifizieren.

**Drei explizite Prüfmodi:** index (Default, schnelle RPC-Indizierung), crosscheck (unabhängige zweite Quelle bei Verfügbarkeit), strict (finalized vollständige Slot-/Block-Reconciliation mit dokumentierter Parent-Link-/Skip-Proof-Semantik). Ein grüner Index- oder Zweiindex-Abgleich = maximal **INDEX_VERIFIED/CROSSCHECK_VERIFIED**. Der bisherige C0-E1-MASTER-Gate verlangt strengere CANONICAL_VERIFIED-Abdeckung zusätzlich zu Capture-/Economics-Qualifikation. Strict darf fehlschlagen/OPEN bleiben, ohne den nutzbaren Indexbericht zu vernichten. **Falls vollständige Canonical-Zusage ohne große Blockdaten nicht erreichbar ist, offen ausweisen, nicht das Gate heimlich absenken.**

**Resume statt Re-Run:** nutze run_id + manifest/data/schema-version als Identität, append-only Signaturjournale, stabilen Pagination-Cursor, Transaktionsstatus, atomare idempotente Checkpoints, Abschnittsbeweise, Retry-/Quota-Status, Bounded Queues; beim nächsten Aufruf nur unvollständige Bereiche/Fälle bearbeiten. Keine nachträgliche Manipulation originaler Capture-Daten. Ausgabe maschinenlesbare JSON/JSONL + lesbarer Bericht, Anbieter/Abdeckung/RPC-Calls, Bytes übertragen, Datenmenge auf Disk, CPU-/RAM-Max soweit messbar, Zeit/Fehlergrund/Unbekanntanteile; ohne API-Schlüssel oder kompletten geheimen Endpoint.

### E. Strict Canonical bleibt qualifiziert, wird aber streamingfähig/fortsetzbar

Der bestehende getBlocks/getBlock-Scanner behält seinen Proof-Vertrag (finality, Parent-Chain, Skips, Decoder, Economics). Baue statt einer einzelnen großflächigen Abfrage **endlich budgetierte Abschnittsscans** mit journaled Resume und Bounded Prefetch/Retention; Protokollinkonsistenzen strikt fail-closed. Die starre 100.000-Slot-Gesamtgrenze darf eine 24h-Messung nicht grundsätzlich ungültig machen; die Limitierung gilt sinnvoll pro batch/operation. Überprüfe die aktuelle 8er-Prefetch-/Retry-Semantik gegen HTTP 429; Nebenlast aus gleichzeitigen Requests und Massendownloads nicht gegen denselben Provider ohne ausreichende Quota zulassen. Ursprüngliche kanonische Source-of-Truth und per-slot Fehler/Parent-Proof nachweisbar erhalten. Keine Speicher- oder Netzwerkvolumen-Garantie aus einer Schätzung ableiten.

Das Standardprofil **index** darf schnell aussagekräftige Markt-Liquidationen liefern, selbst wenn **strict** mangels Providerzugang nicht qualifizierbar ist. Beide Ergebnisse müssen strikt getrennt in der Summary stehen.

### F. Echter Ressourcen- und Speicherschutz

Vorhandene 4h-Evidence (~43 GiB) nach Dateigruppen, Writer-Raten, RAM-Peaks, Queue-Drops, Snapshotgrößen und I/O messbar auseinanderhalten. **Indexing allein behebt weder großen Shadow-JSONL-Output noch RAM-Overflows.** Limitierte Worker, streamingfähige Datenstrukturen, sichere Checkpoints/Rotation bzw. verlustfreie Kompression dort, wo originalgetreue Evidence erhalten bleibt. Laufzeitabhängige Speicher-Preflight nach empirischen Bytes/s + Sicherheitsreserve, Reserve über gesamte gewählte Dauer, während Capture freien Platz überwachen; bei Disk-Engpass kontrollierte fail-closed Terminierung ohne stillen Verlust. Keine vorhandenen Evidence-Dateien automatisch löschen. Eventuelle Kompression erfordert identitätsgetreuen Replay-Test und manifestierte Hashes.

### G. Verbindlicher historischer Replay-Gate des fehlgeschlagenen Runs

**Nicht erneut fünf Stunden messen.** Als ersten echten End-to-End-Historienfall vorhandenen Run **market5h_20261011T010758Z** wählen, sobald Dateien auf dem ausführenden Host vorhanden sind. Die CLI muss Start/Ende **aus Report** ableiten, nicht die bekannten UTC-Konstanten oder Slot 455473465 in Code einbauen. Der Auftrag gilt nur als erfüllt, wenn entweder ein überprüfbarer indexierter Gesamtfensterbericht mit Detailmatches existiert oder ein präziser HISTORICAL_INCOMPLETE-Grund genannt wird; Null Liquidationen bei fehlerhaftem Index ist niemals PASS. Schon erhaltene 181 Live-TX inkl. success/fail mit historischen Index-TX vergleichen; 01:42:38Z bis 06:08:38Z separat als LIVE_COVERAGE_INCOMPLETE ausweisen. Erhaltene Shadow-Entscheidungen nur für belegte Live-Intervalle qualifizieren; historische Indexdaten ersetzen keine historische T_rx-Entscheidung.

### H. Report-Vertrag und Gate-Semantik

Getrennte maschinenlesbare Felder, ohne Rückwärtskompatibilität der bisherigen Ledger künstlich zu brechen:

- **Clock:** requested_seconds, elapsed_seconds, boot/warmup_seconds, capture_start_utc/end_utc, cutoff_provenance, observed_active_seconds vs transport_covered_seconds, last_live_slot_age_s, manifest/commit identity.
- **Transport:** WebSocket connected/ACKed, gap_intervals, gap_count_raw, gaps_merged, reconnect_attempts, reconnect_successful, first_failure_at, last_healthy_at, HTTP_429_count, coverage_outage_seconds, recovered/unrecovered, log_truncation, ingress_overflow.
- **Index:** provider identity, exact time/range provenance, cursor-completion, raw signatures, unique signatures, failed/successful, TX details retrieved/unknown, liquidation counts per type, fees/amounts known-vs-unknown, RPC volume, discrepancies, second-index agreement.
- **Strict:** per-range parent/skip/finality evidence, progress, unresolved, resume, canonical_complete and blocker.
- **Economics:** canonical Market Gross Edge / executable Shadow Net Edge / unknown costs / realized PnL **separat** und keine Hochrechnung ohne ausreichendes Sample.

Statusklassen: CAPTURE_CLOCK_COMPLETE, LIVE_COVERAGE_INCOMPLETE, INDEX_VERIFIED, INDEX_MISMATCH, HISTORICAL_INCOMPLETE, CROSSCHECK_VERIFIED, CANONICAL_VERIFIED, CANONICAL_INCOMPLETE, ECONOMICS_INSUFFICIENT_SAMPLE, C0_E1_MASTER_PASS oder C0_E1_NOT_QUALIFIED. **CAPTURE_CLOCK_COMPLETE darf zusammen mit LIVE_COVERAGE_INCOMPLETE gelten. INDEX_VERIFIED impliziert niemals C0_E1_MASTER_PASS.** Fehlende/verdächtige/zensierte Daten sind UNKNOWN, keine Null.

### I. Ausführung in klaren Paketen (KEINE erneute allgemeine Analyse)

**P0 – Baseline-Freeze (kurz):** Remote/main+aktuelles Ledger lesen, gegen reale Dateien prüfen, zu 429-Methoden/Quotas bestehende Logevidenz zuordnen. Details, die nicht belegbar sind, als offene externe Voraussetzungen dokumentieren. Kein erneuter allgemeiner Planungsdurchlauf.

**P1 – CLI + gemeinsamer MeasurementPlan:** Zeitparser, deterministische 4 Snapshots, monotonic cutoff, UTC manifest, vorhandene quick/full-Logik bereinigen, End-to-End-Start ohne zweite manuelle Zeitvariable. Unit-Gates für 10m, 1h, 5h, 24h und ungerade Sekundendauer.

**Umsetzungsstand 2026-10-11:** P1 implementiert und lokal technisch geprüft; Details/Kommandos im `MEASUREMENT_LEDGER.md`, Evidence in `docs/evidence/C0E1_P1_20261011/`. CLI `scripts/test.sh --duration 10m`; Sekundenbereich 1..86400, exakte Nanosekunden-Viertel. Keine Mainnet-/Langlauf-Qualifikation. Nach expliziter Paketfreigabe folgt P3, danach P2; P4–P6 bleiben offen.

**P2 – Transport/Provider:** WSS-Liveness-State, begrenzte 429-/Reconnect-Recovery, Gap-Intervalljournal + Semantik, Early fail-closed, getrennte Quotas; Simulation 527 Fehler, Provider-Rate-Limit, WSS Frame-Unterbrechung und nachgewiesene tatsächliche Recovery.

**Umsetzungsstand 2026-10-12:** P2 implementiert und lokal qualifiziert: gemeinsame
RPC-/WSS-Admission, Retry-After/Circuit, feste 120-s-/8-Versuchs-Recovery,
ACK+Slotfortschritt als Startbarriere, GAP-Intervalljournal und früher geordneter
INCOMPLETE-Abbruch. 240 bzw. 238 Bibliothekstests PASS (txv1 an/aus), 9 Binary- und
4 CLI-Tests PASS; Release-Build PASS. Evidence/Scope/Provider-Inventar unter
`docs/evidence/C0E1_P2_20261012/`. Kein neuer Mainnet-Lauf, kein Provider-/5h-PASS.
P4–P6 bleiben offen; P2-Status melden, danach nächstes Paket abstimmen.

**P3 – Index-Verifier:** Abfrage per UTC-Manifest, Pagination + Null-Blocktime + Versions-/LUT-/CPI-Support, unabhängige Klassifikation, Event-/Signatur-Differenz, atomare Resume-Journals, optional Crosscheck; echte historische positive TX-Fixtures mit nachgewiesenen Signaturen + negative Fixtures.

**Umsetzungsstand 2026-10-11:** P3 lokal qualifiziert (26 Offline-/Fixture-Tests, 4 P1-CLI-Regressionsprüfungen). `scripts/verify.sh --run <run-id> --mode index`; realer 5h-Abgleich nach 2 Seiten/2.000 Signaturen mit `RPC_RATE_LIMIT` gestoppt, Cursor gesichert, historische Qualifikation OPEN. Kein CANONICAL-/MASTER-PASS. Details und Resume-Befehl im `MEASUREMENT_LEDGER.md`; danach P2, kein Vorziehen von P4–P6.

**P4 – Strict partition/resume + Ressourcen:** strenger Kanonikpfad in Batches, Parent-Link/Skip Proof, rate-limited serial/bounded requests, Speicher-/Disk-Schutz und Evidenz-Integrität.

**P5 – Bericht/alte 5h-Evidence:** automatische Verifier-Ausführung auf vorhandener 5h-Run-ID, Ergebnis gegen 181 Live-TX, klare Index/Capture/Strict-Trennung, belegte Fehlerquote; keine 5h-Neumessung.

**P6 – Qualifikation/Review/Push:** Rust fmt, locked release build, relevante Unit-/Integrationstests, Clippy, Negativ-/Positiv-Fixtures, kontrollierter WSS-Disconnect, 429-Dauer-Ausfall, Replay-Resume, 10m Mainnet smoke **nur nach Provider-Preflight und mit Anwenderfreigabe für externen Live-Read**, Source-Identity sichern. Keine 5h/24h-Mainnet-Neuläufe ohne Benutzerfreigabe nach überprüfbaren Vor-Gates. Tatsächlich nicht ausgeführte Tests = NOT_RUN, nicht PASS.

### J. Harte Abnahmekriterien und Stop-Gates

**Architektur-PASS für Astra** nur bei Beweisen für:

1. Eine beliebig zulässige Dauer pro Aufruf, keine zweite manuelle Snapshotvariable, ein einziges identisches Run-Manifest/Cutoff-Modell in Shell/Rust/Audit; 10m ist reale Capture-Zeit nach READY, kein walltime-gekürzter Pseudo-Test.
2. Zeitablauf ist getrennt von kontinuierlicher Datenverfügbarkeit; 15.960s ohne Slot-Beweis werden sichtbar INCOMPLETE statt „5h MARKET PASS“.
3. 527 transportbedingte GAPs brauchen bounded state und korrekt erhaltene Intervalle; kein Silent-Drop, kein unendlicher 429-Reconnect-Loop, nachgewiesene ACK- und Anwendungsdaten nach Recovery.
4. Rate-Limits, Provider-Quota und HTTP 429 werden korrekt behandelt; ein endlicher Verbindungs-/Historien-Ausfall führt zu einem kontrollierten Abbruch/Checkpoint. Fehlt ausreichende externe Providerkapazität, bleibt der Mainnet-Gate BLOCKED trotz fehlerfreien Codes.
5. Historischer 5h-Run kann unabhängig **ohne manuelle Slots, ohne neue Capture** über vollständige UTC-Zeiten indexiert werden; geprüfte Signaturen, Liquidationen, verlorene Live-TX, unbekannte TX werden maschinenlesbar berichtet.
6. Jeder positive Liquidationstyp und echte historische Erfolgs-/CPI-Fälle sind durch Fixtures nachgewiesen; keine „NULL=VERIFIED“-Behauptung durch leeren/abgeschnittenen Index. Pagination und Resume erzeugen deterministisch denselben Satz wie ununterbrochene Verifikation.
7. index/crosscheck/strict sind beweislogisch unabhängig ausgewiesen; strict behält vollständige Finality-/Parent-/Skip-Pflichten; >100.000 Slots können segmentiert behandelt werden.
8. Kein blindes RAM-/Datei-Wachstum, dokumentierte Speichergrenzen, keine Löschung alter Evidenz, keine unbekannten Economics als 0, LIVE=false und submission=false.
9. Git source SHA/Tests/Clippy und echte Read-only-/Mainnet-Probes nachvollziehbar, keine BSC-Dateien berührt.
10. Erst nach vorherigen Gates **und** dokumentierter Provider-Archiv-/Stream-Kapazität ist ein 5h-Mainnet-Retest technisch sinnvoll. **Niemand kann den fehlerfreien Abschluss eines zukünftigen fünfstündigen Provider-abhängigen Mainnet-Laufs garantieren.** Beim externen Ausfall muss das System jedoch früh, verständlich, beweiserhaltend und ohne Scheingültigkeit reagieren.

### K. Git-/Sicherheits- und Abschlussvertrag für Astra

- Nur project_0/** ändern; vorhandene native Pipeline, Shadow-Economics, Source-Freeze, FIX-139, LIVE-/Submission-Gates beibehalten. BSC-only-Commits auf main dürfen nicht versehentlich rückgängig gemacht werden.
- Remote HEAD vor jedem Commit abgleichen; bei parallelen Pushes re-fetch/rebase/fast-forward; nicht blind force-pushen.
- Arbeitspakete als kleine überprüfte Commits; in **MEASUREMENT_LEDGER.md** aktuelle Resultate mit SHA, Beweispfad, Testkommandos, Ergebnissen und OPEN-Risiken dokumentieren; historische Einträge unverändert. Große Raw-Runs/API-Keys/Sensitivdaten nie nach Git pushen.
- Keine bezahlten Provider-Aktionen, keine unautorisierten 5h-/24h-Läufe, keine LIVE- oder Submission-Tests.
- Astra-Abschlussbericht: Ausgangs-/End-HEAD, genaue Dateien, nachweislich behobene Ursachen vs verbleibende Providerfragen, Testmatrix PASS/FAIL/NOT_RUN, gemessene bytes/RPC-Calls/Storage, Ergebnisse des historischen 5h-Verifiers, Index-vs-Strict-Status, weitere Gating-Kriterien und exakt ein dokumentierter CLI-Aufruf für die spätere Benutzerfreigabe.

**Endzustand:** Bedienung über eine dynamische Dauer (10m/1h/5h/24h), providerbewusste robuste Laufzeit, früher Fail-closed bei Ausfällen, gezielte historische Marginfi-Index-Liquidationsprüfung einschließlich CPI/LUT, reproduzierbarer automatischer Vergleich gegen Bot und Shadow, separate streng kanonische Completion, Resume nach RPC-429, echte ressourcenbezogene Evidence. **Kein Claim auf alle Probleme behoben oder 5h-Verwertbarkeit vor realem grünen Gate.**
