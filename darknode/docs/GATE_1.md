# DarkNode – GATE 1: Research, Anforderungen und Architekturentscheidungen

**Stand:** 26.10.2026 · **Status:** OPEN / DISCOVERY · **Scope:** Dokumentation und Plan; keine Code-/Protobuf-/Gateway-Änderung, keine reale Handelsfreigabe.  
**Leitdokumente:** [README](../README.md), [Gesamtphasenplan](PHASENPLAN.md), [P1-Phasenplan V1.1](PHASENPLAN_P1_V1.md), [Endpoint-Registry](ENDPOINT_REGISTRY.md), [P1-Evidenz](P1_EVIDENCE/BITGET_PUBLIC_API_VERIFIKATION_2026-10-27.md).

## 1. Entscheidungslage

| ID | Entscheidung | Status |
|---|---|---|
| DN-D01 | **Produkt:** 24/7 ein festes Bitget-Handelspaar **pro Bot**; dieselbe Strategie, keine automatische Coinsuche | FESTGELEGT |
| DN-D02 | **Bedienung:** standardisierter, erweiterbarer **Node-Editor bereits V1** mit fünf Gruppen, typisierten Wert-/Bedingungs-/Aktions-/Risiko-Nodes und Parametern direkt im Node; kein verpflichtender Simple Mode | FESTGELEGT |
| DN-D03 | **Node-Graph:** typisierte/versionierte Rust-Strategy-IR; React Flow nur Editor, keine Browser-Trading-Engine | FESTGELEGT |
| DN-D04 | **Ausführung:** Exchange-Order/Position ist maßgeblich; ein Rust Order-/Risk-Owner und persistente Intents; kein Blind-Resend | FESTGELEGT |
| DN-D05 | **Frontend:** bestehender DarkBot-React-Graphite-Stack und **ein** Gateway; versionierte Protobuf-Kommunikation | FESTGELEGT |
| DN-D06 | **Kapital:** Bitget CEX, API-Key Read/Trade ohne Auszahlung; MetaMask nicht nötig | FESTGELEGT |
| DN-D07 | **MVP:** genau ein qualifizierter Bot, danach mehrere isolierte feste Paar-/Strategieinstanzen | FESTGELEGT |
| DN-D08 | **Exakter Kontomodus:** UTA vs anderes, One-way/Hedge, isolated/cross, API V3-Capabilities | OPEN – Bitget-Abgleich; One-way + Isolated ist ein enger V1-Prüfkandidat, keine bereits bestandene Kontofreigabe |
| DN-D09 | **Serverseitige Stop-/TP-/Trailing-Semantik** pro Futures-Kontrakt und Ordertyp | OPEN – Bitget-Demo-Evidenz |
| DN-D10 | **Engine-Reuse:** NautilusTrader, Barter oder schlanker eigener Rust-Kern; Lizenz, Performance, Einbindung | OPEN – technische Spikes |
| DN-D11 | **Trigger:** Schwellen-level vs CROSS, Hysterese/Rearm, Barclose vs intrabar und Exit-Priorität | OPEN – Golden-Regelfälle |
| DN-D12 | **Kosten-/Fillmodell:** tatsächlicher Bitget-Gebührensatz und Maker/Taker, Spread, Slippage, Funding | OPEN – Account + Testdaten |
| DN-D13 | **Mehrere Bots auf identischem Kontrakt:** zunächst explizit verboten | FESTGELEGT für V1 |
| DN-D14 | **Exakte Gateway Topic-/ProjectId-Vergabe:** 70–89 nur Kandidat, noch nicht reserviert | OPEN – Governance |
| DN-D15 | **Leistungsziel:** intern niedrige p99-Latenz messen; keine übertragene BSC/P0-1-ms-End-to-End-Vorgabe | FESTGELEGT |
| DN-D16 | **Oberfläche:** DarkNode-Tabs exakt Overview / Builder / Backtest / Live / Wallet; kein permanenter Inspector, keine Signalprüfung und kein zusätzlicher Monitor/Gate-Tab | FESTGELEGT |
| DN-D17 | **Höchste Logik:** Open/Close-Intents + Stop-/BE-/Trailing-Policies an genau einen Rust Position-/Risk-/Execution-Owner; BOTH ist Berechtigung für beide Richtungen, nicht simultane Gegenpositionen | FESTGELEGT |
| DN-D18 | **No JSON intern:** Bitget-JSON ausschließlich am externen Adapterrand; innerhalb Rust/Protobuf/Binärformate; in L0 dürfen externe Originalbytes opak abgelegt werden | FESTGELEGT; Implementierungsnachweis offen |
| DN-D19 | **P1-Speicher:** Roh-/Normaldaten/Candle-Cache als L0/L1/L2-Binärdateien; kleine transaktionale Control-/Registry-/Gap-/Manifestdaten in SQLite; keine produktiven Redis/Parquet/ClickHouse-Pfade | FESTGELEGT; R2c/R2d offen |

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


## Node-Builder-Referenz und drei verbindliche Beispiele

**Visuelle Referenz:** [builder_examples.html](visuals/builder_examples.html) (Datei herunterladen, lokal im Browser öffnen).  
**README mit GitHub-Mermaid-Ansichten:** [Node-Builder und 24/7-Controller](../README.md#4-node-builder-v1--verbindliche-visuelle-referenz).

Die HTML-Vorschau ist ein **interaktiver, eigenständiger Gate-1-Prototyp** und **keine** ins bestehende Frontend eingecheckte Implementierung. Sie benötigt keine Netzwerk-/Bitget-Verbindung. „Prüfen“ kontrolliert nur einen Teil der Browser-Graphstruktur. „Speichern“ ändert nur den flüchtigen lokalen Entwurf; kein Protobuf-Backend, keine Git-Persistenz, keine Orders. Der Stil lehnt sich an die vorhandenen Graphite-Tokens an, ohne \`frontend/\` zu verändern.

**Verbindliche Benutzerführung:**
- Projekt-DarkNode innerhalb der **bestehenden** DarkBot-Topnavigation; keine eigene Anwendung oder neue Architektur.
- Exakt fünf Tabs: **Overview, Builder, Backtest, Live, Wallet**.
- Links im Builder **Nodes**, gruppiert nach **Marktdaten, Indikatoren, Bedingungen, Aktionen, Risiko**.
- Keine permanenten rechten Node-Inspector-Panels, keine Signalprüfung, kein Gate-1-Badge, kein separater Monitoring-Reiter. Alle editierbaren Schwellen / Perioden direkt am Node.
- Auf dem Canvas: Drag Nodes, Ports typisiert verbinden, Zoom, Vorlage wechseln; künftig React Flow in der bestehenden Graphite-SPA.
- Drei V1-Vorlagen: (1) Williams Long/Short, (2) Williams + Andean + BX mit AND, (3) Cross/Exit/Re-Entry. Diese können als Strategiedrafts gespeichert und **mit echtem Rust-Backend** validiert, versioniert, backgetestet und nach gesondertem Freigabegate aktiviert werden.

**Modell der höchsten Logik:** Anders als bloße frei verdrahtete Entry-/Exit-Endpunkte treffen die visuellen Aktionen **nur Aussagen über gewünschte Aktionen**. Alle Long-/Short-Open/Close-Intents und alle Risiko-Policies treffen sich bei **einem** zentralen Rust Position Controller, der Positionsbesitz, Broker-Synchronisation, genau eine gewählte Richtung oder BOTH (beide Richtungen zulässig, nicht beide gleichzeitig), Entry-/Exit-Priorität, idempotente Sendelogik, Bestätigung des initialen Stops und Rearm steuert. \`LONG+SHORT\` heißt *beide Richtungen handeln dürfen*, **nicht** gleichzeitig gegenläufige Positionen eröffnen. Ein Gegensignal darf keinen ungeregelten Sofort-Flip oder eine zweite Order auslösen.

**Typstruktur:** Indikatoren liefern typisierte rohe/normierte numerische Werte oder explizit benannte Bull/Bear-Signale. Ein Vergleich (\`>\`, \`<\`, CrossAbove, CrossBelow, etc.) erzeugt erst einen Bool3/INVALID-Wert. Logik-Nodes verknüpfen mit AND/OR/XOR/NOT. Aktions-Nodes übernehmen Bool3 und geben einen \`SignalIntent\` aus. Risiko-Nodes liefern \`RiskPolicy\`-Werte an den zentralen Owner; sie sind **keine** direkten API-/Order-Calls. Nicht angeschlossene Eingänge sind nur zulässig, wenn der Nodevertrag sie ausdrücklich optional kennzeichnet; insbesondere dürfen fehlende Inputs nie implizit als TRUE gelten.

**Northtrader-Bezug:** Das nachweislich öffentliche [Northtrader Strategy Builder](https://northtrader.com/platform/strategy-builder)-Konzept bietet fünf Gruppen (6 Marktdaten, 15 Indikatoren, 12 Bedingungen, 9 Risiko, 8 Aktionen), direkt editierbare Bausteine, eine Beispielstrategie mit EMA-Cross/Long/SL/TP sowie Codegenerierung in NorthScript. **Wir adaptieren nur die graphische Gruppierung und Wert/Condition/Action-Trennung.** Wir behaupten **nicht**, Northtrader besitze denselben Rust Controller oder dieselbe Engine. DarkNode übersetzt den versionierten Graphen in typisierte Protobuf-/Rust-Strategy-IR, **nicht** in NorthScript. Eine Code-Rückkonvertierung oder freie Script-Node ist **kein V1-Gate**.

**Gate-1-Nachweisstatus:** UX-Referenz und Mermaid-Entscheidungsfluss dokumentiert; **keine** React-Flow-Implementierung und **keine** bestätigte Rust-Orderlogik. Offene Implementationstests: Portkompatibilität/Einheiten/Clocks, INVALID-Semantik, ungenutzte Kanten, Node-State und Replay/Pine-Parität, neue Signalflanken, gleichzeitig widersprüchliche Signals, Partial-Fill/unknown State, Schutzorder, Gateway-Commands und Reconnect.

### Drei Referenzstrategien (keine Echtgeld-Konfiguration)

| ID | Entry | Exit | Demonstriert |
|---|---|---|---|
| N1 | Wnorm > 70 % long / Wnorm < 30 % short | Long schließen bei Wnorm < 55 %, short bei > 45 % | unabhängige Richtungs-/Exit-Intents |
| N2 | Williams-Limits AND Andean Bull/Bear AND BX Bull/Bear | getrennte Schwellen als Beispiel | 3 parallel berechnete Indikatoren → boolesche AND-Nodes |
| N3 | CrossAbove(70) long, CrossBelow(30) short | CrossBelow(55) long exit, CrossAbove(45) short exit | Edge-getriggerte Orders, Rearm nach bestätigtem FLAT |

\`Wnorm = Wraw + 100\`; für klassische Williams %R sind 70 % ⇒ −30 und 30 % ⇒ −70. Die Exitwerte 55/45 sind **nur illustrative Vorschlagswerte**, die echte Pine-Regel ist separat anhand Golden-Vectors zu definieren.


## P5 – Private Bitget-Futures-Anbindung (Research-Artefakt)

[PHASENPLAN_P5_V1.md](PHASENPLAN_P5_V1.md) (V1.2) dokumentiert offizielle UTA-V3-Konto-/Hebel-/Order-/Stop-/Private-WS-Capabilities, Open-Source-Vergleiche, Demo-Verträge, die zentrale Rust-Order-Autorität und die Sicherheits-/Crash-Gates. **Alle privaten Funktionen sind bisher nur dokumentiert, NICHT RUN**. Der volle USDT-Futures-Symbolkatalog ist ebenfalls noch nicht abgerufen. Keine Modus-/Hebeländerung und keine Order während Gate 1.

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
| [React Flow](https://reactflow.dev/) | Visueller Graph im bestehenden React | MIT/Lizenz, Node-Parameter **inline** (kein dauerhafter Inspector), Toolbar, Kanten, Serialisierung |
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

## P1-Revisionsstatus und V5-Priorität (27.10.2026)

**P1-Phasenplan Revision V1.3** liegt aus Gründen stabiler interner Pfade weiterhin unter [`PHASENPLAN_P1_V1.md`](PHASENPLAN_P1_V1.md). Verbindlicher [komponierbarer Bar-/Trade-Order-Vertrag](P1_BAR_SCHEMA_AND_TRADE_ORDER_V1.md) ergänzt P1.6 und die Tests T26/T27. T25–T27 sowie die Rust-P1-Module sind **NOT_RUN**, nicht als technische `BLOCKED`-Defekte zu deklarieren.

**Vorrang vor dem angekündigten Depth50-V5-Produktivwechsel am 05.11.2026:** 8 V5-Parserfehler in 62 Demo-Frames aus dem 60s-Lauf sichern/ursächlich prüfen, das offizielle SBE-XML-Schema pinnen, einen typisierten Decoder und Recovery anhand echter Frames qualifizieren. Solange dies fehlt, V5 **FAIL/INCONCLUSIVE**; keine produktive Readiness aus früheren 25/25 Offline-Frames ableiten. Kein SIGN/ARM/LIVE.

## P5-Code-Audit / Gesamttest-Gate (27.10.2026)

[P5_OPEN_SOURCE_CODE_AUDIT_AND_TESTPLAN_V1.md](P5_OPEN_SOURCE_CODE_AUDIT_AND_TESTPLAN_V1.md) enthält jetzt gepinnte Upstream-Source-/Tests für NautilusTrader, Freqtrade, Hummingbot, CCXT, Barter sowie `DN-P5-ALL-001` als einheitliches, **noch nicht implementiertes** Abnahmeprogramm mit Source-/Replay-/Public-/Private-Read-/Demo-Write-/Soak-Lanes. CCXT-Routing in Bitget ist nicht automatisch UTA V3, Barter ist kein fertiger Bitget-Connector. **Alle P5-A01..A28 NOT_RUN, P5 NOT_RUN; keine SIGN/ARM/LIVE.**

## 6. Gate 1 → Umsetzung

[PHASENPLAN.md](PHASENPLAN.md) definiert die nachfolgenden Pakete. Keine Implementation im Gate-1-Dokument, keine Mock-Daten als Produktdaten, keine automatische Änderung an BSC/Project_0. Jedes Paket bindet tatsächlichen Source-Commit, Tests und Akzeptanzkriterien; bei BLOCKED kein Überspringen.
