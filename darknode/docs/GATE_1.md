# DarkNode – GATE 1: Research, Anforderungen und Architekturentscheidungen

**Stand:** 26.10.2026 · **Status:** OPEN / DISCOVERY · **Scope:** Dokumentation und Plan; keine Code-/Protobuf-/Gateway-Änderung, keine reale Handelsfreigabe.  
**Leitdokumente:** [README](../README.md), [Phasenplan](PHASENPLAN.md).

## 1. Entscheidungslage

| ID | Entscheidung | Status |
|---|---|---|
| DN-D01 | **Produkt:** 24/7 ein festes Bitget-Handelspaar **pro Bot**; dieselbe Strategie, keine automatische Coinsuche | FESTGELEGT |
| DN-D02 | **Bedienung:** ein standardisierter, erweiterbarer **Node-Editor bereits V1**; Indikator-Properties mit LONG/SHORT/EXIT-Schwellen; kein verpflichtender Simple Mode | FESTGELEGT |
| DN-D03 | **Node-Graph:** typisierte/versionierte Rust-Strategy-IR; React Flow nur Editor, keine Browser-Trading-Engine | FESTGELEGT |
| DN-D04 | **Ausführung:** Exchange-Order/Position ist maßgeblich; ein Rust Order-/Risk-Owner und persistente Intents; kein Blind-Resend | FESTGELEGT |
| DN-D05 | **Frontend:** bestehender DarkBot-React-Graphite-Stack und **ein** Gateway; versionierte Protobuf-Kommunikation | FESTGELEGT |
| DN-D06 | **Kapital:** Bitget CEX, API-Key Read/Trade ohne Auszahlung; MetaMask nicht nötig | FESTGELEGT |
| DN-D07 | **MVP:** genau ein qualifizierter Bot, danach mehrere isolierte feste Paar-/Strategieinstanzen | FESTGELEGT |
| DN-D08 | **Exakter Kontomodus:** UTA vs anderes, One-way/Hedge, isolated/cross, API V3-Capabilities | OPEN – Bitget-Abgleich |
| DN-D09 | **Serverseitige Stop-/TP-/Trailing-Semantik** pro Futures-Kontrakt und Ordertyp | OPEN – Bitget-Demo-Evidenz |
| DN-D10 | **Engine-Reuse:** NautilusTrader, Barter oder schlanker eigener Rust-Kern; Lizenz, Performance, Einbindung | OPEN – technische Spikes |
| DN-D11 | **Trigger:** Schwellen-level vs CROSS, Hysterese/Rearm, Barclose vs intrabar und Exit-Priorität | OPEN – Golden-Regelfälle |
| DN-D12 | **Kosten-/Fillmodell:** tatsächlicher Bitget-Gebührensatz und Maker/Taker, Spread, Slippage, Funding | OPEN – Account + Testdaten |
| DN-D13 | **Mehrere Bots auf identischem Kontrakt:** zunächst explizit verboten | FESTGELEGT für V1 |
| DN-D14 | **Exakte Gateway Topic-/ProjectId-Vergabe:** 70–89 nur Kandidat, noch nicht reserviert | OPEN – Governance |
| DN-D15 | **Leistungsziel:** intern niedrige p99-Latenz messen; keine übertragene BSC/P0-1-ms-End-to-End-Vorgabe | FESTGELEGT |

„FESTGELEGT“ bezeichnet das **Produkt-/Architekturziel**, keinen implementierten und keinen getesteten Sachverhalt. Alle API-/Fremdcode-Behauptungen müssen vor Übernahme gegen exakte Version/Commit und Tests verifiziert werden.

## 2. Bereitgestellte Originalquellen / IST

| Quelle | Was wir wissen | Was noch fehlt |
|---|---|---|
| Pine `DarkMa MTF Pressure Scalp v1.0` | Pine v6 `indicator()`; Andean 40/7, Williams 14, BX, sechs MTFs, RLS/Burg/Ghost, virtuelle TP/SL/Timeout Outcomes | Datei-Hash/Version pinnen, vollständige Funktions-/Alert-/Entry-/Exit-Matrix, Pine-Golden-Vectors, Barclose/Intrabar-Parität |
| Zweites Pine-Screener-Script (früheres Gespräch) | Als weitere User-Referenz genannt | Genaue aktuelle Datei/Version verifizieren, nicht Inhalte hineininterpretieren |
| `sawyerma/ws_ai` | Vorarbeiten für Bitget-WS, REST, historische Daten; alte V1/V2-Endpunkte und mehrere WS-Verantwortlichkeiten in untersuchten Dateien | Tatsächliche Laufzeitpfade, Codeowner, Tests, Performance, KEEP/PORT/REPLACE mit aktuellem SHA |
| `sawyerma/darkbot` | Ein bestehendes React-Frontend, Graphite-Tokens, ein Topic-/Protobuf-Gateway; Projekt-Registry und Schema vorhanden | Exakten Integrationscommit, Freiraum ProjectId/Topics, ACL/Commands, Reporter-Pfade vor Codeänderung pinnen |
| Project_0 README | GitHub Mermaid-Architekturmodell (Quellen → Engine → Ausführung → Journal/Reporter) | Nur Darstellungsprinzip übernehmen, keine fachliche P0-Latenz-/Trading-Logik |

**Beispiel aus dem Benutzerwunsch, nicht aktuelles Pine-Entry-Resultat:** Williams-Normierung `W_norm = W_raw + 100`; LONG > 70 %, SHORT < 30 %. Beide Richtungen brauchen separate Entry/Exit- und Rearm-Regeln. Triple-Barrier-Outcomes im Pine-Script sind zunächst virtuelle Labels, **keine Bitget-Orders**.

## 3. Open-Source-Analyse: Daten, Exchange, Backtest, Nodes, Charts

**Diese Projekte sind Referenzen/Kandidaten, keine bereits geprüften integrierten Dependencies.** Live-Streams und vollständige historische Tick-/Orderbucharchive sind verschiedene Fähigkeiten. Codeübernahme nur nach Version-/Lizenzprüfung.

| Projekt | Ziel für DarkNode | Gate-1-Aufgabe |
|---|---|---|
| [CCXT](https://github.com/ccxt/ccxt) | Bitget-Märkte, Spot/Futures/Precision, REST-History-Methodik | tatsächliche Bitget-Methoden/Rate-Limits/Upstream-Code und Lizenz prüfen; nicht automatisch Rust-Hot-Path |
| [Freqtrade](https://github.com/freqtrade/freqtrade) | Bitget-Integration, Download/Backfill, Gebühren-/Funding-Daten, Bot-Setup, Dry Run | Mark-/Index-/Funding-History, Futures-Modes, Licence, konkrete Beispiele |
| [Hummingbot](https://github.com/hummingbot/hummingbot) | Bitget Spot/Perpetual Connector, Order-Tracker, WS-Recovery, Stops | Private-Fill-/Order-Correlation und API-Version der Connectoren prüfen |
| [Cryptofeed](https://github.com/bmoscon/cryptofeed) | Public Trades/Book-L2, Event-Normalisierung, Replay-Aufzeichnung | Bitget-Feedstatus prüfen; **AGPL-Verträglichkeit** vor Codekopie |
| [NautilusTrader](https://github.com/nautechsystems/nautilus_trader) | Rust Data/Execution/Risk/Portfolio, Live/Backtest-Modell | Bitget-Adapterverfügbarkeit separat prüfen, native Rust API/Embedding und Umfang benchmarken |
| [Barter](https://github.com/barter-rs/barter-rs) | Rust-Marktstreams, Typed Events, Exchange-Traits | API-Reife und Bitget-Capabilities mit Nautilus vergleichen |
| [OctoBot](https://github.com/Drakkar-Software/OctoBot) | Bedienelemente, Bot-Konfiguration, History/Backtest-UX | Lizenz, Futures-Capability, UI-Muster |
| [pybotters](https://github.com/pybotters/pybotters) | WS-Autoreconnect, Bitget Orderbook DataStore | Seq-/Snapshot-/Heartbeat-Semantik prüfen |
| [GoCryptoTrader](https://github.com/thrasher-corp/gocryptotrader) | Exchange-Abstraktion und Historienadapter | Latenz/Komplexität/Bitget-Funktionsmatrix prüfen |
| [LEAN](https://github.com/QuantConnect/Lean) | Holdings-/Ordermodelle, Brokerage-Trennung, Replay-Testarchitektur | nur Architekturideen, Lizenz-/Einbettungsaufwand |
| [HftBacktest](https://github.com/nkaz001/hftbacktest) | Tick/L2-Queue-, Slippage- und Latenz-Simulation | Datenerfordernisse und Modellgrenzen bei Bitget prüfen |
| [React Flow](https://reactflow.dev/) | Visueller Graph im bestehenden React | MIT/Lizenz, Toolbar, Kanten, Node-Inspector, Serialisierung |
| [Rete.js](https://retejs.org/) / [LiteGraph.js](https://github.com/jagenjo/litegraph.js) / [Node-RED](https://github.com/node-red/node-red) | Typisierung, Node-Paletten, Validatoren, UX-Vergleich | nur bewährte Patterns selektieren, keine zweite Runtime |
| [Superalgos](https://github.com/Superalgos/Superalgos) | Visueller Trading-Workflow als Referenz | UX-/Lizenzvergleich |
| [TradingView Lightweight Charts](https://github.com/tradingview/lightweight-charts) | Lightweight Candlestick-/Indicator-/Order-Markierung | React/Graphite-Integration, Replay und FPS |

**Primärquellen für kritisches Exchange-Verhalten:** [Bitget UTA Quick Start](https://www.bitget.com/docs/uta/quick-start), [Bitget Trading / Order Management](https://www.bitget.com/docs/catalog/trading/order-management), [Bitget WS Place Order](https://www.bitget.com/docs/uta/websocket/private/Place-Order-Channel), [Bitget Demo](https://www.bitget.com/docs/uta/demo-trading/websocket), [TradingView Webhooks](https://www.tradingview.com/support/solutions/43000529348-how-to-configure-webhook-alerts/), [NinjaTrader Orderstandort](https://static.ninjatrader.com/support/helpGuides/nt8/where_do_your_orders_reside_.htm). **Dokumentation ist kein Beweis für die konkrete Kontokonfiguration; Demo-Integrationstests sind Pflicht.**

## 4. Offene Architekturfragen mit Entscheidungsexperimenten

| ID | Experiment / Nachweis | Entscheidung nach Ergebnis |
|---|---|---|
| R1 | Bitget Demo: ausgewählter Account, Futures-Kontrakt, Order/Fill/Stop/Modify/Cancel/TP-SL, One-way/Hedge | Capability-Matrix pro Ordertyp; exakte adapterseitige Schutzpolicy |
| R2 | Bitget Public WS: Trades/Depth-Seq/Gaps, Heartbeats, Reconnect, REST-Handover; 5s-Kerzen aus Trades | Kanonische Zeit-/Sequenz-/Qualitätsregeln |
| R3 | Bitget Private WS und REST: verlorenes ACK, Teilfills, Restart mit offener Position, Schutzorder gelöscht | State Machine, Recovery-Holds, Idempotenz-/Journalvertrag |
| R4 | Pine: identische historische Bars, Endbar vs intrabar, MTF-Alignment, Williams-Normierung, Andean/BX | Golden-Vectors, Rust-Port-Reihenfolge |
| R5 | Graph: typisierte Portverbindungen, boolesche INVALID-Semantik, Cycle-Rejection, Entry/Exit-Rearm, versionierter Hash | Compiler-/Graph-Contract |
| R6 | Rust Spike: NautilusTrader vs Barter/eigener Kern (gleiche Strategie/Daten) | KEEP/INTEGRATE/REFERENCE/REJECT anhand Test/Kosten/Latency |
| R7 | Backtest: Candle-Fills vs Trade- vs L2-Fills, Funding/Fees, Same-Bar SL/TP | Simulationsstufe und Evidenzgrenzen |
| R8 | Graphite: bestehende Registry/Tokens/Wire, Protobuf Codegen, Realtime/ACL/Receipts, Graphsave | exakter Feld- und Topic-/Command-Vertrag |

## 5. Gate-1-Abnahmekriterien (aktuell nicht erfüllt)

- [ ] Aktuelle exakte Repo-Commits und Hashes aller Pine-Referenzen dokumentiert; Benutzer-Skript vollständig auf Output-/Signalpfade geprüft.
- [ ] `ws_ai` IST-Callgraph und Komponentenentscheidung `KEEP / PORT / REPLACE / DELETE` mit Dateien/Funktionen/Tests dokumentiert.
- [ ] Bitget-Kontomodus, Kontraktpräzision, Stop-/Trailing-/OCO-/Reduce-only-Semantik anhand aktueller Doku **und Demo-Tests** bestätigt.
- [ ] Öffentlicher/private WS-Owner, Datenlücken, REST-Backfill, Historienverfügbarkeit unter 1m und Wiederanlaufpolitik definiert.
- [ ] Node-Type-/Port-/Unit-/Clock-/Validity-Standard mit negativen Graph-/Boolean-Tests spezifiziert.
- [ ] Long/Short/Exit-Schwellen und Bedingungen getrennt aus Userregeln vs originalem Pine ableitbar; Rearm-Modell getestet.
- [ ] Strategie-IR, Graphhash, Plugin-Typversionen, Golden-Vectors, qualifizierter Backtest-/Live-Paritätspfad entworfen.
- [ ] Signierter Orderintent, ACK-vs-Fill, dauerhafter Journal-Sync, Fremdorders, Fills, initiale Schutzorder und Crash-Matrix spezifiziert.
- [ ] Gemeinsamer DarkBot-Protobuf-/Topic-/ACL-/Command-Entwurf (ohne Projektkollisionen) und gewünschte UI-Felder inventarisiert.
- [ ] Externe Projekte nach Lizenz/Commit und messbarem Integrationsnutzen bewertet; Rust-Engine-/Backtest-Spike protokolliert.
- [ ] Roadmap-Abnahmekriterien und Aufwand/Komplexitätsgrenzen dokumentiert; keine noch offene Safety-Frage als PASS ausgegeben.

**Gate 1 darf erst nach tatsächlicher Evidenz auf ACCEPTED wechseln.** Dieses Dokument beantwortet die Produkt- und High-Level-Architekturfrage, nicht alle kontospezifischen Bitget-, Paritäts- oder Recovery-Fragen.

## 6. Gate 1 → Umsetzung

[PHASENPLAN.md](PHASENPLAN.md) definiert die nachfolgenden Pakete. Keine Implementation im Gate-1-Dokument, keine Mock-Daten als Produktdaten, keine automatische Änderung an BSC/Project_0. Jedes Paket bindet tatsächlichen Source-Commit, Tests und Akzeptanzkriterien; bei BLOCKED kein Überspringen.
