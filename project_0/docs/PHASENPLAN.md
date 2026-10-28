> **VERBINDLICHER NEUER ENTSCHEIDUNGSSTAND 2026-10-25 (P0-DEC-01…15/G-1):** Siehe den normativen Abschnitt am Ende dieses Phasenplans. **Alle 15 Entscheidungen ab sofort vollständig offline implementieren – einschließlich echtem nativen Controller, typisiertem Protobuf, gemeinsamem Gateway, React-Reiter, Rückweg und E2E.** Historische „später entwickeln“-/„GUI erst später“-Verschiebungen gelten insoweit nicht mehr. *Bestehende echte Mainnet-/Provider-/FREE_ONLY-/LIVE-/Signierungs-/Werttransfer-/Deploy-Gates bleiben bestehen; keine Freigabe und kein PASS durch diese Planänderung.*

> **AKTUELLER C0-E1 ASTRA-MASTERAUFTRAG (2026-10-11):** Siehe den neuen verbindlichen Abschnitt „C0-E1 — ASTRA MASTER IMPLEMENTATION CONTRACT 2026-10-11“ am Dokumentende. Der fehlgeschlagene 5h-Run (527 GAPs, langanhaltende 429, 15.960s fehlende Slot-Evidence, 0 kanonische Blocks) ist analysiert. Implementierungs- und Mainnet-Requalifikation sind OFFEN; LIVE/Submission gesperrt. Ältere C0-E1-Handoffs sind historische Evidence, kein erneuter Run-Befehl.

> 2026-10-09: canonical full-slot scanner and typed four-path decoder implemented locally; master remains PARTIAL. Current resume/evidence and exact remaining steps are at the top of MEASUREMENT_LEDGER.md. No 4h qualification or LIVE release.

# DarkBot — Project 0 — Produktions-Phasenplan v4.5.2 (First Realized Profit)

> **VERBINDLICHE REVISIONSEINTEILUNG (Nutzer, 27.10.2026) — REV.1.0 / REV.2.0 / REV.3.0.** Die REV-Markierungen gelten ausschließlich für noch offene Restarbeit. Bereits implementierter oder `OFFLINE_VERIFIED` Code bleibt vollständig erhalten; kein Rückbau, keine Neuentwicklung erledigter Funktionen.

> **REV.1.0 = gesamtes bisher verbindliches Endprodukt vor Wirtschaftlichkeits-/Echtgeldnachweis.** Long-Tail UND Race im selben Rust-Core; S1-RACE/Binary-Geyser, Jito Single/Bundle/gRPC/Prewarm, alle Fundingklassen inkl. JIT-ExactIn, komplette C0-E1/H0/P7/p(win)/P4B, P6-MT/M4, Multi-Wallet/Reservierungen/Inventory/Rebalancing, DEC-01…16, Frontend/CLI/Gateway/Protobuf und P8/P9 mit Safe-LIVE, langem Dauerbetrieb und finalisiertem Netto-PnL bleiben REV.1.0-Pflicht. Die frühere Reihenfolge 'First Profit vor Gesamtfertigstellung' in §§0.6/7/13 und PHASENPLAN.json ist insoweit durch die neue Nutzerentscheidung ersetzt; ihre fachlichen Safety- und Abnahmekriterien bleiben bestehen.

> **REV.2.0 = zusätzliche optionale Optimierungen** wie weitergehendes CPU-/SIMD-/PGO-/Cache-/Routing-/Provider-/Region-Tuning oder ein neuer spekulativer kostenloser Pre-Execution-Sensor nach messbarem Nutzen. **REV.3.0 = optionale hochkomplexe Spezialtechnik** wie eigener Raw-Shred-/Validator-Stack, Direct-TPU-/Leader-/QUIC-Spezialpfad, eigener zusätzlicher lokaler DEX-Router oder Kernel-/NIC-Spezialisierung. Bereits vorhandene Implementierungen bleiben erhalten.

> **Alle bislang verpflichtenden K-Capability-Untersuchungen gehören REV.1.0 an; lediglich der bedingte neue produktive Spezialausbau ist REV.2.0/3.0.** Fachlich notwendige technische Vorabmessungen (z. B. C0-MO vor der evidenzbasierten MT6–MT10-Promotion) bleiben gültige Gates, nicht vorweggenommene Wirtschaftlichkeits- oder LIVE-Freigaben. Ohne echte Genehmigung und Evidence bleiben Mainnet-/Provider-/FREE_ONLY-/LIVE-/Signier-/Geld-/Langlaufaktionen NOT_RUN/BLOCKED; niemals PASS fingieren. `PHASENPLAN.md` bleibt fachliche Autorität, `BUGS.md` Statusautorität.


> **VERBINDLICHER SPEICHERVERTRAG (16.10.2026; verschärft):** Project_0 nutzt intern im gesamten produktiven System ausschließlich native/typisierte Rust-Daten und binäre Ereignisse, Protokolle, Journale und Übergaben. JSON ist ausschließlich am unmittelbar notwendigen externen Solana-JSON-RPC-/WSS-Protokollrand zulässig; kein JSON-Value, JSON-Encode/Decode oder JSON-Text als produktives internes Datenmodell, Zwischenformat, Control-/Evidence-Format oder Primär-/Sekundärspeicher. Historische Beweise bleiben unverändert erhalten. Details und Vorrangregel am Dokumentende unter „Bindender JSON-FREE-INTERNAL-Vertrag“. Umsetzung und Qualifikation sind OFFEN; keine LIVE-Freigabe.

> **VERBINDLICH: NUR PRIORITÄT UND ERSTER PILOTUMFANG GEÄNDERT (2026-10-13).** v4.5.2 **streicht, optionalisiert und verwirft keine** ursprünglich verpflichtende Phase, Capability oder Abnahme aus v4.5/v4.5.1. Race/Binary-Geyser, Prepared TX, Jito Single/Bundle/gRPC, Prewarm und Race-Benchmarks, JIT-ExactIn und alle Fundingklassen, vollständige Markt-/Chain-Qualifikation (C0-E1/Index/Strict), H0/Winner/Konkurrenz/P7/P4B, P6-MT/M4, persistente atomare Multi-Asset-/SOL-/Fee-/Tip-/Rent-Reservierungen, Inventar-Rebalancing, P8/P9, K-Mess-/Entscheidungsphase und Frontend **bleiben verbindlicher Entwicklungsumfang**. Profit-First verkürzt nur den für **einen** sicheren LIVE-Pilot benötigten Critical Path; kein Gate und keine LIVE-Freigabe werden dadurch vorweggenommen. Extern unmögliche kostenlose Fähigkeiten bleiben mit Evidence `EXTERNAL_BLOCKED`, nicht `DONE` oder gestrichen. Experimentelle K-Technik muss untersucht werden; ein Deployment ist weiterhin an Machbarkeit, wirtschaftliche Evidence, Safety und FREE_ONLY gebunden.

> **VERBINDLICHE PRÄZISIERUNG v4.5.1 (2026-10-12):** Die in §0.4, P4A, P5, P6-MT, P7, P8 und C0-E1 Abschnitt L integrierten Ergänzungen gelten zusätzlich zum v4.5-Architekturvertrag. Alte C0-E1-CLI-Beispiele sind, soweit widersprüchlich, von Abschnitt L abgelöst; der kanonische Master-PASS und alle LIVE-/Submission-Gates bleiben unverändert. Dies ist Sollplanung, keine Implementierungs-/Mainnet-Qualifikation.

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

**BELEGT — Yellowstone (Quellenstand 2026-10-22):** `SubscribeDeshred` liefert pre-execution Transactions aus eingehenden Shreds. Das Yellowstone-Changelog dokumentiert die Open-Source-Implementierung seit 2026-07-02; die ältere README-Aussage zu Triton-only/`UNIMPLEMENTED` ist insoweit überholt. Ob ein konkreter Anbieter (z. B. PublicNode) den Stream freischaltet und kostenlos bereitstellt, bleibt `NOT_RUN` und erfordert einen Capability-Test. `SubscribeDeshred` bleibt ein optionaler K-INGRESS-PRE-Sensor, kein Ersatz für autoritative Geyser-State-Updates.

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
→ unter FREE_ONLY nicht autorisiert, auch nicht bei positivem kontrafaktischem Profit
→ nur nach separater ausdrücklicher Änderung der verbindlichen Kostenpolitik
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

**FREE_ONLY — harte aktive Betriebsregel v4.5.1:** Keine verpflichtenden kostenpflichtigen API-, RPC-, Geyser-, Routing-, Submission- oder Server-/Infrastruktur-Abonnements; **kein automatischer Paid-Fallback**, auch nicht bei HTTP 429. Paid-Capabilities können dokumentiert/kontrafaktisch bewertet werden, bleiben im aktiven FREE_ONLY-Profil aber BLOCKED. Ein Wechsel braucht ausdrücklich eine gesonderte Benutzerfreigabe. Providerweite harte Quoten gelten auch bei Parallelität, dokumentiert als requested/allowed/effective. FREE_ONLY bedeutet nicht fee-free: Base-/Priority-Fees, Jito Tips, Swap-/DEX-Kosten, Price Impact, Slippage, Rent, Gebühren gelandeter fehlgeschlagener Transaktionen und Kapitalbindung sind vollständig zu bewerten.

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

## 0.6 FIRST REALIZED PROFIT PATH — höchste Umsetzungspriorität (binding 2026-10-13)

**Vorrangiges Produktziel: der frühestmögliche nachweisbare positive kumulierte REALIZED NET PNL des bestehenden Project_0-Rust-Cores im kontrollierten Mainnet-Betrieb.** Kein Profit wird garantiert. Ein positiver Shadow-Edge, eine positive lokale Simulation, ein BUY-Receipt, eine nicht verkaufte Collateral-Position und ein einzelner positiver Trade trotz größerer Fehlversuchskosten sind **kein** Nachweis des Break-even. Alle Gebühren, Verluste und Inventarverkäufe gehen in die Bilanz ein.

**Konsolenbetrieb ist Produktionsbetrieb:** CLI, Logs, Replay, Ledger, Chain-Reconciliation und harte Safety-Gates reichen für die erste freigegebene Execution-Klasse; GUI, LLM, vollständige Wettbewerbsmodelle und generelle Race-Infrastruktur sind **keine First-Profit-Voraussetzung, bleiben jedoch für das endgültige Produkt verbindlich**. Unabhängige Entwicklung daran darf parallel erfolgen.

**Kein Profit als Abbruch der Roadmap:** Falls FE-00 keinen aktuellen Kandidaten findet oder FE-07 mit NO_TRADE/LOSS/UNKNOWN endet, wird ausschließlich das nicht freigegebene LIVE angehalten. Sämtliche Produkt-, Race-, Markt-, JIT-, M4- und Wallet-Entwicklungstickets bleiben weiter verpflichtend bzw. evidenzbasiert blockiert; der erste Profit ist **kein Startgate** für die restliche Implementierung.

```text
CURRENT MAINNET source/program pin (P1A) + gezielte C0/H0-Evidence für die gewählte Klasse
→ vorhandene S0 / S1-LT / S2 / S3-CORE wiederverwenden und qualifizieren
→ Produkt-P2 / P3 / P4A / P5 für GENAU EINE tatsächlich ausführbare Klasse qualifizieren
→ targeted S4-Parity + Produkt-P6-LT Shadow + realer Kandidaten-/Kostenbeleg
→ persistentes SINGLE-FLIGHT Safety/Wallet/UNKNOWN-Reconciliation-Gate
→ explizite individuelle Benutzerfreigabe mit Gesamt-Spend- und Verlustgrenze
→ P8-LT CONTROLLED MAINNET: genau ein begrenzter Versuch, danach automatischer HALT
→ Signature + finalisierte Chain-Deltas + Fees/Tip/Swap/Rent + Inventarabschluss beweisen
→ kumulierter REALIZED NET PNL nach allen eigenen Fehlversuchen > 0?
   JA: FIRST_REALIZED_PROFIT_EVIDENCED; kontrolliert wiederholen / Kapital erhalten
   NEIN oder UNKNOWN: STOP, Ursache analysieren, kein Gewinnanspruch, kein Auto-Rearm
```

**Funding-Entscheidung anhand aktueller Simulation und Kosten, nicht Enum-Reihenfolge:**
- Zuerst den am schnellsten qualifizierbaren wirtschaftlich plausiblen Pfad wählen: `NO_SWAP_INVENTORY` **nur** bei wirklich vorhandenem Liability-Inventar, protocol-exaktem Repay und einem belegbar ausführbaren Weg zur späteren Collateral-Verwertung; alternativ `ZERO_PREFUND_SAME_TX` nur bei aktueller, erlaubter und ausführbar simulierter Swap-Route mit Mindest-Output-Floor, der den Repay deckt. Beide sind zu überspringen, wenn Profit, Route oder Chain-Semantik UNKNOWN sind.
- `JIT_SHORTFALL_EXACT_IN` ist für den **vollen Produktumfang verpflichtend**, blockiert aber den ersten begrenzten Profitversuch mit einem *anderen vollständig qualifizierten* Fundingpfad nicht. Fehlt ohne JIT jeder wirtschaftlich ausführbare Pfad, wird JIT auf die Critical Path vorgezogen, niemals simuliert oder fingiert.
- Tagging/Premium-Instruktionen aus 0.1.12 sind für diesen Pfad **nur** zulässig, wenn der dazugehörige Stand nachweisbar deployed und P1B→P1A promoted wurde. `T` ist nur dann Pflicht, wenn der freigegebene Ablauf Tagging benutzt.

**Keine Sicherheitsabkürzung:** Der vorhandene `P0_MODE=live`-Block bleibt bestehen, bis die gewählte Klasse ihren eigenen P8-LT-Pilot-Gate belegt. Ein Pilot darf **nicht** bloß `live_enabled=true` setzen: autorisierter, codegebundener One-Shot, maximal ein in-flight Attempt, ein kontrolliertes Wallet-/Mint-Budget, persistente Holds für Liability/SOL/Fee/Tip/Rent und `SUBMITTED/UNKNOWN` über Restart hinweg, Reconciliation vor Freigabe, automatische Abschaltung und manuelle Neubewilligung sind Pflicht. Bei Unknown State, unklarer Nonce/Blockhash-Lifetime, Feed-Gap, Health-/Fee-/Cost-UNKNOWN oder Portfolio-Risiko STOP. Für paralleles LIVE gilt weiterhin die vollständige globale P6-MT.11/P8-RES-Reservation.

**Scope statt Schein-Master-PASS:** Der vollständige C0-E1-v4.5.1-Markt-Census, Crosscheck und Strict-Master bleiben eigenständig und OPEN, bis ihre Gates real bestanden sind. Sie dürfen das Initial-Pilot nicht *allein aufgrund fehlender Vollmarktabdeckung* blockieren, sofern **für den konkreten ausgewählten Kandidaten** aktuelle Chain-State-/Programm-/Bank-/Oracle-Parität, Quellenfrische, komplette TX-Simulation, Gebühren, realistische Exit-Route, Shadow-Replay und P8-Safety unabhängig bewiesen sind. Fehlende Kandidatenbelege oder nicht hinreichend verifizierte Eingangsquelle blockieren den Pilot dagegen zwingend. Pilot-Evidence ist niemals als vierstündiger Market-Master-PASS auszugeben.

**Nach dem ersten nachgewiesenen positiven kumulierten Ergebnis:** kleine wiederholte, je Versuch begrenzte kontrollierte LIVE-Schritte nur mit neuer dokumentierter Freigabe. **Weiterhin verpflichtend umzusetzen und zu qualifizieren:** alle Fundingklassen inklusive `P4A-JIT`, `P6-MT`/M4, P8-RES/INV und parallele Walletverwaltung, H0/P7/P4B, vollständiger C0-E1-Index-/Strict-Master, S1-RACE/Binary-Geyser, Jito Single/Bundle/gRPC/Prewarm, Race-Benchmarks und Race-LIVE-Gates, K-Untersuchungen, Frontend sowie P9. Diese Arbeit darf bereits vorher parallel laufen; ihre Pflicht hängt **weder von positivem FE-07 noch von prognostiziertem Race-Mehrgewinn ab**. Messungen entscheiden über Reihenfolge und produktive Optimierungspromotion, nicht über Streichung. Gewinne werden zuerst gegen Verlust-/Liquiditäts-/Fee-Puffer gerechnet; KI-Credits oder zusätzliche Infrastruktur sind **keine automatische Ausgabe** und bleiben bei laufenden Kosten unter §0.4 FREE_ONLY gesperrt, bis eine separate ausdrückliche Kostenpolitik-Freigabe erfolgt.

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

WSS/RPC sind nur als externe Solana-JSON-RPC-/WSS-Transportgrenzen in den folgenden Anwendungsfällen zulässig; JSON darf nicht hinter die externe Adaptergrenze gelangen (maßgeblich ist der spätere JSON-FREE-INTERNAL-Vertrag):

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

# 3. Historischer Ausgangszustand — Snapshot 2026-09-30

## 3.1 Eigenes Repo

**HISTORISCHER SNAPSHOT, NICHT HEUTIGER IST-ZUSTAND:** Die folgenden Zeilen bewahren den Stand vom **2026-09-30**; insbesondere „Project-0 Production Rust: noch nicht aufgebaut“ und „native/: wird durch S0 eingeführt“ gelten **nicht** mehr als aktuelle Implementierungsaussagen. Auf `main` (2026-10-23, zuletzt geprüfter Projekt-Code bis `2605556c`) existiert der produktive Rust-Workspace `project_0/native/` mit **fünf** Cargo-Mitgliedern (`common`, `solana`, `liquidation`, `runtime`, `reporter`); die Runtime enthält unter anderem `orchestration`, `execution`, `market`, `analytics` und `storage`. Der reale Entwicklungs-/Qualifikationsstand folgt aus Code, `STATUS.md` und aktueller Evidence. Diese historische Momentaufnahme erteilt keine Freigabe.

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

Execution-Deadline (`P0_DEADLINE_NS`) und Opportunity-Alter zählen ab `T_rx`, nie ab `T_detect`; `T_detect` erst nach bestandenem Economic Gate (V2-04, 25.10.).

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

# 7. Gesamtphasenplan — FIRST PROFIT vor Ausbau

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Ältere First-Profit-vor-Vollausbau-Reihenfolge ersetzt; REV.1.0 umfasst alle bisher verpflichtenden Produktanforderungen.

```text
  BASELINE + TARGETED MARKET EVIDENCE
  P0/P1A + C0/H0 für genau eine belegte Mainnet-Opportunity
               +
  S0 → S1-LT → S2 → S3-CORE
               ↓
      Produkt-P2 → P3 → P4A → P5
               ↓
      S4 targeted Parity + P6-LT Shadow
               ↓
  FE-SINGLE-FLIGHT + P8-LT scoped safety gate
               ↓
      kontrollierter 1-Versuch Mainnet-Pilot
               ↓
     Reconciliation + tatsächlicher Exit
               ↓
      KUMULIERTER REALIZED NET PROFIT > 0?
           ├─ Nein/UNKNOWN: STOP + Diagnose
           └─ Ja: First-Profit-Meilenstein
                    ↓
       Mehrfach-LIVE nur nach vollen P6-MT/P8-RES Gates
       P4A-JIT / P7 / P4B / Inventar / P9
       Race: S1-RACE → S3.R → P6-RACE → Race-LIVE
       K: Forschung/Capability-Benchmarks verpflichtend; Einsatz bei nachgewiesener Eignung
       Vollständige C0-E1/H0/P6-MT/P8-RES+INV/Frontend/P9-Lieferung bleibt Pflicht
       auch falls der erste Pilot NO_TRADE/LOSS/UNKNOWN liefert
```

**Vollständiger Umfang bleibt verbindlich:** Das Flussdiagramm ist **keine Kürzung** des detaillierten `P0–P9`-, `S0–S4`-, `C0/C0-E1/H0/T`-, `S1-RACE/S3.R/P6-RACE`-, `P4A-JIT`-, `P6-MT`-, `P8-RES/INV`-, `P7/P4B`-, K- oder Frontend-Auftrags. Eine externe FREE_ONLY-Geyser-Sperre wird belegbar `EXTERNAL_BLOCKED`, nicht zum Entfall von Race. FE-07 ist kein Startgate für die übrige Entwicklung.

Diese Darstellung ist eine **Priorisierung und Scope-Abgrenzung**, kein Ersatz der technischen Detailphasen, kein pauschaler P6-MT-PASS, keine LIVE-/Provider-Freigabe und keine Garantie einer rentablen Opportunity. Die autoritativen Bedingungen für den einmaligen Pilot stehen in §0.6, §13 und dem P8-LT-Gate.

---

# PHASE P0 — REFERENZ-, REUSE- UND LIZENZSTUDIE

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

**FREE_ONLY-Override v4.5.1:** Ein Paid Geyser ist im aktiven Profil **immer gesperrt**; selbst ein positiver erwarteter Mehrgewinn erlaubt keine operative Aktivierung. Prüfung und Kostenvergleich sind nur kontrafaktisch. Ein Wechsel benötigt eine separate ausdrückliche Benutzerentscheidung und eine versionierte Änderung von §0.4; bei fehlender kostenloser qualifizierter Binary-Source bleibt Race-LIVE BLOCKED.

### S1-08 FREE_ONLY-Binary-Quelle: verbindlicher Capability- und Abnahmevertrag (2026-10-22)

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

**Eigener Client, kein zweiter Bot:** `BinaryGeyserSource` wird als adapterneutrale Rust-Implementierung von `darkbot-solana::ingress::IngressSource` gebaut. Vorhandene `IngressEvent`-Typen, Canonical Owner, Provider-Budgets, Gap/Recovery und binäres Journal nutzen. Ein selbst programmierter gRPC-Client ist kein selbst betriebener validatornaher Geyser-Server und garantiert keinen schnelleren Upstream. Ein eigener Validator bleibt K/Infrastructure-Option, nicht Voraussetzung.

**Erster FREE_ONLY-Mainnet-Kandidat:** PublicNode Yellowstone-gRPC (`https://solana.publicnode.com/`, laut Anbieter kostenloser API-Token). Tatsächlicher Zugang für benötigte Accounts, Transactions, Slots, Roots, Filter und Laufzeit-/Datenrate ist **NOT_RUN**, nicht S1-RACE PASS. Andere nachweislich kostenlose Binary-Feeds bleiben austauschbar. Keine automatische Paid-Alternative.

**Versionen/Pin-Vertrag:** Für `SubscribeDeshred` ist `yellowstone-grpc-client` **ab 13.2.0** als API-Baseline belegt (Changelog 2026-07-02); die tatsächliche Client-/Proto-Version wird vor Implementierung nach Kompatibilitätsprüfung **exakt in Cargo.lock** fixiert und gegen den ausgewählten Server getestet. Das Open-Source-Geyser-Plugin implementiert `SubscribeDeshred` ab **14.0.0**; für korrekt aufgelöste ALT-Adressen in Deshred-TXs ist die **14.2.0-Korrektur** vom 2026-07-08 oder ein nachgewiesen gleichwertiger Backport nötig. Quell-/Versionsbeleg: [`yellowstone-grpc`-Changelog, Commit `2a548669`](https://github.com/rpcpool/yellowstone-grpc/blob/2a5486693a7ccf900ff7ccc2de1e6c6570899245/CHANGELOG.md). Der geprüfte Quellstand verwendet Agave **4.3.2**; keine stillschweigende Gleichsetzung mit dem tatsächlich eingesetzten Provider-Server. Serverversion, API-/Proto-Fidelity, ALT-/v0/v1-Verhalten und Fork-/`bank_id`-Abdeckung einzeln qualifizieren; inkompatibel/ungewiss = NOT_RUN/BLOCKED. Keine ungeprüfte Übernahme alter `eva01`-Pins.

**Offline zuerst:** Versionierte Protobuf-Fixtures und lokale gRPC-Mocks für Accounts, TXs, Slots/Roots, `write_version`, Bank-/Fork-Provenienz (`bank_id`, sofern geliefert), ALT/v0/v1, korrekte Reihenfolge, Duplicate, Gap, Replay/`from_slot` nur nach Source-Capability, Reconnect und Backpressure. Nicht hinreichend unterscheidbare Fork-/Bank-Kontexte als UNKNOWN/HALT behandeln, niemals stillschweigend kanonisieren. Kein JSON im Hot-Path, keine neue Engine und kein blockierendes Owner-IO.

**Spätere Live-Quellenqualifikation nur nach separater read-only Freigabe:** Vorab eng begrenztes Zeit-/Request-/Bytebudget und Provider-Quoten festlegen; TLS/Auth, Stream-/Filterfähigkeiten, `SubscribeDeshred` separat, Quota/429, Lücken/Recovery und reale Datenvollständigkeit prüfen. API-Token ausschließlich lokal geheim halten; redigierte Evidence im vorhandenen Journal/Qualifikationspfad führen, keinen neuen Primärspeicher.

**S1-RACE-Gate bleibt strikt:** Verbindliche Mainnet-Belege für komplette relevante State-/TX-/Slot-/Root-Daten, Freshness, Bank/Fork, v0/v1, Gap/Recovery und p50/p95/p99/Source-Kosten. Quellen auf derselben Hardware nur über nachweisbar gleiche Signatur/Slot/Account/Fork-Ereignisse und monotone Empfangszeit vergleichen. Unvergleichbare Werte als UNKNOWN, keine Anbieter-Benchmarkzahlen als eigener PASS. Ein kurzer Capability-Smoke ist kein Race- oder Vollmarkt-PASS. Fehlt eine dauerhaft brauchbare kostenlose Quelle: `EXTERNAL_BLOCKED`, Long-Tail/Premium bleibt unabhängig.

## S1.3 LONG-TAIL/PREMIUM PRODUCTION — `WsRpcControlSource`

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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
→ unter FREE_ONLY nicht zugelassen (kein automatischer Fallback)
→ allenfalls kontrafaktisch auswerten, später nur gesonderte Nutzerfreigabe
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
`S1-08 FREE_ONLY Mainnet binary source + PublicNode capability/coverage/quota/latency qualification (reale Evidence NOT_RUN)`

---
# PHASE S2 — FINAL CANONICAL SOLANA STATE

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

## S3.R — RACE EXTENSIONS — Vorbereitungen parallel erlaubt; Race-Promotion nur nach `S1-RACE PASS`

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

**Zweistufiger Gate-Vertrag v4.5.1:** Unabhängig von `S1-RACE PASS` dürfen FREE_ONLY-Zugangs-/Capability-Prüfung für Jito (ohne bezahlten Vertrag), Offline-Adapter-/Parser-/Serialization-Tests, Mock-Benchmarks, Prewarm-Design, deterministische Bench-Harnesses und kontrollierte read-only Netzwerkdiagnostik parallel laufen. Keine LIVE-Submission, keine echten kostenpflichtigen Requests, keine vorgezogene Race-Primary-Promotion. Produktiver Race-gRPC-/Prewarm-Einsatz, echte Race-Benchmarks unter Mainnet-Last und Race-LIVE bleiben an `S1-RACE PASS` und die bestehenden Submission-/Safety-Gates gebunden. Diese Tickets blockieren P2/P3/P4A/P5/T/P8-LT nicht.

### `JitoGrpcSubmitter`

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

**Vorarbeit vor `S1-RACE PASS`:** Kostenlosen Zugang/Capability prüfen, Adapter gegen lokale Fixtures/Mocks bauen und dessen API-/Encoding-/Version-/Tip-Semantik offline testen. **Produktive Race-Integration und echte Landingqualifikation erst nach `S1-RACE PASS`.**

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

Offline-/Mock-Design und lokale Worker-/Reconnection-Tests sind vor `S1-RACE PASS` parallel erlaubt. Erst nach `S1-RACE PASS` als realer produktiver Race-Prewarm:
- Jito gRPC connections;
- block-engine region state;
- Race submission buffers/queues;
- keine Handshakes/DNS nach Trigger.

### Race Submission Benchmark

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

**First-Profit-Abgrenzung v4.5.2:** JIT-ExactIn bleibt ein verpflichtender Entwicklungs- und späterer Vergleichskandidat, aber kein Vorab-Gate für den einmaligen P8-LT-Pilot, **wenn** eine andere Klasse nach §0.6/§13 in sämtlichen für sie geltenden P4A/P5/P8-Prüfungen qualifiziert ist. Fehlt ein solcher wirtschaftlicher Pfad, rückt JIT wieder auf die FE-Critical-Path. Es ist weder erlaubt, die JIT-Capability vorzutäuschen, noch einen unbekannten Economy-/Swap-Floor als positive Ausführung zu buchen.

## P4A v4.5.1 — verbindlicher JIT-ExactIn-Fundingpfad

Der aktuelle Jupiter-V2-Provider liefert über `quote_exact_out` keinen ausführbaren ExactOut-Pfad. Ergänze im bestehenden Builder-/Router-/Economics-System einen bounded ExactIn-Shortfall-Solver: Ermittelt wird ein Funding-Token-Input `x`, sodass der **ausführbare** Liability-Token-Floor `minimum_executable_out(x) >= liability_shortfall` erreicht. Niemals `amount_in = shortfall` setzen, da dies verschiedene Token-Einheiten sind. Gemeinsames begrenztes Request-/RouteAttemptBudget inklusive Route-Fit, Frische-/Allowlist-/Slippage-/Simulations-Gates; kein erzwungener Remote-Hop im Race-Hotpath.

`NO_SWAP_INVENTORY`, `JIT_SHORTFALL_EXACT_IN`, `ZERO_PREFUND_SAME_TX`, `PREHELD_INVENTORY` als technisch qualifizierte Kandidaten nach tatsächlichem realisierbarem Nettoedge, Kapitalbindung und Safety vergleichen; Enum-Reihenfolge ist kein Ranking. Unknown Route/Fees/Economics ergeben keinen fingierten ausführbaren Kandidaten. Vor LIVE mindestens deterministische Vergleichsfixtures und korrekte Ausführbarkeitsnachweise.

## Gate P4A

Jede unterstützte Opportunity deterministisch und reproduzierbar bewertet.

---

# PHASE P5 — FINAL RECEIVERSHIP EXECUTION PLANNER

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 / REV.2.0 / REV.3.0 – Revisionsmarkierung für den noch offenen Rest:** Verpflichtende Routing-/JIT-/Exit-Pfade REV.1.0; zusätzliche Routen-/Quote-Cache-Feinoptimierung REV.2.0, komplett eigener zusätzlicher lokaler DEX-Router nur mit Messbeleg REV.3.0.

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

### `PAID_ALLOWED` — im aktiven FREE_ONLY-Profil gesperrt

Die Capability darf aus Kompatibilitätsgründen dokumentiert bleiben. Sie ist keine operative Freigabe eines kostenpflichtigen Routing-/API-/Key-Pfads. Ein automatischer Paid-Fallback ist verboten; eine künftige Änderung benötigt ausdrückliche, separate Benutzerfreigabe.

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
→ PAID_ALLOWED als Capability, unter FREE_ONLY BLOCKED
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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

**v4.5.1 globale atomare Ressourcenreservation:** Multi-Mint Liability-/Funding-Bestände, Fee-Payer-SOL, Base-/Priority-Fee-Maximum, Jito Tip, Rent und Exposure werden in **einem** Commit reserviert oder gar nicht. Verfügbar = reconciled Wallet minus alle aktiven Reservationen über sämtliche Target-Accounts. Reservationen sind generation-/candidategebunden, restartfest und bleiben bei unklarem Submission-/Landing-Handoff `UNKNOWN` bis zur eindeutigen Reconciliation gesperrt; ein lokaler Timeout darf sie nicht freigeben. Shadow bildet dieselbe Arbitration virtuell nach. Contention-, Crash- und Replay-Regression vor parallelem LIVE.

Konkurrierende Candidates dürfen nicht beide als executable gezählt werden, wenn sie dieselbe exklusive Ressource benötigen.

### P6-MT.12 Apple-M4 Runtime

> **REV.1.0 / REV.2.0 – Revisionsmarkierung für den noch offenen Rest:** Volle Pflicht-M4-Laufzeit- und Latenz-Gates REV.1.0; zusätzliche nicht vertraglich geforderte Mikrooptimierung erst bei Messbedarf REV.2.0.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Alle MT0–MT13-Fachaufgaben bleiben REV.1.0; Mess-/Promotion-Gates ohne externe Evidence bleiben BLOCKED, nicht als Code-Abkürzung gestrichen.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

Für jede optional betrachtete bezahlte Komponente **ausschließlich kontrafaktisch**, nicht als FREE_ONLY-Ausführungsfreigabe:

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

Im aktiven **FREE_ONLY**-Betrieb **nicht einsetzen**. Auch ein positiver `incremental_net_value` ist ausschließlich eine Messgröße für eine mögliche spätere, separat ausdrücklich vom Benutzer freizugebende Kostenpolitikänderung. Kein automatischer Tarif-/Providerwechsel und kein versteckter Paid-Routing-/RPC-/Geyser-Fallback.

## p(win)

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

**v4.5.1:** SHADOW/UNKNOWN sind keine belegten Winnerlabels. Ein `p(win)`-Modell braucht hinreichend vollständige echte Chain-/Landing-/Konkurrenzlabels, zeitlich getrennte Forward-Validierung und ökonomischen Zusatznutzen gegenüber deterministischer Baseline.

## Gate P7

- Competition ausreichend gemessen;
- Provider wirtschaftlich bewertet;
- p(win) belastbar oder bewusst nicht verwendet.

---

# PHASE P4B — PROBABILISTIC PROFIT OPTIMIZER

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

**First-Profit-Pilot (§0.6/§13):** Vor dem einmaligen seriellen LIVE braucht auch ein einzelner Wallet-/Fundingpfad eine persistente, restartfeste `RESERVED/SUBMITTED/UNKNOWN`-Sperre gegen doppelte wirtschaftliche Handoffs und gegen irrtümliche Freigabe vor finaler Chain-Reconciliation. `Safety::check()` mit ausschließlich flüchtigen Zählern ist hierfür nicht ausreichend. Der One-Shot-Pilot hat `max_inflight=1`, bindet die konkret genehmigte Transaktion und hält bei unbekanntem Outcome. **Diese scoped Pilot-Reservation ist kein Ersatz** für die weiter unten verlangte globale atomare Multi-Mint-/SOL-/Tip-/Rent-Reservation vor parallelem LIVE.

## P8 v4.5.1 — Walletreservation und Inventory Target Policy

Vor parallelem LIVE ist die globale atomare, persistente Liability-/Funding-/SOL-/Priority-/Tip-/Rent-Reservation aus P6-MT.11 **Pflicht**. Keine zwei Worker dürfen dasselbe Inventar oder dieselbe Fee-Reserve verplanen. Status `AVAILABLE → RESERVED → SUBMITTED → RECONCILED → RELEASED/INVENTORY_UPDATED`; bei `UNKNOWN` keine voreilige Freigabe. Alle Token-/Lamport-Deltas nach tatsächlichem Chain-Outcome prüfen.

Rebalancing folgt Sollbeständen für Liability-/Fee-Reserve, Safetybuffer, Risk-/Dust-Limits und vollständigen Umwandlungskosten; kein pauschaler Komplettverkauf von Collateral. Nur nachweisbar nötige oder profitable/riskoreduzierende Inventarverwertung. Ein von Rebalancing abhängiger LIVE-Pfad bleibt bis zum Verwertungs-/Risikogate gesperrt. Mark-to-market ist kein realisierter Gewinn.

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

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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

> **REV.1.0 / REV.2.0 / REV.3.0 – Revisionsmarkierung für den noch offenen Rest:** Die bisher verpflichtende K-Untersuchung ist REV.1.0; zusätzliche produktive Beschleunigung nur nach belegtem Bedarf in REV.2.0/3.0.

K bedeutet **kein Rewrite**.

K optimiert denselben finalen Code.

## Ingress

Wenn `LOSS_REASON = INGRESS`:

prüfen, in dieser Reihenfolge:

1. Filter/Region/Provider des bestehenden Mainnet-Geyser optimieren;
2. besserer kostenloser/bereits verfügbarer Mainnet-Binary-Feed;
3. Paid-Geyser-Angebote **nur kontrafaktisch** dokumentieren; unter FREE_ONLY nicht aktivieren;
4. eigener Geyser / validatornaher Aufbau **nur soweit ohne verpflichtende neue laufende Infrastrukturkosten realisierbar**;
5. eigener Node / andere Region **nur als bestehende kostenlose Ressource oder nach späterer expliziter Policy-Änderung**, nicht als automatischer Ausbau;
6. **erst danach pre-execution / Shred / Deshred** bezüglich realer kostenloser Source-Capability untersuchen, wenn Geyser nicht reicht; fehlende freie Capability bleibt BLOCKED.

### K-INGRESS-PRE — `SubscribeDeshred` Sensor

> **REV.1.0 / REV.2.0 – Revisionsmarkierung für den noch offenen Rest:** Capability-/Wirtschaftlichkeitsprüfung REV.1.0; optionale produktive spekulative Sensoranbindung bei erwiesenem Zusatznutzen REV.2.0.

**BELEGT:** Yellowstone beschreibt `SubscribeDeshred` als separaten Stream für pre-execution Transactions. Der Server rekonstruiert Entries aus eingehenden Shreds und streamt decodierte Transactions, bevor Replay sie ausgeführt hat.

**Quellenstand 2026-10-22 – serverseitige Fähigkeit prüfen:** Das offizielle Yellowstone-Changelog belegt die Open-Source-Implementierung von `SubscribeDeshred` seit 2026-07-02 (geyser 14.0.0), während die README teils weiterhin Triton-only/`UNIMPLEMENTED` angibt. Implementierter Open-Source-Code **beweist nicht**, dass der konkret ausgewählte PublicNode-Server diesen RPC aktiviert oder kostenlos erlaubt. `UNIMPLEMENTED`, fehlende Berechtigung/Quota und unvollständige ALT-/Fork-Fidelity getrennt protokollieren; keine unbelegte Capability als PASS.

**Gepinnte Quellen (Stand `2a548669`, 2026-10-20):** [CHANGELOG](https://github.com/rpcpool/yellowstone-grpc/blob/2a5486693a7ccf900ff7ccc2de1e6c6570899245/CHANGELOG.md) und [README](https://github.com/rpcpool/yellowstone-grpc/blob/2a5486693a7ccf900ff7ccc2de1e6c6570899245/README.md). Geyser 14.0.0 implementiert `SubscribeDeshred`, Geyser 14.2.0 behebt fehlende Deshred-ALT-Auflösung; PublicNode-Capability weiterhin NOT_RUN. `SubscribeDeshred` bleibt optionale K-INGRESS-PRE-Beschleunigung, keine S1-RACE-Ersatzquelle und keine Voraussetzung für die Produktentwicklung.

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

### K-INGRESS-PRE FREE_ONLY-Alternativen: PublicNode Deshred und LocalShred Lite (2026-10-22)

> **REV.1.0 / REV.2.0 – Revisionsmarkierung für den noch offenen Rest:** Kostenlose Provider-/Safety-/Wirkungsprüfung REV.1.0; optionaler zusätzlicher LocalShred-/UDP-Sensor nach GO REV.2.0.

**Rangfolge (Tickets `K-02` und `K-02b`):** (1) Nach S1-Adapter-/Offline-Arbeit `SubscribeDeshred` am kostenlosen PublicNode-Yellowstone-Zugang auf tatsächliche Freischaltung/Filter/ALT/Quoten prüfen. (2) Als unabhängige Alternative `LocalShred Lite` von DedicatedNodes (`https://www.dedicatednodes.io/localshred/`) prüfen: laut Anbieter kostenlos, kein DedicatedNodes-Server erforderlich, Amsterdam als Ausgangspunkt; Zugang/Whitelist, Verfügbarkeit, Limits und Latenz noch NICHT QUALIFIZIERT. Anbieter-eigene Millisekundenwerte sind keine Project_0-Evidence.

**Netz:** UDP-Shreds setzen real erreichbare öffentliche Empfänger-IPv4, funktionierenden UDP-Port, NAT-/Router-/Firewall-Konfiguration und eine Anbieter-Freischaltung voraus; eine per HTTP sichtbare öffentliche IPv4 genügt als Beweis nicht. Lokale vorhandene Hardware zuerst; kein automatisch gebuchter VPS, Business-Vertrag, Paid-Feed oder Hosting. Eigene Validatorinfrastruktur bleibt eine separate kosten-/messungsgebundene K-Entscheidung.

**Strikte Safety-Trennung:** Deshred und rohe UDP-Shreds liefern ausschließlich unbestätigte `PreExecution`-Hinweise (Signatur/Slot/Quelle/Zeit/Fork-/ALT-Provenienz, soweit belegt). Rust-Decoder und geprüfte Open-Source-FEC-/Proxy-Bausteine dürfen spekulativ betroffene Marginfi-/Oracle-/Bank-Abhängigkeiten erkennen und Prepared-Candidate-/TX-Templates vorwärmen. Keine direkte Canonical-State-Mutation, keine finale Health-/Economics-Behauptung, keine Signatur/Submission nur wegen eines Pre-Signals. Vor jeder Handelbarkeit müssen autoritative Account-/Slot-/Root-Updates, aktuelle Economics, Freshness und die bestehenden Safety-/LIVE-Gates erneut bestehen. Vorab gesehene TXs können scheitern, verworfen oder anders geforkt werden.

**Mess- und Promotion-Gate:** Zuerst Offline-Mocks/Replay für Reorder, FEC-Verlust, unvollständige Frames, Duplicate, Fork, nicht ausgeführte TX, ALT und Backpressure. Danach nur explizit genehmigte kurze read-only Mainnet-Proben. Auf identischem Host und mit monotonen Timestamps je tatsächlich paarbarem Signatur-/Slot-/Fork-Ereignis `T_pre` gegen autoritative TX-/Account-Zeitpunkte messen: p50/p95/p99, Coverage, Fehltrigger, Gaps, Recovery und real eingesparte Vorbereitungslatenz. Ein Vorsprung beim Datenempfang beweist weder Race-Win noch Gewinn. Ohne belegten Zusatznutzen bleibt dies `DEFERRED_PRIORITY` oder `EXTERNAL_BLOCKED`, nicht Produkt-Sperre. Kein neues Hauptprojekt und keine Unterbrechung bestehender Phasen.

### K-INGRESS-RAW

> **REV.1.0 / REV.3.0 – Revisionsmarkierung für den noch offenen Rest:** Pflichtuntersuchung/Stop-or-Go REV.1.0; eigener Raw-Shred-/Validator-Stack nur als optionaler Spezialausbau nach Messbeleg REV.3.0.

Raw Shred Delivery / eigener validatornaher Deshred-Pfad bleibt noch eine Stufe später. Erst wenn Geyser **und** ein verfügbarer pre-execution Sensor nachweislich Ingress-Limit sind, wird eigener Shred-/Validator-Stack erwogen.

## CPU

> **REV.1.0 / REV.2.0 – Revisionsmarkierung für den noch offenen Rest:** Alle P6-MT-/Hotpath-Gates REV.1.0; darüber hinausgehendes PGO/SIMD/Cache-/ISA-Tuning nur mit Bedarf REV.2.0.

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

> **REV.1.0 / REV.2.0 / REV.3.0 – Revisionsmarkierung für den noch offenen Rest:** Verpflichtende Routing-/JIT-/Exit-Pfade REV.1.0; zusätzliche Routen-/Quote-Cache-Feinoptimierung REV.2.0, komplett eigener zusätzlicher lokaler DEX-Router nur mit Messbeleg REV.3.0.

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

> **REV.1.0 / REV.2.0 / REV.3.0 – Revisionsmarkierung für den noch offenen Rest:** Verbindliche RPC/Jito/Race-Ausführung REV.1.0; zusätzliche Region-/Redundanzoptimierung REV.2.0, eigene nicht verpflichtende TPU-/NIC-Spezialtechnik REV.3.0.

Wenn `LOSS_REASON = SUBMISSION/TIP`:

1. Jito gRPC / Region / Tip / Bundle Policy optimieren;
2. Jito vs. RPC unter gleichen Bedingungen messen;
3. erst danach Direct TPU QUIC prüfen.

### K-SUB-TPU — `DirectTpuQuicSubmitter`

> **REV.1.0 / REV.3.0 – Revisionsmarkierung für den noch offenen Rest:** Verbindliche SWQoS-/Stake-/Kosten-/Nutzenprüfung REV.1.0; eigens betriebener Direct-TPU-QUIC-Produktivpfad nur bei belegtem Vorteil REV.3.0.

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
- dedizierter Host nur bei bereits kostenlos verfügbarer Ressource; jeder kostenpflichtige Host bleibt unter FREE_ONLY gesperrt, auch bei positiver Messung;
- Kernel/NIC-Spezialisierung erst als späteste Stufe.
## Transaction Version

v0/v1 dauerhaft anhand realer Daten vergleichen.

Keine ideologische Bindung.

---

# 8. Verbindliche Ticket-Reihenfolge

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

## First-Profit-Tickets — höchste Produktpriorität (scope FE-00…FE-07)

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

```text
FE-00  Existing 5h offline report, verfügbare reale Kandidaten / FREE_ONLY-Zugang, NO_ECONOMIC_GO
FE-01  current-mainnet protocol/source freeze + GENAU EIN ausführbarer Fundingpfad
FE-02  exact Health/Size/Route/Build/Sim/Fees + überprüfbarer Collateral-Exit
FE-03  production Shadow + replay/competition for selected class
FE-04  one-shot CLI / persistenter single-flight Wallet-/Fee-Hold / UNKNOWN / Exit-Ledger
FE-05  provider-aware read-only smoke (nur nach separater Freigabe)
FE-06  einzeln genehmigter, budgetierter Mainnet-Versuch + Auto-HALT
FE-07  finalized chain/cost/exit reconciliation + kumulierter REALIZED NET PNL; GO/STOP
```

Diese Tickets überlagern die nachfolgenden technischen Produkt-Tickets **ausschließlich in der Priorität und für den Scope des einzelnen First-Profit-Piloten**, nicht im endgültigen Lieferumfang. Kein ursprüngliches Produkt-/Race-/Jito-/JIT-/Market-/Wallet-/M4-/Frontend-/K-Ticket wird gestrichen oder auf freiwillig gesetzt. Bis zur späteren Bearbeitung gilt `DEFERRED_PRIORITY`, bei paralleler Arbeit `PARALLEL_IN_PROGRESS`, bei evidenzbelegten externen Grenzen `EXTERNAL_BLOCKED` — **niemals stillschweigend CANCELLED oder PASS**. Sie bleiben dieselbe Strategy/Runtime; voller P8-Safety-Umfang gilt vor Mehrfach-LIVE. Wer FE-06 ohne FE-00…FE-05 PASS freigibt, verletzt §0.6/§13.

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
S1-08 FREE_ONLY Mainnet binary source + PublicNode capability/coverage/quota/latency qualification (reale Evidence NOT_RUN)

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

S3-R00 Jito FREE_ONLY access/capability, offline adapter + prewarm mock tests   parallel OK / no LIVE
S3-R01 JitoGrpcSubmitter                  offline preparation OK; production Race requires S1-RACE PASS
S3-R02 race prewarm manager              offline design/mock OK; production prewarm requires S1-RACE PASS
S3-R03 submission race benchmark         harness offline OK; Mainnet/Race qualification requires S1-RACE PASS

S4-01 parity corpus
S4-02 decode/state parity
S4-03 tx parity
S4-04 performance baseline
```

## Project 0

**C0-E1 Implementierungstickets (eigene P3–P6-Paketnummern; nicht mit Produkt-P3/P4A/P5/P6 verwechseln):**

```text
C0E1-P3-M06      start_deleverage six-marker + start/end pairing/protocol fixtures
C0E1-P4-VC       decouple strict from capture; OFF/INDEX/STRICT proof-status separation
C0E1-P4-ID       stable capture_run_id, verification_spec_id; separate attempt/result digests
C0E1-P4-RESUME   bounded strict batches, provider quotas, restart-proof verification
C0E1-P5-CLI      MeasurementConfigV2, common typed CLI / future frontend contract
C0E1-P5-REPORT   normalized evidence, four types/six markers, related events, views/formats
C0E1-P5-AUTO     auto post-capture verification; historical 5h replay without new capture
C0E1-P6-GATE     identity/resume, 429, evidence invariants, Shadow separation, FREE_ONLY/LIVE safety
```

**Abnahme je Ticket:** ein nachvollziehbarer Commit, ausführbare Positive-/Negativ-/Fail-Closed-Fixtures, reproduzierbare CLI/Report-Ausgaben, `MEASUREMENT_LEDGER.md` mit PASS/FAIL/NOT_RUN und Beweisgrenzen; P6-C0E1-Master-PASS nie ohne kanonische Gate-Evidence. `C0E1-P4-ID`: Spec-ID muss vor erstem Verifier-RPC feststehen und bei Resume bitidentisch bleiben; Attempt-/Resultat-IDs entstehen separat. `C0E1-P5-REPORT`: vier Related Events dürfen nie zu Liquidationssummen beitragen.

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
P4A-JIT-01 bounded Jupiter-V2 ExactIn shortfall solver; executable out floor + shared quote budget
P4A-JIT-02 NO_SWAP vs JIT vs ZERO_PREFUND vs PREHELD deterministic executable net/capital tests

P5-01 allowlist enforcement
P5-02 refresh planner
P5-03 NO_SWAP
P5-04 local/precomputed route
P5-05 free route
P5-06 receivership builder
P5-07 prepared execution plan
P5-08 staging fixtures
P5-JIT-01 route/transaction builder integration for JIT ExactIn with allowlist/freshness/simulation

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
P8-RES-01 global atomic multi-mint/SOL fee-tip-rent reservation; no double allocation
P8-RES-02 durable UNKNOWN, crash/restart/landing reconciliation, race/contending targets fixtures
P8-INV-01 inventory target, minimal rebalance, costs/dust/collateral risk and LIVE gate

P9-01 production metrics
P9-02 provider review
P9-03 inventory review
P9-04 protocol-upgrade watch

K-01 ingress loss attribution
K-02 SubscribeDeshred economic/access qualification (optional pre-execution sensor)
K-02b LocalShred Lite FREE_ONLY UDP feed: access/whitelist, IPv4/port, open-source decoder, fidelity and pre-execution value (optional; no own validator)
K-03 own raw-shred/validator infrastructure escalation only if justified (distinct from K-02b provider UDP feed)
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

# 12. Historischer Startstatus 2026-09-30 (keine heutige Autorität)

**HISTORISCHE VERSIONSAUSSAGE:** Die nachfolgende Zeile `v4.2 REMAINS AUTHORITATIVE UNTIL PROMOTION` ist ein damaliger Beschlussstand vom 30.09.2026, **keine gegenwärtige Prioritäts- oder Gültigkeitsregel**. Verbindlich ist der oben definierte **Produktions-Phasenplan v4.5.2 einschließlich aller späteren bindenden Ergänzungen**, insbesondere FE-00…FE-07, PERSIST/JFI, S1-RACE, FREE_ONLY, S1-02/S1-08 und K-02/K-02b/K-03. Historie wird erhalten, ohne ältere Gates stillschweigend zu streichen oder LIVE zu autorisieren.

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
    FREE_ONLY HARD BLOCK / NO AUTOMATIC PAID FALLBACK
    POSITIVE INCREMENTAL NET VALUE IS CONTRAFACTUAL ONLY
    POLICY CHANGE REQUIRES EXPLICIT SEPARATE USER AUTHORIZATION

OWN NODE:
    NOT REQUIRED
    NO MANDATORY PAID OPERATION UNDER FREE_ONLY

LIVE:
    NOT ENABLED
```

---

# 12A. Fable Review Checklist für v4.4

Fable soll diese Review-Fassung **nicht nur sprachlich**, sondern technisch gegen aktuelle Primärquellen prüfen.

## A — Ingress

- Ist v4.2-S1 weiterhin korrekt: Race=BinaryGeyser, Long-Tail/Premium=WSS+Snapshot/Reconcile?
- Ist `SubscribeDeshred` in der geprüften Open-Source-Version implementiert, und welche tatsächlichen Server-/Provider-Capabilities, ALT-Fidelity, Freischaltungen und FREE_ONLY-Limits sind beim gewählten Endpunkt belegt? Ist der Sensor weiterhin ausschließlich optional in K eingeordnet?
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

# 13. Verbindlicher Start — FIRST PROFIT RELEASE TRAIN (13.10.2026)

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Technische FE-/Safety-/Profit-Gates bleiben erhalten, aber Pilot erst nach vollständiger REV.1.0-Software- und Offline-Abnahme.

**Nur der nachweislich kürzeste sichere Weg zur ersten positiven kumulierten realisierten Netto-Bilanz hat Produktpriorität.** Bereits implementierte Module bleiben erhalten. Keine neue GUI, kein zweiter Bot, kein Pfadwechsel ohne Messbeleg. Die folgenden Tickets erhalten im Ledger eigene PASS/PARTIAL/OPEN/NOT_RUN-Gates und Commit-/Fixture-/Chain-Evidence.

| Schritt | Verbindliche Arbeit | Abnahme / Stop |
| --- | --- | --- |
| **FE-00 – Opportunity-Selection** | Bestehende C0-E1 P1–P6 Offline-PASS respektieren, vorhandene 5h-Evidence mit neuer CLI auswerten; einen aktuellen echten Receivership-/Long-Tail-Kandidaten und brauchbare kostenlose Feed-/Chain-Quellen feststellen | Ein realitätsnaher belegter positiver ausführbarer Nettoedge-Kandidat, sonst `NO_ECONOMIC_GO`; alter 5h-Run mit Lücken darf keine vollständige Marktstatistik beweisen |
| **FE-01 – One-Class Freeze** | Tatsächlich deployten 0.1.11/aktuellen ProgramData-Stand, Group, Banken, Integrationen, Oracles und aktuelle Solana-TX-Version pinnen; genau eine `NO_SWAP_INVENTORY`- oder `ZERO_PREFUND_SAME_TX`-Klasse wählen | P1A/S1-LT/S2/S3-CORE/P3/S4-Parität und nur erforderliche P5-Regeln/Accounts für diesen Pfad belegt; bei NULL/UNKNOWN STOP |
| **FE-02 – Executable Economics** | Bestehenden exakten Health/Sizer/Builder/Router/Economics-Weg mit echten TX-/Route-Fixtures und `simulateTransaction` prüfen; gesicherten executable min-out, Repay, Feeds, CU, Fees, Tips, erwarteten Failure-Cost, benötigtes Inventar und realistische Collateral-Exit-Route sichern | Kein Oracle-Gross-Edge als Profit; bei fehlender Exit-/MinOut-/Cost-Provenienz `NOT_ACTIONABLE`, keine Submission |
| **FE-03 – Targeted Production Shadow** | Dieselbe Production-Pipeline ohne Submission über echte erreichbare Kandidaten; verifizierbare Winner-/Loss-/State-/Queue-Latenz und eine vollständige Replay-/Simulationsbeweiskette | Genug echte positive und negative Kandidaten- und Replay-Evidence für genau diese Klasse; kein Mindeststundenwert erfindet fehlende Marktereignisse |
| **FE-04 – Serial Safety + Accounting** | Versioniertes CLI-Einmalfreigabeprofil, harte nominelle/Max-Fee/Tip/Rent-/Verlustgrenzen, `max_inflight=1`, persistent signiertes Intent-/Hold-Journal vor Handoff, Reload→UNKNOWN-Hold und finalisierte Chain-Reconciliation; kontrollierte Inventory-Exit/Realized-PnL-Kette | Crash/Restart, Doppelereignis, TX-failed, nicht gelandet, unknown, Feed-Gap und unvollständiger Exit mit fail-closed-Tests bestanden. **Kein Parallel-LIVE** |
| **FE-05 – Read-only Vorprüfung** | Offline-Freeze und kurzer, ausdrücklichermaßen genehmigter FREE_ONLY Mainnet-WSS/RPC-/Shadow-Smoke mit gültiger Anbieterqualifikation; gezielte Programm-/TX-Simulation gegen reale Zustände | Kein fehlender oder unqualifizierter Provider-/Archive-/Snapshot-Beleg für die im Pilot benötigten Zugriffe, kein stiller Paid-Fallback; vierstündiger C0-E1-Master bleibt separat |
| **FE-06 – Controlled ONE-SHOT** | Nutzer sieht Signatur-/Code-/Programm-/Wallet-Bindung, maximalen Gesamtaufwand und Verlustgrenze; erst nach **separater expliziter Werttransfer-Freigabe** einmaligen `P0_MODE=live`-Pilot-Gate für den freigegebenen Pfad implementierungsseitig öffnen | Genau EIN begrenzter, vom User genehmigter Versuch, danach HALT; keine Auto-Retries/Auto-Arm, kein Jito-ACK als Landing-Nachweis |
| **FE-07 – Profit / Stop Decision** | TX-/Bundle-Outcome, Signature, finalized Token-/Lamport-Deltas, Gebühren aller eigenen Versuche, Restinventar und belegten tatsächlichen Verkauf/Exit auswerten; kumulierten realisierten Nettoertrag berichten | `FIRST_REALIZED_PROFIT_EVIDENCED` nur bei positiver *kumulierter* realisierter Bilanz nach allen angefallenen Kosten; sonst `LOSS/UNKNOWN/NO_TRADE`, STOP und Analyse |

**Spezifische Scope-Regeln vor dem ersten Profit:**
- **Nur für FE-06 nicht vorgelagert, für das Endprodukt weiterhin VERBINDLICH:** vollständige C0-E1-Strict-/Index-Marktqualifikation, S1-RACE/Binary-Geyser und sämtliche Race-/Jito-Optimierungen, H0/P7/P4B/`p(win)`, JIT-ExactIn, MT6–MT13/M4, globale Multi-Wallet-/Gebührenreservierung, Rebalancing und Frontend. Alle ursprünglichen Abnahmekriterien bleiben erhalten. Eigenständige parallele Umsetzung ist ausdrücklich erlaubt; die Verschiebung gilt **nur** für den One-Shot-Critical-Path.
- **Weiterhin verpflichtend vor FE-06:** vollständige konkrete Program-/Health-/State-Parität, aktuelle State-Frische, korrekter TX-Plan, ausführbares ökonomisches Ergebnis, ausreichend tatsächliches Shadow-/Rehearsal-Evidence, P8-Single-Flight-Kapital-/Safety-/Crash-Reconciliation und echte FREE_ONLY-Providerfähigkeit für den gewählten Kandidaten. Ohne einen gewinnfähigen Kandidaten gibt es **keinen** Finanzierungs-/LIVE-Freifahrtschein.
- **G-1 / Umsetzungsentscheidung 25.10.2026 – nur DEC-01…15 jetzt vollständig bauen:** Für die **15 ausdrücklich beschlossenen Designentscheidungen** sind die realen nativen Code-/Controller-/Journal-/Protobuf-/Gateway-/React- und Testpfade jetzt umzusetzen, ohne Verschiebung bis nach FE-07. Die übrige Roadmap (MT6–MT13, H0, P7/P4B, P6-RACE, K, P9, Jito gRPC/Prewarm) bleibt verbindlich, darf parallel entwickelt werden und **behält ihre bisherigen Mess-, Evidenz- und Phasenabhängigkeiten** (MT6–MT10 insbesondere erst nach C0-MO und nachgewiesenem Engpass). Nur unmittelbar zu DEC-01…15 gehörende Teilpfade werden jetzt vorgezogen. Eine pauschale G-1-Ausweitung auf die übrige Roadmap benötigt eine eigene Nutzerentscheidung. Mainnet-/Provider-/Paid-/LIVE-/Signing-/Werttransfer-Gates bleiben unverändert.

## Parallel / danach — Race (vollständig verbindlich)

**Race ist keine optionale Erweiterung nach dem ersten Gewinn.** S1-RACE-Geyser-/Binary-Ingress-Qualifikation, Jito Single/Bundle/gRPC, Prepared TX, Prewarm, Submission-Race und p95/p99-/Landing-/Profit-Optimierung sind weiterhin als Produktphasen verpflichtend. Wenn keine qualifizierte kostenlose Quelle verfügbar ist: `EXTERNAL_BLOCKED` mit Alternativen und Mess-Evidence statt Streichung oder Paid-Fallback. Offline-Vorarbeiten dürfen parallel zum FE-Train stattfinden.

```text
S1-RACE qualification
→ S3-R01 Jito gRPC
→ S3-R02 prewarm
→ S3-R03 race benchmark
→ P6-RACE evidence
→ Race LIVE gate
```

**K-Research, Capability-Checks und Messentscheidungen bleiben verpflichtend.** Der produktive Ausbau einer K-Technik erfolgt weiterhin nur bei belegtem Bottleneck/Mehrwert, Machbarkeit, Safety und FREE_ONLY (insbesondere keine neuen kostenpflichtigen Nodes):

```text
K-02 SubscribeDeshred — Provider-Capability und Zusatznutzen prüfen und messen
K-02b LocalShred Lite — kostenlosen UDP-Zugang, Decoder und Zusatznutzen prüfen und messen
K-03 eigener Raw-Shred-/Validatorpfad — nur nach nachgewiesenem Engpass bewerten
K Direct TPU + SWQoS/stake path — technische/ökonomische Qualifikation
K own validator/node/region/kernel/NIC — bewerten, unter FREE_ONLY keine Paid-Pflicht
```

H0, P7 und P4B bleiben verbindliche Evidence- und Optimierungsphasen. Ihre parallele Umsetzung darf den sicher qualifizierten First-Profit-Pilot nicht künstlich verzögern; aus fehlendem unmittelbarem Profit darf aber kein späteres Streichen dieser Phasen folgen.

**Keine zweite Entwicklung. Kein späterer Race-Rewrite. Kein späterer Rust-Rewrite.  
Ein finaler Core; Long-Tail/Premium wird zuerst wirtschaftlich aktiviert, Race wird danach auf demselben Core freigeschaltet.  
v4.5.2 ändert nur Reihenfolge/Pilotumfang; die mit P6-MT-v2 eingefrorene v4.5/v4.5.1-Gesamtarchitektur und alle verbindlichen Lieferziele bleiben erhalten.**

---

## C0-E1 — 4H ECONOMIC / CAPITAL / COMPETITION / RACE ATTRIBUTION

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

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
    (A2, 18.10.2026: Hauptkennzahl für 9–11 ist die Marge zum ersten lokalen Empfang des Gewinner-Slots; Margen gegen den Live-Empfang des Gewinners nur bei `confirmed`, bei `finalized` None mit Grund.)
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

**Resume statt Re-Run:** nutze run_id + manifest/data/schema-version als Identität, append-only Signaturjournale, stabilen Pagination-Cursor, Transaktionsstatus, atomare idempotente Checkpoints, Abschnittsbeweise, Retry-/Quota-Status, Bounded Queues; beim nächsten Aufruf nur unvollständige Bereiche/Fälle bearbeiten. Keine nachträgliche Manipulation originaler Capture-Daten. Operative Ausgabe primär als typisierte `.p0j`-Verification-/Checkpoint-/Result-Records und nativer Bericht; JSON/JSONL ausschließlich als bewusst angeforderter, nichtautoritativer Offline-/historischer Export gemäß JFI-03, nie als interne produktive Übergabe. Anbieter/Abdeckung/RPC-Calls, Bytes übertragen, Datenmenge auf Disk, CPU-/RAM-Max soweit messbar, Zeit/Fehlergrund/Unbekanntanteile; ohne API-Schlüssel oder geheimen Endpoint.

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

### L. C0-E1 v4.5.1 — Verbindlicher Terminal-, Verifikations- und Frontend-Vertrag (2026-10-12; Konsistenzkorrekturen 2026-10-13)

**Autorität und Status:** Dieser Abschnitt ergänzt Astras Masterauftrag 2026-10-11, insbesondere C0-E1 P3–P6; ältere widersprüchliche Soll-CLI-/Statusbeispiele sind dadurch präzisiert. Er ändert **keine** kanonischen Master-, LIVE- oder Submission-Gates. Bestehende Observer-, P3-Verifier-, Reconciliation-, MeasurementPlan-, Economics- und Shadow-Komponenten **weiterverwenden**, nicht neu bauen. P2 ist auf Remote lokal qualifiziert; P3-Historienprüfung wegen 429 offen; P4–P6 noch nicht qualifiziert. Keine neuen 5h-/24h-Captures ohne Anwenderfreigabe.

**Fortschrittsnachtrag 13.10.2026:** Der vorstehende historische Implementierungsstatus ist überholt: `C0E1-P3-M06` (`aa37dd3`), P4 (`2c62d13`), P5 (`c092bcd`) und P6-Offline (`763c98a`) sind auf Remote lokal qualifiziert. Reale 5h-Replay-, HTTP-429-/Provider-/Archive-, Mainnet- und Master-Gates bleiben OPEN/NOT_RUN gemäß `MEASUREMENT_LEDGER.md`. Der First-Profit-Release-Train in §0.6/§13 priorisiert ausschließlich einen sicher qualifizierten individuellen Pilot; er macht C0-E1 **nicht** zum Markt-Master-PASS.

#### L.1 P3: sechs Instruktionsmarker / vier fachliche Liquidationsarten

`audit_market_quick.py → INSTRUCTION_TYPES` enthält fünf Marker: `lending_account_liquidate`, `start_liquidation`, `end_liquidation`, `end_deleverage`, `lending_pool_handle_bankruptcy`. **`start_deleverage` fehlt in diesem Mapping**, nicht generell in allen Instruktionsdefinitionen. `end_deleverage` ist bereits erkannt; daraus folgt **nicht**, dass sämtliche Forced-Deleverage-Liquidationen fehlen. Nachzubessern sind die vollständige Startmarker-Erkennung, Paar-/Abschluss-Semantik und Regressionen mit gepinnter Marginfi-Quelle, CPI/LUT, Legacy/v0/v1, failed/truncated und echten Positiv-/Negativ-Fixtures.

Vier Klassen: CLASSIC (`lending_account_liquidate`), RECEIVERSHIP (`start_liquidation`/`end_liquidation`), FORCED_DELEVERAGE (`start_deleverage`/`end_deleverage`), BANKRUPTCY_SETTLEMENT (`lending_pool_handle_bankruptcy`). Sechs Marker sind **keine sechs selbstständigen Gewinne**. Start und Abschluss niemals doppelt als realisierte Liquidationen zählen. Unvollständige Paare, unbekannte Outcomes und misslungene TX nicht zu Erfolgen umetikettieren. Alle Marker einzeln **nur für Darstellung/Report** schaltbar, nicht als Capture-/Decoder-/Verifier-Auslassfilter.

#### L.2 P4: Strict aus Capture-Finalizer lösen, Verification als eigene Schicht

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

Aktuell ruft `market_observer::finish_capture()` bei regulärem Abschluss `full_reconciliation_phase()` fest auf; `econ.rs` bindet den bisherigen technischen PASS an Kanonik. Trennung ist zwingend: **Capture inkl. Drain/Gap/Raw Evidence → optional Post-Capture Chain-Verification → Normalisierung/Report → getrennte Masterqualifikation**. `off` schaltet historischen RPC-Abgleich ab, **nicht** notwendige Evidence-Sicherung oder Capture-Finalisierung. Qualifikationsachsen: Capture-Clock, Live-Coverage/Transport, Verification, Economics, Trading/LIVE und C0-E1 Master.

- `off`: Verification `NOT_RUN`; kein Chain-Beweis.
- `index` (Default nur bei ausreichenden geprüften FREE_ONLY-Quoten): vorhandener P3-TX-Index, maximal `INDEX_VERIFIED`; 429/Pruning erzeugt `HISTORICAL_INCOMPLETE` mit Checkpoint.
- `strict` (nur explizit): finalized vollständige Slot-/Block-/Parent-/Skip-Proofs, maximal `CANONICAL_VERIFIED`.
- Optionaler zweiter unabhängiger `crosscheck`-Index aus dem bisherigen Mastervertrag bleibt zulässig; maximal `CROSSCHECK_VERIFIED`.

**C0-E1 MASTER-PASS bleibt an vollständige kanonische Evidence und seine übrigen harten Gates gebunden.** Ein grüner Index-/Crosscheck-Bericht ist kein kanonischer Markt-Census. Strict: bounded idempotente, fortsetzbare Batches, Parent-/Skip-Proofs, Journal, Preflight für Providerarchiv/-quota, Disk/RAM/Netz/Walltime, gemeinsame P2-Admission, requested/allowed/effective-RPC-Budgets. Die interne 100.000-Slot-Gesamtgrenze ist keine allgemeine Solana-Protokollgrenze; längere Intervalle segmentieren. HTTP 429, Pruning oder offene Slots ergeben INCOMPLETE, kein stilles PASS.

**Resume nur für Index-/Strict-Verifikation und Reportverarbeitung**, nicht als Reparatur vergangener Live-Coverage: Ein ursprünglich unterbrochener fünfstündiger Capture bleibt `LIVE_COVERAGE_INCOMPLETE`, selbst wenn später die historischen kanonischen Slots vollständig nachgewiesen werden.

#### L.3 Drei unabhängige persistierte Identitäten

| ID | Gebundener Inhalt | Invariante |
| --- | --- | --- |
| `capture_identity` | **Stabile `capture_run_id`** und immutable Start-Spezifikation: geplantes UTC-/monotones Fenster, Capture-Source, RunConfig-/Plan-/Code-/Protokollpins | Run-ID **vor Capture** erzeugen und persistieren; sie wechselt durch Finalizing, zusätzliche Verifikation, Resume oder Report **niemals**. |
| `verification_identity` | **Stabile `verification_spec_id`** = deterministischer Hash aus `capture_run_id` + unveränderlichem Prüfmodus/-Scope + Decoder-/Protokoll-/Schema-Version + unveränderlicher Provider-Policy bzw. definierter Datenquellenbindung (ohne Secrets) | Weder Checkpoint/Retry/Attempts noch Ergebnisdigest sind Bestandteil dieser stabilen ID. Dieselbe Prüfdefinition kann nach Abbruch wiederaufgenommen werden. |
| `report_query_identity` | `capture_run_id`, **ausgewählte finale `verification_result_id`**, Reportfilter, View, Provenienz und Report-/Renderer-Version | Jede Ansicht ist reproduzierbar; sie verändert Capture/Proof nicht und behauptet keinen fertigen Verifikationsbeweis, wenn kein finaler Resultatdigest existiert. |

**Zweistufiges Capture-Finalizing:** Zuerst `capture_run_id` + Start-Manifest dauerhaft schreiben, dann Capture/Drain/Originaldaten; erst **nach** abgeschlossenem oder fail-closed gestopptem Capture Hashes/Dateiindex/Integrität in einer unveränderlichen `capture_evidence_manifest`-Finalisierung sichern (unterbrechungsfestes Journal). Der Raw-Evidence-Hash ist **nicht Bestandteil der zuvor vergebenen Run-ID**; zusätzliche Proofs dürfen keine neue Run-ID erzeugen. Der Manifeststatus muss vollständige bzw. unvollständige Live-Coverage beibehalten.

**Verifikations-Lebenszyklus:** `verification_spec_id` bleibt stabil; jede Ausführung erhält eine eigene `verification_attempt_id` mit veränderlichem Cursor, Fortschritt, `Retry-After`, Kosten und Checkpoint. Ein erfolgreiches oder terminal unvollständiges Prüfergebnis erhält separat `verification_result_id` (Proof-/Resultatdigest, Abschlussstatus, Evidence-Scope und Provenienz). Resume prüft Spec-/Schema-/Capture-/Source-Identität und lädt nur den zulässigen Checkpoint; es verändert weder `capture_run_id` noch die stabile Spec-ID. Unveränderliche Resultate werden nicht überschrieben. Ein Report auf unvollständiger Verification ist ausdrücklich als `PROVISIONAL/INCOMPLETE` zu kennzeichnen und darf keinen finalen Proof vortäuschen.

Typisierter, versionierter `MeasurementConfigV2` mit intern getrennten `RunConfig`, `VerificationConfig`, `ReportQuery`. Bereits vorhandener `MeasurementPlan` bleibt alleinige Zeit-/Viertel-Snapshot-/Cutoff-Quelle. Frontend und Terminal benutzen denselben Backend-Vertrag; Bash/React implementieren keine eigene Liquidations-/Economics-Klassifikation. Alte Manifeste werden nicht rückwirkend verändert.

#### L.4 Provenienz: SHADOW ist niemals Chain-Beweis

Für **belegte tatsächliche On-chain-Ereignisse** darf `--report-provenance best` feldabhängig nach **STRICT → INDEX → LIVE** auswählen. Bei T_rx etwa zählt die originäre Live-Evidence. **SHADOW ist kein Fallback** bei fehlender Chain-Transaktion, fehlendem Winner oder fehlendem realisiertem Gewinn. SHADOW ist hypothetische Kandidaten-/Simulations-Evidence, die tatsächlichen Entities optional zugeordnet wird; Shadow-only erscheint **separat** als `SHADOW_ONLY/HYPOTHETICAL` und nicht in bestätigten Chain-Liquidations-/Winner-/Realized-PnL-Summen.

Korrelationsschlüssel: Signature + Slot und belegbare TX-/Instruction-/Eventposition, weil eine TX mehrere Marker/Ereignisse enthalten kann. `provenance=all` zeigt mehrere Evidence-Quellen, aggregiert dieselbe Entität aber nur einmal. Unklare Quellenzuordnung = UNKNOWN.

#### L.5 P5: vollständige Terminalsteuerung, automatischer Post-Capture-Verifier und Frontend-Vertrag

**Ziel-CLI, ausdrücklich nicht bereits implementiert:**

~~~bash
./project_0/scripts/test.sh --duration 5h --chain-verify index --capture-source logs
./project_0/scripts/test.sh --duration 5h --chain-verify off
./project_0/scripts/test.sh --duration 5h --chain-verify strict
./project_0/scripts/verify.sh --run RUN_ID --mode strict --resume-verification
./project_0/scripts/report.sh --run RUN_ID --report-types all --report-markers all --report-related-events off --report-provenance best --report-view economics --format table
~~~

Altes `--verify index` nur als Alias desselben Parsers zu `--chain-verify index`. `--capture-source logs|transactions` ist **nicht** die Report-Provenienz und **nicht** die Verification. Keine überladene `--source live/index/strict/shadow`-Option.

Zielparameter: `--duration` (validierter MeasurementPlan; 10m/1h/5h/24h), `--chain-verify off|index|strict`, `--capture-source logs|transactions`, `--report-types` (vier Arten/all), `--report-markers` (sechs Marker einzeln/all/none), `--report-related-events on|off`, `--report-view summary|liquidations|winners|competition|economics`, `--report-provenance best|live|index|strict|shadow|all`, `--report-from`, `--report-to`, `--report-min-net-usd`, `--format table|json|csv`, `--verify-rps`, `--verify-max-calls`, `--verify-max-seconds`, passende Disk-/RAM-Limits und `--resume-verification`.

**Jeder Reportfilter wirkt nur auf die Ansicht**, niemals auf Rohaufzeichnung, Verifier-Scope, Klassifizierung, gespeicherte Economics oder Trading-MinProfit. Unbekannte Nettoergebnisse bleiben UNKNOWN und werden separat gezählt.

**`--report-related-events`: vier explizite Marginfi-Nicht-Liquidationsinstruktionen** anhand des gepinnten Programmstands: `drift_claim_bad_debt` (Bad-Debt-Claim), `purge_deleverage_balance` (Bereinigung eines Deleverage-Bestands), `marginfi_account_keeper_close_order` (Keeper-Schließvorgang) und `marginfi_account_init_liq_record` (Anlegen eines Liquidationsrecords). Jede Operation wird in `RELATED_NON_LIQUIDATION` mit eigener Signature/Slot/Instruktionsprovenienz und Erfolg/Fehler/UNKNOWN ausgewiesen; **nie** als eine der vier realisierten Liquidationsarten, nie in Liquidations-/Winner-/Profitzählungen. Besonders `marginfi_account_init_liq_record` darf als Vorbereitung nicht einen Receivership-Erfolg belegen. Der `on/off`-Schalter beeinflusst ausschließlich die Reportansicht; Erfassung und Decoder bleiben vollständig. P3/P5 prüfen exakte gepinnte Discriminators und negative Fixtures; keine bloße Namens-/Logtext-Heuristik.

Nach Capture-Drain/Manifest-Sicherung den Verifier nach Wahl **automatisch** starten (bei `off` keinen historischen Chain-RPC; Strict nur nach ausdrücklichem Preflight). Bei 429 checkpointen und `HISTORICAL_INCOMPLETE` melden, ohne das Capture fälschlich als vollständige Live-Coverage zu deklarieren. Normalisierte gemeinsame Records für Chain-Entities, vier Klassen/sechs Marker, Live/Index/Strict, Winner, Konkurrenz, Shadow-Zuordnung, tatsächliche Gebühren, Gross/Executable/Realized Economics, UNKNOWN und Statusachsen. Table/JSON/CSV sind nur Renderer **identischer Datensemantik**.

#### L.6 P6: verpflichtende Offline-/Property-/Integrationstests

Raw-Evidence-Hashes invariant unter Reportfiltern; stabile `capture_run_id` vor Capture und nach Finalisierung; `verification_spec_id` invariant unter Abbruch/Checkpoint/Retry/Resume bei identischer Prüfdefinition; getrennte `verification_attempt_id` und unveränderliche `verification_result_id`; reproduzierbare `report_query_identity` mit korrekter Resultatbindung; die vier benannten `RELATED_NON_LIQUIDATION`-Operationen niemals als Liquidations-/Gewinnereignisse; sechs Marker/vier Klassen mit korrekten Start-/Endpaaren; failed/CPI/LUT/v0/v1/Legacy/truncated; kein Marker-Ausblenden ändert LiquidationType; table/json/csv semantisch identisch; `all` ohne Doppelzählung; Shadow-only nie als Chain-Liquidation/-Winner/-Profit; `off/index/crosscheck` nie Canonical; Strict nur bei vollständigen Proofs; abgeschnittener Live-Capture bleibt abgeschnitten; Wiederaufnahme identisch zur ununterbrochenen Verification; HTTP 429/Pruning/defekter Checkpoint/Diskvoll sauber fail-closed; Providerhardlimits nicht überschreibbar; Reportflags aktivieren weder Paid-Fallback noch LIVE/Submission. Sicherheitskritische Invarianten vollständig, übrige Kombinationen systematisch/Pairwise testen.

Code-/Git-SHA, drei Identitäten, Testliste PASS/FAIL/NOT_RUN, tatsächliche Ressourcen-/RPC-/Evidence-Werte und offene Provider-/Mainnet-Qualifikationen im `MEASUREMENT_LEDGER.md` dokumentieren. Keine erfundenen fünfstündigen Marktergebnisse und kein abgesenkter C0-E1-MASTER-PASS.

#### L.7 Querverweise zu Produktphasen und Reihenfolge

**§0.4/S1.6:** FREE_ONLY ohne verpflichtende laufende Provider-/Serverabos und ohne automatische Paid-Fallbacks; variable On-chain Fees, Jito Tips, Swapkosten, Slippage und Rent bleiben echte Kosten. **P4A/P5 (Produkt):** bounded JIT-ExactIn als realer Fundingkandidat im bestehenden Router, gegen NO_SWAP/ZERO_PREFUND/PREHELD wirtschaftlich prüfen. **P6-MT.11/P8:** globale atomare, restartfeste Wallet-/Inventory-/Fee-/Tip-/Rent-Reservation, UNKNOWN bis Reconciliation halten; Inventar-Sollbestände und risikobasiertes Rebalancing, kein pauschaler Vollverkauf. **S3.R/P7/K:** RPC/JitoSingle/JitoBundle und später experimentelles Direct TPU nach realer Landingquote, p95/p99 Handoff-to-Landing, Fees/Tip, Fehlversuch/UNKNOWN und realisiertem Nettoedge vergleichen, Jito-Receipt ist kein Landing-Beweis; Direct TPU braucht SWQoS-/Capability-Beleg. **P6-MT:** bestehender Single-Canonical-Owner, bounded Worker und M4-Safety-/Tail-Gates. **P7/P4B:** `p(win)` erst bei echten ausreichend gelabelten Winner-/Landing-Daten gegen Forward-/Holdout-Baseline; Shadow/UNKNOWN sind keine echten Winnerlabels.

**C0-E1-Abnahmereihenfolge:** P3 Marker-/Paarfix → P4 Strict-Entkopplung/Status/Resume/Budgets → P5 gemeinsame Terminal-/Frontend-Konfiguration/Reports/Auto-Post-Capture → P6 Gesamtinvarianten. Unabhängige Race-/Jito-/Prepared-TX-/JIT-/Reservations-/Rebalancing-/M4-Produktarbeiten dürfen **parallel** vorankommen, ohne bestehende State-/LIVE-/Submission-Gates aufzuweichen.

**Non-Claims:** Diese Planänderung autorisiert weder LIVE/Submission, noch Paid-Provider, neue 5h-/24h-Captures oder Profit-/Markt-Vollständigkeitsbehauptungen.

### Operational RPC/WSS/Mainnet configuration — binding extension 2026-10-14

The FE-00–FE-07 profit-first contract remains unchanged. The bounded initial FE observation (3 reads, 1 RPS, 30 seconds, 16 MiB) is a smoke default, not a permanent product ceiling. Reuse the current provider budget, WSS ingress, runner, MeasurementPlan and verifier; do not create another execution engine.

One typed Rust operational contract owns profiles `smoke/standard/performance/custom`, explicit endpoint priority, RPC rate/burst/concurrency/admission queue, WSS connections/subscriptions/message bounds, attempts/timeouts/backoff/recovery, workload rates, payload/call budgets, existing worker/queue controls, duration and phase budgets, verifier resource limits and diagnostic switches. Terminal help and a redacted human-readable effective-config text view must expose names, types, units, defaults/ranges, active values, sources and restart requirements; later frontend clients consume the same typed/binary contract (no new internal JSON data model). Profile < environment < config file < CLI. Invalid or unknown settings fail closed. Legacy runs remain readable.

Configured throughput is a ceiling, not provider qualification. Prioritize state/health/simulation, bound lower-priority queues, honor shared 429/Retry-After, and adapt within configured ceilings. Explicit fallback lists do not prove independent quotas or free provider entitlement. FREE_ONLY remains mandatory; no implicit paid provider, LIVE or financial authorization may follow from an operational profile. Existing protocol, freshness, capital, economics, single-flight and submission gates remain binding. Retries must never repeat an authorized financial handoff.

Local regression/build/format/differential-Clippy evidence and synchronized Markdown/JSON status are required. Real provider capacity, current source/state parity, full candidate simulation and positive conservative realized exit economics remain separate evidence gates. Mainnet observation, Shadow and any monetary pilot require their respective explicit user authorization. Documentation and scope: `project_0/docs/OPERATIONAL_CONFIG.md`; qualification: `project_0/docs/evidence/OPERATIONS_CONFIG_20261014/`. Race, Jito, JIT and M4 obligations remain unchanged.

### Bindender Speichervertrag — Binärjournal (2026-10-16)

**VERBINDLICH / ZIEL, nicht bereits implementiert oder qualifiziert.** Geltung für Project_0 P6-LT, C0-E1, P6-MT, FE-00 bis FE-07, P8 und Race. Architekturreferenz: bsc/docs/BSC_PRODUCTION_BUILD_MASTER_PLAN.md, Abschnitte 10 und 11. Kein zusätzlicher SQLite-/JSONL-/Parquet-Primärspeicher neben dem Binärjournal.

**PERSIST-01 – Einheitlicher Primärspeicher:** Alle neuen produktiven Runtime-, Capture-, Shadow-, Liquidations-, Winner-, Reconcile- und Replay-Ereignisse werden in EINEM kanonischen, versionierten binären Append-only-Journalvertrag gespeichert. Segmente innerhalb dieses einen Vertrags sind zulässig. Keine neuen parallelen JSONL-/SQLite-/CSV-Writer. Bestehenden typisierten Writer migrieren, keine zweite Runtime bauen. Format mit Magic, Schema/Version, Event-Typ, definierter Endianness, begrenzter Frame-Länge, originalen Bytes bzw. verlustfreien Feldern einschließlich i128/u128, source_seq und journal_seq, Run/Session/Boot/Source/Slot/Fork/Zeiten, Prüfsummen, Hash-Manifest, Schemaevolution und UNKNOWN.

**PERSIST-02 – Harte Hot-Path-Grenze:** Trading-Owner ohne JSON-Encode/Decode, Dateizugriff, SQLite/DB, blockierendes Loggen/Writer-Wait, Flush/fsync oder zusätzliche Recorder-RPCs. Übergabe über typisierte, ownership-sichere, bounded Queue an dedizierten Writer. Bei Überlauf niemals Silent Drop oder COMPLETE; Capture als INCOMPLETE, finanzkritische Admission als BLOCKED/HALT. Keine heimliche synchrone Ausweichschreibweise. Chain-Transport und externe unvermeidliche JSON-RPC-Verträge bleiben außerhalb des zeitkritischen Recorderpfads.

**PERSIST-03 – Geld und Crash-Recovery:** Ein Binärjournal allein beweist keine crashsicheren Wallet-Reservations. FE-04/P8 müssen einen einzigen persistierten, replaybaren Intent-/Reservation-/UNKNOWN-/Reconciliation-Automaten mit klaren Commit-/Sync-Grenzen nutzen. Werttragender Handoff nur mit VORHER nachweislich durable bestätigter Reservation/Intent-Version; fehlende Bestätigung blockiert SEND. Writerfehler, Disk-full, abgeschnittene Frames, Restart, Reorg und UNKNOWN dürfen weder Kapital vorzeitig freigeben noch Doppel-Submission bewirken. Durability-Barriere vor den aktivierten zeitkritischen Trading-Pfad legen.

**PERSIST-04 – Keine Löschung historischer Verträge:** Historische eingefrorene JSON-/JSONL-Evidence, Phasenplan-/Ledger-JSON-Indizes, gebundene Hash-/Spec-IDs, Fixtures und externe RPC-/API-/CLI-Verträge bleiben unverändert nutzbar; dies ist ein read-only-Bestandsschutz und keine JSON-Freigabe für die produktive Runtime. Neue interne Reports, Replay-/Verifier-/Capture-Übergaben, temporäre Dateien und Writer sind typisiert/binär. Historische Reader/Fixtures nur in isolierter Offline-Kompatibilität, keine neuen JSON-/JSONL-Exports im produktiven Ablauf; neue menschenlesbare Terminalberichte als Text. Bindende Nachweise dürfen nicht gelöscht, stillschweigend neu gepinnt oder semantisch verändert werden. Für externe notwendige JSON-RPC-Wire-Verträge gilt ausschließlich die JSON-FREE-INTERNAL-Randregel am Dokumentende.

**PERSIST-05 – Qualifikation vor Abschluss:** 1. Bestehende Writer, Consumer, Evidence-IDs und Referenzhashes inventarisieren. 2. Writer auf Binärframes, Writer-Drain, Manifest und Reader/Replay migrieren; JSONL-Runtime erst nach Consumer-Parität abschalten. 3. Beide Rust-Feature-/Release-Builds und Rust/Python/Verifier-Regressionen, Replay-/Report-/Economics-/i128-Parität nachweisen. 4. Negativtests für Version, Bytekorruption, CRC, Trunkierung, Queue-Overflow, Disk-full, Sync-Fehler, Restart, Reorg, UNKNOWN und Double-Send. 5. Speichervolumen, Writer-Durchsatz und p50/p95/p99-Tails unter vergleichbarer Last messen; ungemessen = NOT_MEASURED. 6. Erst nach vollständiger Evidenz QUALIFIED melden, andernfalls PARTIAL/BLOCKED.

**Unveränderte Gates:** FREE_ONLY, LIVE und Submission gesperrt, kein Mainnet-/Echtgeldlauf ohne separate Freigabe; alle FE-00–FE-07-, Markt- und Chain-Abnahmen gelten weiter. BSC nicht verändern. Diese verbindliche Norm ersetzt frühere Empfehlungen eines SQLite-/JSONL-/Binär-Mischbetriebs für neue primäre Project_0-Runtime-Daten.

### Verbindliche Speicher-/RAM-/Offline-Analysearchitektur (2026-10-17) – Konkretisierung PERSIST-01–05 / JFI-01–05

**VERBINDLICHER ZIELVERTRAG, NICHT BEREITS IMPLEMENTIERT ODER QUALIFIZIERT.** Diese Ergänzung präzisiert den vorhandenen PERSIST-/JFI-Vertrag für Market-Capture, Shadow, Long-Tail, Economics, FE-04/P8 sowie 4h-, 24h- und 48h-Läufe. Keine neue Hauptphase, keine zweite Runtime/Trading-/Economics-Engine, keine globalen Refactorings. Historische Evidence und alle bisherigen FE-, C0-E1-, LIVE-, FREE_ONLY-, Permission- und Safety-Gates bleiben unverändert.

**SK-01 – Drei Ebenen, eine einzige Datenwahrheit.** (A) Externer Solana-Wire-Rand → native Rust ChainTx/ChainBlock → State/Health/Trading/Shadow im RAM ohne I/O-Wait. (B) Typisierte, ownership-sichere, nach Anzahl **und Bytes** begrenzte Übergabe → dedizierter MarketJournalWorker → sequenzielle begrenzte Batches → **kanonische versionierte binäre .p0j-Journalsegmente** mit vollständigen Chain-Fakten, Provenienz, Sequenzen, CRC, Hash-Verkettung, Manifest und Replay. (C) **Optionaler, separater Offline-Rust-Indexer** erzeugt ausschließlich aus verifizierten, versiegelten .p0j-Segmenten einen jederzeit rekonstruierbaren SQLite-Analysecache für historische Suche. .p0j ist die einzige autoritative Markt- und Finanz-Evidence; SQLite, Aggregate und Reports sind Derivate, keine zweite Quelle von Wahrheit. Keine neuen produktiven JSON/JSONL/CSV/SQLite/Parquet-Primärwriter, keine interne JSON-Zwischenpipeline.

**SK-02 – Zuständigkeiten und Projektstruktur (Rust-Module, structs/enums; traits nur bei echtem Bedarf).** Vier vorhandene Crates bleiben; bestehende Implementierungen nicht duplizieren. Zielstruktur, Dateien mit NEU nur bei tatsächlicher Notwendigkeit anlegen:

~~~text
project_0/native/crates/
  common/src/
    channel.rs                 BESTAND: bounded Channel / Overflow
    config.rs                  BESTAND: typisierte Config-Primitiven
  solana/src/
    chain.rs                   BESTAND: native Chain-/Wire-Typen
    runtime_config.rs          BESTAND
  runtime/src/
    analytics/
      journal.rs               BESTAND: EINZIGER kanonischer Frame-Codec/Reader/Writer
      payload.rs               BESTAND: versionierte native Payloads
      evidence.rs              BESTAND: Owner-Evidence und Recovery
      econ.rs                  BESTAND: fachliche Economics / Aggregationen
    market/
      sink.rs                  ANPASSEN: typed MarketSink ohne synchrone SSD-Writes
      capture.rs               ANPASSEN: bestehender Capture-Lifecycle
    storage/                   NEU: nur Orchestrierung des bestehenden Journalcodecs
      mod.rs
      config.rs                StorageConfig
      budget.rs                MemoryBudget / BytePermit
      writer.rs                MarketJournalWorker
      segments.rs              SegmentManager
      preflight.rs             CapacityPreflight
      metrics.rs               StorageMetrics
    offline_analysis/          NEU, optionales Offline-Cargo-Feature
      mod.rs
      schema.rs
      indexer.rs               AnalysisIndexBuilder
      queries.rs               AnalysisQueries
    bin/p0-analyze.rs          NEU: eigener Offline-CLI-Einstieg
~~~

Ownership: darkbot-common besitzt Channel-/Config-Primitiven, darkbot-solana die Chain-Typen, analytics den einzigen Journalcodec und die Economics, market nur Beobachtung/Klassifikation, storage Writer-Orchestrierung/Budgets/Segmente/Preflight, offline_analysis nur Indizes und Abfragen. MarketJournalWorker klassifiziert **keine** Liquidationen; SegmentManager berechnet keine Economics; SQLite-Indexer und Query-Schicht führen **keine** unabhängige Winner-/Economics-/Protokollfachlogik aus. Keine zirkulären Abhängigkeiten. Vorhandenen bounded sync_channel weiterverwenden; kein zusätzlicher komplexer Lock-free-Ringbuffer ohne belegten Engpass.

**SK-03 – Einstellbares, hart begrenztes RAM-Bytebudget.** Der bestehende operative Config-/CLI-Vertrag erhält explizite validierte Felder für initial_mib, max_mib, system_reserve_mib, Queue-, Batch-, Writer-/Indexpuffer-Budgets und Dauer. **Keine verstreuten fixen RAM-Hardcodes.** initial_mib ist die anfängliche Zulassungsgrenze, keine Pflicht-Vorallokation; max_mib ist die **niemals überschreitbare Capture-Zulassungsgrenze**. MemoryBudget/BytePermit reserviert vor der Queueaufnahme Bytes für Event, tatsächliche dynamische Kapazität/Overhead und zusätzliche vom Capture verwaltete Writer-/Serialisierungs-/Indexpuffer; Freigabe erfolgt automatisch. Zusätzlich Prozess-RSS, Allocator-/OS-Druck, macOS-Swap und System-/Trading-Reserve beobachten: ein Byte-Zähler allein garantiert **keine** absolute Gesamtprozess-RSS-Grenze. Adaptive Puffer/Batches wachsen nur innerhalb freier Budgets und geben unbenutzte Kapazitäten zurück. Keine RAM-Collections proportional zur Messdauer, keine unbounded Queues und kein Swap als Ersatz für die Kapazitätsplanung. Verifier und SQLite-Offline-Indexierung haben getrennte harte RAM-/Diskbudgets und keine bevorzugte Trading-Ressourcenfreigabe.

**SK-04 – Separater Batch-Writer und Haltbarkeit.** Trading-Owner und zeitkritischer Ingress warten niemals auf Market-Dateischreiben, Flush/fsync, SQL, JSON-Encode oder Recorder-RPC. Native Market-Events gehen an einen dedizierten Writer; dort bounded Batching, deterministische Frames, effiziente sequenzielle Writes und konfigurierbare Sync-/Checkpoint-Grenzen. Kein Sync pro Markttransaktion, sofern nicht ein ausdrücklich strengerer Durability-Vertrag gilt. OS-gepufferte Daten sind vor bestätigtem Sync **nicht** crashsicher; unsichere Tailbereiche und fehlgeschlagene Writes als INCOMPLETE erfassen. Finanzielle FE-04-/P8-Intent-/Reservation-Versionen bleiben vor SEND zwingend nachweislich durable; keine Lockerung dieser Safety-Barriere durch Market-Batching. Roh-Capture und finanzieller Journalpfad dürfen getrennte Writer-Lanes besitzen, verwenden aber denselben kanonischen Journalvertrag.

**SK-05 – Rückstau und 48h-Verfügbarkeit.** Ein kurzer SSD-Stall führt nicht allein zum sofortigen Abbruch: freie RAM-Reserve innerhalb max_mib nutzen, Batchgrößen begrenzt anpassen, Offline-Analyse pausieren, Roh-Capture priorisieren, Engpass/Restpuffer und prognostizierte Erschöpfungszeit im Terminal melden. **Vor dem Langlauf** anhand realer Datenraten/Peaks und nachhaltiger Writer-Leistung dimensionieren, damit vermeidbare Abbrüche ausgeschlossen werden. Eine physikalisch dauerhaft zu langsame/ausgefallene SSD kann bei hartem RAM-Maximum nicht beliebig verlustfrei überbrückt werden; daher keine unhaltbare 48h-Garantie: Bei nachgewiesen unrettbarer Vollständigkeitsverletzung INCOMPLETE/finanzielle Admission HALT statt Silent Drop, unechtem COMPLETE oder heimlichem unbounded Wachstum. Keine automatische Erhöhung über Nutzer-Maximum.

**SK-06 – CapacityPreflight und laufende adaptive Kontrolle.** Vor 4h-/24h-/48h-Capture bei entsprechender Langlauf-Freigabe: geplante Dauer; belegte durchschnittliche und Burst-Bytes/s; erwartete Journalbytes samt Frame-/Sync-/Segment-Overhead und SSD-Sicherheitsreserve; nachhaltige Writer-Rate und tolerierbare Stallzeiten; Peak-/Capture-RAM einschließlich Betriebssystem- und Trading-Reserve; CPU, Provider-/Archivbudget und **separater** späterer SQLite-Platzbedarf. Fehlende reale Grundlage offen NOT_MEASURED/CAPACITY_UNVERIFIED, keine erfundenen Benchmarks. Bei nicht qualifizierter Kapazität keinen langen Capture automatisch starten. Während des Laufs Datenrate, Queue-Bytes/-Lag, RAM, SSD-Freiraum, Writer-Throughput und Restzeit begrenzt fortschreiben; dynamisch innerhalb der freigegebenen Konfiguration reagieren. 48 Stunden erfordern bei ausreichender permanenter Writer-Rate nicht 48-fachen RAM, wohl aber passende SSD-Kapazität und Reserve. Keine ungebremsten IO-Nebenjobs.

**SK-07 – Segmentierung, Lesbarkeit und schnellere Langzeitauswertung.** Ein Run kann mehrere versionierte .p0j-Segmente desselben vorhandenen Journalvertrags enthalten. Eindeutige Run-/Segmentidentität, fortlaufende Sequenzen, Vorgänger-/Root-Hashes, Zeit-/Slot-/Fork-Provenienz, Sync-/Seal-Status und Run-Manifest verbinden sie prüfbar. Historische/versiegelte Segmente niemals verändern. Crash/Truncation als INCOMPLETE nachweisen, Reader/Replay und Index-Rebuild müssen segmentübergreifend funktionieren. Bestehende Stunden-/Viertelstundenaggregate native/typisiert wiederverwenden; gezielte historische Abfragen dürfen Zeit-/Slot-/Offset-Indizes verwenden. Ein anderer Decoder-/Economics-Versionstand benötigt gegebenenfalls erneute Ableitung; keine stillschweigende Wiederverwendung fachlich veralteter Aggregate. Der aktive C0-E1-Rust-Verifier bleibt unabhängig von der Bot-Marktklassifikation.

**SK-08 – SQLite nur als ausdrückliche enge Offline-Analyseausnahme.** Die bisherigen PERSIST-/JFI-Verbote zusätzlicher **produktiver** SQLite-/JSON-/JSONL-Pfade bleiben unverändert. **Zulässig ist ausschließlich ein optionaler, nichtautoritativer, separat ausführbarer SQLite-Cache nach dem Capture**; Aufbau aus bereits validierten/versiegelten .p0j-Segmenten über einen eigenen Rust-CLI-Einstieg und ein separates Cargo-Feature. Indexiert werden nur notwendige Suchinformationen (z. B. Run/Segment, Signatur, Slot, T_rx, Eventart, Account, Liquidationsklasse, Winner, Segment-Byteoffset) sowie bereits fachlich berechnete Stunden-/Kapital-/Economics-/Coverage-Aggregate. Keine zweite vollständige Roh-TX-Kopie, kein SQL im Trading-, Ingress-, Recorder-, finanziellen, aktiven Chain-Verifier- oder produktiven C0-E1-Reportpfad. Index-Identität bindet capture_run_id, .p0j-Hash/Schema, fachliche Decoder-/Economics-Version und Indexschema; beschädigte/fremde Caches werden verworfen und neu aus .p0j aufgebaut. SQLite-Transaktionen/WAL/Checkpoints und RAM-/Disk-Limits begrenzen; kein unbegrenztes WAL. Exakte i128/u128-Fakten verlustfrei speichern, niemals durch REAL/double ersetzen. SQLite ist **kein** zusätzlicher operativer Eventbus oder Store und darf nie als alleiniges Winner-, Fee-, Reconcile-, Coverage- oder Verifier-Evidence gelten. Neue produktive Reports und Strict-Verifier bleiben direkt nativ auf .p0j und sind ohne SQLite vollständig funktionsfähig. Diese ausdrücklich begrenzte Offline-Derivat-Ausnahme konkretisiert PERSIST-04/JFI-03; **keine** weitere Ausnahme für internes JSON oder produktives SQL.

**SK-09 – Einordnung, Umsetzung und Abnahme.** Keine weitere Planungsphase: (1) laufendes Ticket 3 mit nativen MARKET_TRANSACTION/MARKET_LIQUIDATION/MARKET_COVERAGE-Payloads und historischer Reader-/Verifier-Parität fertigstellen; (2) vorhandenen MarketSink an dedizierten Batch-Writer anbinden; (3) StorageConfig/MemoryBudget/BytePermit/StorageMetrics implementieren; (4) kanonische Segmente und CapacityPreflight ergänzen; (5) native Economics-/Report-Consumers fertig qualifizieren; (6) optional **nachrangig** Offline-SQLite-Indexer/CLI; (7) gezielte PERSIST-/JFI-Negativ-/Paritätstests (Overflow, Disk-full, Sync-Fehler, Crash, Replay, Index-Rebuild, bitgenaue Zahlen), keine unnötigen Testschleifen. STATUS.md §12 und MEASUREMENT_LEDGER.md mit tatsächlichem CODE/LOCAL_PASS/OPEN/BLOCKED fortschreiben. 600-s-/4h-Abnahmen und First-Realized-Profit-Priorität gemäß §0.6 nicht durch optionales SQLite blockieren; 24h-/48h-Mainnet-Läufe bleiben jeweils genehmigungspflichtig; bsc/ unberührt.


### Bindender JSON-FREE-INTERNAL-Vertrag — gesamte produktive Project_0-Architektur (2026-10-16)

**VERBINDLICHE VERSCHÄRFUNG, AB SOFORT ZIELARCHITEKTUR; IMPLEMENTIERUNG NOCH NICHT QUALIFIZIERT.** Gilt übergreifend für P0/P1/C0/H0, S0–S4, P2–P9, T, C0-MO/C0-E1, FE-00–FE-07, P6-MT, P8-RES/INV, Race, K und spätere Frontend-Anbindung. Diese spätere, ausdrückliche Regel hat bei JSON-Widersprüchen Vorrang vor §1.7, dem bisherigen PERSIST-01–05-Text, den älteren CLI-/Evidence-Beispielen und historischen Architektur-Snapshots. Der übrige Phasenplan und alle Gate-/LIVE-/Kosten- und Produktverpflichtungen bleiben verbindlich. Kein Architektur-Neustart, keine zweite Codebasis.

**JFI-01 – Ausschließlich typisiert/binär intern:** Sämtliche **neuen produktiven internen** Zustände, Events, Payloads, Ingress-/Worker-/Owner-/Planner-/Economics-/Wallet-Übergaben, Queues, IPC, Persistenz, Capture, Shadow, Winner, Replay, Verifier, Reporting-/Telemetry-Zwischenstände und Frontend-Datenverträge sind native Rust-Typen bzw. versionierte binäre Formate. Für dauerhaftes Event-/Finanz-Evidence bleibt der **eine** bestehende kanonische versionierte binäre Append-only-Journalvertrag `.p0j` maßgeblich. Keine parallelen Binär-Speichersysteme oder unnötigen Neuimplementierungen. Kein produktives internes `serde_json::Value`, `json!`, `serde_json::to_value/from_value`, internes JSON-Encode/Decode, JSONL-/JSON-Datei-I/O oder JSON-basierter Bus, auch nicht außerhalb des Trading-Hot-Paths. Ein Binärframe aus einem vorher erzeugten JSON-Value zählt **nicht** als JSON-freie interne Verarbeitung. Binäre Wire-/Persistenzschemas müssen deterministisch, versioniert, verlustfrei für i128/u128 und überprüfbar sein; UNKNOWN/fehlende Werte nie stillschweigend ersetzen.

**JFI-02 – Einzige Laufzeit-Ausnahme: zwingender externer JSON-RPC-Wire-Rand:** JSON-Serialisierung/Parsing nur unmittelbar dort, wo ein tatsächlich benötigtes **externes** Solana-RPC-/WSS-/JSON-RPC-Protokoll sie zwingend verlangt (einschließlich dessen Antwort-Nutzlast). Nach dem Wire-Adapter sofort in konkrete, validierte native Rust-Typen dekodieren; davor Requests aus konkreten Typen serialisieren. Keine Weitergabe generischer JSON-Value-Bäume, Roh-JSON-Strings oder JSON-Zwischenobjekte an State, Health, Economics, Scheduler, Recorder, Verifier oder CLI. Nicht zwingend JSON-basierte interne oder externe Schnittstellen dürfen JSON nicht als bequemen Standard wählen. Der vorhandene, zulässige FREE_ONLY-Long-Tail-WSS-Transport bleibt möglich; der Race-Ingress bleibt qualifiziert binär/native. **Einzige zusätzliche externe Ausnahme (Masterauftrag 2026-10-16):** der bestehende Jupiter-`/swap/v2/build`-REST-Adapter (`runtime/src/execution/route.rs`) darf JSON ausschließlich beim unmittelbaren HTTP-Request-/Response-Wire-Encode/Decode dieser einen Schnittstelle verwenden, mit konkreten, validierten Rust-Request-/Response-Typen; dahinter keine generischen JSON-Values und keine Roh-JSON-Strings in Router, Builder, Planner, Economics oder Journal; Atomic Units, ExactIn, MinOut/`other_amount_threshold`, Slippage, Program-Allowlist, LUTs und Fehlerantworten werden typisiert geprüft. Keine neue Routing-Engine, keine Metis-Infrastruktur, keine weiteren REST-JSON-APIs ohne gesonderte Aufnahme an dieser Stelle.

**JFI-03 – Kompatibilität statt Beweisverlust:** Bereits vorhandene und hash-/spec-gepinnte historische JSON-/JSONL-Evidence, Fixtures, Checkpoints, API-Verträge sowie `PHASENPLAN.json`/`MEASUREMENT_LEDGER.json` als **nichtoperative Dokumentations-/Navigationsindizes** bleiben erhalten; sie sind kein produktiver Datenpfad und dürfen nicht zur heimlichen neuen internen JSON-Ausnahme werden. Historische Legacy-Leser/Verifier ggf. isoliert read-only/offline betreiben; aktive neue Verifikation/Replay/Reports direkt an den binären Journalreader und native Typen anbinden. Neu produzierte produktive Evidence ausschließlich binär, menschenlesbare CLI-/Report-Ausgabe als Text; kein automatischer JSON-/JSONL-Rendereinsatz als Bestandteil des produktiven Runs. Gepinnte historische Proof-Identitäten und Bytegleichheit nicht überschreiben. Wenn ein altes eingefrorenes Format nur per Änderung seines Pins weiterproduziert werden könnte: **BLOCKED**, gesonderte überprüfte versionierte Migration/Re-Pin-Freigabe, kein stiller Beweisbruch.

**JFI-04 – Gezielt migrieren, kein Rewrite:** Zuerst die konkreten JSON-Lecks pro aktiver Runtime-Schicht inventarisieren (`serde_json::Value`, `json!`, Encode/Decode, `MarketSink`, `JournalSink`/Journal-Payload, FE-04-Versionen, FE-01-Snapshots, Config/Control/Verifier/Reports und Subagent-/CLI-Handoffs). Bestehende fachliche Algorithmen, Module, historische Leser, Safety-Gates, Journal-Header/Sequenz/CRC/Hashes und Tests behalten. An Schichtgrenzen gezielt auf eigene native DTOs und denselben binären Vertrag umstellen; **keine** globale Refaktorierung, keine parallele Implementierung, keine neuen produktiven JSON-Writer. Die notwendige Format-/Schema-Evolution erhält eindeutige Versions-/Reader-Kompatibilität und Fail-Closed bei nicht unterstützten Versionen.

**JFI-05 – Gezielte Abnahme statt Testschleifen:** Pro tatsächlich verändertem Modul wenige gezielte Positive-/Negativ-/Paritätstests und einen überprüfbaren statischen/dynamischen Nachweis, dass hinter erlaubten externen JSON-RPC-Wire-Adaptern keine interne JSON-Abhängigkeit verbleibt. Historische Evidence-IDs, Read-/Replay-/Verifier-Identität, bitgenaue Zahlen, Crash-/UNKNOWN-/Double-Send-Sicherheit und externe RPC-Kompatibilität müssen erhalten bleiben. Größere Regression einmal am sinnvollen Gate-Abschluss oder nur bei Änderung mit entsprechendem Querschnittsrisiko, nicht nach jedem kleinen Edit. Keine künstlichen PASS-Gates: aktueller Stand **JFI-01–05 OPEN / IMPLEMENTATION_REQUIRED**, bis Code, Tests und erforderliche reale PERSIST-05-/FE-Gates vorliegen.

**Umsetzungsregel:** Dieser Vertrag ist eine verpflichtende Lücke im bestehenden Phasenplan, **keine neue Parallelbaustelle**: pro bestehender Phase nur die tatsächlich betroffenen JSON-Lecks schließen, dort qualifizieren und Phase erst danach PASS setzen. FE-First-Profit-Priorität und echte Vorbedingungen bleiben bestehen. `bsc/` nicht ändern. Keine automatischen Mainnet-/Paid-/LIVE-/Submission-Schritte; separate Nutzerfreigaben gelten unverändert.

### F0-P0UI – Graphite V0 Bestandsabgleich und verbindliche Native-/Protocol-Feldverweise (2026-10-23)

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

**Ergänzung, KEIN neues Frontend/Phasenmodell:** Fachlicher Owner bleibt **dieser** Project_0-Phasenplan; UI-Phasen F0–F8, Graphite-Freeze und Wire gehören weiterhin zum bestehenden [Frontend-Master](../../frontend/docs/FRONTEND_PHASENPLAN.md) und [Wire V1.3](../../frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md). Aktuelle zeilen-/quellengestützte F0-P0UI-Matrix: [Frontend Code Map §10](../../frontend/docs/PROJECT_0_CODE_MAP.md#10-revalidierung-2026-10-23--aktueller-reactprotocolnative-abgleich). Sie ersetzt veraltete **IST-Snapshots**, nicht historische Evidence oder dieses Phasengate. Fünf bestehende Project_0-Reiter **Overview / Liquidator / Capture / Analytics / Wallets** und Graphite V7/V0 einschließlich Wallets **Balances / Send / Auto-Sweep** unverändert behalten; nur Inhalte/Controls in diesen Komponenten präzisieren. Keine zusätzliche Navigation, kein zweiter Store/Control-Client je Aktion und kein BSC-Codeumbau.

**Unveränderliche fachliche Native-Mappings für F0-VAR/F7 (jeweils UI-Label, Kontrolltyp, tatsächlicher Rust-Owner/Variablenname, physikalische Einheit, Provenienz und Gate per Matrix):**

| Bindende Project_0-Variable / Quelle | Vorhandener oder reservierter UI-Slot | Bestehender Phasen-/Sicherheitsvertrag |
|---|---|---|
| \`runtime_config.rs::Resolved::parse/descriptors\`, \`EffectiveConfig\`, \`P0_RPC_RPS\`, \`P0_RPC_BURST\`, \`P0_RPC_CONCURRENCY\`, \`P0_INGRESS_BUFFER\`, \`P0_REMOTE_WORKER_BUFFER\` | Health/Overview und später verifizierte operative Einstellungen | F0-VAR, F6-F7, P6-MT, BOOT; Backend allein liefert Defaults/Min/Max/Change-Klasse und config_version, React keine zweiten Defaults |
| \`p0ctl doctor/preflight/run start/status/stop/verify/report/export/qualify/post-capture\`; \`CapacityReport\`, \`MeasurementPlan\` | Capture: bestehende Dauer/Quelle/Run/Stop/Preflight/Verify/Report-Steuerflächen | C0-E1 L.4/L.5, FEUI-01/02, SK-06, JFI; echte Run-Start-Parameter nur nach typed Command-Contract und Boot-/Provider-Gates |
| \`gateway::control\` actions \`project0.capture.start/stop\` und \`captureControl.ts\` | Bereits existierende Capture-Start/Stop-Steuerung aus globaler Overview im **bestehenden CapturePanel** wiederverwenden | F6/FEUI-01: heutiger Gateway-Start ist **start-configured ohne frei wählbare Panel-Parameter**. Auswahl im CapturePanel bislang nur lokaler Draft; kein neues Command oder unechter Start |
| \`p0ctl run start --chain-verify\` | Capture: off/index/strict/crosscheck | **IST:** \`index\`/\`crosscheck\` derzeit blockiert, bis zum Codefix nicht wirksam darstellen. **SOLL sofort (P0-DEC-08):** beide nativ über den bestehenden Rust-Verification-Owner samt `.p0j`-Resume, gemeinsamen Controller, Protobuf/Gateway/React implementieren; diese reine Code-Sperre nach bestandenen E2E-Tests entfernen. Reale Providerqualifikation separat `NOT_RUN/BLOCKED`; historisches Python nur read-only. |
| \`market::MarketStats\`, \`status_tap::MarketSnapshot/TransportSnapshot/StorageSnapshot\` | Project0 Overview, Health, Capture | Verfügbare protobuf STATE-Topics \`project0.status\` (10, \`SystemStatus\`) und \`project0.capture.state\` (13, \`CaptureState\`); Sample-, Gap-, Detached-, UNKNOWN- und Coverage-Semantik behalten |
| \`solana::ingress::sources::{SourceUnit,SourceControl,OperatingMode,Qualification,ProviderConfig,SourceArbiter}\` | Bestehender Capture-/Health-Bereich: unabhängige WSS-/BinaryGeyser-Provider-/Deshred-/LocalShred-Status/Modi | S1-02/S1-08/K-02/K-02b/S1-RACE, A23-03/04: \`OFF/OBSERVE/ACTIVE\` unterscheidet sich von \`P0_MODE=live\`. Native Modus-/Qualifikations-/Generation-Owner und API derzeit noch nicht vollständig verdrahtet: **SOLL/NO_BACKEND**, kein eingeschalteter Fake-Button |
| \`health\`, \`sizing\`, \`economics\`, \`funding\`, \`Safety\`, \`one_shot::admission/settlement\`, WalletReservations | Liquidator: Single-Bot-/FE-Status, Safety, Gebühren, Limits, Kapital; Analytics: Economics und qualifizierte Ergebnisse | FE-00…FE-07, P4A-JIT, P8-RES/INV, PERSIST, JFI; Browser-Fleet-Drafts sind keine nativen Profile. LIVE/SEND bleibt NO_AUTHORIZATION |
| \`ReportQuery.markers/types/related_types/view/provenance\`, \`measurement_contract.py\`, native Market-Reader/Journale | Capture/Analytics: **sechs Marker, vier Klassen, vier Related-Types**, eindeutige reine Reportfilter plus getrennte Provider-/Strict-/Census-Provenienz | C0-E1/H0/JFI: Roh-Capture niemals durch die Reportfilter beschneiden; Report-Minimum != Trading-Minimum; kein Winner-Bruttogewinn als eigene realisierte Netto-PnL |
| \`one_shot::Journal/Receipt/FinalizedObservation\`, \`Safety\`, \`WalletReservations\` und **erst künftig** qualifizierter PortfolioFinancialSnapshot | Wallets: exakt sieben bestehende Finanzzeilen und Send/Auto-Sweep-Preview; Overview dieselbe künftige finance_snapshot_id | FE-04/FE-07/P8/FEUI-10A/B/C: \`project0.finance.summary\` (11) und \`project0.wallets.state\` (14) **reserviert/unavailable**. Eigene finalisierte Portfolio-PnL, Transfers und Treasury-Controls nicht aus internem Rust-Code oder historischen Winners ableiten |
| \`p0-econ-replay\`, \`p0ctl report/verify/qualify\`, \`.p0j\` Reader, \`P0_ECON_CAPITAL_USD\` | Analytics vorhandene Report/Backtest/Optimize-Unterreiter | C0-E1/P6-MT/F7: \`project0.analytics.jobs\` (15) noch reserviert/unavailable; native Report/Replay ist kein vorhandener UI-Job-Dispatcher, Optimize nur Entwurf |
| \`P6-MT.owner_wakeup_ns\`, \`owner_duty_cycle\`, \`SourceHealth.gaps\`, Capture-/Writer-Storage-Budgets | Overview/Health/Analytics: Latenz-/Coverage-/Rate-/Gap-Diagnose | P6-MT.4/.13/.15, A23-01/08: p50/p95/p99 nur aus tatsächlich gemessenen Verteilungen, kein frei erfundener p95 aus Durchschnitt/Max |

**Wichtig:** \`TopicId\`-Nummern 11/12/14/15/20/21 sind heute reservierte Protobuf-IDs, **keine** verfügbaren Projekt0-Fachsubscriptions. \`project0.status\` (10) und \`project0.capture.state\` (13) sind im \`gateway/src/topics.rs\` und \`frontend/src/core/realtime/topicTable.ts\` typisiert. Aktuelle React-Shell und Capture-Adapter existieren; frühere Code-Map-Zeilen „Project0Page nur Platzhalter“, „p0ctl fehlt“, „nur ein P0-Topic“ sind historische Momentaufnahmen.

**F0-P0UI-DoD:** (1) Revalidierung jedes UI-Controls auf aktuelle Source-SHA, Pfad::Symbol, Einheit/Validator, Engine-/Gateway-/Wire-Feldnummer, Ist/Draft/Soll/Gate und Test; (2) bestehende F0_VAR_*-Rohinventare erneut erstellen (deren Manifest stammt aus altem HEAD; Lexer-Census ≠ verifiziertes Mapping); (3) vorhandenen Capture-Control-Client und Gateway-Lifecycle ohne Doppelimplementierung im bestehenden Panel verbinden, Param-Semantik getrennt freigeben; (4) neue native Statusfelder nur via einzigen versionierten \`protocol/\`-Protobuf-Fachvertrag, Reporter/Gateway/Frontend-Adapter und Rust/TS-Golden-Tests; (5) UI zeigt \`UNKNOWN/UNAVAILABLE/DISABLED\` statt falscher 0 oder erfundener LIVE; (6) Graphite-V0/V7/Wallets-Pixeldiff-/Playwright-Regressionen. Eine Registry-Inventar-ID wird niemals stillschweigend zu einer erlaubten Gateway-Aktion. **Priorität:** Bestehende FE-/C0-E1-/S1-/A23-Gates bleiben zuerst fachlich maßgebend; dieser F0-P0UI-Plan ist ein Anbindungsvertrag, **keine** neue First-Profit-Blockade oder Entwicklung eines zweiten Bots.

---

### P0-PROD-CTRL — ULTRA-VERBINDLICHER Vollimplementierungs-, Schalter- und Frontend-Rundwegvertrag (2026-10-25)

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Nur bisher OFFENE Restarbeit dieses bereits verbindlichen Fachabschnitts; vorhandene Code-/Evidenz-PASS bleiben erhalten, echte externe Gates bleiben separat.

**HÖCHSTE VERBINDLICHKEIT FÜR PROJECT_0 – AB SOFORT.** Die folgenden Regeln P0-PROD-CTRL-01 bis -12 sind harte phasenübergreifende MUSS-Abnahmekriterien für alle verpflichtenden Project_0-Funktionen (P0–P9, S0–S4, C0-MO/C0-E1, P4A-JIT, Jito, Race, P6-MT/LT, FE-00–FE-07, P8-RES/INV, JFI, PERSIST, SK und F0-P0UI/F6/F7). Sie ergänzen und verschärfen JFI-01–05, PERSIST-01–05 und F0-P0UI. **Bei älteren widersprechenden Aussagen über produktive Mock-/Legacy-Ersatzwege, nur vorbereitete Schalter oder CLI-only-Abnahme geht dieser neuere Vertrag vor.** Alle übrigen fachlichen Anforderungen, FREE_ONLY-, Safety-, LIVE-, Signier-, Submit- und Nutzerfreigabegates bleiben unverändert. **Diese Norm ist ein Auftrag, kein behaupteter CODE-, LOCAL_PASS-, QUALIFIED- oder LIVE-Nachweis.**

**P0-PROD-CTRL-01 – NO PRODUCTIVE MOCK/STUB.** Jede Pflichtfunktion vollständig in echten nativen Rust-Produktionspfaden implementieren: reale Datenquellen, aufgerufene Funktionen, tatsächliche Finanz-/Chain-/Providersemantik, Fehler- und Recoverybehandlung. Verboten als produktive Implementierung oder Abnahmenachweis: Mock, Fake, Stub, Demo, Placeholder, hart kodierter Erfolg, fingierter Chain-/Wallet-/Fee-/Quote-/Winner-Zustand, TODO/unimplemented als erreichbarer Produktivpfad, eine immer leere Rückgabe als vorgetäuschte Capability oder unverdrahtete fertiggedeutete Module. Ein Schalter macht fehlenden Code nicht vollständig. Vorhandene strikt isolierte Test-Doubles und unveränderliche historische Testfixtures sind ausschließlich Testmittel, **niemals reale Produktions-/Provider-/Profitqualifikation**. Echte historische verifizierte Chain-/Journal-Evidence bevorzugen. Wenn ein externer Provider eine notwendige Funktion wirklich nicht bereitstellt oder reale Qualifikation fehlt, typisiert UNSUPPORTED/BLOCKED/NOT_RUN melden und sicher ablehnen; **keine** Fake-Alternative oder Umgehung des Gates, aber fehlenden eigenen Produktcode nicht als DONE ausgeben.

**P0-PROD-CTRL-02 – NO PRODUCTIVE LEGACY/INTERNAL JSON.** JFI-01–05 strikt erfüllen: produktiver State, Ingress, Jobs, Replay, Analytics, Verifier, Reporter, CLI-Übergaben, Control und Finance ausschließlich native Typen und die bestehenden versionierten binären Verträge; persistente Evidence-Autorität bleibt das eine .p0j-Journal. Keine interne Python→Rust-JSON-/JSONL-Übergabe, keine produktive JSON-Rendereingabe für neue Consumer, kein Legacy-Fallback, kein JSON-Value versteckt in Binärframes. Nur die expliziten externen JSON-RPC-/Jupiter-Wire-Grenzen aus JFI-02 sind Laufzeitausnahmen. **Unveränderliche historische Proofs, gespeicherte Finanzjournal-Versionen und isolierte read-only Offline-Decoder zwingend erhalten**, damit kein Beweis-/Recoveryverlust entsteht; diese sind keine produktive Legacy-Freigabe. Bei gepinntem Konflikt nicht stillschweigend re-pinnen, sondern Migration belegen oder BLOCKED setzen.

**P0-PROD-CTRL-03 – FULL IMPLEMENTATION THEN SWITCH.** Alle laut Fachplan schaltbaren produktiven Capabilities (Ingress/WSS/Geyser-Quellen, JIT, Jito/Bundle, Inventory/Rebalancing, Capture, Analytics/Backtest/Optimizer usw.) erhalten einen **einzigen kanonischen nativen Project_0-Controller/Policy-Owner** und typisierte validierte Betriebsparameter. Terminalsteuerung via p0ctl und Frontend-Commands führen zur **identischen** Native-Action, nicht zu zwei Control-Engines. OFF/OBSERVE/ACTIVE dort verwenden, wo dieser Modus fachlich existiert (insbesondere Quellen); für Jobs typisierte START/STOP/CANCEL/STATUS-Actions statt erfundener OBSERVE-Modi. requested, effective, qualification, block_reason, version/generation, Provenienz und Aktualität getrennt ausweisen. Jede wirksame Änderung durch Engine-Validierung, CAS/Version, vorgeschriebene Durability, Receipt und sichere Restart-/Reconnect-Semantik führen. **Kein CLI-only-Abschluss und keine Frontend-only-Draft-Schalter für als fertig deklarierte GUI-fähige Funktionen.**

**P0-PROD-CTRL-04 – VERBINDLICHER STATUS-HINWEG BIS ZUM REITER (kein „später Frontend“).** Jede für die bestehende GUI vorgesehene Funktion und jeder Schalter/Parameter muss im zugehörigen Integrations- und Abnahmepaket die folgende bestehende **vollständige** Kette tatsächlich bedienen:

Project_0 Rust-Funktion/Controller → native typisierte, sanitierte öffentliche Projektion → darkbot.project0.v1 Protobuf → Project_0 Reporter/Engine-Feed → gemeinsames Gateway mit STATE/EVENT/Source-Liveness → **ein** same-origin Protobuf-WebSocket → bestehender Project_0 React-Adapter → Project_0 Zustand-Store → richtiger Slot der fünf existierenden Project_0-Reiter (Overview / Liquidator / Capture / Analytics / Wallets).

Keine erfundenen Runtime-, Telemetrie-, UI-, Profit- oder Health-Werte. Unbekannt/Unavailable/Blocked/Detached/Stale/NotRun sowie requested≠effective müssen korrekt sichtbar bleiben. Reserviertes Topic, Struct, DTO oder Mock-Screenshot ohne echten Producer/Consumer ist **SOLL_MISSING**, niemals CONNECTED oder PASS. Keine neue SPA, kein zweiter Gateway, kein paralleler Project_0-Store und kein periodisches Browser-Polling als Ersatz für den Push-Vertrag.

**P0-PROD-CTRL-05 – VERBINDLICHER COMMAND-RÜCKWEG BIS ZUM NATIVE OWNER.** Jeder tatsächliche Frontend-Schalter funktioniert über exakt denselben sicheren Controller wie p0ctl, auf dem folgenden bestehenden Rückweg:

Bestehender Project_0-Reiter → action-spezifische typisierte Protobuf-Parameter → HTTPS POST /api/v1/command → bestehendes Gateway mit Session/Origin/Host/CSRF, projektgebundener Allowlist und Idempotenz → authentisierte Engine-IPC → nativer Project_0-Controller mit CAS/Version und **allen** fachlichen Safety-/Qualification-Gates → durables Intent/Receipt → neue native STATE-Projektion → Reporter → Gateway → Protobuf-WebSocket → Project_0 Adapter/Store/Reiter.

**Keine Commands über den WebSocket; keine freie Browser→Shell/RPC/Signer-Durchleitung.** ACCEPTED ist keine Ausführung; verlorenes ACK/UNKNOWN darf keine Doppel-Submission auslösen. GUI und CLI teilen die native fachliche Authority. Fehlende native Receipts, Protobuf-Commands oder Publisher bleiben OPEN; nicht durch GUI-Ack oder lokalen Toggle-State vortäuschen.

**P0-PROD-CTRL-06 – FRONTEND-/WIRE-VERTRÄGE SIND PFLICHT, KEINE PARALLELARCHITEKTUR.** Normative Schnittstellen: frontend/README.md §§2–4, frontend/docs/FRONTEND_PHASENPLAN.md, frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md, frontend/docs/GOVERNANCE_UND_SICHERHEIT.md, frontend/docs/SOURCE_VARIABLE_MATRIX.md, project_0/docs/F0_VAR_00_SCOPE.md und F0_VAR_01_MATRIX.md. Project_0-Topic-ID-Raum 10–29; bestehende IDs 10/11/12/13/14/15/20/21 gemäß jeweiligem Vertrag verwenden, keine umwidmen oder neu nummerieren. Derzeit reservierte/unavailable Topics und fehlende Fach-Payloads, Publisher, Command-Handler im existierenden Schema/Reporter/Gateway/Frontend ergänzen; schemagebundene Codegen-/Schema-ID-/Golden-/Kompatibilitäts-/Deployment-Gates prüfen. **Genau fünf bestehende Project_0-Reiter und Graphite-Layouts beibehalten.** Keine BSC-Codeänderung und keinerlei Übertragung von BSC-Latenz-/LIVE-Anforderungen auf Project_0.

**P0-PROD-CTRL-07 – BINDENDE CONTROL-ZUORDNUNG UND F0-VAR.** Ingress/WSS/Geyser/Provider samt Modi, Reconnects und Health → bestehender Capture-/Health-Bereich. JIT/ExactIn, Jito/Bundle, Funding, Safety/Admission und Liquidator → Liquidator. Inventory/Rebalancing/Holds → Liquidator/Wallets gemäß echten fachlichen Gates. Native Backtest-/Optimizer-/Replay-/Report-Jobs → Analytics. Capture-/Run-Start/Stop/Source/Verifikation → Capture. Liveness/Gesamtzustand → Overview. Für **jede** GUI-relevante Variable bzw. Action vor Coding/Abnahme die native Datei::Funktion, Owner, Typ, Einheit, Min/Max/Validator, Default-Herkunft, Sensitivität, Qualifikation, Effektivstatus, Source-/Run-/Version-ID, Proto-Message/Feldnummer, Topic, Reporter, Gateway-Command, Adapter, Store und Reiter-Slot in den **bestehenden** F0-VAR-Inventaren und der semantischen Matrix nachführen. Kein separater Master und keine abgeschriebenen React-Defaults. GUI-relevante Controls ohne vollständigen Pfad sind SOLL_MISSING/FUTURE_PHASE_BLOCKED mit Ticket, nicht DONE.

**P0-PROD-CTRL-08 – NO HIDDEN HARDCODING / UNKNOWN ≠ 0.** Betrieblich änderbare Limits, Timeouts, Retries, Quellen-/Provideroptionen, WDRR-Gewichte, Queue-/RAM-/RPC-Budgets usw. als validierte native Config-/Descriptor-Felder mit Typ, Einheit, Default/Min/Max, Quelle, Restart-/Hot-Change-Regel und Run-Manifest-Bindung statt verstreuter fester Literale. CLI und Protobuf/Frontend nutzen dieselbe Rust-Autorität. **Protokoll-/Deployment-Konstanten** dürfen nur source-verifiziert, explizit versioniert und fachlich begründet fest sein – kein willkürlicher GUI-Slider dafür. Unbekannte Reconnect-Zähler, Clock, Slot-Index, Wallet-Wert, Fee oder PnL niemals mit 0/false als angeblich gemessenen Wert ausgeben; Option/UNKNOWN und erforderlichenfalls fail-closed. Insbesondere der Platzhalter reconnects: 0 ist zu ersetzen. Fachlich gleiche Bedeutungen über einen gemeinsamen Typ/konvertierende Tests konsistent halten.

**P0-PROD-CTRL-09 – NO DUPLICATE OR DEAD PRODUCTION PATH.** Jedes Ticket, das einen produktiven Pfad ersetzt, muss im **gleichen Ticket** den alten Aufrufpfad entfernen, auf denselben nativen Einstieg routen oder mit exakt geprüftem historischen Zweck abgrenzen. Tote nur durch Unit-Tests aufgerufene Produktfunktionen, doppelte Reconcile-/Settlement-/Admission-/Evaluator-/Transportpfade, unnötige RPC-Budget-Verbraucher und parallele Economics prüfen und gezielt konsolidieren. Vor Löschung Kapital-/UNKNOWN-/Crash-/Recovery-/Chain-Evidence-Semantik und Tests erhalten; keine pauschale Löschung historischer Reader.

**P0-PROD-CTRL-10 – HÖCHSTENS EIN OPTIONALER PRÜFAGENT (Nutzerentscheidung 2026-10-25, ersetzt die frühere Pflicht zu zwei Vetoprüfern).** Der implementierende Hauptagent arbeitet standardmäßig **allein** und bleibt selbst für die Codequalität verantwortlich. **Höchstens ein** zusätzlicher read-only Prüfagent ist zulässig und nur bei konkret begründetem Nutzen: kritische Änderungen an Wallets, Reservations, Funding, Submission oder Financial Safety; komplexe Crash-/Recovery-/UNKNOWN-Semantik; größere Änderungen über Rust-, Protobuf-, Gateway- und Frontend-Grenzen; schwer auflösbare Architekturkonflikte oder widersprüchliche Codebefunde. Für einfache, klar begrenzte Bugfixes **kein** Agent. Der Prüfer untersucht nur den betroffenen Diff und die tatsächlich relevanten angrenzenden Aufrufpfade (keine Repository-Neuprüfung, keine Agentenkaskaden, keine endlosen Review-Schleifen) und meldet konkrete Befunde mit Datei::Funktion, Zeile, SHA, Begründung. Ein begründeter CRITICAL/HIGH-Befund ⇒ kein Ticket-PASS/Push bis zur Korrektur. Kein Prüfer eingesetzt ist **kein** Abnahmefehler. Reviews read-only; keine konkurrierenden Schreibzugriffe auf Worktrees.

**P0-PROD-CTRL-11 – STRIKTE END-TO-END-DEFINITION-OF-DONE.** Für jede GUI-schaltbare Pflichtfunktion alle folgenden Nachweise zusammen erbringen: echte Rust-Funktion mit produktivem Consumer und echter Evidence; keine Mock-/Stub-/Legacy-Ersatzimplementierung; ein gemeinsamer Engine-Controller für p0ctl und Gateway; versionierte validierte effektive Config und echte Gate-/Durability-/Receipt-Logik; **Status-Hinweg vollständig Engine→Proto→Reporter→Gateway→WS→Adapter→Store→bestehender Reiter**; **Command-Rückweg vollständig Reiter→Proto-POST→Gateway→Engine→Receipt→STATE**; positive und negative Tests für Moduswechsel, invalid params, Version conflict, forbidden, UNKNOWN, Restart, Reconnect, Detached und konkurrierende Bedienung; echte relevante Rust-/Protobuf-/Golden-/Gateway-/Frontend-/Playwright-Gates; ein optionaler Prüfagent nach CTRL-10 nur bei kritischen oder grenzüberschreitenden Änderungen. Tests gezielt: ein RED-Test je unabhängiger Fehlerursache, betroffene Regressionen, das größere fmt-/Clippy-/Feature-/Release-/E2E-Gate einmal je abgeschlossenem Korrekturpaket (nicht nach jedem Edit). **Nur CLI, nur DTO, nur Stub, nur Unit-Test, nur lokaler Frontend-Toggle, nur reserviertes Topic, nur Dokumentation = NICHT PASS.** Fehlt allein gesondert genehmigungspflichtige externe Mainnet-/Provider-/LIVE-Evidence: Code ggf. LOCAL_PASS, betreffende echte Qualifikation NOT_RUN/BLOCKED, **niemals PRODUCTION_QUALIFIED behaupten**.

**P0-PROD-CTRL-12 – FUND: SOFORT ANHALTEN, KORRIGIEREN, NACHWEISEN.** Bei einem entdeckten produktiven Mock-/Stub-/Legacy-/internen JSON-/Hardcoding-/Doppelpfad-/UI-Contract-Bruch **im betroffenen Ticket unmittelbar stoppen**, Fundort, fehlerhafte Klassifizierung und Ursache gegen Datei/Commit/Tests dokumentieren, im vorhandenen Arbeitsumfang echte Lösung statt Schlussbericht-Ausrede liefern; bei kritischen Änderungen ggf. den einen optionalen Prüfagenten (CTRL-10) erneut prüfen lassen. Kann ein externer Vertrag oder eine separate genehmigungspflichtige Qualifikation nicht nachgewiesen werden: betroffenes Ticket OPEN/BLOCKED/NOT_RUN, finanzielle Aktion sicher verweigern; **keine** eigenmächtige Signierung, Submission, Mainnet-/Paid-Abfrage, LIVE-Freigabe oder Safety-Bypass. Sachlich unabhängige sichere Tickets nicht unnötig blockieren. Nur die existierenden project_0/docs/BUGS.md, STATUS.md, MEASUREMENT_LEDGER.md und F0-VAR-Inventare nach tatsächlichem Ist-Stand aktualisieren. Git **ausschließlich main**, keine Branches, kein Force-Push; konkurrierende Worktree-/HEAD-Verschiebungen vor dem Commit beachten, fremde Änderungen nicht überschreiben.

## P0-DEC-2026-10-25 — bindende Designentscheidungen 1–15, G-1 und sofortiger Full-Stack-Umsetzungsauftrag

**Verbindlicher Status: DESIGN_APPROVED, nicht CODE_COMPLETE, nicht OFFLINE_VERIFIED, nicht LIVE_AUTHORIZED.** Entscheidungsnachweis: `project_0/docs/ENTSCHEIDUNGEN.md` v1.3 (15 Beschlüsse vom 25.10.2026, Nachträge N-1 bis N-7). Die **fachliche Plan-/Gate-Autorität** liegt weiterhin in `PHASENPLAN.md`, die **einzige Ticket-Statusautorität** in `BUGS.md`. Der neue G-1-Sofortumfang bezieht sich nur auf DEC-01…15. Dieser Abschnitt ist die fachliche Autorität **innerhalb des vorhandenen** `project_0/docs/PHASENPLAN.md` für die hier genannten neuen Beschlüsse. Er präzisiert P0-PROD-CTRL-01–12, FE-00–07, P4A-JIT, P6-MT, P7/P8, C0-E1, JFI/PERSIST/SK und F0-P0UI/F3/F6/F7. Alte Planstellen, die die **Entwicklung** einer hier beschlossenen Funktion auf „nach FE-07“, einen späteren Frontend-Block oder den dauerhaften historischen Python-Betrieb vertagen, sind ab sofort **SOLL-seitig ersetzt**. Historische IST-Befunde und tatsächlich noch gesperrte ausführbare Codes bleiben als wahrheitsgetreue Ausgangslage stehen und dürfen erst nach nachgewiesener Implementierung als behoben bezeichnet werden. Vorhandene abgeschlossene Codearbeit nicht neu schreiben.

**Beschlossene Nachträge:** N-2 bis N-7 vom 25.10.2026 sind verbindlich; der vollständige Entscheidungsnachweis steht in `ENTSCHEIDUNGEN.md` §7. LIVE/Mainnet/Provider-/Werttransferfreigaben bleiben unverändert gesperrt.

### G-1 — jetzt vollständig implementieren, erst nach externen Gates aktivieren

**DEC-01…16 jetzt gemäß N-5 fertigstellen** (DEC-16 Nutzerentscheidung 26.10.2026; Reihenfolge nach DEC-07: 16 → 06 → 08). Für **jede Entscheidung mit Laufzeitwirkung** sind ein realer nativer Rust-Owner-/Controller-Pfad, die validierte Konfiguration, typisierte persistente `.p0j`-Records einschließlich Crash/Restart/UNKNOWN sowie der **in N-5 fest zugeordnete** Protobuf-/Reporter-/Gateway-/React-Status-Hinweg und gegebenenfalls Command-Rückweg **im selben Fachpaket** nachzuweisen. Positive und negative RED→GREEN-, Loopback-/Golden-/E2E-Tests, F0-VAR- und Registereinträge sind Pflicht. DEC-09 ist ausschließlich eine nachzuweisende Build-/Inventarentscheidung, DEC-14/15 sind Prozessentscheidungen ohne erfundene GUI- oder Laufzeitpfade. Keine nur testweise erreichbare Produktfunktion, kein MOCK-/Stub-/Legacy-/JSON-Produktivpfad, kein GUI-Draft statt echter Steuerung. Für fachlich schaltbare Capabilities `OFF/OBSERVE/ACTIVE` mit gewünschtem vs. wirksamem Zustand/Blockgrund, für Jobs echte START/STOP/CANCEL/STATUS statt erfundenem OBSERVE. Neue freigaberelevante Funktionen **OFF**, außer ausdrücklich vereinbarter OBSERVE-Vergleich. Auch ACTIVE-Code jetzt implementieren und offline testen, aber echte Aktivierung mit nicht erfüllten Nachweisgates **fail-closed**.

**N-5 – verbindliche feste Frontend-/Command-Zuordnung:**

| DEC | Status bis zum bestehenden Reiter | Command-Rückweg |
|---|---|---|
| 01 | Liquidator: `BLOCKHASH_STALE`-Verlustgrund, Wiederanläufe | nein |
| 02 | Liquidator: Simulation wiederverwendet / neu / abgelehnt | nein |
| 03 | Wallets: fünf N-2-Ergebnisklassen, Bindungsdauer, Calls/Bytes | nein |
| 04 | Wallets und Liquidator: Modus, A/B/Ist und Abweichungen | ja: Modus mit Nachweis-Gate |
| 05 | Effektive Werte mit Herkunft in den zuständigen Reitern | nur ausdrücklich editierbare Felder (DEC-06, DEC-07) |
| 06 | Liquidator: Threshold-/Fallback-/Latenzstatus gemäß DEC-06 | ja: Modus-Hot-Change |
| 07 | Capture: TTL, Grenzen, Herkunft und Phase | ja: TTL zwischen Läufen; getrennte Netzwerk-/Startfreigabe |
| 08 | Capture und Analytics: INDEX/CROSSCHECK/STRICT und Jobs | ja: START/STOP/CANCEL/STATUS |
| 09 | keine Anzeige (Build-/Inventartest-Entscheidung) | nein |
| 10 | Liquidator/Wallets: Approval-Status ohne Secrets | nein |
| 11 | Liquidator: Capability `requested/effective/block_reason` | ja: Capability-Modus |
| 12 | keine neue Anzeige; typisierte Gründe in vorhandenen Projektionen | nein |
| 13 | Liquidator/Wallets: Admission- und LIVE-Status | ja: LIVE-Anforderung über Controller, `effective=BLOCKED` bis Freigabe |
| 14, 15 | Prozessentscheidungen ohne neue GUI-Funktion | nein |
| 16 | Capture: Laufzeitmodell, Qualifikationsnachweise, aktuelles Fenster und HALT-Grund | ja: Boot-Ablauf „Bis Stop“ (gesperrt mit Grund ohne Qualifikation), Stop (Drain/Seal/Abschluss), Dauerfreigabe Autostart erteilen/widerrufen (Bindung angezeigt) |

**Kein neues Bedienelement** bei `nein`. Ein solcher Schalter bräuchte eine neue Nutzerentscheidung. Für jede Entscheidung **mit Laufzeitwirkung** ist der typisierte persistente `.p0j`-Nachweis Pflicht; für reine Build- oder Prozessentscheidungen werden keine künstlichen Laufzeitrecords erzeugt.

**Entwicklungsfreigabe ≠ operative Freigabe.** Automatisch aufgehoben sind reine Design-/Reihenfolge-Sperren gegen offline umsetzbaren Code und gegen Frontend-Protobuf-Integration. **Nicht aufgehoben:** echter Mainnet-RPC/WSS/Geyser-Zugriff, FREE_ONLY-/Quoten-/Archivqualifikation, bezahlte Infrastruktur, LIVE, Signing, werttragender Transfer/Submission, echtes Jito-Senden, nicht genehmigte Captures, P1B-Deployment-/Promotion und FE-06-Einzelfreigabe. Ohne externe Belege ist die konkrete reale Aktion `NOT_RUN`, `BLOCKED` oder `NOT_AUTHORIZED`. Kein `PRODUCTION_PASS` aufgrund nur lokaler Tests. Neue reale Evidence kann korrigierende Implementierung notwendig machen; „später nur schalten“ ist ein vollständiges Lieferziel, keine unwahre Fehlerfreiheitsgarantie.

### DEC-01 — V2-09 Blockhash: Frühabbruch **B mit liveness-sicherem Wiederanlauf**

Ein stale/missing CachedBlockhash wird **vor** `plans_with_health` erkannt: keine nutzlose Planung, kein kandidatenbezogener Budgetverbrauch. Der Owner signalisiert den **bestehenden** budgetierten Blockhash-Worker sofort über eine bounded/coalesced Wake-up-Nachricht; **kein RPC, Sleep, Lock-Wait oder I/O im Hotpath**. Kandidat mit unverändertem originalem `T_rx`, Input-Revision und alter Lane (L0 bleibt L0) begrenzt parken, höchstens einer je Account; nach neuer gültiger Blockhash-Generation kontrolliert wiederanstellen und erneut sämtliche State-/Freshness-/Admission-Gates prüfen, ohne Busy-Loop, Duplikat oder Aufwertung eines abgelaufenen T_rx. Späten Freshness-Check vor Build erhalten. Frühverweigerung als eigener belegter `Build`/Blockhash-Befund mit versionierter Loss-Priorität; nicht ausgeführte Routing-/Deadline-Prüfungen nie als PASS/FAIL melden; Vergleichbarkeit historischer Statistiken nachweisen. Tests: 0 Builder-Calls/Budget unverändert, Refresh bei 429, parked-lane/fairness/revision/stale/restart, Race und Begrenzung. **Ist ein sicherer Wiederanlauf nicht vollständig nachweisbar, bleibt der Frühabbruch deaktiviert; allein der bestehende Diagnosezähler und der Spätcheck gelten.**

### DEC-02 — P7-03/V2-15 Simulation: **A, gebundene Wiederverwendung**

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** TEILWEISE laut BUGS.md: aktuelle echte Simulation-/State-Kontextbindung bis zum Handoff.

Nur der **identische Wire-Digest der exakt signierten `SignedPayload`-Bytes einschließlich sämtlicher Signaturen, deren Reihenfolge und Submission-Pfad** legitimiert die Wiederverwendung; ein **Message-Hash allein reicht ausdrücklich nicht**. **Bundle nur mit tatsächlicher Bundle-Simulation** derselben gebundenen Payload. Nur das typisierte `SimResult::Ok` aus einem echten `SimulatedPayload`-Token ist verwendbar, nie Failed/Unavailable/Deadline; `exact_signed_payload_simulated` **aus dem Token ableiten, nie als Literal**. Kontext-Slot ≥ Intent-State-Slot, gleiche State-/Program-/Lookup-Generation, kein Gap/Recovery/ProgramData-Drift seit Simulation, gültiger Blockhash und **Alter ≤ `max_state_age_ms` der Approval** beim Handoff. Bei Verletzung Neusimulation nur mit Budget/Deadline oder fail-closed ohne neu entstehenden Hold. Ein bestehender Pipeline-/FE-04-Admission-Owner, keine zweite Simulation ohne Grund; pro Bindungsbedingung Negative-/Mutationstests und genau eine Simulation im positiven Wiederverwendungsfall.

### DEC-03 — P7-02 Non-Landing: **B negativ + C immer als kanonische Gegenprüfung; positive Sichtung ist Veto**

**N-2 (verbindlich, 25.10.2026):** Jede B-Quelle wird für den konkreten Versuch in genau eine Klasse eingeordnet: **verwendbar** (alle vier B-Kriterien erfüllt: finalisierte Blockhöhe > `lastValidBlockHeight`; `minimumLedgerSlot` ≤ State-Slot; alle gebundenen Signaturen via `getSignatureStatuses(searchTransactionHistory=true)` abwesend; `getTransaction(finalized)=null`), **unbrauchbar** (unerreichbar/429/Pruning/nicht über Ablaufhöhe finalisiert/fehlerhaft; keine Bestätigung) oder **positive Sichtung** (gebundene Signatur finalisiert vorhanden). B-Quellen sind nur unabhängig, wenn sie von verschiedenen Betreibern stammen. Positive Sichtung **irgendeiner** B-Quelle oder der C-Prüfung sperrt jede Non-Landing-Freigabe.

**C läuft in jedem Non-Landing-Fall**, außerhalb des Hotpaths, als lückenlose finalisierte kanonische Block-/Parent-/Skip-/Signatur-Gegenprüfung vom State-Slot bis zu einem Block mit finalisierter Höhe > `lastValidBlockHeight` (`getBlocks`, vorhandene Blöcke vollständig lesen, gebundene Signaturen prüfen). Positive Sichtung löst eine Landing-Prüfung aus: finalisierte Transaktion, vollständige Metadaten und Blockzugehörigkeit der Signatur zum gemeldeten finalisierten Slot, soweit erreichbar aus beiden Quellen. **Bestätigte Landung** führt ausschließlich zur regulären FE-04-Reconciliation von TX-Ergebnis, Token-/Lamport-Deltas, Gebühren und ggf. Exit. **Kapital wird nur durch qualifiziertes durables Journal-Settlement freigegeben.** Keine oder unvollständige Landing-Bestätigung, fehlerhafte/unklare Belege oder C-Veto ohne bestätigte Landung → `UNKNOWN`, Holds bleiben, typisierter Widerspruchsrecord und Alarm, erneute Prüfung zum Retry-Intervall.

**Non-Landing-Freigabe ausschließlich wenn** (1) keine positive Sichtung existiert, (2) C lückenlos qualifiziert und negativ ist, (3) entweder zwei verwendbare unabhängige B-Quellen negativ sind **oder bei weniger als zwei verwendbaren** C allein den vollständigen qualifizierten kanonischen Abwesenheitsbeweis liefert. Fehlender Block, Call-Fehler, C unvollständig oder widersprüchliche positive Sichtung verweigern fail-closed. Kein Operator-Override und kein kostenpflichtiger Fallback; unbekannte/gekürzte Archivdaten sind kein Beweis.

**Wallets zeigt je Versuch exakt eine Ergebnisklasse:** `B+C bestätigt`, `C allein qualifiziert`, `Veto – Landung bestätigt`, `Veto – Widerspruch offen`, `UNKNOWN – unvollständig`. Dazu unbrauchbare Quellen nach Grund, Hold-Dauer, RPC-Calls und Bytes; **kein Schalter**. Die veralteten Klassen `B allein entschieden`/`C nötig` gelten nicht mehr. Negative und positive Loopback-Tests gemäß `ENTSCHEIDUNGEN.md` §7 N-2 einschließlich geprunter/429/unerreichbarer Quellen, B-Veto trotz negativem C, bestätigter Landung, unvollständigen Deltas und idempotentem Retry. Ohne real qualifizierten C-Beweis: `UNKNOWN`.

### DEC-04 — V7: **A-Hold als Sicherheitsautorität + B exakt vollständig gebaut**

A bindet konservativ und ohne Doppelzählung **zwei getrennte Größen**: den vollen relevanten Seizure-/Exposure-Wert ausschließlich im Safety-Inventarlimit und alle tatsächlich vor dem Versuch ausgebbaren Fee-/Tip-/Rent-/Funding- und Input-Verpflichtungen als je Asset geprüfte Wallet-Reservierung. Collateral aus der Liquidation wird niemals als vorab vorhandener Wallet-Saldo vorausgesetzt. B rechnet aus dem **signierten** Plan vollständig die Mint-/Amount-/Decimals-Flüsse, Kamino-/Drift-/JupLend-Umrechnungen, Swap-Floors, Token-2022-Fees, Flash-Borrow/Repay, SOL-/Rent-/Tip-/Priority-Verpflichtungen und finalisierten Restbestand. **Wallet-Reservierung und Inventory-Limit sind getrennte unabhängige Guards.** Wallet-Holds belasten ausschließlich **vor dem Versuch verfügbare ausgebbare Einheiten** (SOL-/Input-/tatsächlich walletseitige Fees/Tip/Rent/Funding) gegen den jeweiligen Saldo abzüglich anderer Holds und Policy-Reserve; je Asset getrennt. **Das bei einer Liquidation übernommene Collateral wird niemals gegen den Wallet-Saldo geprüft**, sondern nur mit konservativem USD-Oberwert gegen das getrennte **Safety-Inventarlimit**; keine Doppelzählung. OBSERVE vergleicht A/B/finalisierte Ist-Flüsse pro Attempt in typisierten `.p0j`-Records, Liquidator/Wallets zeigt Modus/Quellen/Abweichung/Fehlschätzung. B-ACTIVE-Code jetzt vollständig offline bauen; reale Promotion **erst nach mindestens N echten finalisierten Versuchen ohne Unterschätzung (N noch vom Nutzer festzulegen)**, grünen Tests und dokumentierter Freigabe; **qualifiziertes B-ACTIVE spätestens vor parallelen LIVE-Versuchen verpflichtend**, sonst Parallel-LIVE blockiert. **FlashFunded bleibt bis zur eigenen Klassenqualifikation OFF/BLOCKED**. Drift ohne belegte Qualifikation automatisch A, B für diese Position `UNQUALIFIED`; UNKNOWN-Holds bleiben.

### DEC-05 — FS-D11/CTRL-08: **A, eine zentrale typisierte Konfigurationsautorität**

Sämtliche 24 Owner-Parameter und verbleibende Doppeldefaults in `RuntimeConfig/Resolved`-Descriptor migrieren; **jede Betriebsart ausdrücklich genau einem Profil zuordnen, auch `econ4h` ohne Argumente**. **Die Migration darf keinen heute effektiven Wert ändern**: pro Betriebsart alle vorgefundenen effektiven Werte im Profil konservieren und anhand vor/nachher typisiertem Effective-Config-Record belegen. **Präzedenz:** expliziter CLI-/Controller-Override > Konfigurationsdatei > Environment > Profil > zentraler Default, soweit Safety-Hardlimits keine restriktivere Wirksamkeit erfordern; alle Quellen/Einheiten/Default-Min-Max/Restart-Regeln offen ausweisen. Neustart ist Standard, Hot-Change nur ausdrücklich pro Feld mit CAS/Version/Receipt. **Workers nicht blind** auf 1 oder `available_parallelism` setzen: Econ4h-/Shadow-/Rayon-Parallelität und Performance gegen Ausgangsverhalten messen und erhalten. Native Effective-Config-Projektion, keine React-Zweitdefaults. **`workers = 1` im Shadow-Profil bleibt „vorläufig, Messung offen“** und wird genauso im Frontend ausgewiesen; keine ungemessene Parallelitätsoptimierung.

### DEC-06 — FS-C08/V2-13: **A, OFF/OBSERVE/ACTIVE + sicherer Hot-Change**

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** OFFEN laut BUGS.md: Threshold-Modus, sicherer Hot-Change und voller UI-/CLI-/Journal-Rundweg.

OFF: vorhandener Sweep autoritativ. OBSERVE: Sweep bleibt autoritativ; Threshold berechnet auf **derselben belegten State-/Input-Revision** parallel ohne zweite Candidate-Admission und persistiert Treffer-/Fehltreffer-/Latenz-/CPU-Differenzen. **OBSERVE darf den Sweep nicht verlangsamen**: separater Worker auf unveränderlichem State oder nachgewiesenes hartes CPU-/Owner-Zeitbudget; bei Budgetmangel nur OBSERVE sampeln, Sweep nicht bremsen; Owner-Blockzeit messen und anzeigen. ACTIVE: Threshold über **eine gemeinsame Repeat-/Admission-Policy**, nur Promotion **aus belegtem OBSERVE**, mit komplettem Index, Quiescence-/Generation-Barriere, qualifiziertem **Gate 0 fehlende Kandidaten**, festgelegtem CPU-/Health-/Queue-Budget und weiterlaufenden Stichproben-Gegenproben in ACTIVE. **Bis zum fertigen Index-Bootstrap, auch nach Recovery/Neustart: `requested` getrennt erhalten, `effective = SWEEP`, `block_reason = INDEX_BOOTSTRAP`. Ein einziger belegter Fehltreffer der Sweep-Stichprobe in ACTIVE muss automatisch auf SWEEP zurückschalten und einen typisierten Alarm-Record auslösen.** Bei Gap, unvollständiger Revision oder Budgetüberschreitung sichere Refusal/Fallback, keine verlorenen oder doppelt eingereihten Candidates. Native CAS, Policy-Version, dauerhafte Receipts, Restart-/Reconnect-Recovery. **Liquidator-Reiter**: requested/effective/qualification, Misses, Lanes, p50/p95/p99 und Fallbacks. Threshold ACTIVE autorisiert **kein** LIVE-Trading.

### DEC-07 — N7: **A, integrierter Doctor→Audit→Start mit konfigurierbarer TTL**

Gemeinsamer nativer Controller für CLI und **Capture-Reiter**; umfangreiche lokale Preflights **vor** Network-Audit. Network-read-Freigabe und Run-Start-Freigabe bleiben **getrennte explizite** Zustimmungen; keine automatische Reapproval. TTL als **typisiertes Descriptor-Feld in Sekunden**, Default 60, Minimum 10, Maximum 120, **nur zwischen Läufen** änderbar, Quelle und Wirksamkeit in Run-/Audit-Record. TTL beginnt mit dem **ältesten tatsächlichen Network-Read**, nicht Token-Ausstellung. Neuer gültiger Wert ist **an die konkrete Audit-/Approval-Identität gebunden**; keine rückwirkende Verlängerung. Alle heutigen hart auf 60 begrenzten `Spec`, `validate_start`, `validate_admission`, Runtime- und CLI-Prüfungen vertragsgerecht migrieren, ohne BOOT-12-Drift-/Identity-Rechecks, FREE_ONLY/Quotas oder expired-fail-closed abzuschwächen. Capture zeigt Wert/Grenzen/Herkunft/Phase; mit Protobuf-Command/Receipt. Tests **9 s abweisen / 10 s zulassen / 60 s / 120 s zulassen / 121 s abweisen**, langsamer Read, Ablauf, Drift, Doppelstart, Crash.

**Nutzerentscheidung 26.10.2026:** Boot-Status auf Topic 15 im Capture-Reiter; Rohdaten-Capture = Boot-Modus `market`; jeder Mainnet-lesende Start (Rohdaten-Capture, `project0.capture.start`, Autostart, `p0-longtail` direkt) nur mit zugelassenem Record, nativ (`BOOT_AUDIT_REQUIRED`); Provider-Evidence per Record-ID; GUI-Start vorerst nur Chain-Verification `off` (DEC-08). Status: OFFLINE_VERIFIED `65059ed6` (BUGS §22.5).

### DEC-08 — C0-E1/JFI: **A, INDEX/CROSSCHECK/STRICT nativ aus einer Engine**

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** OFFEN laut BUGS.md: gesamter native INDEX/CROSSCHECK/STRICT-Verifier mit Jobs, Reports, Controller und Frontend.

Ein gemeinsamer nativer Signature-Historical-Reader für INDEX, CROSSCHECK und historischen Winner-Consumer mit Paging, Dedup, 429/Backoff, Budget/Bytes/RPS, Cursor, bounded Journal-Checkpoints, Resume; **kein zweiter Parser**, vorhandene `chain.rs`-/`market_truth.rs`-Klassifizierung wiederverwenden. INDEX = qualifizierter historischer Signatur-/TX-Index, CROSSCHECK = zweite **unabhängig** qualifizierte FREE_ONLY-Quelle, STRICT = vollständige finalisierte kanonische Block-/Parent-/Skip-Proofs. Native `.p0j` Spec/Attempt/Result/Resume, `p0ctl` und Controller-/Protobuf-/Gateway-/Capture-/Analytics-Rundweg für **alle** Modi `off|index|crosscheck|strict`. Bestehende produktive `index`-/`crosscheck`-Codeblockade **als Arbeitsauftrag entfernen**, aber erst nach realen E2E-Tests als aktiv darstellen. **Vor Entfernung der Python-Produktivabhängigkeit ein gepinntes Paritäts-Gate** gegen den historischen A4-Bestand (berichtete 2.917 Quellen – Umfang erst am Pin nachweisen) samt echter positiven/negativen Fixtures. Python ggf. historisch isoliert read-only, keine Löschung bestehender Proofs. Separate typisierte Providerbudgets. Keine Ergebnis-Hochstufung: INDEX_VERIFIED < CROSSCHECK_VERIFIED < CANONICAL_VERIFIED; C0_E1_MASTER_PASS erst mit zusätzlichen harten Capture-/Economics-Gates. 429/Pruning/UNKNOWN → INCOMPLETE+Resume.

### DEC-09 — Historical/JSON: **JA, standardmäßiges Produktionsbinary ohne historische Leser**

Historische JSON/JSONL-Read-only-Decoder ausschließlich in opt-in Nicht-Default-Cargo-Feature `historical` oder gesondertem historischen Tool; kein produktiver internal-JSON-Callgraph. Historische Proofs, Wallet-Altschemas und Test-/Import-Kompatibilität erhalten; **externe zwingende JSON-RPC-/Jupiter-Wire-Grenze bleibt zulässig**. Callgraph-, Feature-/Run-Dir- und Legacy-Read-Regressionstests; keine pauschale Löschung historischer Beweise.

### DEC-10 — FE-04 Approval: **JA, natives `key = value` statt produktivem JSON**

Ein strikt schema-/version-/einheiten-/min-max-validierter Parser mit eindeutigen Keys; Code-/Binary-/Program-/Wallet-/Budget-/Expiry-/Intent-Bindung, **streng validierter einmaliger** Freigabe- und Receipt-Vertrag (**keine neu beschlossene kryptografische Approval-Signatur**), `.p0j` als alleinige persistente Autorität. Alte JSON-Approvals nur abgegrenzt historisch lesbar, **niemals unbemerkt übernehmen oder erneut autorisieren**. CLI/GUI spiegeln eine native Approval-Authority; keine Secrets/Signer an Browser. Replay-/Duplikat-/Drift-/Crash-/Invalid-Tests; FE-06-Geldfreigabe bleibt gesondert.

### DEC-11 — FS-B02/FS-B05/FS-F-20/P4A-JIT/Jito: **JA, Offline-Code jetzt bis zur Bundle-Simulationsgrenze bauen**

**Jetzt offline bis zur Bundle-Simulationsgrenze:** JIT ExactIn-Prerequisites, Bundle-Gebührenmodell (Fee je Transaktion plus Tip), SOL-Belastung und durable Holds für mehrere Transaktionen je Intent; `JitoAdapter` über die bestehende Capability (Default OFF), Bundle-Admission statt pauschalem `FE04_BUNDLE_FEES_UNQUOTED`, typisierte Konfiguration `P0_BUNDLE_SIM_RPC_URL`. Ohne qualifizierten Bundle-Simulator bleibt `bundle_simulated=false` und die Bundle-Admission verweigert fail-closed. Loopback-/RED→GREEN-Tests belegen Gebühren, Holds, Admission und Simulationsgrenze sowie null Jito-Aufrufe bei OFF. **Nicht vorgezogen durch DEC-11:** Jito gRPC/Prewarm, reale Jito-Verbindung, tatsächliches Senden und Landing-Verarbeitung versandter Bundles; diese Roadmap-Pflichten behalten ihre Mess-/Evidenzgates. LIVE, Signing und werttragende Submission bleiben BLOCKED; keine produktiven Stubs, keine zweite Engine.

### DEC-12 — V2-14-Reste: **JA**

Typisierte Rejection-/Loss- und native `message_hash`-Identität, harte **konfigurierbare** monotone Jito-/Submission-Handoff-Deadline, sichere Budget-/Cancellation-/Unknown-Receipt-Semantik. Keine Latenz-PASS-Behauptung ohne Benchmark, keine separaten ungeprüften Verzweigungen; RED→GREEN an produktiven Call-Sites und Boundary-/Replay-/Zeit-/Budgettests.

### DEC-13 — FS-F-09/FE-04/FE-06: **JA, Admission voll erreichbar und LIVE weiter gesperrt**

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** TEILWEISE laut BUGS.md: DEC-13b echte Owner→FE-04-Admission/Settlement/Exit/Wallets bis zum LIVE-gateten Pfad.

Vorhandenen echten one-shot `reserve→validate→submit→settle/recover`-Pfad als einen nativen Controller-Consumer und kompletten Liquidator-/Wallets-Protobuf-Rundweg verdrahten, nicht nur als aufrufbare Tests. **Gilt für Versuch und Exit. Die In-Run-Admission gibt die eigene `check()`-Reservierung ausschließlich über das durable Journal-Settlement frei (BUGS C3-Rest), ohne eine zweite RAM-Freigabe oder doppelte Kapitalbuchung.** Persistente atomare Single-Flight-Holds, maximal zulässige Gebühren, Idempotenz, Replay, Crash, UNKNOWN, finalisierte Token-/Lamport-Deltas und harte Stop-/HALT-Gates nachweisen. **Ohne separate FE-06-Freigabe keine werttragende Signierung/Submission**; durch negativen Loopback-/Gateway-Test belegen. Keine neue eigenständige Admission-Engine.

### DEC-14 — BUGS als kanonische Statusautorität, Ursprungsreviews erhalten

Alle stabilen IDs inkl. Unter-IDs/Querverweise aus `bugs_b.md`, `fehlstellen_v1.md`, `bug_v2.md` und vorhandenen Reviews verlustfrei nach `project_0/docs/BUGS.md` als **einem kanonischen Gesamtstatus** mappen; Originaltext, Ursache, SHA und Evidence **nicht löschen**. **N-3:** `bug_v2.md`, `fehlstellen_v1.md` und `bugs_b.md` bleiben unverändert an ihrem Ort und werden erst **ab dem tatsächlich abgeschlossenen Konsolidierungs-Commit** eingefroren (keine neuen Befunde, Statusnachträge oder Statusautorität mehr). Danach führt ausschließlich `BUGS.md` den Status und nennt die drei Prüfberichte im Kopf. Ein Originalbericht darf erst nach `fertig/`, wenn **alle** seine IDs in `BUGS.md` `ERLEDIGT` oder `WIDERLEGT` sind; keine pauschale Verschiebung und kein Löschen von Nachweisen. `LOCAL_PASS` ist kein Closing von externer Qualifikation.

### DEC-15 — Git/Worktree: **JA, nur main, pro Session isolierter Index, ein Integrator**

Jede arbeitende Session ein eigener Worktree/Index; bei gleichzeitigen Sessions höchstens ein Integrations-/Push-Owner für **main**, ansonsten ggf. detached Worktrees **ohne neue Branches**. Vor Übernahme fetch Remote-Main/Commit/Tree, kontrolliert nur eigene Änderungen auf aktuellen Stand integrieren, Tests auf resultierendem Commit, ausschließlich Fast-Forward; kein Force-Push, bei Konflikt STOP. Projektfremde BSC-Fachänderungen nicht anfassen; notwendige gemeinsam genutzte Proto-/Gateway-/Frontend-Dateien gezielt, ohne BSC-Produktlogik umzuschreiben.

### DEC-16 — Dauerbetrieb `UntilStopped`: **jetzt vollständig bauen** (Nutzerentscheidung 26.10.2026)

> **REV.1.0 – Revisionsmarkierung für den noch offenen Rest:** Stufe 1/2 sind bereits OFFLINE_VERIFIED und bleiben unverändert; offener Rest Stufe 3 Shadow-Owner, Drains, Journale, Recovery und Gates.

Dauerbetrieb `UntilStopped` (24/7) jetzt vollständig entwickeln, nach DEC-07 und vor DEC-06; keine neue Architektur, kein späterer Neuaufbau. `RunHorizon::admit()` bleibt der zentrale Admission-Prüfpunkt, aber nicht das einzige Gate; die bestehende Sperre wird nicht pauschal entfernt. Qualifikationen als typisierte, persistente, identitätsgebundene Nachweise: SK-07-Segmentierung und Recovery, rollierende Kapazität, Providerbudgets, verfügbare Provider-Evidence. Ohne vollständige Qualifikation `RUN_HORIZON_UNTIL_STOPPED_GATED`; mit ihr grundsätzlich zulässig, aber nur mit den Netzwerk-, Start- und Betriebsfreigaben. Ressourcen-, Provider- und Integritätsgates bleiben im Betrieb wirksam, Verstoß ⇒ sicherer HALT. Offline-Tests beider Fälle inkl. Crash, Restart, Recovery, Ressourcenerschöpfung; reale Mainnet-Qualifikation `NOT_RUN`/`BLOCKED_EXTERNAL`. Qualifizierter Dauerbetrieb ist **keine** Erlaubnis für unbeaufsichtigte Neustarts und **keine** LIVE-Freigabe.

Stufen innerhalb von DEC-16 (Nutzer 26.10.2026), je mit vollständigem gate.sh und CI auf dem finalen SHA: 1. modusneutral (admit(), Qualifikations-Records, rollierende Segmente, laufende Kapazitäts- und Budgetprüfung, Recovery); 2. econ4h und market; 3. Shadow-Owner (Owner-Fenster, Drains, Owner-Journale). Alle drei Modi, LIVE bleibt gesperrt; admit() bleibt je Modus GATED, bis dessen Qualifikationsnachweise vorliegen. GUI (Capture-Reiter, bestehender Boot-Abschnitt, kein neuer Reiter): Dauer-Auswahl „Bis Stop“ mit Fenster, Runway und Qualifikations-ID, ohne vollständige Qualifikation sichtbar, aber gesperrt mit Grund; Stop laufender UntilStopped-Läufe (Drain, Seal, Abschlussstatus); Anzeige, Erteilen und Widerrufen der Dauerfreigabe für den Autostart, gebunden an Config-Hash, Modus, Provider, Budget, maximale Laufzeit und Ablauf, Bindung vor dem Erteilen angezeigt. Stand: Stufe 1 OFFLINE_VERIFIED `202d4304`, Stufe 2 OFFLINE_VERIFIED `f80a3390`, Stufe 3 OFFLINE_VERIFIED `310ca31a` (BUGS §22.5); Nutzerentscheidungen Stufe 2/3 (27./28.10.2026) in ENTSCHEIDUNGEN.

### Verpflichtender Frontend-Rundweg im **gleichen** Fachpaket – `frontend/README.md` §§1–4 + Wire V1.3

**Nicht bis zu einer späteren Frontend-Phase verschieben.** Bestehende fünf Project_0-Reiter Overview, Liquidator, Capture, Analytics, Wallets unverändert, ebenso Graphite, Stores, ein Gateway und ein WebSocket. Der bereits vorhandene F0-VAR-Census ist fortlaufende betroffene Source-/Semantik-Zuordnung, **kein neues globales Census-Sperrgate** für die beschlossenen Arbeiten.

**Status-Hinweg:** echter Project_0-Rust-Owner/Controller → öffentliche, sanitierte native Projektion mit validierter Unit/Qualität/Run-/Generation-/Version-Provenienz → `protocol/schemas/darkbot/project0/v1/*.proto` → Rust `prost`/TS `protobuf-es` aus **Codegen**, keine manuelle Generated-Datei → vorhandener Project_0-Reporter/Engine-Feed → gemeinsames Gateway mit Topic-/Source-Qualität, Latest STATE (generation, seq) und ggf. durablem EVENT-Cursor → genau **ein** same-origin Protobuf-WS `/ws/v1/gateway` → `frontend/src/features/project0/api/*`-Adapter → vorhandener Project0 Zustand-Store → passender vorhandener Reiter. Interne `.p0j`-Frames, Secrets, Keys, Approval-Datei und signierte TX sind **keine** öffentliche Proto-Payload.

**Command-Rückweg (nur bei N-5-Zuordnung `ja`):** echter Reiter-Control → `darkbot.control.v1.CommandRequest` (`PROJECT_ID_PROJECT_0`, request_id, action, params, resource_id und native expected config/profile/plan/policy version) → separater same-origin HTTP-Protobuf-POST `/api/v1/command` mit CSRF/Session/Origin/ACL/Idempotenz → authentisierte Gateway→Engine-IPC → **derselbe** native Rust-Controller wie `p0ctl` mit CAS, Safety, Gate und durablem Receipt → echte Publisher-STATE-/EVENT-Projektion → Gateway/Protobuf-WS/Adapter/Store/Reiter. **Kein Command über WebSocket, keine zweite React-/Backend-Control-Engine.** ACCEPTED bedeutet nicht EXECUTED; Lost ACK/UNKNOWN darf keine doppelte werttragende Aktion auslösen; fachliche Versionsfelder **nie** aus WS-Seq ableiten.

**Zielreiter/Topics:** Overview P0-Health/Liveness (`project0.status`, Topic 10); Liquidator Threshold-/Blockhash-/JIT-/Bundle-/Admission-/Safety-Zustände und echte Controls (`project0.liquidator.state`, 12); Capture Doctor/TTL/Run/Capture/Provider/INDEX-CROSSCHECK-STRICT inkl. Steuerung und Resume (`project0.capture.state`, 13; Boot-Ablauf über 15, weil 13 nur während eines Captures existiert; Anzeige im Capture-Reiter (N-5)); Analytics Verification-/Replay-/Backtest-/Optimizer-Jobs und Beweislevel (`project0.analytics.jobs`, 15); Wallets A/B/Ist-Holds, Non-Landing B/C/Gegenprobe/Veto/Alarm, qualifizierte Finance-/Settlement-Zahlen (`project0.wallets.state`, 14; `project0.finance.summary`, 11). Vorhandene passende EVENT-Topics 20/21 und Nachrichten nur gemäß dem gültigen Protocol nutzen, IDs nie umwidmen. Kein neuer Reiter, zweiter Store oder erfundene Gewinngröße.

**End-to-end-Abnahme im selben Ticket (gemäß N-5):** Für jede Entscheidung mit Laufzeitwirkung der Status-Hinweg: Native Controller + `.p0j` inklusive Crash/Replay/UNKNOWN; Proto-Messages/-Nummern, Codegen/Golden/Schema-ID; Rust-Reporter→Gateway→WS mit SOURCE_DETACHED, Generation/Reattach/Stale; Adapter→Store→sichtbarer Reiter. **Ausschließlich soweit N-5 der Entscheidung einen Command-Rückweg zuordnet**, zusätzlich UI-Control→echter Protobuf-POST→Gateway/Engine-CAS→durables Receipt→neuer STATE; bei N-5 `nein` kein neues Bedienelement und kein Command-Rückweg. Negative Tests zu ungültigen Parametern, Duplicate-Request, Uhr/Expiry, Versionskonflikt, fehlenden Credentials, Restart/Reconnect und 429/Provider-Ausfall; relevante Playwright-/Rust-/Gateway-Tests. Gemeinsamer Build/Deploy muss alle Reporter, Gateway und Frontend gegen **dieselbe** Schema-ID binden. `IST_CONNECTED` erst bei beweisbarem Hinweg, `OFFLINE_VERIFIED` erst mit kompletter Loopback-Abnahme; `NOT_AUTHORIZED/BLOCKED` weiter für reale Aktionen.

### Messfragen, Freigaben und CI-Zuständigkeit – explizit offen

**Keine vorweggenommene technische Entscheidung:** Vollständige dokumentierte Listen in `project_0/docs/ENTSCHEIDUNGEN.md` §3 (B-1…B-9) und §4 (C-01…C-09); die Datei ist **Entscheidungsnachweis**, kein zusätzlicher Phasenplan oder Fehlerstatus. `PHASENPLAN.md` ist weiterhin Soll-/Gate-Autorität und `BUGS.md` die Statusautorität.

- **B-1…B-9 Messfragen, nicht entschieden:** V2-12 Recovery-Staging, Worker-Zahl Shadow/LIVE, Sweep-Batch, Lane-Gewichte 8:3:1, Pool-Idle/Keepalive, MT6–MT10, R12 Retention-Byte-Peak, Mindestzahl **N** für DEC-04/06, Threshold-OBSERVE-CPU-Budget. **Bis zur Messung gilt der vorherige effektive Wert; keine Selbstfestlegung durch einen Agenten.**
- **C-01…C-09 Freigaben: NICHT ERTEILT:** P1B Schritte 5–7, FREE_ONLY-Providerqualifikation inkl. zweitem unabhängigen Anbieter, S1-08-Smoke, G-B-Network-Boot-Audit, 600-s-Kalibrierung, C0-E1-4h, 72-h-Shadow, FE-05, FE-06/FE-07. Kein automatisches Aktivieren aufgrund dieser Docs.
- **Z-1 / N-4 CI-Pflichtjob-Regel: ENTSCHIEDEN.** `OFFLINE_VERIFIED` verlangt den Project_0-Gate-Job und **alle gemeinsam genutzten Jobs, deren Pfade das Paket berührt**, gemäß feststehender Pfad→Job-Tabelle; bei nicht eindeutig erfasstem geänderten Code-/Shared-Pfad sind sämtliche gemeinsamen Jobs Pflicht. Ein roter Pflichtjob blockiert, **außer** die vier kumulativen Nachweise der eng begrenzten BSC-Ausnahme liegen vor: (a) identische Tests mit identischer Ursache bereits auf dem Basis-Commit rot, (b) Test und Ursache tatsächlich in `bsc/**` und kein `bsc/**`-Diff im Paket, (c) sämtliche übrigen, insbesondere Project_0-relevanten Tests des Jobs grün, (d) eigener `BUGS.md`-Eintrag mit Test-IDs/Baseline/Ursache. Diese Ausnahme gilt **nie** für `project0-gate`. Kein Testskip, keine rote CI als grün werten, keine BSC-Fachänderung aus der P0-Session. Bei Ausnahme: `OFFLINE_VERIFIED (CI-Ausnahme <ID>)`, gemeinsame Gesamt-CI weiter rot. Flaky Tests gesondert erfassen.
- **N-6 – CI-Pflichtjob auf exakt geprüftem SHA: ENTSCHIEDEN.** Ein Pflichtjob zählt nur, wenn er auf dem **exakt geprüften Commit-SHA** erfolgreich abgeschlossen ist; keine Übernahme älterer grüner Ergebnisse; `NOT_RUN`, `SKIPPED` und `FAILED` zählen nicht. Startet ein erforderlicher Workflow nicht durch seine Pfadfilter, wird er per `workflow_dispatch` auf dem aktuellen `main` ausgeführt und der gestartete SHA gegen den geprüften SHA abgeglichen. Vor Integration betroffener Codepakete läuft das vollständige lokale `gate.sh`, bei Protobuf-Änderungen zusätzlich `protocol/scripts/check.sh --check` gegen die korrekte Vergleichsbasis. N-4 einschließlich BSC-Ausnahme bleibt unverändert.
- **N-7 – Project_0-CI-Auslöser: ENTSCHIEDEN.** Änderungen unter `project_0/**` lösen die Project_0-CI aus, ausgenommen Pushes, die ausschließlich `project_0/docs/**` ändern; die Auslöser für `frontend/**`, `gateway/**`, `protocol/**` und die Workflow-Dateien bleiben erhalten.

**Pfad→Job-Mapping am CI-Stand 25.10.2026 einschließlich N-7** (Quelle: `.github/workflows/project0-status.yml`, `.github/workflows/gateway.yml`; identisch mit `ENTSCHEIDUNGEN.md` N-4; bei Workflowänderung die Tabelle auf den tatsächlichen neuen Stand aktualisieren):

| Geänderter Pfad | Pflichtjobs aus den aktuell vorhandenen GitHub-Workflows |
|---|---|
| `project_0/**` außer `project_0/docs/**` (N-7) | `Project 0 status contract`: fünf `project0-gate`-Matrizen, `native`, `contract-and-ui` |
| `frontend/**` | `Project 0 status contract`: fünf `project0-gate`-Matrizen, `native`, `contract-and-ui` |
| `protocol/**`, `gateway/**` | `Project 0 status contract`: fünf `project0-gate`-Matrizen, `native`, `contract-and-ui`; zusätzlich `gateway`-Workflow: `rust`, `protocol` |
| `.github/workflows/project0-status.yml` | alle Jobs des Workflows `Project 0 status contract` |
| `.github/workflows/gateway.yml` | beide Jobs des Workflows `gateway` |
| `project_0/docs/**` allein | keine automatische CI-Auslösung durch diese beiden Workflows (N-7-Ausnahme); Dokumentabgleich gesondert nachweisen, keine erfundene CI-Freigabe |

### Reihenfolge, Register und Aufhebung reiner Entwicklungssperren

1. Aktuellen Git-Main-/Datei-/Teststand pinnen; P0-DEC-01…15 und Querverweise gegen vorhandene Rust-/Wire-/GUI-Pfade mappen. Bereits erledigte Umfänge anerkennen, keine neue Parallelarchitektur oder Neuentwicklung fertiger Features.
2. **Sofort** die verbleibende native Rust-Logik mit vollständiger F0-VAR-Feldzuordnung, Protobuf-/Gateway-/React-Hin- und Command-Rückweg sowie Tests **jeweils im selben Fachpaket** umsetzen. Keine sperrende Phrase „erst nach FE-07 entwickeln“, „Frontend später“, „INDEX/CROSSCHECK dauerhaft Python“ mehr.
3. **Nur reale Code-Sperren nach echtem Fix/Loopback entfernen.** Aktuell fehlender/blocked Command-/Verifier-Code bleibt bis dahin wahrheitsgemäß als `SOLL_MISSING`; reale Provider-/Live-/Archive-/Permission-Sperren bleiben fail-closed, wenn nicht separat qualifiziert. Ein Plantext aktiviert keine Capability.
4. Historische Decoder/Beweise bis zur Byte-/Paritätsabnahme erhalten. Ticketabschluss mit Code-SHA, RED→GREEN-/Feature-/Golden-/Gateway-/Playwright-/Release-/CI-Evidence im `BUGS.md`, `STATUS.md`, `MEASUREMENT_LEDGER.md` und den existierenden F0-VAR-Matrizen. Nach dem Konsolidierungs-Commit die eingefrorenen Quellberichte **nicht** weiter aktualisieren. Rote Shared-CI-Gates nicht als grün verschweigen.
5. Statusfolge getrennt und ohne falsche Gleichsetzung: `DESIGN_APPROVED` → `CODE_COMPLETE` → `OFFLINE_VERIFIED` (belegte Tests inklusive Frontend-Rundweg) → `EXTERNAL_QUALIFIED` (separat genehmigte real-world Evidence) → ggf. `LIVE_AUTHORIZED` (konkrete explizite Werttransferfreigabe) → `PRODUCTION_QUALIFIED` (wirklich gemessene volle Gates). Kein „alles fertig“ allein aus LOCAL_PASS oder Docs.

**ABSOLUT VERBINDLICHE SCHLUSSREGEL:** **ECHTER VOLLSTÄNDIGER NATIVE-RUST-CODE + EIN GEMEINSAMER NATIVE-CONTROLLER FÜR CLI UND FRONTEND + BIDIREKTIONALER PROTOBUF-/GATEWAY-/REACT-RUNDWEG BIS ZUM BESTEHENDEN PROJECT_0-REITER + ECHTE SAFETY-GATES UND ENDE-ZU-ENDE-TESTS.** Keine produktiven Mocks, Stubs, Legacy-Ersatzwege, verdeckten Hardcodes, toten nur getesteten Funktionen oder Schein-Schalter. Fehlende externe Qualifikation bedeutet ehrliches BLOCKED, niemals künstlichen PASS.
